use std::{
    collections::HashMap,
    path::PathBuf,
    pin::Pin,
    sync::{Arc, atomic::AtomicU32},
};

use bincode::{Decode, Encode};
use eyre::{Context, OptionExt};
use iced::{
    Task,
    futures::{StreamExt, channel::oneshot::Cancellation},
};
use rqstream::ResultExt;
use tokio::sync::Mutex;
use tokio_stream::wrappers::{ReceiverStream, WatchStream};

use crate::{
    LiveState, Message, TaskList,
    media::{
        LiveMediaHandle, Media, MediaLifecycle, PlayRequest, Playable, PlayableMedia, SourceMeta,
    },
    player::Play,
    show::ShowId,
};

#[derive(Encode, Decode, Debug)]
pub struct TorrentMeta {
    magnet_source: Option<MagnetSource>,
    seeders: AtomicU32,
    leechers: AtomicU32,
}
#[derive(Encode, Decode, Debug)]
enum MagnetSource {
    Nyaa(String),
    User,
}

#[derive(Encode, Decode, Debug, Clone)]
pub struct TorrentMedia {
    files_for_episode_idx: HashMap<u32, u32>,
    magnet: Arc<str>,
    meta: Arc<TorrentMeta>,
}
impl Media for TorrentMedia {
    fn has_ep(&self, idx: u32) -> bool {
        self.files_for_episode_idx.contains_key(&idx)
    }

    fn play(
        &self,
        for_show: &PlayRequest,
        live: &mut LiveState,
    ) -> Option<iced::Task<eyre::Result<PlayableMedia>>> {
        let PlayRequest {
            show, episode_idx, ..
        } = *for_show;
        let f = *self.files_for_episode_idx.get(&episode_idx)?;
        let get_rqstream = live.get_rqstream();
        let mag = self.magnet.clone();
        let meta = self.meta.clone();

        let (send_error, recv_err) = tokio::sync::watch::channel(None);

        Some(iced::Task::future(async move {
            let rq = get_rqstream.await?;
            let torrent = rq.add_magnet_managed(mag).await.anyhow_to_eyre()?;
            torrent.wait_until_initialized().await.anyhow_to_eyre()?;

            let show: u64 = show.into();
            let (send_lifecycle, mut recv_lifecycle) =
                tokio::sync::watch::channel(MediaLifecycle::Resume);

            let subpath = format!("{show}_e{episode_idx}");
            let path = format!("http://127.0.0.1:9000/stream/{subpath}");
            let mut stream = None;
            let ls_torrent = torrent.clone();

            tokio::spawn(async move {
                while recv_lifecycle.changed().await.is_ok() {
                    let msg = *recv_lifecycle.borrow_and_update();
                    let mut should_break = false;
                    let res = match msg {
                        MediaLifecycle::Pause => {
                            rq.session.pause(&ls_torrent).await.anyhow_to_eyre()
                        }
                        MediaLifecycle::Resume => if stream.is_none() {
                            match rq
                                .stream_file(&ls_torrent, f as usize, subpath.clone())
                                .await
                                .anyhow_to_eyre()
                            {
                                Ok(v) => {
                                    stream = Some(v);
                                    Ok(())
                                }
                                Err(v) => Err(v).wrap_err("starting stream"),
                            }
                        } else {
                            Ok(())
                        }
                        .and(rq.session.unpause(&ls_torrent).await.anyhow_to_eyre()),
                        MediaLifecycle::Destroy => {
                            should_break = true;
                            if let Some(id) = stream {
                                rq.stop_streaming(id).await.anyhow_to_eyre()
                            } else {
                                Ok(())
                            }
                        }
                    };

                    if send_error.send(res.err().map(Arc::new)).is_err() || should_break {
                        // goobye worlw ;u;
                        break;
                    }
                }
            });
            let info = torrent.metadata.load();
            let (file_name, file_size) = info
                .as_ref()
                .and_then(|v| v.file_infos.get(f as usize))
                .map_or((None, None), |v| {
                    (
                        Some(v.relative_filename.to_string_lossy().into()),
                        Some(v.len),
                    )
                });
            Ok(PlayableMedia {
                playable: Playable::Url(path),
                file_name,
                file_size,
                lifecycle: Some(LiveMediaHandle {
                    send: send_lifecycle,
                    recv_err,
                }),
                meta: SourceMeta::Torrent(meta),
            })
        }))
    }
}
