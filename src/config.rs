use std::{
    num::{NonZero, NonZeroU32},
    sync::atomic::AtomicBool,
    time::Duration,
};

#[cfg(test)]
use configurator_schema::schemars;
#[cfg(test)]
use configurator_schema::schemars::JsonSchema;

use cosmic::{
    cosmic_config::{self, cosmic_config_derive::CosmicConfigEntry, CosmicConfigEntry},
    iced::Subscription,
};

use serde::{Deserialize, Serialize};

use crate::{app::APPID, message::AppMsg};

pub const CONFIG_VERSION: u64 = 3;

#[derive(CosmicConfigEntry, Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[cfg_attr(test, derive(JsonSchema))]
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
    pub maximum_entries_by_page: NonZeroU32,
}

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
            maximum_entries_by_page: NonZero::new(50).unwrap(),
        }
    }
}

pub static PRIVATE_MODE: AtomicBool = AtomicBool::new(false);

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

#[cfg(test)]
mod test {
    use std::fs;

    use configurator_schema::ConfigFormat;

    use crate::app::APPID;

    use super::{Config, CONFIG_VERSION};

    #[test]
    fn gen_schema() {
        let string = configurator_schema::gen_schema::<Config>()
            .format(ConfigFormat::CosmicRon)
            .source_home_path(&format!(".config/cosmic/{}/v{}", APPID, CONFIG_VERSION))
            .call()
            .unwrap();

        fs::write("res/config_schema.json", &string).unwrap();
    }
}
