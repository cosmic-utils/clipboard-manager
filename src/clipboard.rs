use std::{
    collections::HashSet,
    sync::atomic::{self},
};

use cosmic::iced::{futures::SinkExt, stream::channel};
use futures::{future::join_all, Stream};
use tokio::{io::AsyncReadExt, net::unix::pipe, sync::mpsc};
use wl_clipboard_rs::{
    copy::{self, MimeSource},
    paste_watch,
};

use crate::{
    config::PRIVATE_MODE,
    db::{EntryTrait, MimeDataMap},
};

#[derive(Debug, Clone)]
pub enum ClipboardMessage {
    Connected,
    Data(MimeDataMap),
    /// Means that the source was closed, or the compurer just started
    /// This means the clipboard manager must become the source, by providing the last entry
    EmptyKeyboard,
    Error(String),
}

pub fn sub() -> impl Stream<Item = ClipboardMessage> {
    channel(500, move |mut output| {
        async move {
            match paste_watch::Watcher::init(paste_watch::ClipboardType::Regular) {
                Ok(mut clipboard_watcher) => {
                    let (tx, mut rx) =
                        mpsc::channel::<Option<std::vec::IntoIter<(pipe::Receiver, String)>>>(5);

                    tokio::task::spawn_blocking(move || loop {
                        // return a vec of maximum 2 mimetypes
                        // 1.the main one
                        // optional 2. metadata
                        let mime_type_filter = |mime_types: HashSet<String>| {
                            info!("mime type {:#?}", mime_types);
                            mime_types.into_iter().collect()
                        };

                        match clipboard_watcher
                            .start_watching(paste_watch::Seat::Unspecified, mime_type_filter)
                        {
                            Ok(res) => {
                                if !PRIVATE_MODE.load(atomic::Ordering::Relaxed) {
                                    tx.blocking_send(Some(res)).expect("can't send");
                                } else {
                                    info!("private mode")
                                }
                            }
                            Err(e) => match e {
                                paste_watch::Error::ClipboardEmpty => {
                                    tx.blocking_send(None).expect("can't send")
                                }
                                _ => {
                                    error!("watch clipboard error: {e}");
                                }
                            },
                        }
                    });
                    output.send(ClipboardMessage::Connected).await.unwrap();

                    loop {
                        match rx.recv().await {
                            Some(Some(res)) => {
                                info!("start reading pipes");

                                let data = join_all(res.map(|(mut pipe, mime_type)| async move {
                                    let mut contents = Vec::new();
                                    pipe.read_to_end(&mut contents).await.unwrap();
                                    (mime_type, contents)
                                }))
                                .await
                                .into_iter()
                                .collect();

                                info!("start sending pipes");

                                output.send(ClipboardMessage::Data(data)).await.unwrap();
                            }

                            Some(None) => {
                                output.send(ClipboardMessage::EmptyKeyboard).await.unwrap();
                            }
                            None => {
                                error!("can't receive");
                                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                            }
                        }
                    }
                }

                Err(e) => {
                    // todo: how to cancel properly?
                    // https://github.com/pop-os/cosmic-files/blob/d96d48995d49e17f01903ca4d89839eb4a1b1104/src/app.rs#L1704
                    output
                        .send(ClipboardMessage::Error(e.to_string()))
                        .await
                        .expect("can't send");
                    loop {
                        error!("inside error: {e}");
                        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                    }
                }
            };
        }
    })
}

pub fn copy<Entry: EntryTrait>(data: Entry) -> Result<(), copy::Error> {
    debug!("copy {:?}", data);

    let mut sources = Vec::with_capacity(data.raw_content().len());

    for (mime, content) in data.into_raw_content() {
        let source = MimeSource {
            source: copy::Source::Bytes(content.into_boxed_slice()),
            mime_type: copy::MimeType::Specific(mime),
        };

        sources.push(source);
    }

    let options = copy::Options::default();
    wl_clipboard_rs::copy::copy_multi(options, sources)?;

    Ok(())
}

// unfold experiment, doesn't work with channel, but better error management
/*

enum State {
    Init,
    Idle(paste_watch::Watcher),
    Error,
}

pub fn sub2() -> Subscription<Message> {
    struct Connect;

    subscription::unfold(
        std::any::TypeId::of::<Connect>(),
        State::Init,
        |state| {

            async move {
                match state {
                    State::Init => {
                        match paste_watch::Watcher::init(paste_watch::ClipboardType::Regular) {
                            Ok(watcher) => {
                                return (Message::Connected, State::Idle(watcher));
                            }
                            Err(e) => {
                                return (Message::Error(e), State::Error);
                            }
                        }
                    }
                    State::Idle(watcher) => {

                        let e = watcher.start_watching2(
                            paste_watch::Seat::Unspecified,
                            paste_watch::MimeType::Any,
                        );


                        todo!()
                    }

                    State::Error => {
                        // todo
                        todo!()
                    }
                }
            }
        },
    )
}

 */
