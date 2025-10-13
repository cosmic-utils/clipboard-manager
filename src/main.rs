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
mod clipboard_watcher;
mod config;
mod db;
mod icon;
mod localize;
mod message;
mod my_widget;
mod navigation;
mod toggle_signal;
mod utils;
mod view;

#[allow(unused_imports)]
#[macro_use]
extern crate tracing;

fn setup_logs() {
    let fmt_layer = fmt::layer().with_target(true);

    #[cfg(debug_assertions)]
    let default_level = "info";
    #[cfg(not(debug_assertions))]
    let default_level = "warn";

    let filter_layer = EnvFilter::try_from_default_env().unwrap_or(EnvFilter::new(format!(
        "{},{}={}",
        default_level,
        env!("CARGO_CRATE_NAME"),
        default_level
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

fn main() {
    for arg in std::env::args().skip(1) {
        if arg == "-V" || arg == "--version" {
            let version = env!("CARGO_PKG_VERSION");
            let commit = option_env!("CLIPBOARD_MANAGER_COMMIT").unwrap_or("unknown");

            println!("clipboard-manager {version} (commit {commit})");
            return;
        }

        if arg == "--toggle" || arg == "-t" {
            if let Err(e) = toggle_signal::send_toggle_signal() {
                error!("Failed to toggle clipboard manager: {}", e);
                std::process::exit(1);
            }
            return;
        }

        if arg == "-h" || arg == "--help" {
            println!("COSMIC Clipboard Manager");
            println!();
            println!("USAGE:");
            println!("    cosmic-ext-applet-clipboard-manager [OPTIONS]");
            println!();
            println!("OPTIONS:");
            println!("    -t, --toggle     Toggle the clipboard manager popup");
            println!("    -V, --version    Print version information");
            println!("    -h, --help       Print this help message");
            println!();
            println!("KEYBOARD SHORTCUT SETUP:");
            println!("    1. Open COSMIC Settings → Keyboard → Custom Shortcuts");
            println!("    2. Click 'Add Custom Shortcut'");
            println!("    3. Name: Clipboard Manager");
            println!("    4. Command: cosmic-ext-applet-clipboard-manager --toggle");
            println!("    5. Shortcut: Press Super+V (or your preferred shortcut)");
            return;
        }
    }

    localize::localize();

    setup_logs();

    if let Err(e) = toggle_signal::ensure_file_exist() {
        error!("ensure toggle file exist {e}")
    }

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

    if let Err(e) = cosmic::applet::run::<AppState<db::DbSqlite>>(flags) {
        error!("{e}");
        panic!();
    }
}
