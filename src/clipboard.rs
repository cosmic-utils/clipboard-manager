use std::{io::Read, sync::mpsc, thread};

use wl_clipboard_rs::paste_watch::{get_contents, ClipboardType, MimeType, Seat};

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
