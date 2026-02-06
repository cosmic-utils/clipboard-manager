//! IPC mechanism for external toggle functionality via file-based signaling.
//!
//! When the `--toggle` command is invoked, it writes a timestamp to a signal file
//! in XDG_RUNTIME_DIR. A polling loop detects the file and notifies the app to
//! toggle the popup. The signal file is deleted after detection to prevent
//! re-triggering.

use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;

use crate::message::AppMsg;
use cosmic::iced_futures::Subscription;

/// Get the signal file path for IPC toggle functionality.
/// Returns None if XDG_RUNTIME_DIR is not set.
pub fn get_signal_file_path() -> Option<PathBuf> {
    std::env::var("XDG_RUNTIME_DIR")
        .ok()
        .map(|runtime_dir| PathBuf::from(runtime_dir).join("cosmic-clipboard-manager-toggle"))
}

/// Send a toggle signal by writing a timestamp to the signal file.
pub fn send_toggle_signal() -> std::io::Result<()> {
    let signal_file = get_signal_file_path().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "XDG_RUNTIME_DIR not set - cannot send toggle signal",
        )
    })?;

    let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?
        .as_millis()
        .to_string();

    fs::write(&signal_file, timestamp)?;
    Ok(())
}

/// Poll for the signal file at a fixed interval.
///
/// Uses a simple 250ms polling loop instead of filesystem notifications.
/// One stat() syscall per 250ms is negligible, and this approach cannot
/// busy-loop or crash due to watcher thread failures.
pub fn signal_file_watcher() -> Subscription<AppMsg> {
    use cosmic::iced::futures::SinkExt;
    use cosmic::iced::stream::channel;

    Subscription::run_with_id(
        "signal_file_watcher",
        channel(1, |mut output| async move {
            let signal_file = match get_signal_file_path() {
                Some(path) => path,
                None => {
                    futures::future::pending::<()>().await;
                    unreachable!();
                }
            };

            loop {
                tokio::time::sleep(tokio::time::Duration::from_millis(250)).await;
                if signal_file.exists() {
                    let _ = std::fs::remove_file(&signal_file);
                    output.send(AppMsg::CheckSignalFile).await.ok();
                }
            }
        }),
    )
}
