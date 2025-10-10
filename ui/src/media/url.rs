use std::sync::Arc;

use bincode::{Decode, Encode};
use iced::Task;

use crate::{
    LiveState,
    media::{Media, PlayRequest, PlayableMedia},
};

/// a single, externally hosted URL of an episode
#[derive(Debug, Clone, Encode, Decode)]
pub struct UrlMedia {
    pub episode: u32,
    pub url: Arc<str>,
    pub meta: Arc<UrlMeta>,
}
impl Media for UrlMedia {
    fn has_ep(&self, idx: u32) -> bool {
        self.episode == idx
    }

    fn play(
        &self,
        for_show: &PlayRequest,
        live: &mut LiveState,
    ) -> Option<iced::Task<eyre::Result<PlayableMedia>>> {
        let PlayRequest {
            show,
            episode_idx,
            pos,
        } = for_show;

        if *episode_idx != self.episode {
            return None;
        };

        Some(Task::done(Ok(PlayableMedia {
            playable: crate::media::Playable::Url(self.url.to_string()),
            file_name: Some(self.meta.file_name.to_string()),
            file_size: None,
            lifecycle: None,
            meta: crate::media::SourceMeta::Url(self.meta.clone()),
        })))
    }
}

#[derive(Debug, Encode, Decode)]
pub struct UrlMeta {
    pub source_name: Arc<str>,
    pub file_name: Arc<str>,
    pub resolution: Option<Arc<str>>,
}
