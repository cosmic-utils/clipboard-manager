use cosmic::iced::window::Id;

use crate::{clipboard::{self, ClipboardMessage}, config::Config, db::Data};




// todo: filter data in update
#[derive(Clone, Debug)]
pub enum AppMessage {
    ChangeConfig(Config),
    TogglePopup,
    PopupClosed(Id),
    Query(String),
    ClipboardEvent(ClipboardMessage),
    RetryConnectingClipboard,
    OnClick(Data),
    Delete(Data),
    Clear,
}