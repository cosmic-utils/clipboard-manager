#![allow(dead_code)]
#![allow(unused_macros)]
#![allow(unused_imports)]

use config::{Config, CONFIG_VERSION};
use cosmic::cosmic_config;
use cosmic::cosmic_config::CosmicConfigEntry;
use log::LevelFilter;
use app::{Flags, Window};

mod clipboard;
mod config;
mod db;
mod localize;
mod utils;
mod view;
mod app;
mod message;

#[allow(unused_imports)]
#[macro_use]
extern crate log;

fn setup_logs() {
    let mut builder = env_logger::builder();

    fn filter_workspace_crates(
        builder: &mut env_logger::Builder,
        level_filter: LevelFilter,
    ) -> &mut env_logger::Builder {
        // allow other crate to show warn level of error
        builder.filter_level(LevelFilter::Warn);
        builder.filter_module("wl_clipboard_rs", level_filter);
        builder.filter_module(env!("CARGO_CRATE_NAME"), level_filter);
        builder
    }

    filter_workspace_crates(&mut builder, LevelFilter::Info);

    builder.init();
}

fn main() -> cosmic::iced::Result {
    localize::localize();

    setup_logs();

    let (config_handler, config) = match cosmic_config::Config::new(app::APP_ID, CONFIG_VERSION)
    {
        Ok(config_handler) => {
            let config = match Config::get_entry(&config_handler) {
                Ok(ok) => ok,
                Err((errs, config)) => {
                    eprintln!("errors loading config: {:?}", errs);
                    config
                }
            };
            (Some(config_handler), config)
        }
        Err(err) => {
            eprintln!("failed to create config handler: {}", err);
            (None, Config::default())
        }
    };

    let flags = Flags {
        config_handler,
        config,
    };
    cosmic::applet::run::<Window>(true, flags)
}
