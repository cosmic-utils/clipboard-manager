use cosmic::iced::window::Id;

use crate::{
    clipboard::{self, ClipboardMessage},
    config::Config,
    db::Data,
    navigation::NavigationMessage,
};

#[derive(Clone, Debug)]
pub enum AppMessage {
    ChangeConfig(Config),
    TogglePopup,
    ToggleQuickSettings,
    ClosePopup,
    ActivateWaylandPopup,
    CloseWaylandPopup,
    Search(String),
    ClipboardEvent(ClipboardMessage),
    RetryConnectingClipboard,
    Copy(Data),
    Delete(Data),
    PrivateMode(bool),
    Clear,
    Navigation(NavigationMessage),
    MoreAction(Option<Data>),
}
