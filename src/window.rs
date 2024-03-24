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

use crate::config::{Config, CONFIG_VERSION};
use cosmic::cosmic_config;

pub const APP_ID: &str = "com.wiiznokes.CosmicClipboardManager";

pub struct Window {
    core: Core,
    config: Config,
    config_handler: Option<cosmic_config::Config>,
    popup: Option<Id>,
    query: String,
}

#[derive(Clone, Debug)]
pub enum Message {
    Config(Config),
    TogglePopup,
    PopupClosed(Id),
    Query(String),
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
                        .max_height(1080.0);
                    get_popup(popup_settings)
                }
            }
            Message::PopupClosed(id) => {
                if self.popup.as_ref() == Some(&id) {
                    self.popup = None;
                }
            }
            Message::Query(query) => {
                self.query = query;
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
        let text_intput = text_input("value", &self.query)
            .on_clear(Message::Query("".to_string()))
            .on_input(Message::Query);

        let values_text = DATA.map(|data| text(data).into());

        let values = Column::with_children(values_text);

        let content_list = vec![text_intput.into(), values.into()];

        let content = Column::with_children(content_list);

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

        Subscription::batch(vec![config])
    }

    fn style(&self) -> Option<<Theme as application::StyleSheet>::Style> {
        Some(cosmic::applet::style())
    }
}

static DATA: [&str; 2] = ["hello", "world"];
