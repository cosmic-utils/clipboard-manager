//! IPC mechanism for external toggle functionality via file-based signaling.
//!
//! This module provides a simple IPC mechanism using a signal file in XDG_RUNTIME_DIR.
//! When the `--toggle` command is invoked, it writes a timestamp to the signal file.
//! The timestamp ensures the file content changes on each toggle, which triggers
//! the file watcher to notify the app to toggle the popup.

use std::path::PathBuf;
use std::fs;
use std::time::SystemTime;

use cosmic::iced_futures::Subscription;
use crate::message::AppMsg;

/// Get the signal file path for IPC toggle functionality.
/// Returns None if XDG_RUNTIME_DIR is not set.
pub fn get_signal_file_path() -> Option<PathBuf> {
    std::env::var("XDG_RUNTIME_DIR").ok().map(|runtime_dir| {
        PathBuf::from(runtime_dir).join("cosmic-clipboard-manager-toggle")
    })
}

/// Send a toggle signal by writing a timestamp to the signal file.
/// The timestamp ensures the file content changes, triggering the file watcher.
pub fn send_toggle_signal() -> std::io::Result<()> {
    let signal_file = get_signal_file_path().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "XDG_RUNTIME_DIR not set - cannot send toggle signal"
        )
    })?;
 
    // Write current timestamp to signal file.
    // The timestamp value itself isn't used, but ensures the file content changes
    // on each toggle, which triggers the file watcher to detect the change.
    let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?
        .as_millis()
        .to_string();
    
    fs::write(&signal_file, timestamp)?;
    Ok(())
}

/// Create a file watcher subscription that monitors the signal file for changes.
/// When the file is modified, it sends a CheckSignalFile message to toggle the popup.
pub fn signal_file_watcher() -> Subscription<AppMsg> {
    use notify::{Watcher, RecursiveMode, Event};
    use futures::stream;
    use tokio::time::{sleep, Duration};
 
    Subscription::run_with_id(
        "signal_file_watcher",
        stream::unfold((), |_| async {
            // Only set up watcher if XDG_RUNTIME_DIR is set
            let signal_file = match get_signal_file_path() {
                Some(path) => path,
                None => {
                    // If XDG_RUNTIME_DIR is not set, just wait forever
                    futures::future::pending::<()>().await;
                    return Some((AppMsg::CheckSignalFile, ()));
                }
            };

            let (tx, mut rx) = tokio::sync::mpsc::channel(1);
            
            let mut watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
                if res.is_ok() {
                    let _ = tx.blocking_send(());
                }
            }).ok()?;

            // Watch the signal file's parent directory since the file might not exist yet
            if let Some(parent) = signal_file.parent() {
                let _ = watcher.watch(parent, RecursiveMode::NonRecursive);

                // Wait for file change notification
                rx.recv().await;

                // Add a 10ms pause to prevent stressing cpu
                sleep(Duration::from_millis(10)).await;
            }

            Some((AppMsg::CheckSignalFile, ()))
        })
    )
}
