use std::{collections::BTreeSet, path::PathBuf};

use bincode::{Decode, Encode};
use chrono::{DateTime, Local, TimeZone};
use derive_more::{From, Into};

use crate::NameKind;

/// persistent unique identifier for a show in the database
#[derive(Debug, Clone, Copy, PartialEq, Eq, From, Into, Hash)]
pub struct ShowId(u64);

/// Instant in seconds since the UNIX epoch
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Encode, Decode, Hash)]
pub struct EpochInstant(u64);
impl EpochInstant {
    fn now() -> Self {
        let dur = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("unix epoch before current time")
            .as_secs();
        Self(dur)
    }
    fn to_local_dt(self) -> DateTime<Local> {
        Local
            .timestamp_opt(
                self.0
                    .try_into()
                    .expect("not to be past the year ~3.5096545041 * 10^13"),
                0,
            )
            .earliest()
            .expect("time to be mappable")
    }
}

#[derive(Debug, Default, Clone, Encode, Decode)]
pub struct Show {
    index_in_list: u32,
    pub(crate) anilist_id: Option<i32>,
    pub(crate) names: ShowNames,
    pub(crate) thumbnail: Option<ThumbnailPath>,
    pub(crate) num_episodes: Option<u32>,
    pub(crate) watch_history: BTreeSet<WatchEvent>,
}
#[derive(Debug, Clone, Encode, Decode)]
pub(crate) struct WatchEvent {
    episode_idx: u32,
    ty: WatchEventType,
    timestamp: EpochInstant,
}
impl PartialEq for WatchEvent {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other).is_eq()
    }
}
impl Eq for WatchEvent {}
impl PartialOrd for WatchEvent {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for WatchEvent {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.timestamp.cmp(&other.timestamp)
    }
}
#[derive(Debug, Clone, Copy, Encode, Decode)]
enum WatchEventType {
    Opened,
    Completed,
}

#[derive(Debug, Default, Clone, Encode, Decode)]
pub struct ShowNames {
    pub(crate) names: BTreeSet<(NameKind, String)>,
}
#[derive(Debug, Clone, Encode, Decode)]
pub(crate) enum ThumbnailPath {
    File(PathBuf),
    Url(String),
}
