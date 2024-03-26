use std::borrow::Cow;

use cosmic::{
    iced::{Alignment, Length, Padding},
    iced_widget::{column, graphics::image::image_rs::flat::View, Row, Scrollable},
    theme,
    widget::{
        self,
        icon::{self, Handle},
        mouse_area, text, text_input, Button, Column, Container, Icon, MouseArea, Space,
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
            .push(self.top_view())
            .push(Space::with_height(20))
            .padding(Padding::new(10f32));

        let content = if self.query.is_empty() {
            content.push(Self::entry_list_view(self.db.iter().rev()))
        } else {
            content.push(Self::entry_list_view(
                self.db.search(&self.query).iter().copied(),
            ))
        };

        mouse_area(content)
            .on_release(AppMessage::TogglePopup)
            .on_right_release(AppMessage::TogglePopup)
            .into()
    }

    fn top_view(&self) -> Element<AppMessage> {
        let mut row = Vec::new();

        let text_input = text_input::search_input("value", &self.query)
            .on_input(AppMessage::Query)
            .on_paste(AppMessage::Query)
            .on_clear(AppMessage::Query("".into()))
            .into();

        row.push(text_input);

        if self.clipboard_state == ClipboardState::Error {
            let icon_bytes = include_bytes!("../resources/icons/sync_problem24.svg") as &[u8];

            let icon = icon::from_svg_bytes(icon_bytes);

            let retry_button = cosmic::widget::button::icon(icon)
                .on_press(AppMessage::RetryConnectingClipboard)
                .style(theme::Button::Suggested)
                .into();

            row.push(Space::with_width(Length::Fill).into());
            row.push(retry_button);
        }

        let mut padding = Padding::new(10f32);
        padding.bottom = 0f32;

        Row::with_children(row)
            .width(Length::Fill)
            .padding(padding)
            .into()
    }

    fn entry_list_view<'a, I>(entries: I) -> Element<'a, AppMessage>
    where
        I: Iterator<Item = &'a Data>,
    {
        fn entry_view(data: &Data) -> Element<AppMessage> {
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
        }

        let entries_view = entries.map(|data| entry_view(data));

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
