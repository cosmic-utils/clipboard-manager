use cosmic::app::{command, Core};

use cosmic::iced::advanced::subscription;
use cosmic::iced::wayland::actions::layer_surface::{IcedMargin, SctkLayerSurfaceSettings};
use cosmic::iced::wayland::layer_surface::{
    destroy_layer_surface, get_layer_surface, Anchor, KeyboardInteractivity,
};
use cosmic::iced::wayland::popup::{destroy_popup, get_popup};
use cosmic::iced::window::Id;
use cosmic::iced::{self, event, Command, Limits};

use cosmic::iced_futures::Subscription;
use cosmic::iced_runtime::command::Action;
use cosmic::iced_runtime::core::window;
use cosmic::iced_style::application;
use cosmic::iced_widget::{qr_code, Column};
use cosmic::widget::{button, icon, text, text_input, MouseArea};

use cosmic::{Element, Theme};
use futures::executor::block_on;

use crate::config::{Config, CONFIG_VERSION, PRIVATE_MODE};
use crate::db::{self, Db, Entry};
use crate::message::AppMessage;
use crate::utils::command_message;
use crate::view::{popup_view, quick_settings_view};
use crate::{clipboard, config, navigation};

use cosmic::cosmic_config;
use std::sync::atomic::{self, AtomicBool};
use std::thread;

pub const QUALIFIER: &str = "io.github";
pub const ORG: &str = "wiiznokes";
pub const APP: &str = "cosmic-ext-applet-clipboard-manager";
pub const APPID: &str = constcat::concat!(QUALIFIER, ".", ORG, ".", APP);

pub struct Window {
    core: Core,
    config: Config,
    config_handler: cosmic_config::Config,
    popup: Option<Popup>,
    state: AppState,
}

pub struct AppState {
    pub db: Db,
    pub clipboard_state: ClipboardState,
    pub focused: usize,
    pub qr_code: Option<Result<qr_code::State, ()>>,
}

impl AppState {
    fn focus_next(&mut self) {
        self.focused = (self.focused + 1) % self.db.len();
    }

    fn focus_previous(&mut self) {
        self.focused = (self.focused + self.db.len() - 1) % self.db.len();
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipboardState {
    Init,
    Connected,
    Error(String),
}

impl ClipboardState {
    pub fn is_error(&self) -> bool {
        matches!(self, ClipboardState::Error(..))
    }
}

#[derive(Clone, Debug)]
pub struct Flags {
    pub config_handler: cosmic_config::Config,
    pub config: Config,
}

#[derive(Debug, Clone)]
struct Popup {
    pub kind: PopupKind,
    pub id: window::Id,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum PopupKind {
    Popup,
    QuickSettings,
}

impl Window {
    fn toggle_popup(&mut self, kind: PopupKind) -> Command<cosmic::app::Message<AppMessage>> {
        self.state.qr_code.take();
        match &self.popup {
            Some(popup) => {
                if popup.kind == kind {
                    self.close_popup()
                } else {
                    Command::batch(vec![self.close_popup(), self.open_popup(kind)])
                }
            }
            None => self.open_popup(kind),
        }
    }

    fn close_popup(&mut self) -> Command<cosmic::app::Message<AppMessage>> {
        self.state.focused = 0;
        self.state.db.set_query_and_search("".into());

        if let Some(popup) = self.popup.take() {
            //info!("destroy {:?}", popup.id);

            if self.config.horizontal {
                destroy_layer_surface(popup.id)
            } else {
                destroy_popup(popup.id)
            }
        } else {
            Command::none()
        }
    }

    fn open_popup(&mut self, kind: PopupKind) -> Command<cosmic::app::Message<AppMessage>> {
        let new_id = Id::unique();
        //info!("will create {:?}", new_id);

        let popup = Popup { kind, id: new_id };
        self.popup.replace(popup);

        if self.config.horizontal {
            get_layer_surface(SctkLayerSurfaceSettings {
                id: new_id,
                keyboard_interactivity: KeyboardInteractivity::OnDemand,
                anchor: Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
                namespace: "clipboard manager".into(),
                size: Some((None, Some(350))),
                size_limits: Limits::NONE.min_width(1.0).min_height(1.0),
                ..Default::default()
            })
        } else {
            let mut popup_settings =
                self.core
                    .applet
                    .get_popup_settings(Id::MAIN, new_id, None, None, None);

            match kind {
                PopupKind::Popup => {
                    popup_settings.positioner.size_limits = Limits::NONE
                        .max_width(400.0)
                        .min_width(300.0)
                        .min_height(200.0)
                        .max_height(500.0);
                    get_popup(popup_settings)
                }
                PopupKind::QuickSettings => {
                    popup_settings.positioner.size_limits = Limits::NONE
                        .max_width(250.0)
                        .min_width(200.0)
                        .min_height(200.0)
                        .max_height(550.0);

                    get_popup(popup_settings)
                }
            }
        }
    }
}

impl cosmic::Application for Window {
    type Executor = cosmic::executor::Default;
    type Flags = Flags;
    type Message = AppMessage;
    const APP_ID: &'static str = APPID;

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
        let config = flags.config;
        PRIVATE_MODE.store(config.private_mode, atomic::Ordering::Relaxed);

        let db = block_on(async { db::Db::new(&config).await.unwrap() });

        let window = Window {
            core,
            config_handler: flags.config_handler,
            popup: None,
            state: AppState {
                db,
                clipboard_state: ClipboardState::Init,
                focused: 0,
                qr_code: None,
            },
            config,
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
        info!("on_close_requested");

        if let Some(popup) = &self.popup {
            if popup.id == id {
                return Some(AppMessage::ClosePopup);
            }
        }
        None
    }

    fn update(&mut self, message: Self::Message) -> Command<cosmic::app::Message<Self::Message>> {
        macro_rules! config_set {
            ($name: ident, $value: expr) => {
                match paste::paste! { self.config.[<set_ $name>](&self.config_handler, $value) } {
                    Ok(_) => {}
                    Err(err) => {
                        error!("failed to save config {:?}: {}", stringify!($name), err);
                    }
                }
            };
        }

        match message {
            AppMessage::ChangeConfig(config) => {
                if config != self.config {
                    PRIVATE_MODE.store(config.private_mode, atomic::Ordering::Relaxed);
                    self.config = config;
                }
            }
            AppMessage::ToggleQuickSettings => {
                return self.toggle_popup(PopupKind::QuickSettings);
            }

            AppMessage::TogglePopup => {
                return self.toggle_popup(PopupKind::Popup);
            }
            AppMessage::ClosePopup => return self.close_popup(),
            AppMessage::Search(query) => {
                self.state.db.set_query_and_search(query);
            }
            AppMessage::ClipboardEvent(message) => match message {
                clipboard::ClipboardMessage::Connected => {
                    self.state.clipboard_state = ClipboardState::Connected;
                }
                clipboard::ClipboardMessage::Data(data) => {
                    block_on(async {
                        if let Err(e) = self.state.db.insert(data).await {
                            error!("can't insert data: {e}");
                        }
                    });
                }
                clipboard::ClipboardMessage::Error(e) => {
                    error!("{e}");
                    self.state.clipboard_state = ClipboardState::Error(e);
                }
                clipboard::ClipboardMessage::EmptyKeyboard => {
                    if let Some(data) = self.state.db.get(0) {
                        if let Err(e) = clipboard::copy(data.to_owned()) {
                            error!("can't copy: {e}");
                        }
                    }
                }
            },
            AppMessage::Copy(data) => {
                if let Err(e) = clipboard::copy(data) {
                    error!("can't copy: {e}");
                }
                return self.close_popup();
            }
            AppMessage::Delete(data) => {
                block_on(async {
                    if let Err(e) = self.state.db.delete(&data).await {
                        error!("can't delete {:?}: {}", data.get_content(), e);
                    }
                });
            }
            AppMessage::Clear => {
                block_on(async {
                    if let Err(e) = self.state.db.clear().await {
                        error!("can't clear db: {e}");
                    }
                });
            }
            AppMessage::RetryConnectingClipboard => {
                self.state.clipboard_state = ClipboardState::Init;
            }
            AppMessage::Navigation(message) => match message {
                navigation::NavigationMessage::Next => {
                    self.state.focus_next();
                }
                navigation::NavigationMessage::Previous => {
                    self.state.focus_previous();
                }
                navigation::NavigationMessage::Enter => {
                    if let Some(data) = self.state.db.get(self.state.focused) {
                        if let Err(e) = clipboard::copy(data.clone()) {
                            error!("can't copy: {e}");
                        }
                        return self.close_popup();
                    }
                }
                navigation::NavigationMessage::Quit => {
                    return self.close_popup();
                }
            },
            AppMessage::PrivateMode(private_mode) => {
                config_set!(private_mode, private_mode);
                PRIVATE_MODE.store(private_mode, atomic::Ordering::Relaxed);
            }
            AppMessage::Db(inner) => {
                block_on(async {
                    if let Err(err) = self.state.db.handle_message(inner).await {
                        error!("{err}");
                    }
                });
            }
            AppMessage::ShowQrCode(e) => {
                // todo: handle better this error
                if e.content.len() < 700 {
                    match qr_code::State::new(&e.content) {
                        Ok(s) => {
                            self.state.qr_code.replace(Ok(s));
                        }
                        Err(e) => {
                            error!("{e}");
                            self.state.qr_code.replace(Err(()));
                        }
                    }
                } else {
                    error!("qr code to long: {}", e.content.len());
                    self.state.qr_code.replace(Err(()));
                }
            }
            AppMessage::ReturnToClipboard => {
                self.state.qr_code.take();
            }
        }
        Command::none()
    }

    fn view(&self) -> Element<Self::Message> {
        let icon = self
            .core
            .applet
            .icon_button(constcat::concat!(APPID, "-symbolic"))
            .on_press(AppMessage::TogglePopup);

        MouseArea::new(icon)
            .on_right_release(AppMessage::ToggleQuickSettings)
            .into()
    }

    fn view_window(&self, _id: Id) -> Element<Self::Message> {
        let Some(popup) = &self.popup else {
            return self
                .core
                .applet
                .popup_container(popup_view(&self.state, &self.config))
                .into();
        };

        let view = match &popup.kind {
            PopupKind::Popup => popup_view(&self.state, &self.config),
            PopupKind::QuickSettings => quick_settings_view(&self.state, &self.config),
        };

        self.core.applet.popup_container(view).into()
    }
    fn subscription(&self) -> Subscription<Self::Message> {
        let mut subscriptions = vec![
            config::sub(),
            navigation::sub().map(AppMessage::Navigation),
            db::sub().map(AppMessage::Db),
        ];

        if !self.state.clipboard_state.is_error() {
            subscriptions.push(clipboard::sub().map(AppMessage::ClipboardEvent));
        }

        Subscription::batch(subscriptions)
    }

    fn style(&self) -> Option<<Theme as application::StyleSheet>::Style> {
        Some(cosmic::applet::style())
    }
}
