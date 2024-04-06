use cosmic::app::{command, Core};

use cosmic::iced::advanced::subscription;
use cosmic::iced::wayland::popup::{destroy_popup, get_popup};
use cosmic::iced::window::Id;
use cosmic::iced::{self, event, Command, Limits};

use cosmic::iced_futures::Subscription;
use cosmic::iced_runtime::command::Action;
use cosmic::iced_runtime::core::window;
use cosmic::iced_style::application;
use cosmic::iced_widget::Column;
use cosmic::widget::{button, icon, text, text_input};

use cosmic::{Element, Theme};

use crate::config::{Config, CONFIG_VERSION};
use crate::db::{self, Data, Db};
use crate::message::AppMessage;
use crate::utils::command_message;
use crate::{clipboard, config, navigation};
use cosmic::cosmic_config;

pub const APP_ID: &str = "com.wiiznokes.CosmicClipboardManager";

pub struct Window {
    core: Core,
    config: Config,
    config_handler: Option<cosmic_config::Config>,
    popup: Option<Id>,
    state: AppState,
}

pub struct AppState {
    pub db: Db,
    pub clipboard_state: ClipboardState,
    pub focused: usize,
}

impl AppState {

}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipboardState {
    Init,
    Connected,
    Error(String),
}

impl ClipboardState {
    pub fn is_error(&self) -> bool {
        if let ClipboardState::Error(..) = self {
            true
        } else {
            false
        }
    }
}

#[derive(Clone, Debug)]
pub struct Flags {
    pub config_handler: Option<cosmic_config::Config>,
    pub config: Config,
}

impl cosmic::Application for Window {
    type Executor = cosmic::executor::Default;
    type Flags = Flags;
    type Message = AppMessage;
    const APP_ID: &'static str = APP_ID;

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(
        core: Core,
        flags: Self::Flags,
    ) -> (Self, cosmic::Command<cosmic::app::Message<Self::Message>>) {
        let window = Window {
            core,
            config: flags.config,
            config_handler: flags.config_handler,
            popup: None,
            state: AppState {
                db: db::Db::new().unwrap(),
                clipboard_state: ClipboardState::Init,
                focused: 0,
            },
        };

        #[cfg(debug_assertions)]
        let command = Command::single(Action::Future(Box::pin(async {
            cosmic::app::Message::App(AppMessage::TogglePopup)
        })));

        #[cfg(not(debug_assertions))]
        let command = Command::none();

        (window, command)
    }

    fn on_close_requested(&self, id: window::Id) -> Option<AppMessage> {
        println!("on_close_requested");
        Some(AppMessage::ClosePopup(id))
    }

    fn update(&mut self, message: Self::Message) -> Command<cosmic::app::Message<Self::Message>> {
        //dbg!(&message);

        macro_rules! config_set {
            ($name: ident, $value: expr) => {
                match &self.config_handler {
                    Some(config_handler) => {
                        match paste::paste! { self.config.[<set_ $name>](config_handler, $value) } {
                            Ok(_) => {}
                            Err(err) => {
                                eprintln!("failed to save config {:?}: {}", stringify!($name), err);
                            }
                        }
                    }
                    None => {
                        self.config.$name = $value;
                        eprintln!(
                            "failed to save config {:?}: no config handler",
                            stringify!($name),
                        );
                    }
                }
            };
        }

        match message {
            AppMessage::ChangeConfig(config) => {
                if config != self.config {
                    self.config = config
                }
            }
            AppMessage::TogglePopup => {
                //info!("TogglePopup");

                return if let Some(p) = self.popup.take() {
                    destroy_popup(p)
                } else {
                    let new_id = Id::unique();
                    self.popup.replace(new_id);
                    let mut popup_settings =
                        self.core
                            .applet
                            .get_popup_settings(Id::MAIN, new_id, None, None, None);
                    popup_settings.positioner.size_limits = Limits::NONE
                        .max_width(500.0)
                        .min_width(300.0)
                        .min_height(200.0)
                        .max_height(550.0);
                    get_popup(popup_settings)
                };
            }
            AppMessage::ClosePopup(id) => {
                //info!("PopupClosed: {id:?}");
                if self.popup.as_ref() == Some(&id) {
                    self.popup = None;
                }
            }
            AppMessage::Search(query) => {
                self.state.db.search(query);
            }
            AppMessage::ClipboardEvent(message) => match message {
                clipboard::ClipboardMessage::Connected => {
                    self.state.clipboard_state = ClipboardState::Connected;
                }
                clipboard::ClipboardMessage::Data(data) => {
                    if let Err(e) = self.state.db.insert(data) {
                        error!("can't insert data: {e}");
                    }
                }
                clipboard::ClipboardMessage::Error(e) => {
                    error!("{e}");
                    self.state.clipboard_state = ClipboardState::Error(e);
                }
            },
            AppMessage::OnClick(data) => {
                if let Err(e) = clipboard::copy(data) {
                    error!("can't copy: {e}");
                }
                return command_message(AppMessage::TogglePopup);
            }
            AppMessage::Delete(data) => {
                if let Err(e) = self.state.db.delete(&data) {
                    error!("can't delete {data}: {e}");
                }
            }
            AppMessage::Clear => {
                if let Err(e) = self.state.db.clear() {
                    error!("can't clear db: {e}");
                }
            }
            AppMessage::RetryConnectingClipboard => {
                self.state.clipboard_state = ClipboardState::Init;
            }
            AppMessage::Navigation(message) => match message {
                navigation::NavigationMessage::Down => return iced::widget::focus_next(),
                navigation::NavigationMessage::Up => {
                    return iced::widget::focus_previous();
                }
                navigation::NavigationMessage::Enter => {}
                navigation::NavigationMessage::Quit => {
                    return command_message(AppMessage::TogglePopup)
                }
            },
        }
        Command::none()
    }

    fn view(&self) -> Element<Self::Message> {
        let icon_bytes = include_bytes!("../resources/icons/assignment24.svg") as &[u8];
        let icon = icon::from_svg_bytes(icon_bytes);

        self.core
            .applet
            .icon_button_with_handle(icon)
            .on_press(AppMessage::TogglePopup)
            .into()
    }

    fn view_window(&self, _id: Id) -> Element<Self::Message> {
        self.core.applet.popup_container(self.state.view()).into()
    }
    fn subscription(&self) -> Subscription<Self::Message> {
        let mut subscriptions = vec![config::sub(), navigation::sub().map(AppMessage::Navigation)];

        if !self.state.clipboard_state.is_error() {
            subscriptions.push(clipboard::sub().map(AppMessage::ClipboardEvent));
        }

        Subscription::batch(subscriptions)
    }

    fn style(&self) -> Option<<Theme as application::StyleSheet>::Style> {
        Some(cosmic::applet::style())
    }
}