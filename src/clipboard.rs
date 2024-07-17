use std::{
    collections::HashSet,
    future::Future,
    io::Read,
    sync::{
        atomic::{self, AtomicBool},
        Arc,
    },
    thread::{self, sleep},
    time::Duration,
};

use cosmic::iced::{futures::SinkExt, subscription, Subscription};
use tl::queryselector::iterable::QueryIterable;
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
                        let (tx, mut rx) = mpsc::channel::<Option<Vec<(PipeReader, String)>>>(5);

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
                                    let metadata = if res.len() == 2 {
                                        let (mut pipe, mimitype) = res.remove(0);


                                        eprintln!("before");
                                        let mut metadata = Vec::new();
                                        pipe.read_to_end(&mut metadata).unwrap();


                                        eprintln!("before");
                                        let mut metadata = String::new();
                                        pipe.read_to_string(&mut metadata).unwrap();

                                        eprintln!("after");

                                        // // #[allow(clippy::assigning_clones)]
                                        // // if mimitype == "text/html" {
                                        // //     if let Some(alt) = find_alt(&metadata) {
                                        // //         metadata = alt.to_owned();
                                        // //     }
                                        // // }

                                        // let metadata = String::from_utf8(metadata).unwrap();
                                        
                                        // find_alt3(&metadata);

                                        // Some(metadata)
                                        // Some(metadata)
                                        None
                                    } else {
                                        None
                                    };

                                    let (mut pipe, mime_type) = res.remove(0);

                                    let mut contents = Vec::new();
                                    pipe.read_to_end(&mut contents).unwrap();

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

#[test]
fn a() {
    let metadata = r#"
    <meta http-equiv=\"content-type\" content=\"text/html; charset=utf-8\"><img
src=\"data:image/png;base64,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\"
data-deferred=\"1\" class=\"rg_i Q4LuWd\" jsname=\"Q4LuWd\" width=\"316\"
height=\"160\" alt=\"The Iterator Pattern: So Simple, It's Genius (Or So They
Say) | by Do Tran | Level Up Coding\" data-iml=\"1916\" data-atf=\"true\">

    "#.to_owned();

    assert_eq!(find_alt(&metadata), Some("The Iterator Pattern: So Simple, It's Genius (Or So They\nSay) | by Do Tran | Level Up Coding"))
}

fn find_alt(html: &str) -> Option<&str> {
    const DEB: &str = "alt=\\\"";

    if let Some(pos) = html.find(DEB) {
        const OFFSET: usize = DEB.as_bytes().len();

        if let Some(pos_end) = html[pos + OFFSET..].find("\\\"") {
            return Some(&html[pos + OFFSET..pos + pos_end + OFFSET]);
        }
    }

    None
}

fn find_alt2(html: &str)  {
    let dom = tl::parse(html, tl::ParserOptions::default()).unwrap();

    let parser = dom.parser();

    let element = dom
        .get_element_by_id("img")
        .expect("Failed to find element")
        .get(parser)
        .unwrap();

    let a = element.inner_text(parser);

    println!("{a}");
}

fn find_alt3(html: &str) {
    // Parse the HTML string
    let dom = tl::parse(html, tl::ParserOptions::default()).unwrap();

    // Get the parser object from the DOM
    let parser = dom.parser();


    let e = dom.get_elements_by_class_name("img").next();

    info!("{e:?}")
  
}

#[test]
fn b() {
    let metadata = r#"
    <meta http-equiv=\"content-type\" content=\"text/html; charset=utf-8\"><img
src=\"data:image/png;base64,iVE2CC\"
data-deferred=\"1\" class=\"rg_i Q4LuWd\" jsname=\"Q4LuWd\" width=\"316\"
height=\"160\" alt=\"The Iterator Pattern: So Simple, It's Genius (Or So They
Say) | by Do Tran | Level Up Coding\" data-iml=\"1916\" data-atf=\"true\">

    "#.to_owned();

    find_alt3(&metadata);
}