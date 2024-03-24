use std::{
    io::Read,
    thread::{self, sleep},
    time::Duration,
};

use cosmic::iced::{futures::SinkExt, subscription, Subscription};
use tokio::sync::mpsc;
use wl_clipboard_rs::paste_watch::{get_contents, ClipboardType, MimeType, Seat};

use crate::db::Data;

use os_pipe::PipeReader;

pub fn sub() -> Subscription<Data> {
    struct Connect;

    subscription::channel(std::any::TypeId::of::<Connect>(), 100, move |mut output| {
        let (tx, mut rx) = mpsc::channel::<(PipeReader, String)>(100);

        tokio::task::spawn_blocking(|| {
            let handle = tokio::runtime::Handle::current();

            let res = handle.block_on(get_contents(
                ClipboardType::Regular,
                Seat::Unspecified,
                MimeType::Any,
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
