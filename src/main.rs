// #![allow(dead_code)]
// #![allow(unused_macros)]
// #![allow(unused_imports)]

use app::{AppState, Flags};
use config::{CONFIG_VERSION, Config};
use cosmic::cosmic_config;
use cosmic::cosmic_config::CosmicConfigEntry;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

mod app;
mod clipboard;
mod config;
mod db;
mod icon;
mod localize;
mod message;
mod my_widget;
mod navigation;
mod utils;
mod view;

#[allow(unused_imports)]
#[macro_use]
extern crate tracing;

fn setup_logs() {
    let fmt_layer = fmt::layer().with_target(true);
    let filter_layer = EnvFilter::try_from_default_env().unwrap_or(EnvFilter::new(format!(
        "warn,{}=warn",
        env!("CARGO_CRATE_NAME")
    )));

    if let Ok(journal_layer) = tracing_journald::layer() {
        tracing_subscriber::registry()
            .with(filter_layer)
            .with(fmt_layer)
            .with(journal_layer)
            .init();
    } else {
        tracing_subscriber::registry()
            .with(filter_layer)
            .with(fmt_layer)
            .init();
    }
}

fn main() -> cosmic::iced::Result {
    localize::localize();

    setup_logs();

    let (config_handler, config) = match cosmic_config::Config::new(app::APPID, CONFIG_VERSION) {
        Ok(config_handler) => {
            let config = match Config::get_entry(&config_handler) {
                Ok(ok) => ok,
                Err((errs, config)) => {
                    error!("errors loading config: {:?}", errs);
                    config
                }
            };
            (config_handler, config)
        }
        Err(err) => {
            error!("failed to create config handler: {}", err);
            panic!();
        }
    };

    let flags = Flags {
        config_handler,
        config,
    };
    cosmic::applet::run::<AppState<db::DbSqlite>>(flags)
}
