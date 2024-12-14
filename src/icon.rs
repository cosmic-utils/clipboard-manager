#![allow(dead_code)]

use cosmic::{
    iced_core::Length,
    widget::{self, icon::Handle, Icon, IconButton},
};

pub static ICON_LENGTH: Length = Length::Fixed(25.0);

#[macro_export]
macro_rules! icon_handle {
    ($name:literal) => {{
        let bytes = include_bytes!(concat!("../res/icons/", $name, ".svg"));
        cosmic::widget::icon::from_svg_bytes(bytes).symbolic(true)
    }};
}

#[macro_export]
macro_rules! icon {
    ($name:literal) => {{
        use $crate::icon::ICON_LENGTH;
        use $crate::icon_handle;

        cosmic::widget::icon::icon(icon_handle!($name))
            .height(ICON_LENGTH)
            .width(ICON_LENGTH)
    }};
}
#[macro_export]
macro_rules! icon_button {
    ($name:literal) => {{
        use $crate::icon_handle;
        cosmic::widget::button::icon(icon_handle!($name))
    }};
}

pub fn icon_from_handle(handle: Handle) -> Icon {
    widget::icon::icon(handle)
        .height(ICON_LENGTH)
        .width(ICON_LENGTH)
}

pub fn icon_button_from_handle<'a, M>(handle: Handle) -> IconButton<'a, M> {
    cosmic::widget::button::icon(handle)
}
