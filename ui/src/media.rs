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

#[derive(Debug, Clone)]
pub struct PlayRequest {
    /// ID of the show to play
    show: ShowId,
    /// index of the episode to play
    episode_idx: u32,
    /// absolute position (from the start of the file) to seek to after opening
    pos: u32,
}

#[enum_delegate::register]
pub trait Media {
    fn has_ep(&self, idx: u32) -> bool;
    fn play(
        &self,
        for_show: &PlayRequest,
        live: &mut LiveState,
    ) -> Option<iced::Task<eyre::Result<PlayableMedia>>>;
}

#[derive(Debug, Clone, Encode, Decode, strum::EnumDiscriminants)]
#[strum_discriminants(name(MediaSource))]
#[enum_delegate::implement(Media)]
pub enum AnyMedia {
    Torrent(TorrentMedia),
    Url(UrlMedia),
}
