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
            max_size: size::Size::from_mib(1500).bytes() as u64,
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
    ) -> impl Iterator<Item = nyaa::SearchQuery> + 'static {
        let season = if self.relations.prequel.is_none() {
            "01"
        } else {
            log::warn!("season number for shows with prequels not yet supported");
            ""
        };
        self.names.iter().map(move |name| SearchQuery {
            query: format!("{} S{season}E{episode:02}", &name.1),
            category: nyaa::MediaCategory::Anime(Some(cat)),
            filter: nyaa::Filter::NoFilter,
            max_page_idx: const { NonZeroUsize::new(5) },
            sort: nyaa::Sort {
                by: nyaa::SortBy::Seeders,
                ..Default::default()
            },
            ..Default::default()
        }).collect::<Vec<_>>().into_iter()
    }
}
