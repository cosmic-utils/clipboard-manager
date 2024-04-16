use cosmic::app::{command, Core};

use cosmic::cctk::sctk::reexports::protocols::wp::presentation_time::client::wp_presentation_feedback::Kind;
use cosmic::iced::advanced::subscription;
use cosmic::iced::wayland::actions::layer_surface::SctkLayerSurfaceSettings;
use cosmic::iced::wayland::popup::{destroy_popup, get_popup};
use cosmic::iced::window::Id;
use cosmic::iced::{self, event, Command, Limits};
use cosmic::iced_core::{Length, Color, Border, alignment, Shadow, color};
use cosmic::iced_sctk::commands::layer_surface::{destroy_layer_surface, KeyboardInteractivity, Anchor, get_layer_surface};
use cosmic::iced_widget::graphics::color;
use cosmic::prelude::CollectionWidget;
use cosmic::widget;
use cosmic::iced_futures::Subscription;
use cosmic::iced_runtime::command::Action;
use cosmic::iced_runtime::core::window;
use cosmic::iced_style::application;
use cosmic::iced_widget::graphics::text::cosmic_text::rustybuzz::ttf_parser::name_id::POST_SCRIPT_NAME;
use cosmic::iced_widget::Column;
use cosmic::widget::{button, icon, text, text_input, MouseArea};

use cosmic::{Element, Theme};

use crate::config::{Config, CONFIG_VERSION, PRIVATE_MODE};
use crate::db::{self, Data, Db};
use crate::message::AppMessage;
use crate::utils::{command_message, formated_value};
use crate::view::{popup_view, quick_settings_view};
use crate::{clipboard, config, navigation};

use cosmic::cosmic_config;
use std::sync::atomic::{self, AtomicBool};

pub const APP_ID: &str = "com.wiiznokes.CosmicClipboardManager";

pub struct Window {
    core: Core,
    config: Config,
    config_handler: Option<cosmic_config::Config>,
    popup: Option<Popup>,
    state: AppState,
    wayland_popup: Option<Id>,
}

pub struct AppState {
    pub db: Db,
    pub clipboard_state: ClipboardState,
    pub focused: usize,
    pub more_action: Option<Data>,
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
    pub config_handler: Option<cosmic_config::Config>,
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
    fn close_popup(&mut self) -> Command<cosmic::app::Message<AppMessage>> {
        self.state.focused = 0;
        self.state.more_action.take();
        self.state.db.search("".into());

        if let Some(popup) = self.popup.take() {
            //info!("destroy {:?}", popup.id);
            destroy_popup(popup.id)
        } else {
            Command::none()
        }
    }

    fn toogle_popup(&mut self, kind: PopupKind) -> Command<cosmic::app::Message<AppMessage>> {
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

    fn open_popup(&mut self, kind: PopupKind) -> Command<cosmic::app::Message<AppMessage>> {
        let new_id = Id::unique();
        //info!("will create {:?}", new_id);

        let popup = Popup { kind, id: new_id };

        self.popup.replace(popup);
        let mut popup_settings =
            self.core
                .applet
                .get_popup_settings(Id::MAIN, new_id, None, None, None);

        match kind {
            PopupKind::Popup => {
                popup_settings.positioner.size_limits = Limits::NONE
                    .max_width(500.0)
                    .min_width(300.0)
                    .min_height(200.0)
                    .max_height(550.0);
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
        let config = flags.config;
        PRIVATE_MODE.store(config.private_mode, atomic::Ordering::Relaxed);
        let window = Window {
            core,
            config_handler: flags.config_handler,
            popup: None,
            state: AppState {
                db: db::Db::new().unwrap(),
                clipboard_state: ClipboardState::Init,
                focused: 0,
                more_action: None,
            },
            config,
            wayland_popup: None,
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

        if let Some(popup) = &self.popup {
            if popup.id == id {
                return Some(AppMessage::ClosePopup);
            }
        }
        None
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
                    PRIVATE_MODE.store(config.private_mode, atomic::Ordering::Relaxed);
                    self.config = config;
                }
            }
            AppMessage::ToggleQuickSettings => {
                return self.toogle_popup(PopupKind::QuickSettings);
            }

            AppMessage::TogglePopup => {
                return self.toogle_popup(PopupKind::Popup);
            }
            AppMessage::ClosePopup => return self.close_popup(),
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
            AppMessage::Copy(data) => {
                if let Err(e) = clipboard::copy(data) {
                    error!("can't copy: {e}");
                }
                return self.close_popup();
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
            AppMessage::MoreAction(data) => {
                self.state.more_action = data;
            }
            AppMessage::CloseWaylandPopup => {
                if let Some(id) = self.wayland_popup.take() {
                    return destroy_layer_surface(id);
                }
            }
            AppMessage::ActivateWaylandPopup => {
                let id = window::Id::unique();
                self.wayland_popup.replace(id);
                return get_layer_surface(SctkLayerSurfaceSettings {
                    id,
                    keyboard_interactivity: KeyboardInteractivity::OnDemand,
                    anchor: Anchor::all(),
                    namespace: "clibpard indicator".into(),
                    size: Some((None, None)),
                    size_limits: Limits::NONE.min_width(1.0).min_height(1.0),
                    ..Default::default()
                });
            }
        }
        Command::none()
    }

    fn view(&self) -> Element<Self::Message> {
        let icon = self
            .core
            .applet
            .icon_button("/usr/share/com.wiiznokes.CosmicClipboardManager/icons/assignment24.svg")
            .on_press(AppMessage::TogglePopup);

        MouseArea::new(icon)
            .on_right_release(AppMessage::ToggleQuickSettings)
            .into()
    }

    fn view_window(&self, id: Id) -> Element<Self::Message> {
        //dbg!(&_id, &self.popup);

        return if matches!(&self.popup, Some(p) if p.id == id) {
            let view = match self.popup.as_ref().unwrap().kind {
                PopupKind::Popup => popup_view(&self.state, &self.config),
                PopupKind::QuickSettings => quick_settings_view(&self.state, &self.config),
            };
            self.core.applet.popup_container(view).into()
        } else if matches!(self.wayland_popup, Some(p) if p == id) {
            let mut content = widget::row::with_capacity(10).spacing(10);

            for entry in self.state.db.iter().take(10) {
                let content_group = content_group(&entry.value);
                let container;
                match content_group {
                    ContentGroup::Color(hex) => {
                        let color = convert_color(hex);
                        let txt = widget::text(format!("#{:0>6x}", hex));
                        let txt_container = widget::container(txt)
                            .center_x()
                            .center_y()
                            .padding(10)
                            .style(cosmic::theme::Container::custom(move |_theme| {
                                widget::container::Appearance {
                                    background: Some(Color::BLACK.into()),
                                    ..Default::default()
                                }
                            }));

                        container = widget::container(txt_container)
                            .center_x()
                            .center_y()
                            .style(cosmic::theme::Container::custom(move |_theme| {
                                widget::container::Appearance {
                                    background: Some(color.into()),
                                    ..Default::default()
                                }
                            }));
                    }
                    ContentGroup::Emoji(emoji) => {
                        container =
                            widget::container(widget::text(emoji).size(40).width(70).height(70))
                                .center_x()
                                .center_y()
                                .style(cosmic::theme::Container::custom(move |_theme| {
                                    widget::container::Appearance {
                                        background: Some(color!(0xfca903).into()),
                                        ..Default::default()
                                    }
                                }));
                    }
                    ContentGroup::Text(text) => {
                        container = widget::container(widget::text(text)).style(
                            cosmic::theme::Container::custom(move |_theme| {
                                widget::container::Appearance {
                                    background: Some(color!(0x994e15).into()),
                                    ..Default::default()
                                }
                            }),
                        );
                    }
                }

                content = content.push(container.width(250).height(250));
            }

            widget::mouse_area(
                widget::container(
                    widget::container(content)
                        /*
                        .style(cosmic::theme::Container::custom(|theme| {
                            widget::container::Appearance {
                                icon_color: Some(theme.cosmic().background.on.into()),
                                text_color: Some(theme.cosmic().background.on.into()),
                                background: Some(Color::from(theme.cosmic().secondary.base).into()),
                                // border: Border {
                                //     radius: 12.0.into(),
                                //     width: 2.0,
                                //     color: theme.cosmic().bg_divider().into(),
                                // },
                                shadow: Shadow::default(),
                                ..Default::default()
                            }
                        }))
                        */
                        .width(Length::Fill)
                        .height(Length::Shrink),
                )
                .align_x(alignment::Horizontal::Center)
                .align_y(alignment::Vertical::Bottom)
                .width(Length::Fill)
                .height(Length::Fill),
            )
            .on_press(AppMessage::CloseWaylandPopup)
            .on_right_press(AppMessage::CloseWaylandPopup)
            .on_middle_press(AppMessage::CloseWaylandPopup)
            .into()
        } else {
            widget::text("").into()
        };
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

fn content_group(content: &str) -> ContentGroup {
    let content_trim = content.trim();
    if content_trim.is_ascii() && content_trim.len() <= 9 {
        let color_value = content_trim.strip_prefix('#').unwrap_or(content_trim);
        if let Ok(mut color) = u32::from_str_radix(color_value, 16) {
            if color_value.len() == 3 {
                color = convert_short_color(color)
            }
            return ContentGroup::Color(color);
        }
    }
    if let Some(emoji) = emojis::get(content_trim) {
        return ContentGroup::Emoji(emoji.as_str().into());
    }
    return ContentGroup::Text(content.into());
}
fn convert_color(hex: u32) -> Color {
    let has_alpha = hex > 0x00ffffff;
    let alpha = if has_alpha { 8 } else { 0 };
    let r = (hex >> (16 + alpha)) & 0xff;
    let g = (hex >> (8 + alpha)) & 0xff;
    let b = (hex >> (alpha)) & 0xff;
    if has_alpha {
        let a = hex & 0xff;
        return Color::from_rgba8(r as _, g as _, b as _, a as f32 / 255.0);
    }
    return Color::from_rgb8(r as _, g as _, b as _);
}
fn convert_short_color(hex: u32) -> u32 {
    let r = (hex >> 8) & 0xf;
    let g = (hex >> 4) & 0xf;
    let b = hex & 0xf;
    return (r << 20) | (r << 16) | (g << 12) | (g << 8) | (b << 4) | b;
}

enum ContentGroup {
    Color(u32),
    Emoji(String),
    Text(String),
}
