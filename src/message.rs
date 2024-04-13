use cosmic::iced::window::Id;

use crate::{
    clipboard::{self, ClipboardMessage},
    config::Config,
    db::Data,
    navigation::NavigationMessage,
};

// todo: filter data in update
#[derive(Clone, Debug)]
pub enum AppMessage {
    ChangeConfig(Config),
    TogglePopup,
    QuickSettings,
    ClosePopup(Id),
    Search(String),
    ClipboardEvent(ClipboardMessage),
    RetryConnectingClipboard,
    OnClick(Data),
    Delete(Data),
    PrivateMode(bool),
    Clear,
    Navigation(NavigationMessage),
    MoreAction(Option<Data>),
}
