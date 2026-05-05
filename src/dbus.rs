use zbus::{interface, connection};
use cosmic::iced::{futures::SinkExt, stream::channel};
use futures::Stream;
use crate::message::AppMsg;

pub struct ClipboardInterface {
    tx: tokio::sync::mpsc::Sender<()>,
}

#[interface(name = "io.github.cosmic_utils.ClipboardManager")]
impl ClipboardInterface {
    async fn toggle(&self) {
        eprintln!("D-Bus: Toggle called!");
        let _ = self.tx.send(()).await;
    }
}

pub fn sub() -> impl Stream<Item = AppMsg> {
    channel(1, async |mut output| {
        eprintln!("D-Bus: Starting D-Bus server...");
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        let interface = ClipboardInterface { tx };
        
        let connection = connection::Builder::session()
            .unwrap()
            .name("io.github.cosmic_utils.ClipboardManager")
            .unwrap()
            .serve_at("/io/github/cosmic_utils/ClipboardManager", interface)
            .unwrap()
            .build()
            .await
            .unwrap();

        eprintln!("D-Bus: Server running at /io/github/cosmic_utils/ClipboardManager");

        loop {
            if rx.recv().await.is_some() {
                eprintln!("D-Bus: Relaying TogglePopup message");
                let _ = output.send(AppMsg::TogglePopup).await;
            } else {
                break;
            }
        }
        
        drop(connection);
    })
}
