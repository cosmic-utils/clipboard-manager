use std::{borrow::Cow, cmp::min, fs, sync::LazyLock};

use cosmic::{
    Apply, Element,
    iced::{Alignment, Length, alignment::Horizontal, padding},
    iced_widget::{
        Stack,
        scrollable::{Direction, Scrollbar},
    },
    theme::Button,
    widget::{
        self, Id,
        button::{self},
        column, container, horizontal_space, image, markdown, row, scrollable, text, text_input,
        toggler,
    },
};
use itertools::Itertools;

use crate::{
    app::{AppState, ClipboardState, ErrorState},
    db::{Content, DbTrait, EntryTrait},
    fl, icon, icon_button,
    message::{AppMsg, ConfigMsg, ContextMenuMsg},
    my_widget,
    utils::formatted_value,
};

pub static SCROLLABLE_ID: LazyLock<Id> = LazyLock::new(|| Id::new("scrollable"));

impl<Db: DbTrait> AppState<Db> {
    pub fn quick_settings_view(&self) -> Element<'_, AppMsg> {
        fn toggle_settings<'a>(
            info: impl Into<Cow<'a, str>> + 'a,
            value: bool,
            f: impl Fn(bool) -> AppMsg + 'a,
        ) -> Element<'a, AppMsg> {
            row()
                .push(text(info))
                .push(horizontal_space())
                .push(toggler(value).on_toggle(f))
                .into()
        }

        column()
            .width(Length::Fill)
            .spacing(20)
            .padding(10)
            .push(toggle_settings(
                fl!("incognito"),
                self.config.private_mode,
                |v| AppMsg::Config(ConfigMsg::PrivateMode(v)),
            ))
            // .push(toggle_settings(
            //     fl!("horizontal_layout"),
            //     self.config.horizontal,
            //     |v| AppMsg::Config(ConfigMsg::Horizontal(v)),
            // ))
            .push(toggle_settings(
                fl!("unique_session"),
                self.config.unique_session,
                |v| AppMsg::Config(ConfigMsg::UniqueSession(v)),
            ))
            .push(button::destructive(fl!("clear_entries")).on_press(AppMsg::Clear))
            .into()
    }

    pub fn popup_view(&self) -> Element<'_, AppMsg> {
        container(if let ClipboardState::Error(e) = &self.clipboard_state {
            self.error_view(e)
        } else if let Some(qr_code_res) = &self.qr_code {
            self.qr_code_view(qr_code_res)
        } else {
            self.list_view()
        })
        .height(if self.config.horizontal {
            Length::Fill
        } else {
            Length::Fixed(530f32)
        })
        .width(if self.config.horizontal {
            Length::Fill
        } else {
            Length::Fixed(400f32)
        })
        .into()
    }
    pub fn page_count(&self) -> usize {
        self.db.len() / self.config.maximum_entries_by_page.get() as usize
    }

    fn list_view(&self) -> Element<'_, AppMsg> {
        column()
            .push(
                container(
                    row()
                        .push(
                            text_input::search_input(fl!("search_entries"), self.db.get_query())
                                .always_active()
                                .on_input(AppMsg::Search)
                                .on_paste(AppMsg::Search)
                                .on_clear(AppMsg::Search("".into()))
                                .width(match self.config.horizontal {
                                    true => Length::Fixed(250f32),
                                    false => Length::Fill,
                                }),
                        )
                        .push(horizontal_space().width(5))
                        .push(icon_button!("arrow_back_ios_new24").on_press_maybe(
                            if self.page > 0 {
                                Some(AppMsg::PreviousPage)
                            } else {
                                None
                            },
                        ))
                        .push(icon_button!("arrow_forward_ios24").on_press_maybe(
                            if self.page < self.page_count() {
                                Some(AppMsg::NextPage)
                            } else {
                                None
                            },
                        )),
                )
                .padding(padding::all(15f32).bottom(0)),
            )
            .push(
                container({
                    let maximum_entries_by_page =
                        self.config.maximum_entries_by_page.get() as usize;
                    let range = self.page * maximum_entries_by_page
                        ..(self.page + 1) * maximum_entries_by_page;

                    let entries_view: Vec<_> = self
                        .db
                        .either_iter()
                        .enumerate()
                        .get(range)
                        .map(|(pos, data)| {
                            match data.preferred_content(&self.preferred_mime_types_regex) {
                                Some((_, content)) => match content {
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
                                None => self.unknown_entry(data, pos == self.focused),
                            }
                        })
                        .collect();

                    if self.config.horizontal {
                        let column = row::with_children(entries_view)
                            .spacing(5f32)
                            .padding(padding::bottom(10))
                            .width(Length::Shrink)
                            .apply(Element::from);

                        cosmic::iced::widget::Scrollable::with_direction(
                            column,
                            Direction::Horizontal(Scrollbar::new()),
                        )
                        .scroller_width(8.0)
                        .scrollbar_width(8.0)
                        // scrollable::horizontal(column)
                        // .id(SCROLLABLE_ID.clone())
                        .apply(Element::from)
                    } else {
                        let column = column::with_children(entries_view)
                            .spacing(5f32)
                            .padding(padding::right(10));

                        scrollable(column)
                            // .id(SCROLLABLE_ID.clone())
                            // XXX: why ?
                            // .height(Length::FillPortion(2))
                            .into()
                    }
                })
                .padding(padding::all(20).top(0)),
            )
            .spacing(20)
            .align_x(Alignment::Center)
            .into()
    }

    fn qr_code_view<'a>(
        &'a self,
        qr_code: &'a Result<cosmic::widget::qr_code::Data, ()>,
    ) -> Element<'a, AppMsg> {
        column()
            .push(
                container(
                    button::text(fl!("return_to_clipboard"))
                        .on_press(AppMsg::ReturnToClipboard)
                        .width(match self.config.horizontal {
                            true => Length::Shrink,
                            false => Length::Fill,
                        }),
                )
                .padding(padding::all(15f32).bottom(0)),
            )
            .push(
                container(
                    container(match qr_code {
                        Ok(c) => widget::qr_code(c).apply(Element::from),
                        Err(()) => text(fl!("qr_code_error")).into(),
                    })
                    .center(Length::Fill),
                )
                .padding(padding::all(20).top(0)),
            )
            .spacing(20)
            .align_x(Alignment::Center)
            .into()
    }

    fn error_view(&self, error: &ErrorState) -> Element<'_, AppMsg> {
        match error {
            ErrorState::MissingDataControlProtocol => {
                let command = r#"```sh
echo 'export COSMIC_DATA_CONTROL_ENABLED=1' | sudo tee /etc/profile.d/data_control_cosmic.sh > /dev/null
```"#;

                let content = format!(
                    "### {}\n\n{}\n\n{}\n\n{command}",
                    fl!("data_control", "title"),
                    fl!("data_control", "explanation"),
                    fl!("data_control", "cosmic")
                );

                let items = markdown::parse(&content).collect_vec();

                let e = markdown::view(
                    &items,
                    markdown::Settings::default(),
                    markdown::Style::from_palette(cosmic::iced::Theme::TokyoNightStorm.palette()),
                )
                .map(AppMsg::LinkClicked)
                .apply(Element::from);

                container(e)
                    .align_top(Length::Fill)
                    .center_x(Length::Fill)
                    .padding(15)
                    .apply(Element::from)
            }
            ErrorState::Other(e) => text(format!("{e}")).into(),
        }
    }

    fn image_entry<'a>(
        &'a self,
        entry: &'a Db::Entry,
        is_focused: bool,
        image_data: &'a [u8],
    ) -> Element<'a, AppMsg> {
        let handle = image::Handle::from_bytes(image_data.to_owned());

        self.base_entry(entry, is_focused, image(handle).width(Length::Fill))
    }

    fn uris_entry<'a>(
        &'a self,
        entry: &'a Db::Entry,
        is_focused: bool,
        uris: &[&'a str],
    ) -> Element<'a, AppMsg> {
        let max = 3;

        let mut lines = Vec::with_capacity(min(uris.len(), max + 1));

        for uri in uris.iter().take(max) {
            lines.push(text(*uri).into());
        }

        if uris.len() > max {
            lines.push(text("...").into());
        }

        self.base_entry(
            entry,
            is_focused,
            column::with_children(lines).width(Length::Fill),
        )
    }

    fn unknown_entry<'a>(&'a self, entry: &'a Db::Entry, is_focused: bool) -> Element<'a, AppMsg> {
        let len = entry.raw_content().len();
        let max = 3;
        let mut lines = Vec::new();
        lines.push(text(fl!("unknown_mime_types_title")).into());

        for mime in entry.raw_content().keys().take(max) {
            lines.push(text(format!("- {mime}")).into());
        }

        if len > max {
            lines.push(text(format!("... ({})", len - max)).into());
        }

        self.base_entry(
            entry,
            is_focused,
            column::with_children(lines).width(Length::Fill),
        )
    }

    fn text_entry<'a>(
        &'a self,
        entry: &'a Db::Entry,
        is_focused: bool,
        content: &'a str,
    ) -> Element<'a, AppMsg> {
        // todo: remove this max line things: display the maximum
        if self.config.horizontal {
            self.base_entry(entry, is_focused, text(formatted_value(content, 10, 500)))
        } else {
            self.base_entry(entry, is_focused, text(formatted_value(content, 5, 200)))
        }
    }

    fn base_entry<'a>(
        &'a self,
        entry: &'a Db::Entry,
        is_focused: bool,
        content: impl Into<Element<'a, AppMsg>>,
    ) -> Element<'a, AppMsg> {
        let btn = button::custom(content)
            .on_press(AppMsg::Copy(entry.id()))
            .padding([8, 16])
            .class(Button::Custom {
                active: Box::new(move |focused, theme| {
                    let rad_s = theme.cosmic().corner_radii.radius_s;
                    let focused = is_focused || focused;

                    let a = if focused {
                        button::Catalog::hovered(theme, focused, focused, &Button::Text)
                    } else {
                        button::Catalog::active(theme, focused, focused, &Button::Standard)
                    };
                    button::Style {
                        border_radius: rad_s.into(),
                        outline_width: 0.0,
                        ..a
                    }
                }),
                hovered: Box::new(move |focused, theme| {
                    let focused = is_focused || focused;
                    let rad_s = theme.cosmic().corner_radii.radius_s;

                    let text = button::Catalog::hovered(theme, focused, focused, &Button::Text);
                    button::Style {
                        border_radius: rad_s.into(),
                        outline_width: 0.0,
                        ..text
                    }
                }),
                disabled: Box::new(|theme| button::Catalog::disabled(theme, &Button::Text)),
                pressed: Box::new(move |focused, theme| {
                    let focused = is_focused || focused;
                    let rad_s = theme.cosmic().corner_radii.radius_s;

                    let text = button::Catalog::pressed(theme, focused, focused, &Button::Text);
                    button::Style {
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

        let content: Element<_> = if entry.is_favorite() {
            Stack::new()
                .push(btn)
                .push(
                    column()
                        .align_x(Horizontal::Right)
                        .width(Length::Fill)
                        .push(icon!("star24")),
                )
                .into()
        } else {
            btn
        };

        let overlay: Element<_> = column()
            .padding(3)
            .push(if entry.is_favorite() {
                button::text(fl!("remove_favorite"))
                    .on_press(ContextMenuMsg::RemoveFavorite(entry.id()))
            } else {
                button::text(fl!("add_favorite")).on_press(ContextMenuMsg::AddFavorite(entry.id()))
            })
            .push(
                button::text(fl!("show_qr_code")).on_press(ContextMenuMsg::ShowQrCode(entry.id())),
            )
            .push(
                button::text(fl!("delete_entry"))
                    .on_press(ContextMenuMsg::Delete(entry.id()))
                    .class(Button::Destructive),
            )
            .apply(Element::from)
            .map(AppMsg::ContextMenu);

        let overlay = container(overlay)
            .class(cosmic::theme::Container::Card)
            .padding(padding::all(5));

        my_widget::context_menu(content, overlay).into()
    }
}

/*
let items = vec![
            if entry.is_favorite() {
                menu::Item::Button(
                    fl!("remove_favorite"),
                    None,
                    ContextMenuMsg::RemoveFavorite(entry.id()),
                )
            } else {
                menu::Item::Button(
                    fl!("add_favorite"),
                    None,
                    ContextMenuMsg::AddFavorite(entry.id()),
                )
            },
            menu::Item::Button(
                fl!("show_qr_code"),
                None,
                ContextMenuMsg::ShowQrCode(entry.id()),
            ),
            menu::Item::Button(
                fl!("delete_entry"),
                None,
                ContextMenuMsg::Delete(entry.id()),
            ),
        ];

        let tree = menu::items(&HashMap::new(), items);

        context_menu(content, Some(tree)).into()

*/
