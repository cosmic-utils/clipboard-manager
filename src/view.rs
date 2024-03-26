use std::borrow::Cow;

use cosmic::{
    iced::{Alignment, Length, Padding},
    iced_widget::{column, Row, Scrollable},
    theme,
    widget::{mouse_area, text, text_input, Button, Column, Container, MouseArea, Space},
    Element,
};

use crate::{db::Data, utils::formated_value, window::Message};

fn entry_view(data: &Data) -> Element<Message> {
    let delete_button = Button::new(text("Delete"))
        .on_press(Message::Delete(data.clone()))
        .style(theme::Button::Destructive);

    let content = Row::new()
        .align_items(Alignment::Center)
        .push(
            // todo: remove this fixed size
            Container::new(text(formated_value(&data.value, 2, 50)).width(Length::Fixed(300f32))),
        )
        .push(Space::with_width(Length::Fill))
        .push(delete_button)
        .padding(5f32);

    let card = Container::new(content).style(cosmic::theme::Container::Card);

    MouseArea::new(card)
        .on_release(Message::OnClick(data.clone()))
        .into()
}

// todo: padding scroll bar
fn entry_list_view<'a, I>(entries: I) -> Element<'a, Message>
where
    I: Iterator<Item = &'a Data>,
{
    let entries_view = entries.map(|data| entry_view(data));

    let column = Column::with_children(entries_view).spacing(5);

    Scrollable::new(column)
        .height(Length::FillPortion(2))
        .into()
}

fn query_view(query: &str) -> Element<Message> {
    text_input::search_input("value", query)
        .on_input(Message::Query)
        .on_paste(Message::Query)
        .on_clear(Message::Query("".into()))
        .into()
}

pub fn windows_view<'a, I>(query: &'a str, entries: I) -> Element<'a, Message>
where
    I: Iterator<Item = &'a Data>,
{
    let content = Column::new()
        .push(query_view(query))
        .push(Space::with_height(20))
        .push(entry_list_view(entries))
        .padding(Padding::new(10f32));

    mouse_area(content)
        .on_release(Message::TogglePopup)
        .on_right_release(Message::TogglePopup)
        .into()
}
