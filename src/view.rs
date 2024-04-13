use std::borrow::Cow;

use cosmic::{
    iced::{Alignment, Length, Padding},
    iced_widget::{column, graphics::image::image_rs::flat::View, Row, Scrollable},
    theme::{self, Button},
    widget::{
        self, button, container,
        icon::{self, Handle},
        mouse_area, text, text_input, toggler, Column, Container, Icon, MouseArea, Space,
    },
    Element,
};

use crate::{
    app::{AppState, ClipboardState},
    config::Config,
    db::Data,
    message::AppMessage,
    my_widgets,
    utils::{formated_value, horizontal_padding},
};

pub fn quick_settings_view<'a>(
    _state: &'a AppState,
    config: &'a Config,
) -> Element<'a, AppMessage> {
    Column::new()
        .width(Length::Fill)
        .spacing(20)
        .padding(10)
        .push(toggler(
            "Incognito".to_string(),
            config.private_mode,
            AppMessage::PrivateMode,
        ))
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
    let mut row = Vec::new();

    let text_input = text_input::search_input("value", state.db.query())
        .on_input(AppMessage::Search)
        .on_paste(AppMessage::Search)
        .on_clear(AppMessage::Search("".into()))
        .width(Length::FillPortion(8))
        .into();

    row.push(text_input);

    row.push(Space::with_width(Length::Fill).into());

    let clear_button = widget::button::destructive("Clear")
        .on_press(AppMessage::Clear)
        .into();

    row.push(clear_button);

    let mut padding = Padding::new(10f32);
    padding.bottom = 0f32;

    Row::with_children(row)
        .width(Length::Fill)
        .align_items(Alignment::Center)
        .padding(padding)
        .into()
}

fn entries<'a>(state: &'a AppState) -> Element<'a, AppMessage> {
    let entries_view = state
        .db
        .iter()
        .enumerate()
        .filter(|(_, data)| !data.value.is_empty())
        .map(|(index, data)| {
            let more_action = if let Some(d) = &state.more_action {
                d == data
            } else {
                false
            };

            entry(data, index == state.focused, more_action)
        });

    let mut padding = horizontal_padding(10f32);
    // try to fix scroll bar
    padding.right += 10f32;

    let column = Column::with_children(entries_view)
        .spacing(10f32)
        .padding(padding);

    Scrollable::new(column)
        .height(Length::FillPortion(2))
        .into()
}

fn entry<'a>(
    entry: &'a Data,
    is_focused: bool,
    more_action_expanded: bool,
) -> Element<'a, AppMessage> {
    let content = text(formated_value(&entry.value, 2, 50)).width(Length::Fixed(300f32));

    let btn = mouse_area(
        cosmic::widget::button(content)
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
            }),
    )
    .on_right_release(AppMessage::MoreAction(Some(entry.clone())));

    if more_action_expanded {
        let overlay = Column::new()
            .push(
                button("Delete")
                    .on_press(AppMessage::Delete(entry.clone()))
                    .width(Length::Fill)
                    .style(Button::Destructive),
            )
            .spacing(15)
            .padding(7);

        let overlay = container(overlay).style(theme::Container::Dropdown);

        // todo: change it by a context menu instead
        my_widgets::drop_down::DropDown::new(btn, overlay, true)
            .on_dismiss(AppMessage::MoreAction(None))
            .alignment(my_widgets::alignment::Alignment::Bottom)
            .width(Length::Fixed(180.0))
            .into()
    } else {
        btn.into()
    }
}
