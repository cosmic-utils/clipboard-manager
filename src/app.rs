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
use crate::db::{Content, DbMessage, DbTrait, EntryId, EntryTrait, MimeDataMap};
use crate::editor_ipc::{self, EditorToApp};
use crate::message::{AppMsg, ConfigMsg, ContextMenuMsg};
use crate::navigation::EventMsg;
use crate::ipc::EntrySummary;
use crate::utils::{sanitize_preview, task_message};
use crate::view::SCROLLABLE_ID;
use crate::{clipboard, clipboard_watcher, config, ipc, navigation};

use cosmic::{cosmic_config, iced_runtime};
use std::sync::atomic::{self, AtomicU64};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub const QUALIFIER: &str = "io.github";
pub const ORG: &str = "cosmic_utils";
pub const APP: &str = "cosmic-ext-applet-clipboard-manager";
pub const APPID: &str = constcat::concat!(QUALIFIER, ".", ORG, ".", APP);

/// MIME type set by password managers (e.g. KeePassXC) to indicate sensitive clipboard content.
/// When present, the entry should not be stored in clipboard history and the clipboard should
/// not be restored when the source application clears it.
const PASSWORD_MANAGER_HINT_MIME: &str = "x-kde-passwordManagerHint";

static EDITOR_SESSION_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Tracks the editor subprocess spawned by the applet.
pub struct EditorProcess {
    pub entry_id: EntryId,
    pub mime: String,
    pub stdin_handle: std::process::ChildStdin,
    pub child: std::process::Child,
    pub stdout_rx: Arc<Mutex<Option<tokio::sync::mpsc::Receiver<EditorToApp>>>>,
    pub session_id: u64,
}

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
    /// Tracks whether the last clipboard entry was sensitive (e.g. from a password manager).
    /// When true, the clipboard will not be restored on clear.
    last_entry_sensitive: bool,
    pub editor: Option<EditorProcess>,
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
    /// Whether this popup was opened as a layer surface (for --toggle)
    /// vs an XDG popup (for icon click).
    pub is_layer_surface: bool,
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
                    y: delta_y.clamp(0.0, 1.0),
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
                    y: delta_y.clamp(0.0, 1.0),
                },
            ))
        } else {
            Task::none()
        }
    }

    fn toggle_popup(&mut self, kind: PopupKind) -> Task<AppMsg> {
        self.toggle_popup_ext(kind, false)
    }

    fn toggle_popup_ext(&mut self, kind: PopupKind, force_layer_surface: bool) -> Task<AppMsg> {
        self.qr_code.take();
        match &self.popup {
            Some(popup) => {
                if popup.kind == kind {
                    self.close_popup()
                } else {
                    Task::batch(vec![self.close_popup(), self.open_popup(kind, force_layer_surface)])
                }
            }
            None => self.open_popup(kind, force_layer_surface),
        }
    }

    fn close_popup(&mut self) -> Task<AppMsg> {
        self.focused = 0;
        self.page = 0;
        self.db.set_query_and_search("".into());

        if let Some(popup) = self.popup.take() {
            self.last_quit = Some((Utc::now().timestamp_millis(), popup.kind));

            if popup.is_layer_surface {
                destroy_layer_surface(popup.id)
            } else {
                destroy_popup(popup.id)
            }
        } else {
            Task::none()
        }
    }

    fn open_popup(&mut self, kind: PopupKind, force_layer_surface: bool) -> Task<AppMsg> {
        // handle the case where the popup was closed by clicking the icon
        if self
            .last_quit
            .map(|(t, k)| (Utc::now().timestamp_millis() - t) < 200 && k == kind)
            .unwrap_or(false)
        {
            return Task::none();
        }

        let new_id = Id::unique();
        let use_layer_surface = force_layer_surface || self.config.horizontal;

        let popup = Popup {
            kind,
            id: new_id,
            is_layer_surface: use_layer_surface && kind == PopupKind::Popup,
        };
        self.popup.replace(popup);

        match kind {
            PopupKind::Popup if use_layer_surface => {
                // Use layer surface for --toggle or horizontal layout.
                // Empty anchor = centered on screen (no edge attachment).
                get_layer_surface(SctkLayerSurfaceSettings {
                    id: new_id,
                    keyboard_interactivity: KeyboardInteractivity::Exclusive,
                    anchor: if self.config.horizontal {
                        layer_surface::Anchor::BOTTOM
                            | layer_surface::Anchor::LEFT
                            | layer_surface::Anchor::RIGHT
                    } else {
                        layer_surface::Anchor::empty()
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
            PopupKind::Popup => {
                // Use XDG popup anchored to applet icon for normal click
                let mut popup_settings = self.core.applet.get_popup_settings(
                    self.core.main_window_id().unwrap(),
                    new_id,
                    None,
                    None,
                    None,
                );

                popup_settings.positioner.size_limits = Limits::NONE
                    .min_width(300.0)
                    .max_width(400.0)
                    .min_height(200.0)
                    .max_height(500.0);
                get_popup(popup_settings)
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

    /// Spawn the editor as a separate process.
    ///
    /// IPC channels:
    /// - Applet → Editor: child's stdin pipe (length-prefixed JSON frames)
    /// - Editor → Applet: dedicated pipe on FD 3 (avoids stdout, which COSMIC writes to)
    fn open_editor_process(
        &mut self,
        entry_id: EntryId,
        text: &str,
        mime: String,
    ) -> Task<AppMsg> {
        use std::os::unix::io::{FromRawFd, IntoRawFd};
        use std::os::unix::process::CommandExt;
        use nix::unistd::{close, dup2, pipe};

        /// The file descriptor number the child uses for IPC writes back to the applet.
        const IPC_FD: i32 = 3;

        // Close existing editor if open
        if let Some(mut old_editor) = self.editor.take() {
            let _ = editor_ipc::write_frame(
                &mut old_editor.stdin_handle,
                &editor_ipc::AppToEditor::CloseRequested,
            );
            // Reap in background to avoid zombies
            std::thread::spawn(move || {
                let _ = old_editor.child.wait();
            });
        }

        let exe = std::env::current_exe()
            .unwrap_or_else(|_| "cosmic-ext-applet-clipboard-manager".into());

        // Create a dedicated pipe for Editor → Applet IPC.
        // The child writes to IPC_FD (3), the parent reads from ipc_read_fd.
        let (ipc_read_owned, ipc_write_owned) = match pipe() {
            Ok(fds) => fds,
            Err(e) => {
                error!("failed to create IPC pipe: {e}");
                return Task::none();
            }
        };

        // Extract raw fd values for use in pre_exec and manual management.
        // into_raw_fd() transfers ownership — we manage lifetimes manually from here.
        let ipc_read_raw = ipc_read_owned.into_raw_fd();
        let ipc_write_raw = ipc_write_owned.into_raw_fd();

        let mut cmd = std::process::Command::new(&exe);
        cmd.arg("--editor-window")
            .stdin(std::process::Stdio::piped())
            // stdout/stderr are inherited — COSMIC can write freely without corrupting IPC
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit());

        // In the child (pre-fork), place the IPC write end at FD 3.
        unsafe {
            cmd.pre_exec(move || {
                // Move write end to the well-known IPC_FD
                dup2(ipc_write_raw, IPC_FD)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
                // Close the original write-end fd (now duplicated at IPC_FD)
                if ipc_write_raw != IPC_FD {
                    close(ipc_write_raw).ok();
                }
                // Close the read end in the child — only the parent reads from it
                close(ipc_read_raw).ok();
                Ok(())
            });
        }

        let mut child = match cmd.spawn() {
            Ok(child) => child,
            Err(e) => {
                error!("failed to spawn editor: {e}");
                // Clean up pipe fds on failure
                close(ipc_read_raw).ok();
                close(ipc_write_raw).ok();
                return Task::none();
            }
        };

        // Parent: close the write end (only the child writes)
        close(ipc_write_raw).ok();

        // Wrap the read end as a File for the reader thread
        let ipc_read_file = unsafe { std::fs::File::from_raw_fd(ipc_read_raw) };

        let mut stdin_handle = child.stdin.take().unwrap();

        // Send Init message
        if let Err(e) = editor_ipc::write_frame(
            &mut stdin_handle,
            &editor_ipc::AppToEditor::Init {
                entry_id,
                mime: mime.clone(),
                content: text.to_string(),
            },
        ) {
            error!("failed to send Init to editor: {e}");
            return Task::none();
        }

        // Spawn reader thread for the dedicated IPC pipe (not stdout)
        let (tx, rx) = tokio::sync::mpsc::channel::<EditorToApp>(8);
        std::thread::spawn(move || {
            let mut reader = std::io::BufReader::new(ipc_read_file);
            loop {
                match editor_ipc::read_frame::<EditorToApp>(&mut reader) {
                    Ok(msg) => {
                        eprintln!("[applet-reader] Read frame: {msg:?}");
                        if tx.blocking_send(msg).is_err() {
                            eprintln!("[applet-reader] mpsc send failed, breaking");
                            break;
                        }
                    }
                    Err(e) => {
                        eprintln!("[applet-reader] read_frame error: {e}");
                        break;
                    }
                }
            }
            eprintln!("[applet-reader] Reader thread exiting");
        });

        let session_id = EDITOR_SESSION_COUNTER.fetch_add(1, atomic::Ordering::Relaxed);

        self.editor = Some(EditorProcess {
            entry_id,
            mime,
            stdin_handle,
            child,
            stdout_rx: Arc::new(Mutex::new(Some(rx))),
            session_id,
        });

        Task::none()
    }

    /// Send a message to the editor process. Returns false if send failed.
    fn send_to_editor(&mut self, msg: &editor_ipc::AppToEditor) -> bool {
        if let Some(editor) = &mut self.editor {
            if editor_ipc::write_frame(&mut editor.stdin_handle, msg).is_err() {
                warn!("failed to send to editor (pipe broken)");
                return false;
            }
            true
        } else {
            false
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
            last_entry_sensitive: false,
            editor: None,
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
            AppMsg::DbusToggle => {
                self.last_quit = None;
                return self.toggle_popup_ext(PopupKind::Popup, true);
            }
            AppMsg::DbusListEntries { reply } => {
                let summaries: Vec<EntrySummary> = self
                    .db
                    .iter()
                    .map(|entry| {
                        let preview = match entry
                            .preferred_content(&self.preferred_mime_types_regex)
                        {
                            Some((_, Content::Text(text))) => {
                                sanitize_preview(text, 100)
                            }
                            Some((_, Content::UriList(uris))) => {
                                sanitize_preview(&uris.join(" "), 100)
                            }
                            Some((_, Content::Image(_))) => "[image]".to_string(),
                            None => "[unknown]".to_string(),
                        };
                        EntrySummary {
                            id: entry.id(),
                            is_favorite: entry.is_favorite(),
                            preview,
                        }
                    })
                    .collect();
                if let Some(tx) = reply.lock().unwrap().take() {
                    let _ = tx.send(summaries);
                }
            }
            AppMsg::DbusCopyEntry { id, reply } => {
                let result = match self.db.get_from_id(id) {
                    Some(data) => {
                        let task = copy_iced(data.raw_content().clone());
                        if let Some(tx) = reply.lock().unwrap().take() {
                            let _ = tx.send(Ok(()));
                        }
                        return task;
                    }
                    None => Err(format!("entry {id} not found")),
                };
                if let Some(tx) = reply.lock().unwrap().take() {
                    let _ = tx.send(result);
                }
            }
            AppMsg::DbusGetEntry { id, reply } => {
                let result = match self.db.get_from_id(id) {
                    Some(entry) => {
                        match entry.preferred_content(&self.preferred_mime_types_regex) {
                            Some(((mime, raw), _)) => {
                                Ok((mime.to_string(), raw.clone()))
                            }
                            None => Err(format!("entry {id} has no displayable content")),
                        }
                    }
                    None => Err(format!("entry {id} not found")),
                };
                if let Some(tx) = reply.lock().unwrap().take() {
                    let _ = tx.send(result);
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
                    if data.contains_key(PASSWORD_MANAGER_HINT_MIME) {
                        info!("clipboard contains password manager hint, skipping storage");
                        self.last_entry_sensitive = true;
                    } else {
                        self.last_entry_sensitive = false;
                        if let Err(e) = block_on(self.db.insert(data)) {
                            error!("can't insert data: {e}");
                        }
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
                    if self.last_entry_sensitive {
                        info!("clipboard cleared by password manager, not restoring");
                        self.last_entry_sensitive = false;
                    } else if let Some(data) = self.db.get(0) {
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
            AppMsg::Navigation(message) => {
                match message {
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
            }
            }
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
                if self.page > 0 {
                    self.page -= 1;
                    self.focused = self.page * self.config.maximum_entries_by_page.get() as usize;
                }
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
                ContextMenuMsg::Edit(id) => {
                    let edit_info = self.db.get_from_id(id).and_then(|entry| {
                        if let Some(((mime, _), Content::Text(text))) =
                            entry.preferred_content(&self.preferred_mime_types_regex)
                        {
                            Some((text.to_string(), mime.to_string()))
                        } else {
                            None
                        }
                    });
                    if let Some((text, mime)) = edit_info {
                        return self.open_editor_process(id, &text, mime);
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
                    // If editing this entry, tell editor to close without saving
                    if self
                        .editor
                        .as_ref()
                        .is_some_and(|e| e.entry_id == id)
                    {
                        self.send_to_editor(&editor_ipc::AppToEditor::EntryDeleted);
                    }
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
            AppMsg::EditLatest => {
                self.last_quit = None;
                // Find the most recent text entry and open editor process
                let edit_info = self
                    .db
                    .iter()
                    .find(|e| {
                        matches!(
                            e.preferred_content(&self.preferred_mime_types_regex),
                            Some((_, Content::Text(_)))
                        )
                    })
                    .and_then(|entry| {
                        if let Some(((mime, _), Content::Text(text))) =
                            entry.preferred_content(&self.preferred_mime_types_regex)
                        {
                            Some((entry.id(), text.to_string(), mime.to_string()))
                        } else {
                            None
                        }
                    });
                if let Some((id, text, mime)) = edit_info {
                    return self.open_editor_process(id, &text, mime);
                }
            }
            AppMsg::EditorEvent(msg) => {
                eprintln!("[applet] EditorEvent: {msg:?}");
                match msg {
                    EditorToApp::Ready => {}
                    EditorToApp::SaveAsNew { content } => {
                        eprintln!("[applet] SaveAsNew content_len={}", content.len());
                        let mut data = MimeDataMap::new();
                        data.insert("text/plain".to_string(), content.as_bytes().to_vec());
                        match block_on(self.db.insert(data.clone())) {
                            Ok(()) => {
                                eprintln!("[applet] SaveAsNew: inserted as new entry");
                                return copy_iced(data);
                            }
                            Err(e) => error!("failed to save as new entry: {e}"),
                        }
                    }
                    EditorToApp::Closed => {
                        eprintln!("[applet] Editor sent Closed (no changes)");
                    }
                }
            },
            AppMsg::EditorProcessExited => {
                eprintln!("[applet] EditorProcessExited");
                if let Some(mut editor) = self.editor.take() {
                    let _ = editor.child.wait();
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
            cosmic::iced::time::every(Duration::from_millis(5000)).map(|_| DbMessage::CheckUpdate)
        }

        let mut subscriptions = vec![
            config::sub(),
            navigation::sub().map(AppMsg::Navigation),
            db_sub().map(AppMsg::Db),
            ipc::dbus_toggle_subscription(),
        ];

        // Editor process IPC subscription
        if let Some(editor) = &self.editor {
            let rx_arc = editor.stdout_rx.clone();
            let session_id = editor.session_id;
            subscriptions.push(Subscription::run_with_id(
                session_id,
                cosmic::iced::stream::channel(8, move |mut output| async move {
                    use cosmic::iced::futures::SinkExt;
                    let rx_opt = rx_arc.lock().unwrap().take();
                    let Some(mut rx) = rx_opt else {
                        futures::future::pending::<()>().await;
                        unreachable!();
                    };
                    loop {
                        match rx.recv().await {
                            Some(msg) => {
                                eprintln!("[applet-sub] Received from editor: {msg:?}");
                                output.send(AppMsg::EditorEvent(msg)).await.ok();
                            }
                            None => {
                                eprintln!("[applet-sub] Editor channel closed, sending EditorProcessExited");
                                output.send(AppMsg::EditorProcessExited).await.ok();
                                futures::future::pending::<()>().await;
                            }
                        }
                    }
                }),
            ));
        }

        if !self.clipboard_state.is_error() {
            subscriptions.push(Subscription::run(|| {
                clipboard::sub().map(AppMsg::ClipboardEvent)
            }));
        }

        Subscription::batch(subscriptions)
    }

    fn on_app_exit(&mut self) -> Option<Self::Message> {
        // Tell editor to close gracefully before applet exits
        if let Some(mut editor) = self.editor.take() {
            let _ = editor_ipc::write_frame(
                &mut editor.stdin_handle,
                &editor_ipc::AppToEditor::CloseRequested,
            );
            let _ = editor.child.wait();
        }

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
