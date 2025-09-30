use std::num::NonZeroUsize;

use nyaa::{AnimeKind, SearchQuery};
use serde::{Deserialize, Serialize};

use crate::show::Show;

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Config {
    min_seeders: u32,
    max_size: u64,
    nyaa: ::nyaa::NyaaClientConfig,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            min_seeders: Default::default(),
            max_size: size::Size::from_mib(800).bytes() as u64,
            nyaa: Default::default(),
        }
    }
}
impl Show {
    pub(crate) fn nyaa_query_for(
        &self,
        config: &crate::Config,
        episode: u32,
        cat: AnimeKind,
    ) -> nyaa::SearchQuery {
        SearchQuery {
            query: format!("{} - {episode:02}", self.get_preferred_name(config)),
            category: nyaa::MediaCategory::Anime(Some(cat)),
            filter: nyaa::Filter::TrustedOnly,
            max_page_idx: const { NonZeroUsize::new(5) },
            sort: nyaa::Sort {
                by: nyaa::SortBy::Seeders,
                ..Default::default()
            },
            ..Default::default()
        }
    }
}
