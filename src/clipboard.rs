use std::{
    io::Read,
    thread::{self, sleep},
    time::Duration,
};

use cosmic::iced::{futures::SinkExt, subscription, Subscription};
use tokio::sync::mpsc;
use wl_clipboard_rs::{copy, paste_watch};

use crate::db::Data;

use os_pipe::PipeReader;

pub fn sub() -> Subscription<Data> {
    struct Connect;

    subscription::channel(std::any::TypeId::of::<Connect>(), 100, move |mut output| {
        let (tx, mut rx) = mpsc::channel::<(PipeReader, String)>(100);

        tokio::task::spawn_blocking(|| {
            let handle = tokio::runtime::Handle::current();

            let res = handle.block_on(paste_watch::get_contents(
                paste_watch::ClipboardType::Regular,
                paste_watch::Seat::Unspecified,
                paste_watch::MimeType::Any,
                tx,
            ));
            if let Err(e) = res {
                error!("{e}");
            }
        });

        async move {
            loop {
                match rx.recv().await {
                    Some((mut pipe, mime_type)) => {
                        let mut contents = String::new();
                        pipe.read_to_string(&mut contents).unwrap();
                        let data = Data::new(mime_type, contents);
                        //info!("sending: {:?}", data);
                        output.send(data).await.unwrap();
                    }
                    None => {
                        error!("can't receive from wayland");
                        panic!();
                    }
                }
            }
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
