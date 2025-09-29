use anilist_moe::models::Anime;
use serde::{Deserialize, Serialize};

use crate::{
    NameKind,
    show::{Relations, Show, ThumbnailPath},
};

impl Show {
    pub(crate) fn update_with(&mut self, anime: &Anime) {
        self.anilist_id = Some(anime.id);
        if self.thumbnail.is_none()
            && let Some(Some(v)) = &anime.cover_image.as_ref().map(|v| v.medium.as_ref())
        {
            self.thumbnail = Some(ThumbnailPath::Url(v.to_string()))
        }
        let mut h = |kind, s: Option<&String>| {
            if let Some(s) = s {
                self.names.insert((kind, s.to_owned()));
            }
        };
        if let Some(title) = &anime.title {
            h(NameKind::English, title.english.as_ref());
            h(NameKind::Romaji, title.romaji.as_ref());
            h(NameKind::Native, title.native.as_ref());
        }
        if let Some(s) = &anime.synonyms {
            s.iter().for_each(|v| h(NameKind::Synonym, Some(v)));
        }
    }
}

#[derive(Default, Debug, Serialize, Deserialize)]
pub struct Config {
    sync_on_startup: bool,
    api_key: Option<String>,
}
impl From<&Anime> for Relations {
    fn from(value: &Anime) -> Self {
        todo!()
    }
}
