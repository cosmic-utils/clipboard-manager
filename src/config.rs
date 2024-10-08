use std::{sync::atomic::AtomicBool, time::Duration};

use cosmic::{
    cosmic_config::{self, cosmic_config_derive::CosmicConfigEntry, CosmicConfigEntry},
    iced::Subscription,
};

use serde::{Deserialize, Serialize};

use crate::{app::APPID, message::AppMsg, utils};

pub const CONFIG_VERSION: u64 = 2;

#[derive(CosmicConfigEntry, Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub struct Config {
    pub private_mode: bool,
    /// In second
    pub maximum_entries_lifetime: Option<u64>,
    pub maximum_entries_number: Option<u32>,
    pub horizontal: bool,
    pub unique_session: bool,
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
