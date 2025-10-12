use std::{
    collections::{BTreeMap, BTreeSet},
    num::{NonZero, NonZeroU32},
    path::PathBuf,
};

use bincode::{Decode, Encode};
use chrono::{DateTime, Local, TimeZone};
use derive_more::{From, Into};

use crate::{Config, NameKind, media::AnyMedia};

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
    pub(crate) watched_episodes: Vec<bool>,
    pub(crate) num_episodes: Option<NonZeroU32>,
    pub(crate) media_cache: Vec<AnyMedia>,
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
    pub(crate) fn episode_to_watch(&self) -> Option<(u32, Option<u32>)> {
        let mut ep = self.num_episodes.map(NonZero::get).unwrap_or(1) - 1;
        if self.watched_episodes.len() != (ep + 1) as usize {
            log::warn!("watched episodes mismatch with num_episodes");
            return None;
        }
        while ep != 0 {
            if self.watched_episodes[ep as usize] {
                // watched last episode
                return None;
            }
            if self.watched_episodes[(ep - 1) as usize] {
                break;
            }
            ep -= 1;
        }
        let pause = self.watch_history.iter().rev().find_map(|v| {
            if let WatchEvent {
                episode,
                ty: WatchEventType::Closed(ts),
            } = &v.1
                && *episode == ep
            {
                *ts
            } else {
                None
            }
        });
        Some((ep, pause))
    }
    pub(crate) fn season_number_guess(&self) -> Option<u32> {
        self.names
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
                        .next_back()
                        .and_then(|v| parse_season(&v.to_lowercase()));
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
                    } else {
                        next = &lower == "season";
                    }
                }
                None
            })
            .or(self.relations.prequel.is_none().then_some(1))
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
    pub episode: u32,
    pub ty: WatchEventType,
}

#[derive(Debug, Clone, Copy, Encode, Decode)]
pub enum WatchEventType {
    Opened,
    /// optional timestamp within the video in seconds that locates the pause
    Closed(Option<u32>),
}

#[derive(Debug, Clone, Encode, Decode)]
pub(crate) enum ThumbnailPath {
    File(PathBuf),
    Url(String),
}
