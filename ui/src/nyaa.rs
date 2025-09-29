use std::num::NonZeroUsize;

use nyaa::{AnimeKind, SearchQuery};

use crate::show::Show;

pub(crate) struct Config {
    min_seeders: u32,
    max_size: u64,
}
impl Show {
    fn nyaa_query_for(
        &self,
        config: &crate::Config,
        season_episode: (u32, u32),
        cat: AnimeKind,
    ) -> nyaa::SearchQuery {
        SearchQuery {
            query: self.get_preferred_name(config).into(),
            category: nyaa::MediaCategory::Anime(Some(cat)),
            filter: nyaa::Filter::TrustedOnly,
            max_page_idx: const { NonZeroUsize::new(5) },
            ..Default::default()
        }
    }
}
