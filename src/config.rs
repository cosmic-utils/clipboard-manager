use std::sync::atomic::AtomicBool;

use cosmic::{
    cosmic_config::{self, cosmic_config_derive::CosmicConfigEntry, CosmicConfigEntry},
    iced::Subscription,
};

use serde::{Deserialize, Serialize};
use time::Duration;

use crate::{app::APP_ID, message::AppMessage, utils};

pub const CONFIG_VERSION: u64 = 1;

#[derive(CosmicConfigEntry, Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub struct Config {
    pub private_mode: bool,
    pub remove_old_entries: Option<Duration>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            private_mode: false,
            remove_old_entries: Some(time::Duration::days(30)),
        }
    }
}

pub static PRIVATE_MODE: AtomicBool = AtomicBool::new(false);

pub fn sub() -> Subscription<AppMessage> {
    struct ConfigSubscription;

    cosmic_config::config_subscription(
        std::any::TypeId::of::<ConfigSubscription>(),
        APP_ID.into(),
        CONFIG_VERSION,
    )
    .map(|update| {
        if !update.errors.is_empty() {
            error!("can't load config {:?}: {:?}", update.keys, update.errors);
        }
        AppMessage::ChangeConfig(update.config)
    })
}
