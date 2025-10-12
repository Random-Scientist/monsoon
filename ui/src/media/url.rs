use std::sync::Arc;

use bincode::{Decode, Encode};

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
    ) -> Option<Box<dyn Future<Output = eyre::Result<PlayableMedia>> + Send + 'static>> {
        let PlayRequest { episode_idx, .. } = *for_show;

        if episode_idx != self.episode {
            return None;
        };
        let playable = crate::media::Playable::Url(self.url.to_string());
        let file_name = Some(self.meta.file_name.to_string());
        let meta = crate::media::SourceMeta::Url(self.meta.clone());

        Some(Box::new(async move {
            Ok(PlayableMedia {
                playable,
                file_name,
                file_size: None,
                lifecycle: None,
                meta,
            })
        }))
    }
    fn identifier(&self) -> Arc<str> {
        self.url.clone()
    }
}

#[derive(Debug, Encode, Decode)]
pub struct UrlMeta {
    pub source_name: Arc<str>,
    pub file_name: Arc<str>,
    pub resolution: Option<Arc<str>>,
}
