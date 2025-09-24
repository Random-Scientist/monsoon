use std::{
    collections::{HashMap, HashSet},
    io::SeekFrom,
    sync::Arc,
};

use anyhow::Context;
use axum::{
    Router,
    extract::{Path, State},
    response::{IntoResponse, Response},
    routing::get,
};
use http::{HeaderMap, HeaderValue, StatusCode};
use librqbit::{
    AddTorrentOptions, ListOnlyResponse, ManagedTorrent, Session, SessionOptions,
    storage::{StorageFactory, StorageFactoryExt},
};
use mime_guess::Mime;
use slab::Slab;
use tokio::{
    io::{AsyncRead, AsyncSeek, AsyncSeekExt},
    net::{TcpListener, ToSocketAddrs},
    sync::RwLock,
};
use tokio_util::io::ReaderStream;

pub struct StreamingFile {
    path: Arc<str>,
    mime: Mime,
    torrent: Arc<ManagedTorrent>,
    file_id: usize,
}
struct Rqstream {
    pub session: Arc<Session>,
    routes: RwLock<HashMap<Arc<str>, StreamId>>,
    streaming_files: RwLock<Slab<StreamingFile>>,
}
impl Rqstream {
    pub async fn create(host: impl ToSocketAddrs) -> anyhow::Result<Arc<Self>> {
        let session = Session::new_with_opts(
            "".into(),
            SessionOptions {
                disable_dht_persistence: true,
                default_storage_factory: InMemStorageFactory.boxed().into(),
                ..Default::default()
            },
        )
        .await?;
        let this = Arc::new(Self {
            session,
            routes: HashMap::new().into(),
            streaming_files: Slab::new().into(),
        });
        let route = Router::new()
            .route("stream/{}", get(h_http_stream))
            .with_state(Arc::clone(&this));
        let listener = TcpListener::bind(host).await?;
        tokio::spawn(async { axum::serve(listener, route).await });
        // TODO impl host service, spin up and spawn here
        Ok(this)
    }
    pub async fn get_info(&self, magnet: String) -> anyhow::Result<ListOnlyResponse> {
        let librqbit::AddTorrentResponse::ListOnly(list) = self
            .session
            .add_torrent(
                librqbit::AddTorrent::Url(std::borrow::Cow::Owned(magnet)),
                Some(AddTorrentOptions {
                    list_only: true,
                    ..Default::default()
                }),
            )
            .await?
        else {
            unreachable!("library guarantee")
        };
        Ok(list)
    }
    pub async fn stream_file(
        &self,
        torrent: &Arc<ManagedTorrent>,
        file_id: usize,
        path_name: String,
    ) -> anyhow::Result<StreamId> {
        let to_path = path_name.into();
        self.session
            .update_only_files(torrent, &[file_id].into())
            .await?;
        let mime = mime_guess::from_path(
            &torrent
                .metadata
                .load()
                .as_ref()
                .context("get meta")?
                .file_infos[file_id]
                .relative_filename,
        )
        .first_or_octet_stream();

        let torrent = Arc::clone(torrent);

        let mut streams = self.streaming_files.write().await;
        let id = StreamId(streams.insert(StreamingFile {
            path: Arc::clone(&to_path),
            mime,
            torrent,
            file_id,
        }));

        let mut routes = self.routes.write().await;
        routes.insert(to_path, id);
        Ok(id)
    }
    pub async fn stop_streaming(&self, id: StreamId) -> anyhow::Result<()> {
        let file = self.streaming_files.write().await.remove(id.0);

        self.session
            .update_only_files(&file.torrent, &HashSet::new())
            .await?;
        self.routes.write().await.remove(&file.path);
        Ok(())
    }
}
#[derive(Debug)]
pub struct Error {
    status: Option<StatusCode>,
    error: anyhow::Error,
}
impl From<anyhow::Error> for Error {
    fn from(value: anyhow::Error) -> Self {
        Error {
            status: None,
            error: value,
        }
    }
}
impl IntoResponse for Error {
    fn into_response(self) -> Response {
        self.status
            .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR)
            .into_response()
    }
}
type AResult<T> = ::std::result::Result<T, Error>;

async fn h_http_stream(
    State(state): State<Arc<Rqstream>>,
    Path(file_name): Path<String>,
    headers: http::HeaderMap,
) -> AResult<impl IntoResponse> {
    // mostly copied from rqbit's http api
    let routes = state.routes.read().await;
    let id = routes.get(&*file_name).context("no route")?;

    let streaming = state.streaming_files.read().await;
    let file = streaming.get(id.0).context("no file")?;

    let cloned = Arc::clone(&file.torrent);
    let mut stream = cloned.stream(file.file_id)?;

    let mut status = StatusCode::OK;
    let mut output_headers = HeaderMap::new();
    output_headers.insert("Accept-Ranges", HeaderValue::from_static("bytes"));

    const DLNA_TRANSFER_MODE: &str = "transferMode.dlna.org";
    const DLNA_GET_CONTENT_FEATURES: &str = "getcontentFeatures.dlna.org";
    const DLNA_CONTENT_FEATURES: &str = "contentFeatures.dlna.org";

    if headers
        .get(DLNA_TRANSFER_MODE)
        .map(|v| matches!(v.as_bytes(), b"Streaming" | b"streaming"))
        .unwrap_or(false)
    {
        output_headers.insert(DLNA_TRANSFER_MODE, HeaderValue::from_static("Streaming"));
    }

    if headers
        .get(DLNA_GET_CONTENT_FEATURES)
        .map(|v| v.as_bytes() == b"1")
        .unwrap_or(false)
    {
        output_headers.insert(
            DLNA_CONTENT_FEATURES,
            HeaderValue::from_static("DLNA.ORG_OP=01"),
        );
    }

    output_headers.insert(
        http::header::CONTENT_TYPE,
        HeaderValue::from_str(file.mime.essence_str()).expect("valid mime"),
    );

    let range_header = headers.get(http::header::RANGE);

    if let Some(range) = range_header {
        let offset: Option<u64> = range
            .to_str()
            .ok()
            .and_then(|s| s.strip_prefix("bytes="))
            .and_then(|s| s.strip_suffix('-'))
            .and_then(|s| s.parse().ok());
        if let Some(offset) = offset {
            status = StatusCode::PARTIAL_CONTENT;
            stream
                .seek(SeekFrom::Start(offset))
                .await
                .context("error seeking")?;

            output_headers.insert(
                http::header::CONTENT_LENGTH,
                HeaderValue::from_str(&format!("{}", stream.len() - stream.position()))
                    .context("bug")?,
            );
            output_headers.insert(
                http::header::CONTENT_RANGE,
                HeaderValue::from_str(&format!(
                    "bytes {}-{}/{}",
                    stream.position(),
                    stream.len().saturating_sub(1),
                    stream.len()
                ))
                .context("bug")?,
            );
        }
    } else {
        output_headers.insert(
            http::header::CONTENT_LENGTH,
            HeaderValue::from_str(&format!("{}", stream.len())).context("bug")?,
        );
    }

    let s = ReaderStream::with_capacity(stream, 65536);
    Ok((status, (output_headers, axum::body::Body::from_stream(s))))
}

#[derive(Debug, Clone, Copy)]
pub struct StreamId(usize);

#[derive(Clone, Copy)]
pub struct InMemStorageFactory;

pub struct InMemStorage {
    files: Vec<std::sync::RwLock<Vec<u8>>>,
}

impl StorageFactory for InMemStorageFactory {
    type Storage = InMemStorage;

    fn create(
        &self,
        _shared: &librqbit::ManagedTorrentShared,
        metadata: &librqbit::TorrentMetadata,
    ) -> anyhow::Result<Self::Storage> {
        // TODO delegate to rqbit's disk TorrentStorage for large files
        Ok(InMemStorage {
            files: Vec::with_capacity(metadata.file_infos.len()),
        })
    }

    fn clone_box(&self) -> librqbit::storage::BoxStorageFactory {
        self.boxed()
    }
}
mod storage_impl {
    use crate::InMemStorage;
    use anyhow::{Context, anyhow};
    use librqbit::storage::TorrentStorage;
    use std::{mem::take, sync::RwLock};

    impl TorrentStorage for InMemStorage {
        fn init(
            &mut self,
            _shared: &librqbit::ManagedTorrentShared,
            _metadata: &librqbit::TorrentMetadata,
        ) -> anyhow::Result<()> {
            Ok(())
        }

        fn pread_exact(&self, file_id: usize, offset: u64, buf: &mut [u8]) -> anyhow::Result<()> {
            let s = self
                .files
                .get(file_id)
                .context("file was none")?
                .read()
                .map_err(|_| anyhow!("bug"))?;
            let offset = offset as usize;
            let remaining = s.len() - offset;
            if buf.len() > remaining {
                return Err(anyhow!("out of bounds read of file"));
            }
            buf.copy_from_slice(&s[offset..(offset + buf.len())]);
            Ok(())
        }

        fn pwrite_all(&self, file_id: usize, offset: u64, buf: &[u8]) -> anyhow::Result<()> {
            let mut s = self
                .files
                .get(file_id)
                .context("file was none")?
                .write()
                .map_err(|_| anyhow!("bug"))?;
            let offset = offset as usize;
            let remaining = s.len() - offset;
            if buf.len() > remaining {
                return Err(anyhow!("out of bounds write of file"));
            }

            s[offset..(offset + buf.len())].copy_from_slice(buf);
            Ok(())
        }

        fn remove_file(&self, file_id: usize, _filename: &std::path::Path) -> anyhow::Result<()> {
            // we can't remove outright but we can at least dealloc the backing buffer
            let mut s = self
                .files
                .get(file_id)
                .context("file was none")?
                .write()
                .map_err(|_| anyhow!("bug"))?;
            drop(take(&mut *s));
            // unsupported
            Ok(())
        }

        fn remove_directory_if_empty(&self, _path: &std::path::Path) -> anyhow::Result<()> {
            // unsupported
            Ok(())
        }

        fn ensure_file_length(&self, file_id: usize, length: u64) -> anyhow::Result<()> {
            let mut s = self
                .files
                .get(file_id)
                .context("file was none")?
                .write()
                .map_err(|_| anyhow!("bug"))?;
            s.resize(length as usize, 0);
            Ok(())
        }

        fn take(&self) -> anyhow::Result<Box<dyn TorrentStorage>> {
            let mut files = Vec::with_capacity(self.files.len());
            for file in self.files.iter().map(|v| v.write()) {
                let mut file = file.map_err(|_| anyhow!("bug"))?;
                files.push(RwLock::new(take(&mut *file)));
            }
            Ok(Box::new(Self { files }) as Box<dyn TorrentStorage>)
        }
    }
}
