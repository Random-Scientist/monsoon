use std::num::NonZeroUsize;

use nyaa::{AnimeKind, SearchQuery};
use serde::{Deserialize, Serialize};

use crate::{NameKind, show::Show};

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
        if self.relations.prequel.is_some() {
            log::warn!(
                "season number for anilist shows with prequels not yet supported through anilist"
            );
        }
        let season = self
            .names
            .iter()
            .find_map(|v| {
                fn parse_season(s: &str) -> Option<u32> {
                    if let Ok(v) = s.parse() {
                        // is this sane??
                        if v >= 15 { None } else { Some(v) }
                    } else {
                        Some(match s {
                            "first" | "one" => 1,
                            "second" | "two" => 2,
                            "third" | "three" => 3,
                            "fourth" | "four" => 4,
                            "fifth" | "five" => 5,
                            "sixth" | "six" => 6,
                            "seventh" | "seven" => 7,
                            "eighth" | "eight" => 8,
                            "ninth" | "nine" => 9,
                            "tenth" | "ten" => 10,
                            _ => return None,
                        })
                    }
                }
                if !matches!(v.0, NameKind::English | NameKind::Romaji) {
                    return None;
                }
                let mut next = false;
                let s =
                    v.1.split(' ')
                        .last()
                        .map(|v| parse_season(&v.to_lowercase()))
                        .flatten();
                if s.is_some() {
                    return s;
                }
                for word in v.1.split(' ') {
                    let lower = word.to_lowercase();
                    if next {
                        let s = parse_season(&lower);
                        if s.is_some() {
                            return s;
                        }
                    } else if &lower == "season" {
                        next = true;
                    } else {
                        next = false;
                    }
                }
                None
            })
            .unwrap_or(1);

        self.names
            .iter()
            .map(move |name| SearchQuery {
                query: format!("{} S{season:02}E{episode:02}", &name.1),
                category: nyaa::MediaCategory::Anime(Some(cat)),
                filter: nyaa::Filter::NoFilter,
                max_page_idx: const { NonZeroUsize::new(5) },
                sort: nyaa::Sort {
                    by: nyaa::SortBy::Seeders,
                    ..Default::default()
                },
                ..Default::default()
            })
            .collect::<Vec<_>>()
            .into_iter()
    }
}
