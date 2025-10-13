use std::fs::{self, OpenOptions};
use std::path::PathBuf;
use std::time::SystemTime;

use crate::message::AppMsg;
use cosmic::iced::stream::channel;
use futures::{SinkExt, Stream};
use notify::{RecursiveMode, Watcher};

fn get_signal_file_path() -> anyhow::Result<PathBuf> {
    let path = std::env::var("XDG_RUNTIME_DIR")
        .map(|runtime_dir| PathBuf::from(runtime_dir).join("cosmic-clipboard-manager-toggle"))?;
    Ok(path)
}

pub fn ensure_file_exist() -> anyhow::Result<()> {
    let path = get_signal_file_path()?;
    OpenOptions::new().write(true).create_new(true).open(path)?;
    Ok(())
}

pub fn read_toggle_signal() -> anyhow::Result<u128> {
    let path = get_signal_file_path()?;
    let content = std::fs::read_to_string(&path)?;
    let timestamp = content.parse()?;
    Ok(timestamp)
}

pub fn send_toggle_signal() -> anyhow::Result<()> {
    let signal_file = get_signal_file_path()?;

    let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)?
        .as_millis()
        .to_string();

    fs::write(&signal_file, timestamp)?;
    Ok(())
}

pub fn sub() -> impl Stream<Item = AppMsg> {
    channel(2, move |mut output| async move {
        let path = match get_signal_file_path() {
            Ok(path) => path,
            Err(e) => {
                error!("get file {e}");
                return;
            }
        };

        let (tx, mut rx) = tokio::sync::mpsc::channel(1);

        let mut watcher =
            match notify::recommended_watcher(move |res: Result<notify::Event, notify::Error>| {
                if res.is_ok() {
                    tx.blocking_send(()).unwrap();
                }
            }) {
                Ok(watcher) => watcher,
                Err(e) => {
                    error!("build watcher {e}");
                    return;
                }
            };

        if let Err(e) = watcher.watch(&path, RecursiveMode::NonRecursive) {
            error!("watch {e}");
            return;
        }

        while let Some(()) = rx.recv().await {
            output.send(AppMsg::CheckSignalFile).await.unwrap();
        }
    })
}
