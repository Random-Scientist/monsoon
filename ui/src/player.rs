#[cfg(unix)]
use std::path::Path;
use std::{
    env::temp_dir,
    fs::File,
    io::Write,
    time::Duration,
};

use eyre::OptionExt;
use mpv_ipc::{MpvIpc, MpvSpawnOptions};
use tokio::sync::watch::Receiver;

use crate::FAILED_LOAD_IMAGE;

pub struct PlayerSessionMpv {
    mpv: MpvIpc,
    recv_path: Receiver<serde_json::Value>,
}

impl PlayerSessionMpv {
    async fn new() -> eyre::Result<Self> {
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
            inherit_stdout: true,
        })
        .await?;
        tokio::time::sleep(Duration::from_millis(50)).await;
        let recv_path = mpv.observe_prop("path", serde_json::Value::Null).await;
        Ok(Self { mpv, recv_path })
    }

    async fn play(&mut self, url: impl Into<String>) -> eyre::Result<()> {
        let url = url.into();

        self.mpv
            .send_command(["loadfile".to_string(), url.clone()].into())
            .await?;
        // wait for file to start playing
        self.recv_path
            .wait_for(move |v| matches!(v, serde_json::Value::String(path) if path == &url))
            .await?;
        // plus a little extra to be nice :3
        tokio::time::sleep(Duration::from_millis(50)).await;
        Ok(())
    }
    async fn seek(&mut self, abs_ts: u32) -> eyre::Result<()> {
        self.mpv
            .send_command(
                [
                    "seek".into(),
                    format!("{}.0", abs_ts),
                    "absolute+keyframes".into(),
                ]
                .into(),
            )
            .await?;
        Ok(())
    }
}
