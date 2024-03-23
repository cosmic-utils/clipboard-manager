use cosmic::cosmic_config::{self, cosmic_config_derive::CosmicConfigEntry, CosmicConfigEntry};

use serde::{Deserialize, Serialize};

pub const CONFIG_VERSION: u64 = 1;

#[derive(CosmicConfigEntry, Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct Config {}
