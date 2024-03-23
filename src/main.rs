#![allow(dead_code)]
#![allow(unused_macros)]
#![allow(unused_imports)]

use config::{Config, CONFIG_VERSION};
use cosmic::cosmic_config;
use cosmic::cosmic_config::CosmicConfigEntry;
use window::{Flags, Window};

mod config;
mod localize;
mod window;

fn main() -> cosmic::iced::Result {
    localize::localize();
    env_logger::init();

    let (config_handler, config) = match cosmic_config::Config::new(window::APP_ID, CONFIG_VERSION)
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
