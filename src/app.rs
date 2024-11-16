use chrono::Utc;
use cosmic::app::Core;

use cosmic::iced::keyboard::key::Named;
use cosmic::iced::window::Id;
use cosmic::iced::{self, Limits};

use cosmic::iced_futures::Subscription;
use cosmic::iced_runtime::core::window;
use cosmic::iced_runtime::platform_specific::wayland::layer_surface::SctkLayerSurfaceSettings;
use cosmic::iced_widget::qr_code;
use cosmic::iced_winit::commands::layer_surface::{
    self, destroy_layer_surface, get_layer_surface, KeyboardInteractivity,
};
use cosmic::iced_winit::commands::popup::{destroy_popup, get_popup};
use cosmic::widget::{MouseArea, Space};

use cosmic::{Element, Task};
use futures::executor::block_on;
use futures::StreamExt;

use crate::config::{Config, PRIVATE_MODE};
use crate::db::{self, Db, DbMessage};
use crate::message::{AppMsg, ConfigMsg};
use crate::navigation::EventMsg;
use crate::utils::task_message;
use crate::{clipboard, config, navigation};

use cosmic::cosmic_config;
use std::sync::atomic::{self};
use std::time::Duration;

pub const QUALIFIER: &str = "io.github";
pub const ORG: &str = "wiiznokes";
pub const APP: &str = "cosmic-ext-applet-clipboard-manager";
pub const APPID: &str = constcat::concat!(QUALIFIER, ".", ORG, ".", APP);

pub struct AppState {
    core: Core,
    config_handler: cosmic_config::Config,
    popup: Option<Popup>,
    pub config: Config,
    pub db: Db,
    pub clipboard_state: ClipboardState,
    pub focused: usize,
    pub qr_code: Option<Result<qr_code::Data, ()>>,
    last_quit: Option<(i64, PopupKind)>,
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

impl AppState {
    fn toggle_popup(&mut self, kind: PopupKind) -> Task<cosmic::app::Message<AppMsg>> {
        self.qr_code.take();
        match &self.popup {
            Some(popup) => {
                if popup.kind == kind {
                    self.close_popup()
                } else {
                    Task::batch(vec![self.close_popup(), self.open_popup(kind)])
                }
            }
            None => self.open_popup(kind),
        }
    }

    fn close_popup(&mut self) -> Task<cosmic::app::Message<AppMsg>> {
        self.focused = 0;
        self.db.set_query_and_search("".into());

        if let Some(popup) = self.popup.take() {
            // info!("destroy {:?}", popup.id);

            self.last_quit = Some((Utc::now().timestamp_millis(), popup.kind));

            if self.config.horizontal {
                destroy_layer_surface(popup.id)
            } else {
                destroy_popup(popup.id)
            }
        } else {
            Task::none()
        }
    }

    fn open_popup(&mut self, kind: PopupKind) -> Task<cosmic::app::Message<AppMsg>> {
        // handle the case where the popup was closed by clicking the icon
        if self
            .last_quit
            .map(|(t, k)| (Utc::now().timestamp_millis() - t) < 200 && k == kind)
            .unwrap_or(false)
        {
            return Task::none();
        }

        let new_id = Id::unique();
        // info!("will create {:?}", new_id);

        let popup = Popup { kind, id: new_id };
        self.popup.replace(popup);

        match kind {
            PopupKind::Popup => {
                if self.config.horizontal {
                    get_layer_surface(SctkLayerSurfaceSettings {
                        id: new_id,
                        keyboard_interactivity: KeyboardInteractivity::OnDemand,
                        anchor: layer_surface::Anchor::BOTTOM
                            | layer_surface::Anchor::LEFT
                            | layer_surface::Anchor::RIGHT,
                        namespace: "clipboard manager".into(),
                        size: Some((None, Some(350))),
                        size_limits: Limits::NONE.min_width(1.0).min_height(1.0),
                        ..Default::default()
                    })
                } else {
                    let mut popup_settings =
                        self.core
                            .applet
                            .get_popup_settings(Id::RESERVED, new_id, None, None, None);

                    popup_settings.positioner.size_limits = Limits::NONE
                        .max_width(400.0)
                        .min_width(300.0)
                        .min_height(200.0)
                        .max_height(500.0);
                    get_popup(popup_settings)
                }
            }
            PopupKind::QuickSettings => {
                let mut popup_settings =
                    self.core
                        .applet
                        .get_popup_settings(Id::RESERVED, new_id, None, None, None);

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

impl cosmic::Application for AppState {
    type Executor = cosmic::executor::Default;
    type Flags = Flags;
    type Message = AppMsg;
    const APP_ID: &'static str = APPID;

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(core: Core, flags: Self::Flags) -> (Self, Task<cosmic::app::Message<Self::Message>>) {
        let config = flags.config;
        PRIVATE_MODE.store(config.private_mode, atomic::Ordering::Relaxed);

        let db = block_on(async { db::Db::new(&config).await.unwrap() });

        let window = AppState {
            core,
            config_handler: flags.config_handler,
            popup: None,
            db,
            clipboard_state: ClipboardState::Init,
            focused: 0,
            qr_code: None,
            config,
            last_quit: None,
        };

        #[cfg(debug_assertions)]
        let command = task_message(AppMsg::TogglePopup);

        #[cfg(not(debug_assertions))]
        let command = Command::none();

        (window, command)
    }

    fn on_close_requested(&self, id: window::Id) -> Option<AppMsg> {
        info!("on_close_requested");

        if let Some(popup) = &self.popup {
            if popup.id == id {
                return Some(AppMsg::ClosePopup);
            }
        }
        None
    }

    fn update(&mut self, message: Self::Message) -> Task<cosmic::app::Message<Self::Message>> {
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
            AppMsg::ChangeConfig(config) => {
                if config != self.config {
                    PRIVATE_MODE.store(config.private_mode, atomic::Ordering::Relaxed);
                    self.config = config;
                }
            }
            AppMsg::ToggleQuickSettings => {
                return self.toggle_popup(PopupKind::QuickSettings);
            }

            AppMsg::TogglePopup => {
                return self.toggle_popup(PopupKind::Popup);
            }
            AppMsg::ClosePopup => return self.close_popup(),
            AppMsg::Search(query) => {
                self.db.set_query_and_search(query);
            }
            AppMsg::ClipboardEvent(message) => match message {
                clipboard::ClipboardMessage::Connected => {
                    self.clipboard_state = ClipboardState::Connected;
                }
                clipboard::ClipboardMessage::Data(data) => {
                    block_on(async {
                        if let Err(e) = self.db.insert(data).await {
                            error!("can't insert data: {e}");
                        }
                    });
                }
                clipboard::ClipboardMessage::Error(e) => {
                    error!("{e}");
                    self.clipboard_state = ClipboardState::Error(e);
                }
                clipboard::ClipboardMessage::EmptyKeyboard => {
                    if let Some(data) = self.db.get(0) {
                        if let Err(e) = clipboard::copy(data.to_owned()) {
                            error!("can't copy: {e}");
                        }
                    }
                }
            },
            AppMsg::Copy(data) => {
                if let Err(e) = clipboard::copy(data) {
                    error!("can't copy: {e}");
                }
                return self.close_popup();
            }
            AppMsg::Delete(data) => {
                block_on(async {
                    if let Err(e) = self.db.delete(&data).await {
                        error!("can't delete {:?}: {}", data.get_content(), e);
                    }
                });
            }
            AppMsg::Clear => {
                block_on(async {
                    if let Err(e) = self.db.clear().await {
                        error!("can't clear db: {e}");
                    }
                });
            }
            AppMsg::RetryConnectingClipboard => {
                self.clipboard_state = ClipboardState::Init;
            }
            AppMsg::Navigation(message) => match message {
                navigation::EventMsg::Event(e) => {
                    let message = match e {
                        Named::Enter => EventMsg::Enter,
                        Named::Escape => EventMsg::Quit,
                        Named::ArrowDown if !self.config.horizontal => EventMsg::Next,
                        Named::ArrowUp if !self.config.horizontal => EventMsg::Previous,
                        Named::ArrowLeft if self.config.horizontal => EventMsg::Previous,
                        Named::ArrowRight if self.config.horizontal => EventMsg::Next,
                        _ => EventMsg::None,
                    };

                    return task_message(AppMsg::Navigation(message));
                }
                navigation::EventMsg::Next => {
                    self.focus_next();
                }
                navigation::EventMsg::Previous => {
                    self.focus_previous();
                }
                navigation::EventMsg::Enter => {
                    if let Some(data) = self.db.get(self.focused) {
                        if let Err(e) = clipboard::copy(data.clone()) {
                            error!("can't copy: {e}");
                        }
                        return self.close_popup();
                    }
                }
                navigation::EventMsg::Quit => {
                    return self.close_popup();
                }
                EventMsg::None => {}
            },
            AppMsg::Db(inner) => {
                block_on(async {
                    if let Err(err) = self.db.handle_message(inner).await {
                        error!("{err}");
                    }
                });
            }
            AppMsg::ShowQrCode(e) => {
                // todo: handle better this error
                if e.content.len() < 700 {
                    match qr_code::Data::new(&e.content) {
                        Ok(s) => {
                            self.qr_code.replace(Ok(s));
                        }
                        Err(e) => {
                            error!("{e}");
                            self.qr_code.replace(Err(()));
                        }
                    }
                } else {
                    error!("qr code to long: {}", e.content.len());
                    self.qr_code.replace(Err(()));
                }
            }
            AppMsg::ReturnToClipboard => {
                self.qr_code.take();
            }
            AppMsg::Config(msg) => match msg {
                ConfigMsg::PrivateMode(private_mode) => {
                    config_set!(private_mode, private_mode);
                    PRIVATE_MODE.store(private_mode, atomic::Ordering::Relaxed);
                }
                ConfigMsg::Horizontal(horizontal) => {
                    config_set!(horizontal, horizontal);
                }
                ConfigMsg::UniqueSession(unique_session) => {
                    config_set!(unique_session, unique_session);
                }
            },
            AppMsg::AddFavorite(entry) => {
                block_on(async {
                    if let Err(err) = self.db.add_favorite(&entry, None).await {
                        error!("{err}");
                    }
                });
            }
            AppMsg::RemoveFavorite(entry) => {
                block_on(async {
                    if let Err(err) = self.db.remove_favorite(&entry).await {
                        error!("{err}");
                    }
                });
            }
        }
        Task::none()
    }

    fn view(&self) -> Element<Self::Message> {
        let icon = self
            .core
            .applet
            .icon_button(constcat::concat!(APPID, "-symbolic"))
            .on_press(AppMsg::TogglePopup);

        MouseArea::new(icon)
            .on_right_release(AppMsg::ToggleQuickSettings)
            .into()
    }

    fn view_window(&self, _id: Id) -> Element<Self::Message> {
        let Some(popup) = &self.popup else {
            return Space::new(0, 0).into();
        };

        let view = match &popup.kind {
            PopupKind::Popup => self.popup_view(),
            PopupKind::QuickSettings => self.quick_settings_view(),
        };

        self.core.applet.popup_container(view).into()
    }
    fn subscription(&self) -> Subscription<Self::Message> {
        pub fn db_sub() -> Subscription<DbMessage> {
            cosmic::iced::time::every(Duration::from_millis(1000)).map(|_| DbMessage::CheckUpdate)
        }

        let mut subscriptions = vec![
            config::sub(),
            navigation::sub().map(AppMsg::Navigation),
            db_sub().map(AppMsg::Db),
        ];

        if !self.clipboard_state.is_error() {
            subscriptions.push(Subscription::run(|| {
                clipboard::sub().map(AppMsg::ClipboardEvent)
            }));
        }

        Subscription::batch(subscriptions)
    }

    fn on_app_exit(&mut self) -> Option<Self::Message> {
        block_on(async {
            if let Err(err) = self.db.clear().await {
                error!("{err}");
            }
        });
        None
    }

    fn style(&self) -> Option<iced::runtime::Appearance> {
        Some(cosmic::applet::style())
    }
}
