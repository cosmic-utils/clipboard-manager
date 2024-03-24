use std::{io::Read, sync::mpsc, thread};

use cosmic::iced::{futures::SinkExt, subscription, Subscription};
use wl_clipboard_rs::paste_watch::{get_contents, ClipboardType, MimeType, Seat};

use crate::db::Data;




pub fn sub() -> Subscription<Data> {

    struct Connect;

    subscription::channel(
        std::any::TypeId::of::<Connect>(),
        100,
        |mut output| async move {
            let (tx, rx) = mpsc::channel();

            thread::spawn(|| {
                if let Err(e) = get_contents(ClipboardType::Regular, Seat::Unspecified, MimeType::Any, tx) {
                    error!("{e}");
                }
            });

            loop {
                match rx.recv() {
                    Ok((mut pipe, mime_type)) => {

                        let mut contents = String::new();
                        pipe.read_to_string(&mut contents).unwrap();
                        info!("received {}", contents);
                        
                        let data = Data::new(mime_type, contents);
                        //output.send(data).await.unwrap();
                    }
                    Err(e) => {
                        error!("can't receive from wayland {e}");
                        panic!();
                    }
                }
            }

        }
    )

}

pub fn watch_keyboard() {
    let (tx, rx) = mpsc::channel();

    thread::spawn(|| {
        if let Err(e) = get_contents(ClipboardType::Regular, Seat::Unspecified, MimeType::Any, tx) {
            error!("{e}");
        }
    });

    loop {
        match rx.recv() {
            Ok((mut pipe, _mime_type)) => {
                //println!("Got data of the {} MIME type", &mime_type);

                let mut contents = String::new();
                pipe.read_to_string(&mut contents).unwrap();
                println!("{}", contents);

                //info!("{mime_type}");
                //println!("hello: {mime_type}")
            }
            Err(e) => {
                error!("{e}");
                panic!();
            }
        }
    }
}
