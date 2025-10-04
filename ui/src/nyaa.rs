use std::num::NonZeroUsize;

use nyaa::{AnimeKind, SearchQuery};
use serde::{Deserialize, Serialize};

use crate::show::Show;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Config {
    /// ignore torrents with fewer seeders than this
    pub(crate) min_seeders: u32,
    /// ignore torrents larger than this
    pub(crate) max_size: u64,
    /// penalize torrents larger than this when selecting
    pub(crate) preferred_size: u64,
    /// client config
    pub(crate) nyaa: ::nyaa::NyaaClientConfig,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            min_seeders: 5,
            max_size: size::Size::from_mib(800).bytes() as u64,
            preferred_size: size::Size::from_mib(400).bytes() as u64,
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
