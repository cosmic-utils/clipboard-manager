use std::{
    sync::{
        Arc,
        atomic::{self},
    },
    time::Duration,
};

use cosmic::iced::{futures::SinkExt, stream::channel};
use futures::{Stream, future::join_all};
use itertools::Itertools;
use tokio::{io::AsyncReadExt, sync::mpsc};
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
    Error(ClipboardError),
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum ClipboardError {
    #[error(transparent)]
    Watch(Arc<paste_watch::Error>),
}

enum WatchRes<I> {
    Some(I),
    None,
    Err(paste_watch::Error),
}

pub fn sub() -> impl Stream<Item = ClipboardMessage> {
    channel(500, move |mut output| {
        async move {
            match paste_watch::Watcher::init(paste_watch::ClipboardType::Regular) {
                Ok(mut clipboard_watcher) => {
                    let (tx, mut rx) = mpsc::channel(5);

                    tokio::task::spawn_blocking(move || {
                        loop {
                            match clipboard_watcher.start_watching(paste_watch::Seat::Unspecified) {
                                Ok(res) => {
                                    if !PRIVATE_MODE.load(atomic::Ordering::Relaxed) {
                                        tx.blocking_send(WatchRes::Some(res)).unwrap();
                                    } else {
                                        info!("private mode")
                                    }
                                }
                                Err(e) => match e {
                                    paste_watch::Error::ClipboardEmpty => {
                                        tx.blocking_send(WatchRes::None).unwrap();
                                    }
                                    _ => {
                                        tx.blocking_send(WatchRes::Err(e)).unwrap();
                                        break;
                                    }
                                },
                            }
                        }
                    });
                    output.send(ClipboardMessage::Connected).await.unwrap();

                    let mut i = 0;
                    loop {
                        let s = debug_span!("", i);
                        let _s = s.enter();
                        i += 1;

                        match rx.recv().await {
                            Some(WatchRes::Some(res)) => {
                                let data: MimeDataMap =
                                    join_all(res.map(|(mime_type, mut pipe)| async move {
                                        let mut contents = Vec::new();

                                        match tokio::time::timeout(
                                            Duration::from_millis(5000),
                                            pipe.read_to_end(&mut contents),
                                        )
                                        .await
                                        {
                                            Ok(Ok(len)) => {
                                                if len == 0 {
                                                    warn!("data is empty: {mime_type}");
                                                    None
                                                } else  {Some((mime_type, contents)) }
                                            },
                                            Ok(Err(e)) => {
                                                warn!("read error on external pipe clipboard: {mime_type} {e}");
                                                None
                                            }
                                            Err(e) => {
                                                warn!("read timeout on external pipe clipboard: {mime_type} {e}");
                                                None
                                            }
                                        }
                                    }))
                                    .await
                                    .into_iter()
                                    .flatten()
                                    .collect();

                                if !data.is_empty() {
                                    let mimes = data
                                        .iter()
                                        .map(|(m, d)| (m.to_string(), d.len()))
                                        .collect_vec();

                                    debug!("send mime types to db: {mimes:?}");
                                    output.send(ClipboardMessage::Data(data)).await.unwrap();
                                }
                            }

                            Some(WatchRes::None) => {
                                output.send(ClipboardMessage::EmptyKeyboard).await.unwrap();
                            }
                            Some(WatchRes::Err(e)) => {
                                output
                                    .send(ClipboardMessage::Error(ClipboardError::Watch(e.into())))
                                    .await
                                    .unwrap();
                                std::future::pending::<()>().await;
                            }
                            None => {
                                std::future::pending::<()>().await;
                            }
                        }
                    }
                }

                Err(e) => {
                    // todo: how to cancel properly?
                    // https://github.com/pop-os/cosmic-files/blob/d96d48995d49e17f01903ca4d89839eb4a1b1104/src/app.rs#L1704
                    output
                        .send(ClipboardMessage::Error(ClipboardError::Watch(e.into())))
                        .await
                        .unwrap();

                    std::future::pending::<()>().await;
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
