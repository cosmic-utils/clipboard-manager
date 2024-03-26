use cosmic::app::Core;

use cosmic::iced::wayland::popup::{destroy_popup, get_popup};
use cosmic::iced::window::Id;
use cosmic::iced::{Command, Limits};

use cosmic::iced_futures::Subscription;
use cosmic::iced_runtime::command::Action;
use cosmic::iced_runtime::core::window;
use cosmic::iced_style::application;
use cosmic::iced_widget::Column;
use cosmic::widget::{button, text, text_input};

use cosmic::{Element, Theme};

use crate::clipboard;
use crate::config::{Config, CONFIG_VERSION};
use crate::db::{self, Data, Db};
use crate::message::AppMessage;
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
    pub query: String,
    pub db: Db,
    pub clipboard_state: ClipboardState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipboardState {
    Init,
    Connected,
    Error,
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
                query: "".to_string(),
                db: db::Db::new().unwrap(),
                clipboard_state: ClipboardState::Init,
            }
        };

        let command = Command::single(Action::Future(Box::pin(async {
            cosmic::app::Message::App(AppMessage::TogglePopup)
        })));

        // let command = Command::none();

        (window, command)
    }

    fn on_close_requested(&self, id: window::Id) -> Option<AppMessage> {
        Some(AppMessage::PopupClosed(id))
    }

    fn update(&mut self, message: Self::Message) -> Command<cosmic::app::Message<Self::Message>> {
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
            AppMessage::PopupClosed(id) => {
                //info!("PopupClosed: {id:?}");
                if self.popup.as_ref() == Some(&id) {
                    self.popup = None;
                }
            }
            AppMessage::Query(query) => {
                self.state.query = query;
            }
            AppMessage::ClipboardEvent(message) => {
                match message {
                    clipboard::ClipboardMessage::Connected => {
                        self.state.clipboard_state = ClipboardState::Error;
                    }
                    clipboard::ClipboardMessage::Data(data) => {
                        if let Err(e) = self.state.db.insert(data) {
                            error!("can't insert data: {e}");
                        }
                    }
                    clipboard::ClipboardMessage::Error(_e) => {
                        // todo: print error, or desc on the icon
                        self.state.clipboard_state = ClipboardState::Error;
                    }
                }
            }
            AppMessage::OnClick(data) => {
                if let Err(e) = clipboard::copy(data) {
                    error!("can't copy: {e}");
                }
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
            },
        }
        Command::none()
    }

    fn view(&self) -> Element<Self::Message> {
        button(text("Clipboard").size(14.0))
            .style(cosmic::theme::Button::AppletIcon)
            .on_press(AppMessage::TogglePopup)
            .into()
    }

    fn view_window(&self, _id: Id) -> Element<Self::Message> {
        self.core.applet.popup_container(self.state.view()).into()
    }
    fn subscription(&self) -> Subscription<Self::Message> {
        let mut subscriptions = Vec::new();

        struct ConfigSubscription;
        let config = cosmic_config::config_subscription(
            std::any::TypeId::of::<ConfigSubscription>(),
            Self::APP_ID.into(),
            CONFIG_VERSION,
        )
        .map(|update| {
            if !update.errors.is_empty() {
                eprintln!(
                    "errors loading config {:?}: {:?}",
                    update.keys, update.errors
                );
            }
            AppMessage::ChangeConfig(update.config)
        });

        subscriptions.push(config);

        if self.state.clipboard_state != ClipboardState::Error {
            subscriptions.push(clipboard::sub().map(AppMessage::ClipboardEvent));
        }

        Subscription::batch(subscriptions)
    }

    fn style(&self) -> Option<<Theme as application::StyleSheet>::Style> {
        Some(cosmic::applet::style())
    }
}
