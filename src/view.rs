use std::{borrow::Cow, cmp::min, path::PathBuf};

use cosmic::{
    iced::{Alignment, Length, Padding},
    iced_widget::{
        graphics::image::image_rs::flat::View,
        qr_code,
        scrollable::{Direction, Properties},
        QRCode, Row, Scrollable,
    },
    prelude::CollectionWidget,
    theme::{self, Button},
    widget::{
        self,
        button::{self},
        column, container, context_menu, flex_row, grid,
        icon::{self, Handle},
        image, menu, mouse_area, row, text, text_input, toggler, Column, Container, Icon,
        MouseArea, Space, Text, TextEditor,
    },
    Element,
};

use anyhow::{anyhow, bail, Result};

use crate::{
    app::{AppState, ClipboardState},
    config::Config,
    db::{Content, Entry},
    fl,
    message::{AppMsg, ConfigMsg},
    utils::{formatted_value, horizontal_padding, vertical_padding},
};

#[macro_export]
macro_rules! icon {
    ($name:literal) => {{
        let bytes = include_bytes!(concat!("../../res/icons/", $name, "px.svg"));
        cosmic::widget::icon::from_svg_bytes(bytes)
    }};
}

impl AppState {
    pub fn quick_settings_view(&self) -> Element<'_, AppMsg> {
        fn toggle_settings<'a>(
            info: impl Into<Cow<'a, str>> + 'a,
            value: bool,
            f: impl Fn(bool) -> AppMsg + 'a,
        ) -> Element<'a, AppMsg> {
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
            .push(toggle_settings(
                fl!("incognito"),
                self.config.private_mode,
                |v| AppMsg::Config(ConfigMsg::PrivateMode(v)),
            ))
            .push(toggle_settings(
                fl!("horizontal_layout"),
                self.config.horizontal,
                |v| AppMsg::Config(ConfigMsg::Horizontal(v)),
            ))
            .push(widget::button::destructive(fl!("clear_entries")).on_press(AppMsg::Clear))
            .into()
    }

    pub fn popup_view(&self) -> Element<'_, AppMsg> {
        Column::new()
            .push(self.top_bar())
            .push(self.content())
            .width(Length::Fill)
            .height(Length::Fill)
            .spacing(20)
            .padding(10)
            .align_items(Alignment::Center)
            .into()
    }

    fn top_bar(&self) -> Element<'_, AppMsg> {
        let content: Element<_> = match self.qr_code.is_none() {
            true => text_input::search_input(fl!("search_entries"), self.db.query())
                .always_active()
                .on_input(AppMsg::Search)
                .on_paste(AppMsg::Search)
                .on_clear(AppMsg::Search("".into()))
                .width(match self.config.horizontal {
                    true => Length::Fixed(250f32),
                    false => Length::Fill,
                })
                .into(),
            false => button::text(fl!("return_to_clipboard"))
                .on_press(AppMsg::ReturnToClipboard)
                .width(match self.config.horizontal {
                    true => Length::Shrink,
                    false => Length::Fill,
                })
                .into(),
        };

        let mut padding = Padding::new(10f32);
        padding.bottom = 0f32;

        let content = container(content).padding(padding);

        content.into()
    }

    fn content(&self) -> Element<'_, AppMsg> {
        match &self.qr_code {
            Some(qr_code) => {
                let qr_code_content: Element<_> = match qr_code {
                    Ok(c) => QRCode::new(c).into(),
                    Err(()) => text(fl!("qr_code_error")).into(),
                };

                return container(qr_code_content)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .center_x()
                    .center_y()
                    .into();
            }
            None => {
                let entries_view: Vec<_> = if self.db.query().is_empty() {
                    self.db
                        .iter()
                        .enumerate()
                        .filter_map(|(pos, data)| match data.get_content() {
                            Ok(c) => match c {
                                Content::Text(text) => {
                                    self.text_entry(data, pos == self.focused, text)
                                }
                                Content::Image(image) => {
                                    self.image_entry(data, pos == self.focused, image)
                                }
                                Content::UriList(uris) => {
                                    self.uris_entry(data, pos == self.focused, &uris)
                                }
                            },
                            Err(_) => None,
                        })
                        .collect()
                } else {
                    self.db
                        .search_iter()
                        .enumerate()
                        .filter_map(|(pos, (data, indices))| match data.get_content() {
                            Ok(c) => match c {
                                Content::Text(text) => self.text_entry_with_indices(
                                    data,
                                    pos == self.focused,
                                    text,
                                    indices,
                                ),
                                Content::Image(image) => {
                                    self.image_entry(data, pos == self.focused, image)
                                }
                                Content::UriList(uris) => {
                                    self.uris_entry(data, pos == self.focused, &uris)
                                }
                            },
                            Err(_) => None,
                        })
                        .collect()
                };

                if self.config.horizontal {
                    // try to fix scroll bar
                    let padding = Padding {
                        top: 0f32,
                        right: 10f32,
                        bottom: 20f32,
                        left: 10f32,
                    };

                    let column = row::with_children(entries_view)
                        .spacing(5f32)
                        .padding(padding);

                    Scrollable::new(column)
                        .direction(Direction::Horizontal(Properties::default()))
                        .into()
                } else {
                    // try to fix scroll bar
                    let padding = Padding {
                        top: 0f32,
                        right: 20f32,
                        bottom: 0f32,
                        left: 10f32,
                    };

                    let column = column::with_children(entries_view)
                        .spacing(5f32)
                        .padding(padding);

                    Scrollable::new(column)
                        // XXX: why ?
                        .height(Length::FillPortion(2))
                        .into()
                }
            }
        }
    }

    fn image_entry<'a>(
        &'a self,
        entry: &'a Entry,
        is_focused: bool,
        image_data: &'a [u8],
    ) -> Option<Element<'a, AppMsg>> {
        let handle = image::Handle::from_memory(image_data.to_owned());

        Some(self.base_entry(entry, is_focused, image(handle).width(Length::Fill)))
    }

    fn uris_entry<'a>(
        &'a self,
        entry: &'a Entry,
        is_focused: bool,
        uris: &[&'a str],
    ) -> Option<Element<'a, AppMsg>> {
        if uris.is_empty() {
            return None;
        }

        let max = 3;

        let mut lines = Vec::with_capacity(min(uris.len(), max + 1));

        for uri in uris.iter().take(max) {
            lines.push(text(*uri).into());
        }

        if uris.len() > max {
            lines.push(text("...").into());
        }

        Some(self.base_entry(
            entry,
            is_focused,
            column::with_children(lines).width(Length::Fill),
        ))
    }

    fn text_entry_with_indices<'a>(
        &'a self,
        entry: &'a Entry,
        is_focused: bool,
        content: &'a str,
        _indices: &'a [u32],
    ) -> Option<Element<'a, AppMsg>> {
        self.text_entry(entry, is_focused, content)
    }

    fn text_entry<'a>(
        &'a self,
        entry: &'a Entry,
        is_focused: bool,
        content: &'a str,
    ) -> Option<Element<'a, AppMsg>> {
        if content.is_empty() {
            return None;
        }
        // todo: remove this max line things: display the maximum
        if self.config.horizontal {
            Some(self.base_entry(entry, is_focused, text(formatted_value(content, 10, 500))))
        } else {
            Some(self.base_entry(entry, is_focused, text(formatted_value(content, 5, 200))))
        }
    }

    fn base_entry<'a>(
        &'a self,
        entry: &'a Entry,
        is_focused: bool,
        content: impl Into<Element<'a, AppMsg>>,
    ) -> Element<'a, AppMsg> {
        let btn = button::custom(content)
            .on_press(AppMsg::Copy(entry.clone()))
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

        let btn: Element<_> = if self.config.horizontal {
            container(btn.width(Length::Fill).height(Length::Fill))
                .max_width(350f32)
                .into()
        } else {
            btn.width(Length::Fill).into()
        };

        context_menu(
            btn,
            Some(vec![
                menu::Tree::new(
                    button::text(fl!("delete_entry"))
                        .on_press(AppMsg::Delete(entry.clone()))
                        .width(Length::Fill)
                        .style(Button::Destructive),
                ),
                menu::Tree::new(
                    button::text(fl!("show_qr_code"))
                        .on_press(AppMsg::ShowQrCode(entry.clone()))
                        .width(Length::Fill),
                ),
                if entry.is_favorite {
                    menu::Tree::new(
                        button::text(fl!("remove_favorite"))
                            .on_press(AppMsg::RemoveFavorite(entry.clone()))
                            .width(Length::Fill),
                    )
                } else {
                    menu::Tree::new(
                        button::text(fl!("add_favorite"))
                            .on_press(AppMsg::AddFavorite(entry.clone()))
                            .width(Length::Fill),
                    )
                },
            ]),
        )
        .into()
    }
}
