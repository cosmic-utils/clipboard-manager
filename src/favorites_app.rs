//! Standalone resizable COSMIC window for browsing favorites.
//! Communicates with the applet via D-Bus (no pipe IPC needed).
//! Launched via `--favorites-window` flag or from the applet.

use cosmic::app::{Core, Task};
use cosmic::iced::alignment::Alignment;
use cosmic::iced::{Length, padding};
use cosmic::widget::{column, container, row, scrollable, text, text_input};
use cosmic::Element;

use crate::{ai, ipc};

const FAVORITES_APP_ID: &str = "io.github.cosmic_utils.clipboard-favorites";

#[derive(Debug, Clone)]
pub enum FavMsg {
    /// Tell applet to copy this entry, then exit.
    Copy(i64),
    /// Edit entry in the editor (via D-Bus to applet).
    Edit(i64),
    /// Editor finished — refresh the list.
    EditDone,
    /// Favorites loaded from D-Bus.
    Loaded(Vec<(i64, String, String)>),
    SearchChanged(String),
    SortBy(SortColumn),
    CloseWindow,
    /// Start editing the title for this entry.
    BeginTitleEdit(i64),
    /// Title input changed while editing.
    TitleInput(String),
    /// Accept the current AI suggestion (Tab/Right pressed).
    AcceptSuggestion,
    /// Commit the title edit (Enter or click away).
    CommitTitle,
    /// Cancel editing without saving.
    CancelTitleEdit,
    /// AI suggestion arrived.
    TitleSuggested(i64, Option<String>),
    /// Remove entry from favorites.
    Unfavorite(i64),
    /// Keyboard event for Tab/Right/Enter/Escape handling.
    KeyEvent(cosmic::iced::keyboard::key::Named),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortColumn {
    Title,
    Content,
    Date,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDir {
    Asc,
    Desc,
}

/// State for inline title editing.
struct TitleEditState {
    entry_id: i64,
    input: String,
    suggestion: Option<String>,
    suggesting: bool,
    original_title: String,
}

pub struct FavoritesApp {
    core: Core,
    /// Raw data from D-Bus: (id, title, preview)
    entries: Vec<(i64, String, String)>,
    search: String,
    sort_col: SortColumn,
    sort_dir: SortDir,
    /// Active inline title editing state.
    title_edit: Option<TitleEditState>,
}

pub struct FavoritesFlags;

impl FavoritesApp {
    /// Get filtered + sorted entries.
    fn visible_entries(&self) -> Vec<&(i64, String, String)> {
        let query = self.search.to_lowercase();
        let mut filtered: Vec<_> = self
            .entries
            .iter()
            .filter(|(_, title, preview)| {
                if query.is_empty() {
                    return true;
                }
                title.to_lowercase().contains(&query)
                    || preview.to_lowercase().contains(&query)
            })
            .collect();

        filtered.sort_by(|a, b| {
            let cmp = match self.sort_col {
                SortColumn::Title => {
                    let ta = if a.1.is_empty() { "~" } else { &a.1 };
                    let tb = if b.1.is_empty() { "~" } else { &b.1 };
                    ta.to_lowercase().cmp(&tb.to_lowercase())
                }
                SortColumn::Content => a.2.to_lowercase().cmp(&b.2.to_lowercase()),
                SortColumn::Date => a.0.cmp(&b.0),
            };
            match self.sort_dir {
                SortDir::Asc => cmp,
                SortDir::Desc => cmp.reverse(),
            }
        });

        filtered
    }

    fn format_timestamp(millis: i64) -> String {
        use std::time::{Duration, UNIX_EPOCH};
        let d = UNIX_EPOCH + Duration::from_millis(millis as u64);
        let dt: chrono::DateTime<chrono::Local> = d.into();
        dt.format("%Y-%m-%d %H:%M").to_string()
    }

    fn update_header_title(&mut self) {
        let visible = self.visible_entries().len();
        let total = self.entries.len();
        self.core.window.header_title = if visible == total {
            format!("Favorites ({total})")
        } else {
            format!("Favorites ({visible}/{total})")
        };
    }

    fn column_header<'a>(
        &self,
        label: &'a str,
        col: SortColumn,
        width: Length,
    ) -> Element<'a, FavMsg> {
        let arrow = if self.sort_col == col {
            match self.sort_dir {
                SortDir::Asc => " ^",
                SortDir::Desc => " v",
            }
        } else {
            ""
        };
        let label_text = format!("{label}{arrow}");

        cosmic::widget::button::custom(
            text::body(label_text).class(cosmic::style::Text::Default),
        )
        .padding(padding::all(6))
        .on_press(FavMsg::SortBy(col))
        .class(cosmic::theme::Button::Text)
        .width(width)
        .into()
    }

    /// Save the current title edit to the applet via D-Bus.
    fn save_title_edit(&mut self) -> Task<FavMsg> {
        if let Some(state) = self.title_edit.take() {
            let title = state.input.trim().to_string();
            // Only save if changed
            if title != state.original_title {
                let id = state.entry_id;
                // Update local data immediately
                if let Some(entry) = self.entries.iter_mut().find(|(eid, _, _)| *eid == id) {
                    entry.1 = title.clone();
                }
                // Send to applet via D-Bus
                return Task::perform(
                    async move {
                        if let Err(e) =
                            ipc::send_set_favorite_title_async(id, &title).await
                        {
                            eprintln!("[favorites] Failed to set title for {id}: {e}");
                        }
                    },
                    |_| cosmic::action::app(FavMsg::CommitTitle),
                );
            }
        }
        Task::none()
    }
}

impl cosmic::Application for FavoritesApp {
    type Executor = cosmic::executor::Default;
    type Flags = FavoritesFlags;
    type Message = FavMsg;
    const APP_ID: &'static str = FAVORITES_APP_ID;

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(mut core: Core, _flags: Self::Flags) -> (Self, Task<Self::Message>) {
        core.window.show_headerbar = true;
        core.window.show_minimize = true;
        core.window.show_maximize = true;
        core.window.show_close = true;
        core.window.header_title = "Favorites".into();

        let app = Self {
            core,
            entries: Vec::new(),
            search: String::new(),
            sort_col: SortColumn::Date,
            sort_dir: SortDir::Desc,
            title_edit: None,
        };

        let task = Task::perform(
            async { load_favorites_async().await },
            |entries| cosmic::action::app(FavMsg::Loaded(entries)),
        );

        (app, task)
    }

    fn update(&mut self, message: Self::Message) -> Task<Self::Message> {
        match message {
            FavMsg::Copy(id) => {
                // If editing title, commit first
                if self.title_edit.is_some() {
                    return self.save_title_edit();
                }
                // Tell the applet to copy this entry via D-Bus, then exit.
                std::thread::spawn(move || {
                    if let Err(e) = ipc::send_copy_entry(id) {
                        eprintln!("[favorites] Failed to copy entry {id}: {e}");
                    }
                });
                Task::perform(
                    async {
                        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                    },
                    |_| cosmic::action::app(FavMsg::CloseWindow),
                )
            }
            FavMsg::CloseWindow => {
                std::process::exit(0);
            }
            FavMsg::Edit(id) => Task::perform(
                async move {
                    if let Err(e) = ipc::send_edit_entry_async(id).await {
                        eprintln!("[favorites] Failed to edit entry {id}: {e}");
                    }
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                },
                |_| cosmic::action::app(FavMsg::EditDone),
            ),
            FavMsg::EditDone => Task::perform(
                async { load_favorites_async().await },
                |entries| cosmic::action::app(FavMsg::Loaded(entries)),
            ),
            FavMsg::Loaded(entries) => {
                self.entries = entries;
                self.update_header_title();
                Task::none()
            }
            FavMsg::SearchChanged(query) => {
                self.search = query;
                self.update_header_title();
                Task::none()
            }
            FavMsg::SortBy(col) => {
                if self.sort_col == col {
                    self.sort_dir = match self.sort_dir {
                        SortDir::Asc => SortDir::Desc,
                        SortDir::Desc => SortDir::Asc,
                    };
                } else {
                    self.sort_col = col;
                    self.sort_dir = SortDir::Asc;
                }
                Task::none()
            }
            FavMsg::Unfavorite(id) => {
                // Remove from local list immediately
                self.entries.retain(|(eid, _, _)| *eid != id);
                self.update_header_title();
                // Tell applet via D-Bus
                Task::perform(
                    async move {
                        if let Err(e) = ipc::send_remove_favorite_async(id).await {
                            eprintln!("[favorites] Failed to remove favorite {id}: {e}");
                        }
                    },
                    |_| cosmic::action::app(FavMsg::CommitTitle),
                )
            }
            FavMsg::BeginTitleEdit(id) => {
                // If already editing a different entry, commit the old one first
                if let Some(state) = &self.title_edit {
                    if state.entry_id != id {
                        let task = self.save_title_edit();
                        // Start new edit
                        let current_title = self
                            .entries
                            .iter()
                            .find(|(eid, _, _)| *eid == id)
                            .map(|(_, t, _)| t.clone())
                            .unwrap_or_default();
                        self.title_edit = Some(TitleEditState {
                            entry_id: id,
                            input: current_title.clone(),
                            suggestion: None,
                            suggesting: true,
                            original_title: current_title,
                        });
                        // Kick off AI suggestion
                        let content = self
                            .entries
                            .iter()
                            .find(|(eid, _, _)| *eid == id)
                            .map(|(_, _, p)| p.clone())
                            .unwrap_or_default();
                        let ai_task = Task::perform(
                            async move { ai::suggest_title(&content).await },
                            move |suggestion| {
                                cosmic::action::app(FavMsg::TitleSuggested(id, suggestion))
                            },
                        );
                        return Task::batch([task, ai_task]);
                    }
                    return Task::none();
                }

                let current_title = self
                    .entries
                    .iter()
                    .find(|(eid, _, _)| *eid == id)
                    .map(|(_, t, _)| t.clone())
                    .unwrap_or_default();
                self.title_edit = Some(TitleEditState {
                    entry_id: id,
                    input: current_title.clone(),
                    suggestion: None,
                    suggesting: true,
                    original_title: current_title,
                });
                // Kick off AI suggestion using the entry's content preview
                let content = self
                    .entries
                    .iter()
                    .find(|(eid, _, _)| *eid == id)
                    .map(|(_, _, p)| p.clone())
                    .unwrap_or_default();
                Task::perform(
                    async move { ai::suggest_title(&content).await },
                    move |suggestion| {
                        cosmic::action::app(FavMsg::TitleSuggested(id, suggestion))
                    },
                )
            }
            FavMsg::TitleInput(input) => {
                if let Some(state) = &mut self.title_edit {
                    state.input = input;
                }
                Task::none()
            }
            FavMsg::TitleSuggested(id, suggestion) => {
                if let Some(state) = &mut self.title_edit {
                    if state.entry_id == id {
                        state.suggesting = false;
                        if state.input.is_empty() {
                            // Only set suggestion if user hasn't typed anything yet
                            state.suggestion = suggestion;
                        }
                    }
                }
                Task::none()
            }
            FavMsg::AcceptSuggestion => {
                if let Some(state) = &mut self.title_edit {
                    if let Some(suggestion) = state.suggestion.take() {
                        state.input = suggestion;
                    }
                }
                Task::none()
            }
            FavMsg::CommitTitle => {
                // Called from text_input on_submit (Enter) or after save completes
                if self.title_edit.is_some() {
                    return self.save_title_edit();
                }
                Task::none()
            }
            FavMsg::CancelTitleEdit => {
                self.title_edit = None;
                Task::none()
            }
            FavMsg::KeyEvent(key) => {
                use cosmic::iced::keyboard::key::Named;
                if self.title_edit.is_some() {
                    match key {
                        Named::Tab | Named::ArrowRight => {
                            // Accept suggestion only if there's one and input is empty
                            let should_accept = self.title_edit.as_ref().is_some_and(|s| {
                                s.suggestion.is_some() && s.input.is_empty()
                            });
                            if should_accept {
                                if let Some(state) = &mut self.title_edit {
                                    if let Some(suggestion) = state.suggestion.take() {
                                        state.input = suggestion;
                                    }
                                }
                            }
                        }
                        Named::Enter => {
                            return self.save_title_edit();
                        }
                        Named::Escape => {
                            self.title_edit = None;
                        }
                        _ => {}
                    }
                }
                Task::none()
            }
        }
    }

    fn subscription(&self) -> cosmic::iced_futures::Subscription<Self::Message> {
        cosmic::iced_futures::event::listen_with(|event, _status, _id| {
            if let cosmic::iced::Event::Keyboard(cosmic::iced::keyboard::Event::KeyPressed {
                key,
                ..
            }) = event
            {
                if let cosmic::iced::keyboard::Key::Named(named) = key {
                    return Some(FavMsg::KeyEvent(named));
                }
            }
            None
        })
    }

    fn view(&self) -> Element<'_, Self::Message> {
        let title_width = Length::Fixed(200f32);
        let date_width = Length::Fixed(150f32);

        // Search bar
        let search_bar = container(
            text_input("Search favorites...", &self.search)
                .on_input(FavMsg::SearchChanged)
                .width(Length::Fill),
        )
        .padding(padding::all(12).bottom(4));

        // Column headers
        let headers = container(
            row()
                .spacing(12)
                .align_y(Alignment::Center)
                .push(self.column_header("Title", SortColumn::Title, title_width))
                .push(self.column_header("Content", SortColumn::Content, Length::Fill))
                .push(self.column_header("Date", SortColumn::Date, date_width))
                // Spacer for edit + remove button columns
                .push(container(text("")).width(Length::Fixed(72f32))),
        )
        .padding(padding::all(12).top(4).bottom(0));

        // Separator
        let separator = container(cosmic::widget::divider::horizontal::default())
            .padding(padding::all(12).top(4).bottom(4));

        // Entry rows
        let visible = self.visible_entries();
        let items: Vec<Element<'_, Self::Message>> = visible
            .iter()
            .map(|(id, title, preview)| {
                let entry_id = *id;
                let date_str = Self::format_timestamp(*id);
                let is_editing = self
                    .title_edit
                    .as_ref()
                    .is_some_and(|s| s.entry_id == entry_id);

                // Title cell: either inline editor or clickable text
                let title_cell: Element<'_, FavMsg> = if is_editing {
                    let state = self.title_edit.as_ref().unwrap();
                    // Show text_input with AI suggestion as placeholder
                    let placeholder = if state.suggesting {
                        "Suggesting title..."
                    } else if let Some(ref s) = state.suggestion {
                        s.as_str()
                    } else {
                        "Enter title..."
                    };
                    container(
                        text_input(placeholder, &state.input)
                            .on_input(FavMsg::TitleInput)
                            .on_submit(|_| FavMsg::CommitTitle)
                            .width(Length::Fill),
                    )
                    .width(title_width)
                    .into()
                } else {
                    let display_title = if title.is_empty() {
                        "untitled".to_string()
                    } else {
                        title.clone()
                    };
                    // Clickable title — click to edit
                    cosmic::widget::button::custom(
                        text::body(display_title).class(cosmic::style::Text::Accent),
                    )
                    .padding(padding::all(4))
                    .on_press(FavMsg::BeginTitleEdit(entry_id))
                    .class(cosmic::theme::Button::Text)
                    .width(title_width)
                    .into()
                };

                // Content + date (clickable for copy)
                let copy_row = row()
                    .spacing(12)
                    .align_y(Alignment::Center)
                    .push(container(text(preview.as_str())).width(Length::Fill))
                    .push(
                        container(
                            text::caption(date_str)
                                .class(cosmic::style::Text::Default),
                        )
                        .width(date_width),
                    );

                let copy_btn = cosmic::widget::button::custom(copy_row)
                    .padding(padding::all(8))
                    .on_press(FavMsg::Copy(entry_id))
                    .class(cosmic::theme::Button::ListItem)
                    .width(Length::Fill);

                // Edit (pencil) icon button
                let edit_icon = cosmic::widget::icon::from_name("edit-symbolic").size(16);
                let edit_btn = cosmic::widget::button::icon(edit_icon)
                    .extra_small()
                    .on_press(FavMsg::Edit(entry_id))
                    .class(cosmic::theme::Button::Text);

                // Remove (X) icon button
                let remove_icon =
                    cosmic::widget::icon::from_name("window-close-symbolic").size(16);
                let remove_btn = cosmic::widget::button::icon(remove_icon)
                    .extra_small()
                    .on_press(FavMsg::Unfavorite(entry_id))
                    .class(cosmic::theme::Button::Text);

                // Full row: [title] [copy button (content+date)] [edit] [remove]
                row()
                    .spacing(4)
                    .align_y(Alignment::Center)
                    .push(title_cell)
                    .push(copy_btn)
                    .push(edit_btn)
                    .push(remove_btn)
                    .into()
            })
            .collect();

        let content: Element<'_, Self::Message> = if items.is_empty() {
            container(text(if self.search.is_empty() {
                "No favorites yet"
            } else {
                "No matching favorites"
            }))
            .padding(40)
            .align_x(Alignment::Center)
            .width(Length::Fill)
            .into()
        } else {
            scrollable(
                column::with_children(items)
                    .spacing(2)
                    .padding(padding::all(12).top(0).right(24)),
            )
            .into()
        };

        column()
            .push(search_bar)
            .push(headers)
            .push(separator)
            .push(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}

async fn load_favorites_async() -> Vec<(i64, String, String)> {
    match ipc::send_list_favorites_async().await {
        Ok(entries) => entries,
        Err(e) => {
            eprintln!("[favorites] Failed to load favorites: {e}");
            Vec::new()
        }
    }
}

pub fn run_favorites() {
    let settings = cosmic::app::Settings::default()
        .size(cosmic::iced::Size::new(1200.0, 600.0))
        .transparent(false);

    let flags = FavoritesFlags;

    if let Err(e) = cosmic::app::run::<FavoritesApp>(settings, flags) {
        eprintln!("Favorites window failed: {e}");
        std::process::exit(1);
    }
}
