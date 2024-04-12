use std::borrow::Cow;

use cosmic::{
    iced::{Alignment, Length, Padding},
    iced_widget::{column, graphics::image::image_rs::flat::View, Row, Scrollable},
    theme,
    widget::{
        self,
        icon::{self, Handle},
        mouse_area, text, text_input, toggler, Button, Column, Container, Icon, MouseArea, Space,
    },
    Element,
};

use crate::{
    app::{AppState, ClipboardState},
    config::Config,
    db::Data,
    message::AppMessage,
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


    fn entry<'a>(entry: &'a Data, _focused: bool) -> Element<'a, AppMessage> {

        let icon_bytes = include_bytes!("../resources/icons/close24.svg") as &[u8];

        let icon = icon::from_svg_bytes(icon_bytes);

        let delete_button = widget::button::icon(icon)
            .extra_small()
            .on_press(AppMessage::Delete(entry.clone()))
            .style(theme::Button::Destructive);

        let content = Row::new()
            .align_items(Alignment::Center)
            .push(
                // todo: remove this fixed size
                Container::new(
                    text(formated_value(&entry.value, 2, 50)).width(Length::Fixed(300f32)),
                ),
            )
            .push(Space::with_width(Length::Fill))
            .push(delete_button);

        let card = Container::new(content)
            .padding(10f32)
            .style(cosmic::theme::Container::Card);

        MouseArea::new(card)
            .on_release(AppMessage::OnClick(entry.clone()))
            .into()
    }   

    fn entries<'a, I>(entries: I, focused: usize) -> Element<'a, AppMessage>
    where
        I: Iterator<Item = &'a Data>,
    {
       
        let entries_view = entries
            .enumerate()
            .filter(|(_, data)| !data.value.is_empty())
            .map(|(index, data)| entry(data, index == focused));

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

    Column::new()
        .width(Length::Fill)
        .spacing(20)
        .padding(10)
        .push(top_view(state))
        .push(entries(state.db.iter(), state.focused))
        .into()
}
