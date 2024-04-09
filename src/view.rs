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
    db::Data,
    message::AppMessage,
    utils::{formated_value, horizontal_padding},
};

impl AppState {
    pub fn view(&self) -> Element<AppMessage> {
        let content = Column::new()
            .width(Length::Fill)
            .spacing(20)
            .padding(10)
            .push(self.top_view())
            .push(Self::entry_list_view(self.db.iter(), self.focused))
            .push(self.bottom_view());

        content.into()
    }

    fn bottom_view(&self) -> Element<AppMessage> {
        let private_mode = toggler(
            "Incognito".to_string(),
            self.private_mode,
            AppMessage::PrivateMode,
        );
        let space = widget::horizontal_space(Length::Fill);
        let row = widget::row::with_capacity(2).push(space).push(private_mode);
        row.padding([0, 10, 10, 10]).into()
    }

    fn top_view(&self) -> Element<AppMessage> {
        let mut row = Vec::new();

        let text_input = text_input::search_input("value", self.db.query())
            .on_input(AppMessage::Search)
            .on_paste(AppMessage::Search)
            .on_clear(AppMessage::Search("".into()))
            .into();

        row.push(text_input);

        row.push(Space::with_width(Length::Fill).into());

        let clear_button = cosmic::widget::button("Clear")
            .on_press(AppMessage::Clear)
            .style(theme::Button::Destructive)
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

    fn entry_list_view<'a, I>(entries: I, focused: usize) -> Element<'a, AppMessage>
    where
        I: Iterator<Item = &'a Data>,
    {
        let entry_view = |index: usize, data: &'a Data| -> Element<'a, AppMessage> {
            let is_focused = focused == index;

            let icon_bytes = include_bytes!("../resources/icons/close24.svg") as &[u8];

            let icon = icon::from_svg_bytes(icon_bytes);

            let delete_button = cosmic::widget::button::icon(icon)
                .extra_small()
                .on_press(AppMessage::Delete(data.clone()))
                .style(theme::Button::Destructive);

            let content = Row::new()
                .align_items(Alignment::Center)
                .push(
                    // todo: remove this fixed size
                    Container::new(
                        text(formated_value(&data.value, 2, 50)).width(Length::Fixed(300f32)),
                    ),
                )
                .push(Space::with_width(Length::Fill))
                .push(delete_button);

            let card = Container::new(content)
                .padding(10f32)
                .style(cosmic::theme::Container::Card);

            MouseArea::new(card)
                .on_release(AppMessage::OnClick(data.clone()))
                .into()
        };

        let entries_view = entries
            .enumerate()
            .map(|(index, data)| entry_view(index, data));

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
}
