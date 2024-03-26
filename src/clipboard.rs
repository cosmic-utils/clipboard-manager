use std::{
    future::Future,
    io::Read,
    thread::{self, sleep},
    time::Duration,
};

use cosmic::iced::{futures::SinkExt, subscription, Subscription};
use tokio::sync::mpsc;
use wl_clipboard_rs::{copy, paste_watch};

use crate::db::Data;

use os_pipe::PipeReader;

#[derive(Debug, Clone)]
pub enum Message {
    Connected,
    Data(Data),
    Error(String),
}

pub fn sub() -> Subscription<Message> {
    enum State {
        Init,
        Idle(paste_watch::Watcher),
        Error,
    }

    subscription::channel(std::any::TypeId::of::<State>(), 100, move |mut output| {
        async move {
            match paste_watch::Watcher::init(paste_watch::ClipboardType::Regular) {
                Ok(mut clipboard_watcher) => {
                    let (tx, mut rx) = mpsc::channel::<(PipeReader, String)>(100);

                    tokio::task::spawn_blocking(move || loop {
                        match clipboard_watcher.start_watching(
                            paste_watch::Seat::Unspecified,
                            paste_watch::MimeType::Any,
                        ) {
                            Ok(res) => {
                                tx.blocking_send(res).expect("can't send");
                            }
                            Err(e) => {
                                error!("{e}");
                            }
                        }
                    });

                    loop {
                        match rx.recv().await {
                            Some((mut pipe, mime_type)) => {
                                let mut contents = String::new();
                                pipe.read_to_string(&mut contents).unwrap();
                                let data = Data::new(mime_type, contents);
                                //info!("sending: {:?}", data);
                                output.send(Message::Data(data)).await.unwrap();
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
                        .send(Message::Error(e.to_string()))
                        .await
                        .expect("can't send");
                    loop {
                        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                    }
                }
            };
        }
    })
}

pub fn copy(data: Data) -> Result<(), copy::Error> {
    let options = copy::Options::default();

    let bytes = data.value.into_bytes().into_boxed_slice();

    let source = copy::Source::Bytes(bytes);

    let mime_type = copy::MimeType::Specific(data.mime);

    wl_clipboard_rs::copy::copy(options, source, mime_type)?;

    Ok(())
}

// unfold experiment, doesn't work with channel, but better error management
/*

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
