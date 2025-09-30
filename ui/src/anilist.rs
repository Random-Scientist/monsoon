use anilist_moe::models::Anime;
use serde::{Deserialize, Serialize};

use crate::{
    NameKind,
    show::{RelationId, Show, ThumbnailPath},
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
        if let Some(eps) = &anime.episodes
            && let Ok(eps) = eps.abs_diff(0).try_into()
        {
            self.num_episodes = Some(eps)
        }
        if let Some(r) = anime.relations.as_ref() {
            // user specified sequel or prequel takes precedence
            let (mut skip_prequel, mut skip_sequel) = (
                matches!(self.relations.prequel, Some(RelationId::Local(_))),
                matches!(self.relations.sequel, Some(RelationId::Local(_))),
            );
            for media in r.edges.iter() {
                match media.relation_type {
                    anilist_moe::models::anime::MediaRelation::Prequel if !skip_prequel => {
                        match self.relations.prequel {
                            Some(RelationId::Anilist(v)) => {
                                if v != media.node.id {
                                    log::warn!(
                                        "found multiple prequel candidates for Anilist anime ({}, {}), while scanning relations of {}",
                                        media.id,
                                        v,
                                        self.names.first().map(|n| &*n.1).unwrap_or(""),
                                    );
                                    self.relations.prequel = None;
                                }
                                skip_prequel = true;
                            }
                            None => {
                                self.relations.prequel =
                                    Some(crate::show::RelationId::Anilist(media.node.id))
                            }
                            _ => unreachable!(),
                        }
                    }
                    anilist_moe::models::anime::MediaRelation::Sequel if !skip_sequel => {
                        match self.relations.prequel {
                            Some(RelationId::Anilist(v)) => {
                                if v != media.node.id {
                                    log::warn!(
                                        "found multiple sequel candidates for Anilist anime ({}, {}), while scanning relations of {}. Giving up",
                                        media.id,
                                        v,
                                        self.names.first().map(|n| &*n.1).unwrap_or(""),
                                    );
                                    self.relations.sequel = None;
                                }
                                skip_sequel = true;
                            }
                            None => {
                                self.relations.sequel =
                                    Some(crate::show::RelationId::Anilist(media.node.id))
                            }
                            _ => unreachable!(),
                        }
                    }
                    _ => {}
                }
                if skip_prequel && skip_sequel {
                    // nothing left to find
                    break;
                }
            }
        }
    }
}

#[derive(Default, Debug, Serialize, Deserialize)]
pub struct Config {
    sync_on_startup: bool,
    api_key: Option<String>,
}
