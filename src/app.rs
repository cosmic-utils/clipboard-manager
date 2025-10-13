use chrono::Utc;
use cosmic::app::Core;

use cosmic::iced::clipboard::mime::AsMimeTypes;
use cosmic::iced::keyboard::key::Named;
use cosmic::iced::window::Id;
use cosmic::iced::{self, Limits};

use cosmic::iced_core::widget::operation;
use cosmic::iced_futures::Subscription;
use cosmic::iced_runtime::core::window;
use cosmic::iced_runtime::platform_specific::wayland::layer_surface::SctkLayerSurfaceSettings;
use cosmic::iced_widget::qr_code;
use cosmic::iced_widget::scrollable::RelativeOffset;
use cosmic::iced_winit::commands::layer_surface::{
    self, KeyboardInteractivity, destroy_layer_surface, get_layer_surface,
};
use cosmic::iced_winit::commands::popup::{destroy_popup, get_popup};
use cosmic::widget::{MouseArea, Space};

use cosmic::{Element, app::Task};
use futures::StreamExt;
use futures::executor::block_on;
use regex::Regex;

use crate::clipboard::ClipboardError;
use crate::config::{Config, PRIVATE_MODE};
use crate::db::{DbMessage, DbTrait, EntryTrait, MimeDataMap};
use crate::message::{AppMsg, ConfigMsg, ContextMenuMsg};
use crate::navigation::EventMsg;
use crate::utils::task_message;
use crate::view::SCROLLABLE_ID;
use crate::{clipboard, clipboard_watcher, config, ipc, navigation};

use cosmic::{cosmic_config, iced_runtime};
use std::sync::atomic::{self};
use std::time::Duration;

pub const QUALIFIER: &str = "io.github";
pub const ORG: &str = "cosmic_utils";
pub const APP: &str = "cosmic-ext-applet-clipboard-manager";
pub const APPID: &str = constcat::concat!(QUALIFIER, ".", ORG, ".", APP);

pub struct AppState<Db: DbTrait> {
    core: Core,
    config_handler: cosmic_config::Config,
    popup: Option<Popup>,
    pub config: Config,
    pub db: Db,
    pub clipboard_state: ClipboardState,
    pub focused: usize,
    pub page: usize,
    pub qr_code: Option<Result<qr_code::Data, ()>>,
    last_quit: Option<(i64, PopupKind)>,
    pub preferred_mime_types_regex: Vec<Regex>,
    last_signal_content: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipboardState {
    Init,
    Connected,
    Error(ErrorState),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorState {
    MissingDataControlProtocol,
    Other(String),
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

impl<Db: DbTrait> AppState<Db> {
    fn focus_next(&mut self) -> Task<AppMsg> {
        if self.db.len() > 0 {
            self.focused = (self.focused + 1) % self.db.len();
            self.page = self.focused / self.config.maximum_entries_by_page.get() as usize;

            debug!("");
            debug!("len = {}", self.db.len());
            debug!("focused = {}", self.focused);
            debug!(
                "maximum_entries_by_page = {}",
                self.config.maximum_entries_by_page.get() as usize
            );
            debug!("page = {}", self.page);

            // will not work with last page but it is not used anyway because have bug
            let delta_y = (self.focused % self.config.maximum_entries_by_page.get() as usize)
                as f32
                / self.config.maximum_entries_by_page.get() as f32;

            debug!("delta_y = {}", delta_y);

            iced_runtime::task::widget(operation::scrollable::snap_to(
                SCROLLABLE_ID.clone(),
                RelativeOffset {
                    x: 0.,
                    y: delta_y.max(1.).max(0.0),
                },
            ))
        } else {
            Task::none()
        }
    }

    fn focus_previous(&mut self) -> Task<AppMsg> {
        if self.db.len() > 0 {
            self.focused = (self.focused + self.db.len() - 1) % self.db.len();
            self.page = self.focused / self.config.maximum_entries_by_page.get() as usize;

            debug!("");
            debug!("len = {}", self.db.len());
            debug!("focused = {}", self.focused);
            debug!(
                "maximum_entries_by_page = {}",
                self.config.maximum_entries_by_page.get() as usize
            );
            debug!("page = {}", self.page);

            let delta_y = (self.focused % self.config.maximum_entries_by_page.get() as usize)
                as f32
                / self.config.maximum_entries_by_page.get() as f32;

            debug!("delta_y = {}", delta_y);
            iced_runtime::task::widget(operation::scrollable::snap_to(
                SCROLLABLE_ID.clone(),
                RelativeOffset {
                    x: 0.,
                    y: delta_y.max(1.).max(0.0),
                },
            ))
        } else {
            Task::none()
        }
    }

    fn toggle_popup(&mut self, kind: PopupKind) -> Task<AppMsg> {
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

    fn close_popup(&mut self) -> Task<AppMsg> {
        self.focused = 0;
        self.page = 0;
        self.db.set_query_and_search("".into());

        if let Some(popup) = self.popup.take() {
            // info!("destroy {:?}", popup.id);

            self.last_quit = Some((Utc::now().timestamp_millis(), popup.kind));

            // Popup now always uses layer surface for reliable keyboard focus
            if popup.kind == PopupKind::Popup {
                destroy_layer_surface(popup.id)
            } else {
                // QuickSettings still uses popup
                destroy_popup(popup.id)
            }
        } else {
            Task::none()
        }
    }

    fn open_popup(&mut self, kind: PopupKind) -> Task<AppMsg> {
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
                // Always use layer surface for external toggles to ensure keyboard focus
                // Popups don't reliably receive keyboard focus when opened programmatically
                get_layer_surface(SctkLayerSurfaceSettings {
                    id: new_id,
                    keyboard_interactivity: KeyboardInteractivity::Exclusive,
                    anchor: if self.config.horizontal {
                        layer_surface::Anchor::BOTTOM
                            | layer_surface::Anchor::LEFT
                            | layer_surface::Anchor::RIGHT
                    } else {
                        // Position at top-right for vertical layout
                        layer_surface::Anchor::TOP | layer_surface::Anchor::RIGHT
                    },
                    namespace: "clipboard manager".into(),
                    size: if self.config.horizontal {
                        Some((None, Some(350)))
                    } else {
                        Some((Some(400), Some(530)))
                    },
                    size_limits: Limits::NONE.min_width(1.0).min_height(1.0),
                    ..Default::default()
                })
            }
            PopupKind::QuickSettings => {
                let mut popup_settings = self.core.applet.get_popup_settings(
                    self.core.main_window_id().unwrap(),
                    new_id,
                    None,
                    None,
                    None,
                );

                popup_settings.positioner.size_limits = Limits::NONE
                    .min_width(200.0)
                    .max_width(250.0)
                    .min_height(200.0)
                    .max_height(550.0);

                get_popup(popup_settings)
            }
        }
    }
}

impl<Db: DbTrait + 'static> cosmic::Application for AppState<Db> {
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

    fn init(core: Core, flags: Self::Flags) -> (Self, Task<Self::Message>) {
        let config = flags.config;
        PRIVATE_MODE.store(config.private_mode, atomic::Ordering::Relaxed);

        let db = block_on(async { Db::new(&config).await.unwrap() });

        let state = AppState {
            core,
            config_handler: flags.config_handler,
            popup: None,
            db,
            clipboard_state: ClipboardState::Init,
            focused: 0,
            qr_code: None,
            last_quit: None,
            page: 0,
            preferred_mime_types_regex: config
                .preferred_mime_types
                .iter()
                .filter_map(|r| match Regex::new(r) {
                    Ok(r) => Some(r),
                    Err(e) => {
                        error!("regex {e}");
                        None
                    }
                })
                .collect(),
            config,
            last_signal_content: None,
        };

        #[cfg(debug_assertions)]
        let command = task_message(AppMsg::TogglePopup);

        #[cfg(not(debug_assertions))]
        let command = Task::none();

        (state, command)
    }

    fn on_close_requested(&self, id: window::Id) -> Option<AppMsg> {
        info!("on_close_requested");

        if let Some(popup) = &self.popup
            && popup.id == id
        {
            return Some(AppMsg::ClosePopup);
        }
        None
    }

    fn update(&mut self, message: Self::Message) -> Task<Self::Message> {
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
            AppMsg::CheckSignalFile => {
                // Only check signal file if XDG_RUNTIME_DIR is set
                if let Some(signal_file) = ipc::get_signal_file_path() {
                    if let Ok(content) = std::fs::read_to_string(&signal_file) {
                        if self.last_signal_content.as_ref() != Some(&content) {
                            self.last_signal_content = Some(content);

                            // Clear last_quit for external toggles to ensure it works
                            self.last_quit = None;

                            // Toggle the popup using the existing toggle_popup function
                            return self.toggle_popup(PopupKind::Popup);
                        }
                    }
                }
            }
            AppMsg::ChangeConfig(config) => {
                if config.private_mode != self.config.private_mode {
                    PRIVATE_MODE.store(config.private_mode, atomic::Ordering::Relaxed);
                }
                if config.preferred_mime_types != self.config.preferred_mime_types {
                    self.preferred_mime_types_regex = config
                        .preferred_mime_types
                        .iter()
                        .filter_map(|r| match Regex::new(r) {
                            Ok(r) => Some(r),
                            Err(e) => {
                                error!("regex {e}");
                                None
                            }
                        })
                        .collect();
                }
                self.config = config;
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
                    if let Err(e) = block_on(self.db.insert(data)) {
                        error!("can't insert data: {e}");
                    }
                }
                #[expect(irrefutable_let_patterns)]
                clipboard::ClipboardMessage::Error(e) => {
                    error!("clipboard: {e}");

                    self.clipboard_state = if let ClipboardError::Watch(ref e) = e
                        && let clipboard_watcher::Error::MissingProtocol { name, .. } = **e
                        && name == "zwlr_data_control_manager_v1"
                    {
                        ClipboardState::Error(ErrorState::MissingDataControlProtocol)
                    } else {
                        ClipboardState::Error(ErrorState::Other(e.to_string()))
                    };
                }
                clipboard::ClipboardMessage::EmptyKeyboard => {
                    if let Some(data) = self.db.get(0) {
                        return copy_iced(data.raw_content().clone());
                    }
                }
            },
            AppMsg::Copy(id) => {
                let task = match self.db.get_from_id(id) {
                    Some(data) => copy_iced(data.raw_content().clone()),
                    None => {
                        error!("id not found");
                        Task::none()
                    }
                };

                return Task::batch([task, self.close_popup()]);
            }

            AppMsg::CopySpecial(data) => {
                return copy_iced(data);
            }
            AppMsg::Clear => {
                if let Err(e) = block_on(self.db.clear()) {
                    error!("can't clear db: {e}");
                }
            }
            AppMsg::RetryConnectingClipboard => {
                self.clipboard_state = ClipboardState::Init;
            }
            AppMsg::Navigation(message) => match message {
                EventMsg::Event(e) => {
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
                EventMsg::Next => {
                    return self.focus_next();
                }
                EventMsg::Previous => {
                    return self.focus_previous();
                }
                EventMsg::Enter => {
                    if matches!(
                        self.popup,
                        Some(Popup {
                            kind: PopupKind::Popup,
                            ..
                        })
                    ) && let Some(data) = self.db.get(self.focused)
                    {
                        return Task::batch([
                            copy_iced(data.raw_content().clone()),
                            self.close_popup(),
                        ]);
                    }
                }
                EventMsg::Quit => {
                    return self.close_popup();
                }
                EventMsg::None => {}
            },
            AppMsg::Db(inner) => {
                if let Err(err) = block_on(self.db.handle_message(inner)) {
                    error!("{err}");
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
            AppMsg::NextPage => {
                self.page += 1;
                self.focused = self.page * self.config.maximum_entries_by_page.get() as usize;
            }
            AppMsg::PreviousPage => {
                self.page -= 1;
                self.focused = self.page * self.config.maximum_entries_by_page.get() as usize;
            }
            AppMsg::ContextMenu(msg) => match msg {
                ContextMenuMsg::RemoveFavorite(entry) => {
                    if let Err(err) = block_on(self.db.remove_favorite(entry)) {
                        error!("{err}");
                    }
                }
                ContextMenuMsg::AddFavorite(entry) => {
                    if let Err(err) = block_on(self.db.add_favorite(entry, None)) {
                        error!("{err}");
                    }
                }
                ContextMenuMsg::ShowQrCode(id) => {
                    match self.db.get_from_id(id) {
                        Some(entry) => {
                            if let Some(((_, content), _)) =
                                entry.preferred_content(&self.preferred_mime_types_regex)
                            {
                                // todo: handle better this error
                                if content.len() < 700 {
                                    match qr_code::Data::new(content) {
                                        Ok(s) => {
                                            self.qr_code.replace(Ok(s));
                                        }
                                        Err(e) => {
                                            error!("{e}");
                                            self.qr_code.replace(Err(()));
                                        }
                                    }
                                } else {
                                    error!("qr code to long: {}", content.len());
                                    self.qr_code.replace(Err(()));
                                }
                            }
                        }
                        None => error!("id not found"),
                    }
                }
                ContextMenuMsg::Delete(id) => {
                    if let Err(e) = block_on(self.db.delete(id)) {
                        error!("can't delete {}: {}", id, e);
                    }
                }
            },
            AppMsg::LinkClicked(url) => {
                info!("open: {url}");
                if let Err(e) = open::that(url.as_str()) {
                    error!("{e}");
                }
            }
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Self::Message> {
        let icon = self
            .core
            .applet
            .icon_button(constcat::concat!(APPID, "-symbolic"))
            .on_press(AppMsg::TogglePopup);

        MouseArea::new(icon)
            .on_right_release(AppMsg::ToggleQuickSettings)
            .into()
    }

    fn view_window(&self, _id: Id) -> Element<'_, Self::Message> {
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
            ipc::signal_file_watcher(),
        ];

        if !self.clipboard_state.is_error() {
            subscriptions.push(Subscription::run(|| {
                clipboard::sub().map(AppMsg::ClipboardEvent)
            }));
        }

        Subscription::batch(subscriptions)
    }

    fn on_app_exit(&mut self) -> Option<Self::Message> {
        if self.config.unique_session
            && let Err(err) = block_on(self.db.clear())
        {
            error!("{err}");
        }
        None
    }

    fn style(&self) -> Option<iced::runtime::Appearance> {
        Some(cosmic::applet::style())
    }
}

// used because wl_clipboard can't copy when zwlr_data_control_manager_v1 is not there
fn copy_iced(data: MimeDataMap) -> Task<AppMsg> {
    struct MimeDataMapN(MimeDataMap);

    impl AsMimeTypes for MimeDataMapN {
        fn available(&self) -> std::borrow::Cow<'static, [String]> {
            std::borrow::Cow::Owned(self.0.keys().cloned().collect())
        }

        fn as_bytes(&self, mime_type: &str) -> Option<std::borrow::Cow<'static, [u8]>> {
            self.0
                .get(mime_type)
                .map(|d| std::borrow::Cow::Owned(d.clone()))
        }
    }

    cosmic::iced::clipboard::write_data(MimeDataMapN(data))
}
