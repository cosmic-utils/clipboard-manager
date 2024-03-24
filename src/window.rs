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
use crate::view::windows_view;
use cosmic::cosmic_config;

pub const APP_ID: &str = "com.wiiznokes.CosmicClipboardManager";

pub struct Window {
    core: Core,
    config: Config,
    config_handler: Option<cosmic_config::Config>,
    popup: Option<Id>,

    query: String,
    db: Db,
}

#[derive(Clone, Debug)]
pub enum Message {
    Config(Config),
    TogglePopup,
    PopupClosed(Id),
    Query(String),
    ClipBoardEvent(Data),
    OnClick(Data),
    Delete(Data),
    Clear,
}

#[derive(Clone, Debug)]
pub struct Flags {
    pub config_handler: Option<cosmic_config::Config>,
    pub config: Config,
}

impl cosmic::Application for Window {
    type Executor = cosmic::executor::Default;
    type Flags = Flags;
    type Message = Message;
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
            query: "".to_string(),
            db: db::Db::new().unwrap(),
        };

        let command = Command::single(Action::Future(Box::pin(async {
            cosmic::app::Message::App(Message::TogglePopup)
        })));

        //let command = Command::none();

        (window, command)
    }

    fn on_close_requested(&self, id: window::Id) -> Option<Message> {
        Some(Message::PopupClosed(id))
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
            Message::Config(config) => {
                if config != self.config {
                    self.config = config
                }
            }
            Message::TogglePopup => {
                info!("TogglePopup");

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
            Message::PopupClosed(id) => {
                info!("PopupClosed: {id:?}");
                if self.popup.as_ref() == Some(&id) {
                    self.popup = None;
                }
            }
            Message::Query(query) => {
                self.query = query;
            }
            Message::ClipBoardEvent(data) => {
                if let Err(e) = self.db.insert(data) {
                    error!("can't insert data: {e}");
                }
            }
            Message::OnClick(_data) => {
                // todo
            }
            Message::Delete(data) => {
                if let Err(e) = self.db.delete(&data) {
                    error!("can't delete {data}: {e}");
                }
            }
            Message::Clear => {
                if let Err(e) = self.db.clear() {
                    error!("can't clear db: {e}");
                }
            }
        }
        Command::none()
    }

    fn view(&self) -> Element<Self::Message> {
        button(text("Clipboard Manager").size(14.0))
            .style(cosmic::theme::Button::AppletIcon)
            .on_press(Message::TogglePopup)
            .into()
    }

    fn view_window(&self, _id: Id) -> Element<Self::Message> {
        let entries = self.db.iter().rev();

        let content = windows_view(&self.query, entries);
        self.core.applet.popup_container(content).into()
    }
    fn subscription(&self) -> Subscription<Self::Message> {
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
            Message::Config(update.config)
        });

        let clipboard = clipboard::sub().map(Message::ClipBoardEvent);

        Subscription::batch(vec![config, clipboard])
    }

    fn style(&self) -> Option<<Theme as application::StyleSheet>::Style> {
        Some(cosmic::applet::style())
    }
}
