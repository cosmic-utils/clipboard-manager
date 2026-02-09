use std::sync::{Arc, Mutex};

use crate::{
    clipboard::ClipboardMessage,
    config::Config,
    db::{DbMessage, EntryId, MimeDataMap},
    editor_ipc::EditorToApp,
    ipc::EntrySummary,
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
    CopySpecial(MimeDataMap),
    Clear,
    Navigation(EventMsg),
    Db(DbMessage),
    ReturnToClipboard,
    Config(ConfigMsg),
    NextPage,
    PreviousPage,
    ContextMenu(ContextMenuMsg),
    LinkClicked(markdown::Url),
    DbusToggle,
    DbusListEntries {
        reply: Arc<Mutex<Option<tokio::sync::oneshot::Sender<Vec<EntrySummary>>>>>,
    },
    DbusCopyEntry {
        id: EntryId,
        reply: Arc<Mutex<Option<tokio::sync::oneshot::Sender<Result<(), String>>>>>,
    },
    DbusGetEntry {
        id: EntryId,
        reply: Arc<Mutex<Option<tokio::sync::oneshot::Sender<Result<(String, Vec<u8>), String>>>>>,
    },
    EditLatest,
    EditorEvent(EditorToApp),
    EditorProcessExited,
    DbusFavorites,
    DbusListFavorites {
        reply: Arc<Mutex<Option<tokio::sync::oneshot::Sender<Vec<FavoriteSummary>>>>>,
    },
    BeginFavorite(EntryId),
    CancelFavorite,
    ConfirmFavorite(EntryId, Option<String>),
    #[allow(dead_code)]
    SuggestTitle(EntryId),
    TitleSuggested(EntryId, Option<String>),
    SetFavoriteTitle(EntryId, String),
    FavoriteTitleInput(String),
}

/// Summary of a favorite entry for CLI listing.
#[derive(Clone, Debug)]
pub struct FavoriteSummary {
    pub id: i64,
    pub title: Option<String>,
    pub preview: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Copy)]
pub enum ContextMenuMsg {
    RemoveFavorite(EntryId),
    AddFavorite(EntryId),
    Edit(EntryId),
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
