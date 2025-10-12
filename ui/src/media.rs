use std::{path::PathBuf, sync::Arc};

use bincode::{Decode, Encode};

use crate::{
    LiveState,
    media::{
        torrent::{TorrentMedia, TorrentMeta},
        url::{UrlMedia, UrlMeta},
    },
    show::ShowId,
};

pub mod torrent;
pub mod url;

#[derive(Clone, Debug)]
pub enum Playable {
    Url(String),
    File(PathBuf),
}
#[derive(Clone, Debug)]
pub struct PlayableMedia {
    pub playable: Playable,
    pub file_name: Option<String>,
    pub file_size: Option<u64>,
    pub lifecycle: Option<LiveMediaHandle>,
    pub meta: SourceMeta,
}

#[derive(Debug, Clone, Copy)]
pub enum MediaLifecycle {
    Pause,
    Resume,
    Destroy,
}

#[derive(Clone, Debug)]
pub struct LiveMediaHandle {
    send: tokio::sync::watch::Sender<MediaLifecycle>,
    recv_err: tokio::sync::watch::Receiver<Option<Arc<eyre::Report>>>,
}

impl LiveMediaHandle {
    pub(crate) async fn update(
        &mut self,
        msg: MediaLifecycle,
    ) -> eyre::Result<Option<Arc<eyre::Report>>> {
        self.send.send(msg)?;
        self.recv_err.changed().await?;
        Ok(self.recv_err.borrow_and_update().clone())
    }
}

#[derive(Debug, Clone)]
pub enum SourceMeta {
    Torrent(Arc<TorrentMeta>),
    Url(Arc<UrlMeta>),
}

#[derive(Debug, Clone, Copy)]
pub struct PlayRequest {
    /// ID of the show to play
    pub show: ShowId,
    /// index of the episode to play
    pub episode_idx: u32,
    /// absolute position (from the start of the file) to seek to after opening
    pub pos: u32,
}

#[derive(Debug, Clone)]
pub struct PlayingMedia {
    /// ID of the show to play
    pub show: ShowId,
    /// index of the episode being played
    pub episode_idx: u32,
    /// the media to be played
    pub media: PlayableMedia,
}
#[enum_delegate::register]
pub trait Media {
    /// string that uniquely identifies this [`Media`]
    fn identifier(&self) -> Arc<str>;
    fn has_ep(&self, idx: u32) -> bool;
    fn play(
        &self,
        for_show: &PlayRequest,
        live: &mut LiveState,
    ) -> Option<Box<dyn Future<Output = eyre::Result<PlayableMedia>> + Send + 'static>>;
}

#[derive(Debug, Clone, Encode, Decode)]
#[enum_delegate::implement(Media)]
pub enum AnyMedia {
    Torrent(TorrentMedia),
    Url(UrlMedia),
}
