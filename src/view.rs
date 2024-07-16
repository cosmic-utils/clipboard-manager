use std::borrow::Cow;

use cosmic::{
    iced::{Alignment, Length, Padding},
    iced_widget::{graphics::image::image_rs::flat::View, Row, Scrollable},
    theme::{self, Button},
    widget::{
        self,
        button::{self, button},
        column, container, context_menu,
        icon::{self, Handle},
        menu, mouse_area, text, text_input, toggler, Column, Container, Icon, MouseArea, Space,
        Text, TextEditor,
    },
    Element,
};

use crate::{
    app::{AppState, ClipboardState},
    config::Config,
    db::{Content, Entry},
    fl,
    message::AppMessage,
    utils::{formated_value, horizontal_padding},
};

pub fn quick_settings_view<'a>(
    _state: &'a AppState,
    config: &'a Config,
) -> Element<'a, AppMessage> {
    fn toogle_settings<'a>(
        info: impl Into<Cow<'a, str>> + 'a,
        value: bool,
        f: impl Fn(bool) -> AppMessage + 'a,
    ) -> Element<'a, AppMessage> {
        Row::new()
            .push(text(info))
            .push(Space::with_width(Length::Fill))
            .push(toggler(None, value, f))
            .into()
    }

    Column::new()
        .width(Length::Fill)
        .spacing(20)
        .padding(10)
        .push(toogle_settings(
            fl!("incognito"),
            config.private_mode,
            AppMessage::PrivateMode,
        ))
        .push(widget::button::destructive(fl!("clear_entries")).on_press(AppMessage::Clear))
        .into()
}

pub fn popup_view<'a>(state: &'a AppState, _config: &'a Config) -> Element<'a, AppMessage> {
    Column::new()
        .width(Length::Fill)
        .spacing(20)
        .padding(10)
        .push(top_view(state))
        .push(entries(state))
        .into()
}

fn top_view(state: &AppState) -> Element<AppMessage> {
    let mut padding = Padding::new(10f32);
    padding.bottom = 0f32;

    let input = text_input::search_input(fl!("search_entries"), state.db.query())
        .always_active()
        .on_input(AppMessage::Search)
        .on_paste(AppMessage::Search)
        .on_clear(AppMessage::Search("".into()));

    container(input).padding(padding).into()
}

fn entries(state: &AppState) -> Element<'_, AppMessage> {
    let entries_view: Vec<_> = if state.db.query().is_empty() {
        state
            .db
            .iter()
            .enumerate()
            .filter_map(|(pos, data)| match data.get_content() {
                Ok(c) => match c {
                    Content::Text(text) => {
                        if text.is_empty() {
                            None
                        } else {
                            Some(text_entry(data, pos == state.focused, text))
                        }
                    }
                },
                Err(_) => None,
            })
            .collect()
    } else {
        state
            .db
            .search_iter()
            .enumerate()
            .filter_map(|(pos, (data, indices))| match data.get_content() {
                Ok(c) => match c {
                    Content::Text(text) => {
                        if text.is_empty() {
                            None
                        } else {
                            Some(text_entry_with_indices(
                                data,
                                pos == state.focused,
                                text,
                                indices,
                            ))
                        }
                    }
                },
                Err(_) => None,
            })
            .collect()
    };

    let mut padding = horizontal_padding(10f32);
    // try to fix scroll bar
    padding.right += 10f32;

    let column = column::with_children(entries_view)
        .spacing(5f32)
        .padding(padding);

    Scrollable::new(column)
        .height(Length::FillPortion(2))
        .into()
}

fn image_entry(_entry: &Entry, _is_focused: bool) -> Element<'_, AppMessage> {
    todo!()
}

fn text_entry_with_indices<'a>(
    entry: &'a Entry,
    is_focused: bool,
    content: &'a str,
    _indices: &'a [u32],
) -> Element<'a, AppMessage> {
    text_entry(entry, is_focused, content)
}

fn text_entry<'a>(entry: &'a Entry, is_focused: bool, content: &'a str) -> Element<'a, AppMessage> {
    let content = text(formated_value(content, 5, 200));

    let btn = cosmic::widget::button(content)
        .width(Length::Fill)
        .on_press(AppMessage::Copy(entry.clone()))
        .padding([8, 16])
        .style(Button::Custom {
            active: Box::new(move |focused, theme| {
                let rad_s = theme.cosmic().corner_radii.radius_s;
                let focused = is_focused || focused;

                let a = if focused {
                    button::StyleSheet::hovered(theme, focused, focused, &Button::Text)
                } else {
                    button::StyleSheet::active(theme, focused, focused, &Button::Standard)
                };
                button::Appearance {
                    border_radius: rad_s.into(),
                    outline_width: 0.0,
                    ..a
                }
            }),
            hovered: Box::new(move |focused, theme| {
                let focused = is_focused || focused;
                let rad_s = theme.cosmic().corner_radii.radius_s;

                let text = button::StyleSheet::hovered(theme, focused, focused, &Button::Text);
                button::Appearance {
                    border_radius: rad_s.into(),
                    outline_width: 0.0,
                    ..text
                }
            }),
            disabled: Box::new(|theme| button::StyleSheet::disabled(theme, &Button::Text)),
            pressed: Box::new(move |focused, theme| {
                let focused = is_focused || focused;
                let rad_s = theme.cosmic().corner_radii.radius_s;

                let text = button::StyleSheet::pressed(theme, focused, focused, &Button::Text);
                button::Appearance {
                    border_radius: rad_s.into(),
                    outline_width: 0.0,
                    ..text
                }
            }),
        });

    context_menu(
        btn,
        Some(vec![menu::Tree::new(
            button(text(fl!("delete_entry")))
                .on_press(AppMessage::Delete(entry.clone()))
                .width(Length::Fill)
                .style(Button::Destructive),
        )]),
    )
    .into()
}
