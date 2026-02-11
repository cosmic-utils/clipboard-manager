use std::{
    num::{NonZero, NonZeroU32},
    sync::atomic::AtomicBool,
    time::Duration,
};

// #[cfg(test)]
// use configurator_schema::schemars;
// #[cfg(test)]
// use configurator_schema::schemars::JsonSchema;

use cosmic::{
    cosmic_config::{self, CosmicConfigEntry, cosmic_config_derive::CosmicConfigEntry},
    iced::Subscription,
};

use serde::{Deserialize, Serialize};

use crate::{app::APPID, message::AppMsg};

pub const CONFIG_VERSION: u64 = 3;

#[derive(CosmicConfigEntry, Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
// #[cfg_attr(test, derive(JsonSchema))]
#[serde(default)]
pub struct Config {
    /// Disable the clipboard manager
    pub private_mode: bool,
    /// In second
    pub maximum_entries_lifetime: Option<u64>,
    pub maximum_entries_number: Option<u32>,
    /// Enable horizontal layout
    pub horizontal: bool,
    /// Reset the database at each login
    pub unique_session: bool,
    /// Enable the selection buffer (replaces sync_primary_selection)
    pub selection_buffer_enabled: bool,
    /// When selection buffer is enabled, also copy selected text to clipboard via wl-copy
    pub selection_buffer_sync_clipboard: bool,
    /// Maximum entries in the selection buffer
    pub selection_buffer_max_entries: u32,
    pub maximum_entries_by_page: NonZeroU32,
    pub preferred_mime_types: Vec<String>,
}

pub static PRIVATE_MODE: AtomicBool = AtomicBool::new(false);
pub static SELECTION_BUFFER_ENABLED: AtomicBool = AtomicBool::new(false);
/// Set to true just before wl-copy from primary selection sync.
/// The regular clipboard handler checks this flag and skips DB insert if set.
pub static SKIP_NEXT_CLIPBOARD: AtomicBool = AtomicBool::new(false);

impl Config {
    pub fn maximum_entries_lifetime(&self) -> Option<Duration> {
        self.maximum_entries_lifetime
            .map(|s| Duration::from_secs(s * 24 * 60 * 60))
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            private_mode: false,
            maximum_entries_lifetime: Some(30), // 30 days,
            maximum_entries_number: Some(500),
            horizontal: false,
            unique_session: false,
            selection_buffer_enabled: false,
            selection_buffer_sync_clipboard: true,
            selection_buffer_max_entries: 1000,
            maximum_entries_by_page: NonZero::new(50).unwrap(),
            preferred_mime_types: Vec::new(),
        }
    }
}

pub fn sub() -> Subscription<AppMsg> {
    struct ConfigSubscription;

    cosmic_config::config_subscription(
        std::any::TypeId::of::<ConfigSubscription>(),
        APPID.into(),
        CONFIG_VERSION,
    )
    .map(|update| {
        if !update.errors.is_empty() {
            error!("can't load config {:?}: {:?}", update.keys, update.errors);
        }
        AppMsg::ChangeConfig(update.config)
    })
}

// #[cfg(test)]
// mod test {
//     use std::fs;

//     use configurator_schema::ConfigFormat;

//     use crate::app::APPID;

//     use super::{CONFIG_VERSION, Config};

//     #[test]
//     fn gen_schema() {
//         let string = configurator_schema::gen_schema::<Config>()
//             .format(ConfigFormat::CosmicRon)
//             .source_home_path(&format!(".config/cosmic/{}/v{}", APPID, CONFIG_VERSION))
//             .call()
//             .unwrap();

//         fs::write("res/config_schema.json", &string).unwrap();
//     }
// }
