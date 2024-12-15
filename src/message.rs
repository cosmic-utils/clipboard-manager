use crate::{
    clipboard::ClipboardMessage,
    config::Config,
    db::{DbMessage, EntryId, EntryTrait},
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
    Copy(EntryId),
    Delete(EntryId),
    Clear,
    Navigation(EventMsg),
    Db(DbMessage),
    ShowQrCode(EntryId),
    ReturnToClipboard,
    Config(ConfigMsg),
    AddFavorite(EntryId),
    RemoveFavorite(EntryId),
    NextPage,
    PreviousPage,
}

#[derive(Clone, Debug)]
pub enum ConfigMsg {
    PrivateMode(bool),
    Horizontal(bool),
    UniqueSession(bool),
}
