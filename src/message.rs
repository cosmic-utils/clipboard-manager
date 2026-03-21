use crate::{
    clipboard::ClipboardMessage,
    config::Config,
    db::{DbMessage, EntryId, MimeDataMap},
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
    #[expect(dead_code)]
    RetryConnectingClipboard,
    Copy(EntryId),
    #[expect(dead_code)]
    CopySpecial(MimeDataMap),
    Clear,
    Navigation(EventMsg),
    Db(DbMessage),
    ReturnToClipboard,
    Config(ConfigMsg),
    NextPage,
    PreviousPage,
    ContextMenu(ContextMenuMsg),
    #[expect(dead_code)]
    LinkClicked(markdown::Uri),
}

#[derive(Clone, Debug, PartialEq, Eq, Copy)]
pub enum ContextMenuMsg {
    RemoveFavorite(EntryId),
    AddFavorite(EntryId),
    ShowQrCode(EntryId),
    Delete(EntryId),
}

use cosmic::widget::{markdown, menu::action::MenuAction};

impl MenuAction for ContextMenuMsg {
    type Message = AppMsg;

    fn message(&self) -> Self::Message {
        AppMsg::ContextMenu(*self)
    }
}

#[derive(Clone, Debug)]
pub enum ConfigMsg {
    PrivateMode(bool),
    #[expect(dead_code)]
    Horizontal(bool),
    UniqueSession(bool),
}
