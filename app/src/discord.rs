use std::time::Duration;

use discord_rich_presence::{
    DiscordIpc, DiscordIpcClient,
    activity::{Activity, Assets, Timestamps},
};
use tokio::sync::mpsc::UnboundedSender;

use crate::show::EpochInstant;

pub struct DiscordPresence {
    inner: DiscordIpcClient,
}
#[derive(Debug)]
pub enum UpdatePresence {
    Clear,
    Show {
        title: String,
        thumb_url: Option<String>,
        episode_idx: Option<u32>,
    },
    Timestamp {
        timestamp_secs: u32,
        remaining_secs: u32,
    },
}

impl DiscordPresence {
    pub fn spawn() -> UnboundedSender<UpdatePresence> {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

        let mut discorb = Self {
            inner: DiscordIpcClient::new("1440661664847626350"),
        };
        let mut s_title = String::new();
        let mut s_url = None;
        let mut s_episode = String::new();
        let mut s_ts = 0;
        let mut s_remaining = 0;
        let mut paused_count = 0;

        tokio::spawn(async move {
            loop {
                while let Ok(true) = discorb.try_connect() {
                    tokio::time::sleep(Duration::from_secs(30)).await;
                }

                match 'reconnect: loop {
                    let msg = rx.recv().await;

                    let res = match msg {
                        Some(v) => match v {
                            UpdatePresence::Clear => {
                                log::trace!("clearing discord activity");
                                discorb.inner.clear_activity()
                            }
                            UpdatePresence::Show {
                                title,
                                thumb_url,
                                episode_idx,
                            } => {
                                log::trace!("update show to title {title}");
                                s_title = title;
                                s_url = thumb_url;
                                s_episode = s_title.clone();
                                if let Some(ep) = episode_idx {
                                    use std::fmt::Write;
                                    write!(&mut s_episode, " • Episode {}", ep + 1)
                                        .expect("write to string to succeed");
                                }
                                paused_count = 0;
                                discorb.transmit_activity(
                                    &s_title,
                                    s_url.as_deref(),
                                    &s_episode,
                                    s_ts,
                                    s_remaining,
                                )
                            }
                            UpdatePresence::Timestamp {
                                timestamp_secs,
                                remaining_secs,
                            } => {
                                if s_remaining.abs_diff(remaining_secs) < 1 {
                                    paused_count += 1;
                                } else {
                                    paused_count = 0;
                                }

                                log::trace!("update timestamp to {timestamp_secs}");
                                s_remaining = remaining_secs;
                                s_ts = timestamp_secs;
                                if paused_count > 5 {
                                    discorb.inner.clear_activity()
                                } else {
                                    discorb.transmit_activity(
                                        &s_title,
                                        s_url.as_deref(),
                                        &s_episode,
                                        s_ts,
                                        s_remaining,
                                    )
                                }
                            }
                        },
                        None => {
                            let _ = discorb.inner.close();
                            break 'reconnect Ok(false);
                        }
                    };
                    match res {
                        Ok(_) => {}
                        Err(e) => {
                            if Self::should_retry(&e) {
                                break 'reconnect Ok(true);
                            } else {
                                break 'reconnect Err(e);
                            }
                        }
                    }
                } {
                    Ok(v) => {
                        if v {
                            log::info!("discord rpc exiting");
                            break;
                        } else {
                            log::info!("discord rpc retrying");
                        }
                    }
                    Err(e) => {
                        log::error!("discord rpc fatal error: {e:#}. Exiting");
                        break;
                    }
                }
            }
        });
        tx
    }
    pub fn try_connect(&mut self) -> eyre::Result<bool> {
        match self.inner.connect() {
            Ok(_) => Ok(false),
            Err(e) => match e {
                e if Self::should_retry(&e) => Ok(true),
                e => Err(e.into()),
            },
        }
    }
    pub fn should_retry(err: &discord_rich_presence::error::Error) -> bool {
        matches!(
            err,
            discord_rich_presence::error::Error::IPCConnectionFailed
                | discord_rich_presence::error::Error::ReadError(_)
                | discord_rich_presence::error::Error::WriteError(_)
                | discord_rich_presence::error::Error::FlushError(_)
        )
    }
    pub fn transmit_activity(
        &mut self,
        title: &str,
        thumbnail_url: Option<&str>,
        episode: &str,
        ts: u32,
        rem: u32,
    ) -> Result<(), discord_rich_presence::error::Error> {
        let mut assets = Assets::new().large_text(title);
        if let Some(url) = thumbnail_url {
            assets = assets.large_image(url);
        }

        let now_millis = EpochInstant::now().secs() * 1000;

        let past = now_millis - (ts as u64 * 1000);
        let future = now_millis + rem as u64 * 1000;
        let ac = Activity::new()
            .activity_type(discord_rich_presence::activity::ActivityType::Watching)
            .state(episode)
            .timestamps(Timestamps::new().start(past as i64).end(future as i64))
            .status_display_type(discord_rich_presence::activity::StatusDisplayType::State)
            .details(title)
            .assets(assets);
        self.inner.set_activity(ac)
    }
}
