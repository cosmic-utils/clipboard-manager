use std::{
    collections::HashSet,
    fs::File,
    future::Future,
    io::{Read, Write},
    sync::{
        atomic::{self, AtomicBool},
        Arc,
    },
    thread::{self, sleep},
    time::Duration,
};

use cosmic::iced::{futures::SinkExt, subscription, Subscription};
use tokio::sync::mpsc;
use wl_clipboard_rs::{copy, paste_watch};

use crate::config::PRIVATE_MODE;
use crate::db::Entry;
use os_pipe::PipeReader;

// prefer popular formats
// orderer by priority
const IMAGE_MIME_TYPES: [&str; 3] = ["image/png", "image/jpeg", "image/ico"];

// prefer popular formats
// orderer by priority
const TEXT_MIME_TYPES: [&str; 2] = ["text/plain;charset=utf-8", "UTF8_STRING"];

#[derive(Debug, Clone)]
pub enum ClipboardMessage {
    Connected,
    Data(Entry),
    /// Means that the source was closed, or the compurer just started
    /// This means the clipboard manager must become the source, by providing the last entry
    EmptyKeyboard,
    Error(String),
}

pub fn sub() -> Subscription<ClipboardMessage> {
    struct ClipboardSub;

    subscription::channel(
        std::any::TypeId::of::<ClipboardSub>(),
        500,
        move |mut output| {
            async move {
                match paste_watch::Watcher::init(paste_watch::ClipboardType::Regular) {
                    Ok(mut clipboard_watcher) => {
                        let (tx, mut rx) =
                            mpsc::channel::<Option<std::vec::IntoIter<(PipeReader, String)>>>(5);

                        tokio::task::spawn_blocking(move || loop {
                            // return a vec of maximum 2 mimetypes
                            // 1.the main one
                            // optional 2. metadata
                            let mime_type_filter = |mut mime_types: HashSet<String>| {
                                debug!("mime type {:?}", mime_types);

                                let mut request = Vec::new();

                                if let Some(mime) = mime_types.take("text/uri-list") {
                                    request.push(mime);
                                    return request;
                                }

                                if mime_types.iter().any(|m| m.starts_with("image/")) {
                                    for prefered_image_format in IMAGE_MIME_TYPES {
                                        if let Some(mime) = mime_types.take(prefered_image_format) {
                                            request.push(mime);
                                            break;
                                        }
                                    }

                                    if request.is_empty() {
                                        return request;
                                    }

                                    // can be useful for metadata (alt)
                                    if let Some(mime) = mime_types.take("text/html") {
                                        request.push(mime);
                                    }
                                    return request;
                                }

                                if mime_types.iter().any(|m| m.starts_with("text/")) {
                                    for prefered_text_format in IMAGE_MIME_TYPES {
                                        if let Some(mime) = mime_types.take(prefered_text_format) {
                                            request.push(mime);
                                            return request;
                                        }
                                    }

                                    for mime in mime_types {
                                        if mime.starts_with("text/") {
                                            request.push(mime);
                                            return request;
                                        }
                                    }
                                }

                                request
                            };

                            match clipboard_watcher
                                .start_watching(paste_watch::Seat::Unspecified, mime_type_filter)
                            {
                                Ok(res) => {
                                    debug_assert!(res.len() == 1 || res.len() == 2);

                                    if !PRIVATE_MODE.load(atomic::Ordering::Relaxed) {
                                        tx.blocking_send(Some(res)).expect("can't send");
                                    } else {
                                        log::info!("private mode")
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
                                Some(Some(mut res)) => {
                                    let (mut pipe, mime_type) = res.next().unwrap();

                                    let mut contents = Vec::new();
                                    pipe.read_to_end(&mut contents).unwrap();

                                    let metadata = if let Some((mut pipe, mimitype)) = res.next() {
                                        let mut metadata = String::new();
                                        pipe.read_to_string(&mut metadata).unwrap();

                                        debug!("metadata = {}", metadata);

                                        #[allow(clippy::assigning_clones)]
                                        if mimitype == "text/html" {
                                            if let Some(alt) = find_alt(&metadata) {
                                                metadata = alt.to_owned();
                                            }
                                        }

                                        debug!("final metadata = {}", metadata);

                                        Some(metadata)
                                    } else {
                                        None
                                    };

                                    let data = Entry::new_now(mime_type, contents, metadata);

                                    info!("sending data to database: {:?}", data);
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
                            log::error!("inside error: {e}");
                            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                        }
                    }
                };
            }
        },
    )
}

// currently best effort
fn find_alt(html: &str) -> Option<&str> {
    const DEB: &str = "alt=\"";

    if let Some(pos) = html.find(DEB) {
        const OFFSET: usize = DEB.as_bytes().len();

        if let Some(pos_end) = html[pos + OFFSET..].find('"') {
            return Some(&html[pos + OFFSET..pos + pos_end + OFFSET]);
        }
    }

    None
}

pub fn copy(data: Entry) -> Result<(), copy::Error> {
    //dbg!("copy", &data);
    let options = copy::Options::default();
    let bytes = data.content.into_boxed_slice();

    let source = copy::Source::Bytes(bytes);

    let mime_type = copy::MimeType::Specific(data.mime);

    wl_clipboard_rs::copy::copy(options, source, mime_type)?;

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
