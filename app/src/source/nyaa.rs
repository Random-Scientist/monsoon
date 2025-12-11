use std::{
    collections::{BTreeMap, HashMap},
    num::{NonZero, NonZeroUsize},
    sync::Arc,
};

use anitomy::ElementObject;
use iced_runtime::futures::futures::future::join_all;
use log::trace;
use nyaa::Item;
use rqstream::ResultExt;
use serde::{Deserialize, Serialize};

use crate::{
    media::{
        AnyMedia,
        torrent::{TorrentMedia, TorrentMeta, resolve_torrent_url},
    },
    source::{QueryItem, Source, SourceKind},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
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
            max_size: size::Size::from_mib(900).bytes() as u64,
            preferred_size: size::Size::from_mib(400).bytes() as u64,
            nyaa: Default::default(),
        }
    }
}

pub struct Nyaa;
impl Source for Nyaa {
    fn query(
        &self,
        live: &mut crate::LiveState,
        config: &crate::Config,
        show: &crate::show::Show,
        filter_episode: Option<u32>,
    ) -> impl Future<Output = eyre::Result<Vec<QueryItem>>> + Send + 'static {
        pub struct QueryWithMeta {
            _has_episode: bool,
            has_season: bool,
            used_name: String,
            query: ::nyaa::SearchQuery,
        }
        fn meta(name: String, (ep, season, query): (bool, bool, String)) -> QueryWithMeta {
            QueryWithMeta {
                _has_episode: ep,
                has_season: season,
                used_name: name,
                query: ::nyaa::SearchQuery {
                    query,
                    category: nyaa::MediaCategory::Anime(Some(nyaa::AnimeKind::SubEnglish)),
                    filter: nyaa::Filter::NoFilter,
                    max_page_idx: NonZeroUsize::new(5),
                    sort: Default::default(),
                    user: None,
                },
            }
        }

        let make_query = |name: &String| {
            meta(
                name.clone(),
                match (show.season_number_guess(), filter_episode.map(|v| v + 1)) {
                    (None, None) => {
                        log::warn!(
                            "no season number guess or episode for nyaa query. multi-season batches are currently unsupported, things may not work right"
                        );
                        (false, false, name.to_string())
                    }
                    // TODO make query retrying/fallbacks more robust. this is a special case for first seasons to increase the batch search envelope.
                    (Some(1), None) => (false, false, name.to_string()),
                    (None, Some(episode)) => {
                        (false, true, format!("{name} - {episode:02} E{episode:02}"))
                    }
                    (Some(season), None) => (true, false, format!("{name} S{season:02}")),
                    (Some(season), Some(episode)) => {
                        (true, true, format!("{name} S{season:02}E{episode:02}"))
                    }
                },
            )
        };

        let queries: Vec<_> = show
            .names
            .iter()
            .filter(|&(_, name)| {
                // matches!(
                //     kind,
                //     crate::NameKind::English | crate::NameKind::Romaji | crate::NameKind::Synonym
                // )
                name.is_ascii()
            })
            .map(|(_, name)| make_query(name))
            .collect();

        let mut conf = config.nyaa.clone();
        let is_batch = filter_episode.is_none();
        if is_batch {
            let eps = show.num_episodes.map(NonZero::get).unwrap_or(1) as u64;
            // adjust for batch searches
            conf.max_size *= eps;
            conf.preferred_size *= eps;
        }
        if show.num_episodes.is_some_and(|v| v.get() == 1) {
            // raise size limit for movies/ovas etc
            conf.max_size *= 10;
            conf.preferred_size *= 10;
        }

        let nyaa = live.nyaa.clone();
        let rq = live.get_rqstream();

        async move {
            let rq = rq.await?;
            let mut all_items = BTreeMap::new();
            // basic heuristic for scoring responses based on a combination of desired size and seeder/leecher counts
            let score_item = |it: &Item| {
                let s = *it.size.as_ref().unwrap() as i64;
                if s == 0 {
                    return i64::MAX;
                }
                // negative if below the preferred size, positive if past it
                // normalize to percentage above/below preferred size
                ((s - conf.preferred_size as i64) / (s / 10)) - it.seeders as i64 * 15
                    + it.leechers as i64 / 10
            };
            let mk_media =
                |it: Item| -> Box<dyn Future<Output = eyre::Result<AnyMedia>> + Send + 'static> {
                    let rq = rq.clone();
                    Box::new(async move {
                        println!("attempting media");
                        let torrent_parsed = ElementObject::from_iter(anitomy::parse(&it.title));

                        let info = rq
                            .get_info(resolve_torrent_url(&it.magnet_link).await?)
                            .await
                            .anyhow_to_eyre()?;
                        let mut files_for_episode_idx = HashMap::new();
                        for (idx, details) in
                            info.info.iter_file_details().anyhow_to_eyre()?.enumerate()
                        {
                            // TODO properly support sourcing from multi season batches
                            let Some(Ok(v)) = details.filename.iter_components().last() else {
                                continue;
                            };
                            let el = ElementObject::from_iter(anitomy::parse(v));
                            if let Some(ext) = &el.file_extension
                                && matches!(&**ext, "mp4" | "mkv" | "webm" | "mov")
                                && let Some(Ok(ep)) = torrent_parsed
                                    .episode
                                    .as_ref()
                                    .or(el.episode.as_ref())
                                    .map(|v| v.parse::<u32>())
                            {
                                if ep == 0 {
                                    log::warn!("found file candidate {v} with episode 0. Skipping");
                                    continue;
                                }
                                log::info!(
                                    "torrent file candidate {v} (file id {idx} episode {ep:02})"
                                );
                                files_for_episode_idx.insert(ep - 1, idx as u32);
                            }
                        }
                        println!("made media");
                        Ok(TorrentMedia {
                            files_for_episode_idx,
                            magnet_or_torrent_file_url: it.magnet_link.into(),
                            meta: Arc::new(TorrentMeta {
                                title: it.title,
                                magnet_source: Some(crate::media::torrent::MagnetSource::Nyaa(
                                    it.nyaa_id,
                                )),
                                seeders: it.seeders.into(),
                                leechers: it.leechers.into(),
                            }),
                        }
                        .into())
                    })
                };
            for resp in
                join_all(queries.iter().map(|query| async {
                    let results = nyaa.search(dbg!(&query.query)).await?;

                    Ok::<_, eyre::Report>(results.results.into_iter().filter_map(|v| {
                        let parsed = ElementObject::from_iter(anitomy::parse(&v.title));
                        let season = |s: &str| s.contains("season") || s.contains("Season");
                        // FIXME this should be shelled out to OpenAI /s
                        let name = &*v.title;
                        if v.seeders < conf.min_seeders {
                            trace!("rejected source {name}: not enough seeders");
                            return None;
                        }

                        if v.size
                            .as_ref()
                            .ok()
                            .is_none_or(|&size| size > conf.max_size)
                        {
                            trace!("rejected source {name}: too large (max_size: {})", conf.max_size);
                            return None;
                        }

                        if !v.title.contains(&query.used_name) {
                            trace!("rejected source {name}: did not contain name for query (name: {})", &query.used_name);
                            return None;
                        }

                        if !query.has_season && !season(&query.used_name) && season(&v.title) {
                            trace!("rejected source {name}: season in non-season batch search");
                            return None;
                        }

                        if is_batch && parsed.episode.is_some() {
                            trace!("rejected source {name}: single episode in batch mode (anitomy: {parsed:#?})");
                            return None;
                        }
                        Some((
                            score_item(&v),
                            QueryItem {
                                source: SourceKind::Nyaa,
                                name: v.title.clone(),
                                file_size: v.size.as_ref().ok().copied(),
                                media: mk_media(v).into(),
                            },
                        ))
                    }))
                })
            )
            .await
            .into_iter()
            {
                all_items.extend(resp?);
            }
            println!("completed query");
            Ok(all_items.into_values().collect())
        }
    }
}
