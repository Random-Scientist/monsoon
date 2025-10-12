use std::{
    collections::{BTreeMap, HashMap},
    num::NonZeroUsize,
    sync::Arc,
};

use anitomy::ElementObject;
use iced::futures::future::join_all;
use nyaa::Item;
use rqstream::ResultExt;
use serde::{Deserialize, Serialize};

use crate::{
    media::{
        AnyMedia,
        torrent::{TorrentMedia, TorrentMeta},
    },
    source::{QueryItem, Source, SourceKind},
};

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

pub struct Nyaa;
impl Source for Nyaa {
    fn query(
        &self,
        live: &mut crate::LiveState,
        config: &crate::Config,
        show: &crate::show::Show,
        filter_episode: Option<u32>,
    ) -> impl Future<Output = eyre::Result<Vec<QueryItem>>> + Send + 'static {
        let make_query = |name| match (show.season_number_guess(), filter_episode.map(|v| v + 1)) {
            (None, None) => {
                log::warn!(
                    "no season number guess or episode for nyaa query. multi-season batches are currently unsupported, things may not work right"
                );
                format!("{name} batch")
            }
            (None, Some(episode)) => format!("{name} - {episode:02} E{episode:02}"),
            (Some(season), None) => format!("{name} S{season:02} batch"),
            (Some(season), Some(episode)) => format!("{name} S{season:02}E{episode:02}"),
        };

        let queries: Vec<_> = show
            .names
            .iter()
            .map(|(_, name)| ::nyaa::SearchQuery {
                query: make_query(name),
                category: nyaa::MediaCategory::Anime(Some(nyaa::AnimeKind::SubEnglish)),
                filter: nyaa::Filter::NoFilter,
                max_page_idx: NonZeroUsize::new(5),
                sort: Default::default(),
                user: None,
            })
            .collect();

        let conf = config.nyaa.clone();
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
                ((s - conf.preferred_size as i64) / (s / 15)) - it.seeders as i64 * 15
                    + it.leechers as i64 / 10
            };
            let mk_media =
                |it: Item| -> Box<dyn Future<Output = eyre::Result<AnyMedia>> + Send + 'static> {
                    let rq = rq.clone();
                    Box::new(async move {
                        let torrent_parsed = ElementObject::from_iter(anitomy::parse(&it.title));
                        let info = rq.get_info(&it.magnet_link).await.anyhow_to_eyre()?;
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
                        Ok(TorrentMedia {
                            files_for_episode_idx,
                            magnet: it.magnet_link.into(),
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
            for resp in join_all(queries.iter().map(|v| nyaa.search(v))).await {
                all_items.extend(resp?.results.into_iter().filter_map(|v| {
                    (v.seeders >= conf.min_seeders
                        && v.size.as_ref().is_ok_and(|&v| v <= conf.max_size))
                    .then(|| {
                        let score = score_item(&v);
                        let name = v.title.clone();
                        let file_size = v.size.as_ref().ok().copied();
                        (
                            score,
                            QueryItem {
                                source: SourceKind::Nyaa,
                                name,
                                file_size,
                                media: mk_media(v).into(),
                            },
                        )
                    })
                }));
            }
            Ok(all_items.into_values().collect())
        }
    }
}
