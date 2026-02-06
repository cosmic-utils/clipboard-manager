//! D-Bus IPC for toggling the clipboard manager popup and editing entries
//! from external commands (keyboard shortcuts).
//!
//! The applet registers a D-Bus service on the session bus. When `--toggle` or
//! `--edit` is invoked, it calls the corresponding method on the running service
//! instance, which sends a message through the iced subscription to the app.

use crate::message::AppMsg;
use cosmic::iced_futures::Subscription;

const BUS_NAME: &str = "io.github.cosmic_utils.ClipboardManager";
const OBJECT_PATH: &str = "/io/github/cosmic_utils/ClipboardManager";
const INTERFACE_NAME: &str = "io.github.cosmic_utils.ClipboardManager1";

enum IpcCommand {
    Toggle,
    EditLatest,
}

/// D-Bus service that receives Toggle/EditLatest calls and forwards them via a channel.
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
}

/// Subscription that registers the D-Bus service and listens for IPC calls.
pub fn dbus_toggle_subscription() -> Subscription<AppMsg> {
    use cosmic::iced::futures::SinkExt;
    use cosmic::iced::stream::channel;

    Subscription::run_with_id(
        "dbus_toggle",
        channel(1, |mut output| async move {
            let (tx, mut rx) = tokio::sync::mpsc::channel::<IpcCommand>(1);
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
