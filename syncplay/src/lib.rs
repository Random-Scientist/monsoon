use std::collections::VecDeque;
use std::io;

use crate::proto::{ClientMessage, HelloFeatures, RoomIdentifier, ServerMessage};
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{TcpStream, ToSocketAddrs};

pub mod proto;

#[derive(Debug, Error)]
pub enum SyncplayError {
    #[error("I/O error")]
    IoError(#[from] io::Error),
}
pub type Result<T> = std::result::Result<T, SyncplayError>;

pub struct SyncplayClient {
    write: BufWriter<OwnedWriteHalf>,
    reader: LineReader,
}
pub struct ConnectInfo {
    passwd_hash: Option<[u8; 16]>,
    user_name: String,
    room_name: String,
}
impl SyncplayClient {
    pub async fn new(info: ConnectInfo, address: impl ToSocketAddrs) -> Result<Self> {
        dbg!("entered new");
        let stream = TcpStream::connect(address).await?;

        dbg!("connected");
        let _ = stream.set_nodelay(true);

        let (read, write) = stream.into_split();

        let mut write = BufWriter::new(write);
        let mut reader = LineReader::new(read);
        let msg = ClientMessage::Hello {
            username: info.user_name,
            password_md5: info.passwd_hash,
            room: RoomIdentifier {
                name: info.room_name,
            },
            version: "1.2.255".into(),
            realversion: "1.7.3".into(),
            features: HelloFeatures {
                shared_playlists: true,
                chat: true,
                feature_list: true,
                readiness: true,
                managed_rooms: false,
            },
        };
        let mut s = serde_json::to_string(&msg).expect("expected a serializable message");
        s.push_str("\r\n");
        write.write_all(s.as_bytes()).await?;
        write.flush().await?;

        let hello = loop {
            let resp = reader.read_line().await?.trim();
            let msg = serde_json::from_str::<ServerMessage>(resp).expect("bad server response");
            match dbg!(&msg) {
                ServerMessage::Hello { .. } => break msg,
                _ => {}
            }
        };
        dbg!(&hello);

        dbg!(s);
        Ok(Self { write, reader })
    }
}

pub struct LineReader {
    inner: BufReader<OwnedReadHalf>,
    buf: String,
}
impl LineReader {
    fn new(stream: OwnedReadHalf) -> Self {
        Self {
            inner: BufReader::new(stream),
            buf: String::new(),
        }
    }
    async fn read_line(&mut self) -> Result<&str> {
        self.buf.clear();
        self.inner.read_line(&mut self.buf).await?;
        Ok(&self.buf)
    }
}
#[cfg(test)]
mod test {
    use tokio::net::lookup_host;

    use crate::SyncplayClient;
    #[tokio::test]
    async fn test() {
        dbg!("entered");
        let addr = lookup_host("syncplay.pl:8999")
            .await
            .unwrap()
            .next()
            .unwrap();
        dbg!(&addr);
        dbg!("looked up");
        let client = SyncplayClient::new(
            crate::ConnectInfo {
                passwd_hash: None,
                user_name: "farsidefarewell".into(),
                room_name: "farsidefarewell".into(),
            },
            "syncplay.pl:8999",
        )
        .await
        .unwrap();
    }
}
