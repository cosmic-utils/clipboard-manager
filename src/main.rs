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
mod editor_app;
mod editor_ipc;
mod icon;
mod ipc;
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

fn print_help() {
    println!("COSMIC Clipboard Manager");
    println!();
    println!("USAGE:");
    println!("    clipboard-manager [OPTIONS]");
    println!();
    println!("OPTIONS:");
    println!("    -t, --toggle       Toggle the clipboard manager popup");
    println!("    -e, --edit         Open editor with latest text entry");
    println!("    -l, --list         List clipboard history to stdout");
    println!("    -c, --copy ID      Copy entry with given ID to clipboard");
    println!("    -g, --get ID       Output raw entry content to stdout");
    println!("    -V, --version      Print version information");
    println!("    -h, --help         Print this help message");
    println!();
    println!("EXAMPLES:");
    println!("    # Interactive terminal picker");
    println!("    clipboard-manager --list | fzf | cut -f1 | tr -d '* ' | xargs clipboard-manager --copy");
    println!();
    println!("    # Graphical picker");
    println!("    clipboard-manager --list | rofi -dmenu -p Clipboard | cut -f1 | tr -d '* ' | xargs clipboard-manager --copy");
    println!();
    println!("    # Preview an image entry");
    println!("    clipboard-manager --get 1738857400000 | feh -");
    println!("    clipboard-manager --get 1738857400000 | kitty +kitten icat");
    println!();
    println!("LIST OUTPUT FORMAT:");
    println!("    *1738857600000\\tImportant pinned note...");
    println!("     1738857500000\\tgit commit -m \"Fix the thing\"");
    println!("     1738857400000\\t[image]");
    println!();
    println!("    * prefix = favorite, space = normal");
    println!("    Tab-separated: {{star}}{{id}}\\t{{preview}}");
    println!();
    println!("KEYBOARD SHORTCUT SETUP:");
    println!("    1. Open COSMIC Settings > Keyboard > Custom Shortcuts");
    println!("    2. Click 'Add Custom Shortcut'");
    println!("    3. Name: Clipboard Manager");
    println!("    4. Command: cosmic-ext-applet-clipboard-manager --toggle");
    println!("    5. Shortcut: Press Super+V (or your preferred shortcut)");
    println!();
    println!("    For quick edit (opens editor with latest text entry):");
    println!("    Command: cosmic-ext-applet-clipboard-manager --edit");
    println!("    Shortcut: Press Super+Shift+V (or your preferred shortcut)");
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < args.len() {
        let arg = &args[i];

        if arg == "--editor-window" {
            localize::localize();
            setup_logs();
            editor_app::run_editor();
            return;
        }

        if arg == "-V" || arg == "--version" {
            let version = env!("CARGO_PKG_VERSION");
            let commit = option_env!("CLIPBOARD_MANAGER_COMMIT").unwrap_or("unknown");

            println!("clipboard-manager {version} (commit {commit})");
            return;
        }

        if arg == "--toggle" || arg == "-t" {
            if let Err(e) = ipc::send_toggle() {
                eprintln!("Failed to toggle clipboard manager: {e}");
                std::process::exit(1);
            }
            return;
        }

        if arg == "--edit" || arg == "-e" {
            if let Err(e) = ipc::send_edit_latest() {
                eprintln!("Failed to open clipboard editor: {e}");
                std::process::exit(1);
            }
            return;
        }

        if arg == "--list" || arg == "-l" {
            match ipc::send_list_entries() {
                Ok(entries) => {
                    for (id, is_fav, preview) in entries {
                        let star = if is_fav { '*' } else { ' ' };
                        println!("{star}{id}\t{preview}");
                    }
                }
                Err(e) => {
                    eprintln!("Failed to list clipboard entries: {e}");
                    eprintln!("Is the clipboard manager applet running?");
                    std::process::exit(1);
                }
            }
            return;
        }

        if arg == "--copy" || arg == "-c" {
            i += 1;
            if i >= args.len() {
                eprintln!("--copy requires an entry ID argument");
                std::process::exit(1);
            }
            let id: i64 = match args[i].parse() {
                Ok(id) => id,
                Err(_) => {
                    eprintln!("Invalid entry ID: {}", args[i]);
                    std::process::exit(1);
                }
            };
            if let Err(e) = ipc::send_copy_entry(id) {
                eprintln!("Failed to copy entry: {e}");
                std::process::exit(1);
            }
            return;
        }

        if arg == "--get" || arg == "-g" {
            i += 1;
            if i >= args.len() {
                eprintln!("--get requires an entry ID argument");
                std::process::exit(1);
            }
            let id: i64 = match args[i].parse() {
                Ok(id) => id,
                Err(_) => {
                    eprintln!("Invalid entry ID: {}", args[i]);
                    std::process::exit(1);
                }
            };
            match ipc::send_get_entry(id) {
                Ok((_mime, data)) => {
                    use std::io::Write;
                    let stdout = std::io::stdout();
                    let mut handle = stdout.lock();
                    if let Err(e) = handle.write_all(&data) {
                        eprintln!("Failed to write to stdout: {e}");
                        std::process::exit(1);
                    }
                }
                Err(e) => {
                    eprintln!("Failed to get entry: {e}");
                    std::process::exit(1);
                }
            }
            return;
        }

        if arg == "-h" || arg == "--help" {
            print_help();
            return;
        }

        i += 1;
    }

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

    if let Err(e) = cosmic::applet::run::<AppState<db::DbSqlite>>(flags) {
        error!("{e}");
        panic!();
    }
}
