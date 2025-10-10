#[cfg(unix)]
use std::path::Path;
use std::{env::temp_dir, fs::File, io::Write, sync::Arc, time::Duration};

use eyre::{Context, OptionExt};
use mpv_ipc::{MpvIpc, MpvSpawnOptions};
use rqstream::StreamId;
use tokio::sync::{Mutex, watch::Receiver};

use crate::{
    FAILED_LOAD_IMAGE,
    media::{PlayRequest, PlayableMedia},
    show::{MediaSource, ShowId},
};

#[derive(Debug, Clone)]
pub struct Play {
    pub show: ShowId,
    pub episode_idx: u32,
    pub pos: u32,
    pub media: Option<PlayableMedia>,
    pub stream: Option<StreamId>,
}

#[derive(Debug, Clone)]
pub struct PlayMedia {
    media: PlayableMedia,
    request: PlayRequest,
}

#[derive(Debug)]
pub struct PlayerSession {
    // TODO mutex dyn PlayerInstance
    pub instance: Arc<Mutex<PlayerSessionMpv>>,
    pub playing: Option<Play>,
}

#[derive(Debug)]
pub struct PlayerSessionMpv {
    mpv: MpvIpc,
    recv_core_idle: Receiver<serde_json::Value>,
    recv_playing: Receiver<bool>,
}

impl PlayerSessionMpv {
    pub(crate) async fn quit(&mut self) {
        self.mpv.quit().await;
    }
    pub(crate) async fn dead(&self) -> bool {
        !self.mpv.running().await
    }
    pub(crate) async fn ensure_started(&mut self) -> eyre::Result<()> {
        if self.dead().await {
            let mut n = Self::new().await?;
            std::mem::swap(self, &mut n);
        }
        Ok(())
    }
    pub(crate) async fn new() -> eyre::Result<Self> {
        #[cfg(unix)]
        fn mpv_path() -> Option<(bool, &'static Path)> {
            let p = Path::new("mpv");
            if p.exists() {
                return Some((false, p));
            }
            let p = Path::new("/Applications/IINA.app/Contents/MacOS/iina-cli");
            if p.exists() {
                return Some((true, p));
            }
            None
        }
        #[cfg(not(unix))]
        fn mpv_path() -> PathBuf {
            unimplemented!()
        }

        let (is_iina, path) = mpv_path().ok_or_eyre("failed to locate mpv")?;

        let mut mpv = MpvIpc::spawn(&MpvSpawnOptions {
            mpv_path: Some(path.into()),
            mpv_additional_args: if is_iina {
                // TODO figure out how to avoid this fuckass contraption (Syncplay does it so we might be stuck with it)
                let dummy = temp_dir().join("dummy_image.jpg");
                if !dummy.exists() {
                    let mut f = File::create(&dummy)?;
                    // TODO bundle a better splash image
                    f.write_all(FAILED_LOAD_IMAGE)?;
                }

                vec![
                    "--no-stdin".into(),
                    dummy.to_string_lossy().to_string(),
                    "--".into(),
                ]
            } else {
                Vec::new()
            },
            ipc_path: None,
            config_dir: None,
            inherit_stdout: false,
        })
        .await?;
        tokio::time::sleep(Duration::from_millis(50)).await;
        let recv_path = mpv.observe_prop("path", serde_json::Value::Null).await;
        let recv_playing = mpv.observe_prop("core-idle", true).await;
        Ok(Self {
            mpv,
            recv_core_idle: recv_path,
            recv_playing,
        })
    }

    pub(crate) async fn play(&mut self, url: impl Into<String>) -> eyre::Result<()> {
        self.ensure_started().await?;
        let url = url.into();
        self.mpv
            .send_command(["loadfile".to_string(), url.clone()].into())
            .await?;
        // wait for file to be set
        self.recv_core_idle
            .wait_for(move |v| matches!(v, serde_json::Value::String(path) if path == &url))
            .await?;
        // wait for the core to start playing
        self.recv_playing.wait_for(|v| !v).await?;

        Ok(())
    }
    pub(crate) async fn seek(&mut self, ts: u32) -> eyre::Result<()> {
        self.mpv
            .send_command(
                [
                    "seek".into(),
                    format!("{}.0", ts),
                    "absolute+keyframes".into(),
                ]
                .into(),
            )
            .await?;
        Ok(())
    }
    pub(crate) async fn pos(&mut self) -> eyre::Result<u32> {
        let val = self
            .mpv
            .send_command(["expand-text", "${=time-pos}"].into())
            .await?;
        let f: f64 = val
            .as_str()
            .ok_or_eyre("json response not a string")?
            .parse()
            .wrap_err("failed to parse mpv player time-pos")?;
        Ok(f as u32)
    }
}
