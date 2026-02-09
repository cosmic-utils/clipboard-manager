//! D-Bus IPC for toggling the clipboard manager popup, editing entries,
//! and browsing clipboard history from external commands (keyboard shortcuts, CLI).
//!
//! The applet registers a D-Bus service on the session bus. When `--toggle`,
//! `--edit`, `--list`, or `--copy` is invoked, it calls the corresponding method
//! on the running service instance, which sends a message through the iced
//! subscription to the app.

use std::sync::{Arc, Mutex};

use crate::message::{AppMsg, FavoriteSummary};
use cosmic::iced_futures::Subscription;

const BUS_NAME: &str = "io.github.cosmic_utils.ClipboardManager";
const OBJECT_PATH: &str = "/io/github/cosmic_utils/ClipboardManager";
const INTERFACE_NAME: &str = "io.github.cosmic_utils.ClipboardManager1";

/// Summary of a clipboard entry for CLI listing.
#[derive(Clone, Debug)]
pub struct EntrySummary {
    pub id: i64,
    pub is_favorite: bool,
    pub preview: String,
}

enum IpcCommand {
    Toggle,
    EditLatest,
    ListEntries {
        reply: tokio::sync::oneshot::Sender<Vec<EntrySummary>>,
    },
    CopyEntry {
        id: i64,
        reply: tokio::sync::oneshot::Sender<Result<(), String>>,
    },
    GetEntry {
        id: i64,
        reply: tokio::sync::oneshot::Sender<Result<(String, Vec<u8>), String>>,
    },
    ToggleFavorites,
    ListFavorites {
        reply: tokio::sync::oneshot::Sender<Vec<FavoriteSummary>>,
    },
}

/// D-Bus service that receives calls and forwards them via a channel.
struct ClipboardService {
    tx: tokio::sync::mpsc::Sender<IpcCommand>,
}

#[zbus::interface(name = "io.github.cosmic_utils.ClipboardManager1")]
impl ClipboardService {
    async fn toggle(&self) {
        let _ = self.tx.send(IpcCommand::Toggle).await;
    }

    async fn edit_latest(&self) {
        let _ = self.tx.send(IpcCommand::EditLatest).await;
    }

    /// Returns Vec<(id, is_favorite, preview)> — native D-Bus tuple serialization.
    async fn list_entries(&self) -> Vec<(i64, bool, String)> {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        if self
            .tx
            .send(IpcCommand::ListEntries { reply: reply_tx })
            .await
            .is_err()
        {
            return Vec::new();
        }
        match reply_rx.await {
            Ok(entries) => entries
                .into_iter()
                .map(|e| (e.id, e.is_favorite, e.preview))
                .collect(),
            Err(_) => Vec::new(),
        }
    }

    /// Get entry content by ID. Returns (mime, content_bytes, error).
    /// If error is non-empty, the other fields are empty.
    async fn get_entry(&self, id: i64) -> (String, Vec<u8>, String) {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        if self
            .tx
            .send(IpcCommand::GetEntry {
                id,
                reply: reply_tx,
            })
            .await
            .is_err()
        {
            return (String::new(), Vec::new(), "applet not responding".to_string());
        }
        match reply_rx.await {
            Ok(Ok((mime, data))) => (mime, data, String::new()),
            Ok(Err(e)) => (String::new(), Vec::new(), e),
            Err(_) => (String::new(), Vec::new(), "applet did not reply".to_string()),
        }
    }

    async fn toggle_favorites(&self) {
        let _ = self.tx.send(IpcCommand::ToggleFavorites).await;
    }

    /// Returns Vec<(id, title, preview)> — only favorite entries.
    async fn list_favorites(&self) -> Vec<(i64, String, String)> {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        if self
            .tx
            .send(IpcCommand::ListFavorites { reply: reply_tx })
            .await
            .is_err()
        {
            return Vec::new();
        }
        match reply_rx.await {
            Ok(entries) => entries
                .into_iter()
                .map(|e| {
                    let title = e.title.unwrap_or_default();
                    (e.id, title, e.preview)
                })
                .collect(),
            Err(_) => Vec::new(),
        }
    }

    /// Copy entry by ID. Returns empty string on success, error message on failure.
    async fn copy_entry(&self, id: i64) -> String {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        if self
            .tx
            .send(IpcCommand::CopyEntry {
                id,
                reply: reply_tx,
            })
            .await
            .is_err()
        {
            return "applet not responding".to_string();
        }
        match reply_rx.await {
            Ok(Ok(())) => String::new(),
            Ok(Err(e)) => e,
            Err(_) => "applet did not reply".to_string(),
        }
    }
}

/// Subscription that registers the D-Bus service and listens for IPC calls.
pub fn dbus_toggle_subscription() -> Subscription<AppMsg> {
    use cosmic::iced::futures::SinkExt;
    use cosmic::iced::stream::channel;

    Subscription::run_with_id(
        "dbus_toggle",
        channel(4, |mut output| async move {
            let (tx, mut rx) = tokio::sync::mpsc::channel::<IpcCommand>(4);
            let service = ClipboardService { tx };

            let conn = match zbus::connection::Builder::session()
                .and_then(|b| b.name(BUS_NAME))
                .and_then(|b| b.serve_at(OBJECT_PATH, service))
            {
                Ok(builder) => match builder.build().await {
                    Ok(conn) => conn,
                    Err(e) => {
                        error!("D-Bus connection failed: {e}");
                        futures::future::pending::<()>().await;
                        unreachable!();
                    }
                },
                Err(e) => {
                    error!("D-Bus builder failed: {e}");
                    futures::future::pending::<()>().await;
                    unreachable!();
                }
            };

            // Keep connection alive for the lifetime of the subscription
            let _conn = conn;

            loop {
                match rx.recv().await {
                    Some(IpcCommand::Toggle) => {
                        output.send(AppMsg::DbusToggle).await.ok();
                    }
                    Some(IpcCommand::EditLatest) => {
                        output.send(AppMsg::EditLatest).await.ok();
                    }
                    Some(IpcCommand::ListEntries { reply }) => {
                        output
                            .send(AppMsg::DbusListEntries {
                                reply: Arc::new(Mutex::new(Some(reply))),
                            })
                            .await
                            .ok();
                    }
                    Some(IpcCommand::CopyEntry { id, reply }) => {
                        output
                            .send(AppMsg::DbusCopyEntry {
                                id,
                                reply: Arc::new(Mutex::new(Some(reply))),
                            })
                            .await
                            .ok();
                    }
                    Some(IpcCommand::GetEntry { id, reply }) => {
                        output
                            .send(AppMsg::DbusGetEntry {
                                id,
                                reply: Arc::new(Mutex::new(Some(reply))),
                            })
                            .await
                            .ok();
                    }
                    Some(IpcCommand::ToggleFavorites) => {
                        output.send(AppMsg::DbusFavorites).await.ok();
                    }
                    Some(IpcCommand::ListFavorites { reply }) => {
                        output
                            .send(AppMsg::DbusListFavorites {
                                reply: Arc::new(Mutex::new(Some(reply))),
                            })
                            .await
                            .ok();
                    }
                    None => {
                        // Channel closed, wait forever to avoid busy loop
                        futures::future::pending::<()>().await;
                    }
                }
            }
        }),
    )
}

/// Send a Toggle call to the running applet via D-Bus (blocking, for CLI use).
pub fn send_toggle() -> Result<(), Box<dyn std::error::Error>> {
    let connection = zbus::blocking::Connection::session()?;
    let proxy = zbus::blocking::Proxy::new(
        &connection,
        BUS_NAME,
        OBJECT_PATH,
        INTERFACE_NAME,
    )?;
    proxy.call_method("Toggle", &())?;
    Ok(())
}

/// Send an EditLatest call to the running applet via D-Bus (blocking, for CLI use).
pub fn send_edit_latest() -> Result<(), Box<dyn std::error::Error>> {
    let connection = zbus::blocking::Connection::session()?;
    let proxy = zbus::blocking::Proxy::new(
        &connection,
        BUS_NAME,
        OBJECT_PATH,
        INTERFACE_NAME,
    )?;
    proxy.call_method("EditLatest", &())?;
    Ok(())
}

/// List all clipboard entries from the running applet via D-Bus (blocking, for CLI use).
pub fn send_list_entries() -> Result<Vec<(i64, bool, String)>, Box<dyn std::error::Error>> {
    let connection = zbus::blocking::Connection::session()?;
    let proxy = zbus::blocking::Proxy::new(
        &connection,
        BUS_NAME,
        OBJECT_PATH,
        INTERFACE_NAME,
    )?;
    let reply = proxy.call_method("ListEntries", &())?;
    let entries: Vec<(i64, bool, String)> = reply.body().deserialize()?;
    Ok(entries)
}

/// Get raw content of a specific entry by ID via D-Bus (blocking, for CLI use).
/// Returns Ok((mime, bytes)) on success, Err with message on failure.
pub fn send_get_entry(id: i64) -> Result<(String, Vec<u8>), Box<dyn std::error::Error>> {
    let connection = zbus::blocking::Connection::session()?;
    let proxy = zbus::blocking::Proxy::new(
        &connection,
        BUS_NAME,
        OBJECT_PATH,
        INTERFACE_NAME,
    )?;
    let reply = proxy.call_method("GetEntry", &(id,))?;
    let (mime, data, error): (String, Vec<u8>, String) = reply.body().deserialize()?;
    if error.is_empty() {
        Ok((mime, data))
    } else {
        Err(error.into())
    }
}

/// Send a ToggleFavorites call to the running applet via D-Bus (blocking, for CLI use).
pub fn send_toggle_favorites() -> Result<(), Box<dyn std::error::Error>> {
    let connection = zbus::blocking::Connection::session()?;
    let proxy = zbus::blocking::Proxy::new(
        &connection,
        BUS_NAME,
        OBJECT_PATH,
        INTERFACE_NAME,
    )?;
    proxy.call_method("ToggleFavorites", &())?;
    Ok(())
}

/// List favorite entries from the running applet via D-Bus (blocking, for CLI use).
/// Returns Vec<(id, title, preview)>.
pub fn send_list_favorites() -> Result<Vec<(i64, String, String)>, Box<dyn std::error::Error>> {
    let connection = zbus::blocking::Connection::session()?;
    let proxy = zbus::blocking::Proxy::new(
        &connection,
        BUS_NAME,
        OBJECT_PATH,
        INTERFACE_NAME,
    )?;
    let reply = proxy.call_method("ListFavorites", &())?;
    let entries: Vec<(i64, String, String)> = reply.body().deserialize()?;
    Ok(entries)
}

/// Copy a specific entry by ID via D-Bus (blocking, for CLI use).
/// Returns Ok(()) on success, Err with message on failure.
pub fn send_copy_entry(id: i64) -> Result<(), Box<dyn std::error::Error>> {
    let connection = zbus::blocking::Connection::session()?;
    let proxy = zbus::blocking::Proxy::new(
        &connection,
        BUS_NAME,
        OBJECT_PATH,
        INTERFACE_NAME,
    )?;
    let reply = proxy.call_method("CopyEntry", &(id,))?;
    let result: String = reply.body().deserialize()?;
    if result.is_empty() {
        Ok(())
    } else {
        Err(result.into())
    }
}
