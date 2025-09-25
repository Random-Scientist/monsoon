use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};

use bincode::{Decode, Encode};
use chrono::{DateTime, Local, TimeZone};
use derive_more::{From, Into};

use crate::NameKind;

/// persistent unique identifier for a show in the database
#[derive(Debug, Clone, Copy, PartialEq, Eq, From, Into, Hash)]
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
    pub(crate) names: ShowNames,
    pub(crate) thumbnail: Option<ThumbnailPath>,
    pub(crate) watch_history: BTreeMap<EpochInstant, WatchEvent>,
}
#[derive(Debug, Clone, Encode, Decode)]
pub struct WatchEvent {
    episode_idx: u32,
    ty: WatchEventType,
}
#[derive(Debug, Clone, Copy, Encode, Decode)]
pub enum WatchEventType {
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
