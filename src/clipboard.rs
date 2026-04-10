use std::{
    os::fd::{FromRawFd, IntoRawFd, OwnedFd},
    sync::{
        Arc,
        atomic::{self},
    },
};

use cosmic::iced::{futures::SinkExt, stream::channel};
use futures::Stream;
use futures::future::join_all;
use itertools::Itertools;
use tokio::io::AsyncReadExt;
use tokio::sync::mpsc;
use tokio::time::{Duration, timeout};

use crate::{clipboard_watcher, config::PRIVATE_MODE, db::MimeDataMap};

#[derive(Debug, Clone)]
pub enum ClipboardMessage {
    Connected,
    Data(MimeDataMap),
    /// Means that the source was closed, or the compurer just started
    /// This means the clipboard manager must become the source, by providing the last entry
    EmptyKeyboard,
    /// Recoverable error - can potentially retry
    ErrorRecoverable(ClipboardError),
    /// Fatal error - requires intervention or configuration change
    ErrorFatal(ClipboardError),
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum ClipboardError {
    #[error(transparent)]
    Watch(Arc<clipboard_watcher::Error>),
}

impl ClipboardError {
    /// Returns true if this error is potentially recoverable (transient)
    pub fn is_recoverable(&self) -> bool {
        match self {
            ClipboardError::Watch(e) => matches!(
                **e,
                clipboard_watcher::Error::ClipboardEmpty
                    | clipboard_watcher::Error::WaylandCommunication(_)
                    | clipboard_watcher::Error::OfferNotFound
                    | clipboard_watcher::Error::RegistryInvalidId
            ),
        }
    }
}

enum WatchRes<I> {
    Some(I),
    None,
    Err(clipboard_watcher::Error),
}

pub fn sub() -> impl Stream<Item = ClipboardMessage> {
    channel(500, async |mut output| {
        match clipboard_watcher::Watcher::init() {
            Ok(mut clipboard_watcher) => {
                let (tx, mut rx) = mpsc::channel(5);

                tokio::task::spawn_blocking(move || {
                    loop {
                        debug!("start watching");
                        match clipboard_watcher.start_watching(clipboard_watcher::Seat::Unspecified)
                        {
                            Ok(res) => {
                                if !PRIVATE_MODE.load(atomic::Ordering::Relaxed) {
                                    if tx.blocking_send(WatchRes::Some(res)).is_err() {
                                        debug!(
                                            "clipboard channel receiver dropped, exiting watcher loop"
                                        );
                                        break;
                                    }
                                } else {
                                    info!("private mode")
                                }
                            }
                            Err(e) => match e {
                                clipboard_watcher::Error::ClipboardEmpty => {
                                    if tx.blocking_send(WatchRes::None).is_err() {
                                        debug!(
                                            "clipboard channel receiver dropped, exiting watcher loop"
                                        );
                                        break;
                                    }
                                }
                                _ => {
                                    // Try to send error, but don't panic if channel is closed
                                    let _ = tx.blocking_send(WatchRes::Err(e));
                                    break;
                                }
                            },
                        }
                    }
                });

                if output.send(ClipboardMessage::Connected).await.is_err() {
                    warn!("clipboard output channel closed during connect");
                    return;
                }

                let mut i = 0;
                loop {
                    let s = debug_span!("", i);
                    let _s = s.enter();
                    i += 1;

                    match rx.recv().await {
                        Some(WatchRes::Some(res)) => {
                            let mut data = MimeDataMap::new();

                            let read_futures = res.into_iter().map(|(mime_type, pipe)| async move {
                                let raw_fd = pipe.into_raw_fd();
                                let owned_fd = unsafe { OwnedFd::from_raw_fd(raw_fd) };

                                let mut async_pipe =
                                    match tokio::net::unix::pipe::Receiver::from_owned_fd(owned_fd)
                                    {
                                        Ok(receiver) => receiver,
                                        Err(e) => {
                                            warn!(
                                                "failed to create async pipe receiver: {mime_type} {e}"
                                            );
                                            return None;
                                        }
                                    };

                                let mut contents = Vec::new();
                                match timeout(
                                    Duration::from_millis(500),
                                    async_pipe.read_to_end(&mut contents),
                                )
                                .await
                                {
                                    Ok(Ok(len)) => {
                                        if len == 0 {
                                            debug!("data is empty: {mime_type}");
                                            None
                                        } else {
                                            Some((mime_type, contents))
                                        }
                                    }
                                    Ok(Err(e)) => {
                                        warn!("read error on external pipe clipboard: {mime_type} {e}");
                                        None
                                    }
                                    Err(_) => {
                                        warn!("read timeout (500ms): {mime_type}");
                                        None
                                    }
                                }
                            });

                            let read_results = join_all(read_futures).await;
                            for (mime_type, contents) in read_results.into_iter().flatten() {
                                data.insert(mime_type, contents);
                            }

                            if !data.is_empty() {
                                let mimes = data
                                    .iter()
                                    .map(|(m, d)| (m.to_string(), d.len()))
                                    .collect_vec();

                                debug!("send mime types to db: {mimes:?}");
                                if output.send(ClipboardMessage::Data(data)).await.is_err() {
                                    warn!("clipboard output channel closed");
                                    return;
                                }
                            }
                        }

                        Some(WatchRes::None) => {
                            debug!("empty keyboard");
                            if output.send(ClipboardMessage::EmptyKeyboard).await.is_err() {
                                warn!("clipboard output channel closed");
                                return;
                            }
                        }
                        Some(WatchRes::Err(e)) => {
                            let error = ClipboardError::Watch(e.into());
                            let message = if error.is_recoverable() {
                                ClipboardMessage::ErrorRecoverable(error)
                            } else {
                                ClipboardMessage::ErrorFatal(error)
                            };
                            if output.send(message).await.is_err() {
                                warn!("clipboard output channel closed during error report");
                            }
                            return;
                        }
                        None => {
                            debug!("clipboard watcher channel closed");
                            return;
                        }
                    }
                }
            }

            Err(e) => {
                let error = ClipboardError::Watch(e.into());
                let message = if error.is_recoverable() {
                    ClipboardMessage::ErrorRecoverable(error)
                } else {
                    ClipboardMessage::ErrorFatal(error)
                };
                if output.send(message).await.is_err() {
                    warn!("clipboard output channel closed during init error report");
                }
            }
        };
    })
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
