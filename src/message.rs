use crate::{
    clipboard::ClipboardMessage,
    config::Config,
    db::{DbMessage, EntryId},
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
    Clear,
    Navigation(EventMsg),
    Db(DbMessage),
    ReturnToClipboard,
    Config(ConfigMsg),
    NextPage,
    PreviousPage,
    ContextMenu(ContextMenuMsg),
    LinkClicked(markdown::Url),
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
