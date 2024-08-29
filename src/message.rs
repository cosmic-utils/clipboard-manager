use cosmic::iced::window::Id;

use crate::{
    clipboard::{self, ClipboardMessage},
    config::Config,
    db::{self, DbMessage, Entry},
    navigation::NavigationMessage,
};

#[derive(Clone, Debug)]
pub enum AppMsg {
    ChangeConfig(Config),
    TogglePopup,
    ToggleQuickSettings,
    ClosePopup,
    Search(String),
    ClipboardEvent(ClipboardMessage),
    RetryConnectingClipboard,
    Copy(Entry),
    Delete(Entry),
    Clear,
    Navigation(NavigationMessage),
    Db(DbMessage),
    ShowQrCode(Entry),
    ReturnToClipboard,
    Config(ConfigMsg),
}

#[derive(Clone, Debug)]
pub enum ConfigMsg {
    PrivateMode(bool),
    Horizontal(bool),
}
