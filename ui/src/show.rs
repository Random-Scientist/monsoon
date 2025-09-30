use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    num::NonZeroU32,
    path::PathBuf,
};

use bincode::{Decode, Encode};
use chrono::{DateTime, Local, TimeZone};
use derive_more::{From, Into};

use crate::{Config, NameKind};

/// persistent unique identifier for a show in the database
#[derive(Debug, Clone, Copy, PartialEq, Eq, From, Into, Hash, Encode, Decode)]
pub struct ShowId(u64);

/// Instant in seconds + subsec nanos since the UNIX epoch
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Encode, Decode, Hash)]
pub struct EpochInstant(u64, u32);

impl EpochInstant {
    pub(crate) fn now() -> Self {
        let dur = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("unix epoch before current time");
        let secs = dur.as_secs();
        let nanos = dur.subsec_nanos();
        Self(secs, nanos)
    }

    pub(crate) fn to_local_dt(self) -> DateTime<Local> {
        Local
            .timestamp_opt(
                self.0
                    .try_into()
                    .expect("not to be past the year ~3.5096545041 * 10^13"),
                self.1,
            )
            .earliest()
            .expect("time to be mappable")
    }
}

#[derive(Debug, Default, Clone, Encode, Decode)]
pub struct Show {
    pub(crate) anilist_id: Option<i32>,
    pub(crate) names: BTreeSet<(NameKind, String)>,
    pub(crate) thumbnail: Option<ThumbnailPath>,
    pub(crate) watch_history: BTreeMap<EpochInstant, WatchEvent>,
    pub(crate) num_episodes: Option<NonZeroU32>,
    pub(crate) episode_sources: HashMap<u32, MediaSource>,
    pub(crate) relations: Relations,
}
#[derive(Debug, Clone, Encode, Decode)]
pub enum MediaSource {
    Magnet(String),
    DirectUrl(String),
    File(PathBuf),
}
impl Show {
    pub(crate) fn get_preferred_name(&self, config: &Config) -> &str {
        self.names
            .iter()
            .find(|v| v.0 == config.preferred_name_kind)
            .map(|v| &*v.1)
            .unwrap_or("")
    }
    pub(crate) fn episode_to_watch_idx(&self) -> Option<(u32, Option<u32>)> {
        let mut episode = 1;
        let mut pause = None;
        for (_, event) in self.watch_history.iter() {
            pause = None;
            match event.ty {
                WatchEventType::Opened => {}
                WatchEventType::Paused(p) => {
                    pause = p;
                }
                WatchEventType::Completed => {
                    episode = episode.max(event.episode_idx + 1);
                }
            }
        }
        if self.num_episodes.is_some_and(|v| (episode + 1) > v.get()) {
            None
        } else {
            Some((episode, pause))
        }
    }
}

#[derive(Debug, Clone, Encode, Decode)]
pub enum RelationId {
    Local(ShowId),
    Anilist(i32),
}

#[derive(Debug, Default, Clone, Encode, Decode)]
pub struct Relations {
    pub prequel: Option<RelationId>,
    pub sequel: Option<RelationId>,
}

#[derive(Debug, Clone, Encode, Decode)]
pub struct WatchEvent {
    episode_idx: u32,
    ty: WatchEventType,
}

#[derive(Debug, Clone, Copy, Encode, Decode)]
pub enum WatchEventType {
    Opened,
    /// optional timestamp within the video in seconds that locates the pause
    Paused(Option<u32>),
    Completed,
}

#[derive(Debug, Clone, Encode, Decode)]
pub(crate) enum ThumbnailPath {
    File(PathBuf),
    Url(String),
}
