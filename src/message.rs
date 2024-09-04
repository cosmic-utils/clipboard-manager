use cosmic::iced::window::Id;
use freedesktop_desktop_entry::DesktopEntry;

use crate::{
    clipboard::{self, ClipboardMessage},
    config::Config,
    db::{self, DbMessage, Entry},
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
    RetryConnectingClipboard,
    Copy(Entry),
    Delete(Entry),
    Clear,
    Navigation(EventMsg),
    Db(DbMessage),
    ShowQrCode(Entry),
    ReturnToClipboard,
    Config(ConfigMsg),
    Open(Entry),
    OpenWith {
        entry: Entry,
        desktop_entry: DesktopEntry<'static>,
    },

}

#[derive(Clone, Debug)]
pub enum ConfigMsg {
    PrivateMode(bool),
    Horizontal(bool),
}
