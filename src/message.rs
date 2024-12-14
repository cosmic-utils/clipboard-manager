use crate::{
    clipboard::ClipboardMessage,
    config::Config,
    db::{DbMessage, Entry},
    navigation::EventMsg,
};

#[derive(Clone, Debug)]
pub enum AppMsg {
    ChangeConfig(Config),
    TogglePopup,
    ToggleQuickSettings,
    ClosePopup,
    Search(String),
    ClipboardEvent(ClipboardMessage),
    #[allow(dead_code)]
    RetryConnectingClipboard,
    Copy(Entry),
    Delete(Entry),
    Clear,
    Navigation(EventMsg),
    Db(DbMessage),
    ShowQrCode(Entry),
    ReturnToClipboard,
    Config(ConfigMsg),
    AddFavorite(Entry),
    RemoveFavorite(Entry),
    NextPage,
    PreviousPage,
}

#[derive(Clone, Debug)]
pub enum ConfigMsg {
    PrivateMode(bool),
    Horizontal(bool),
    UniqueSession(bool),
}
