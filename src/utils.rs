use std::cmp::min;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{borrow::Cow, time::Duration};

use cosmic::{app::Message, iced::Padding, iced_runtime::command::Action, Command};

pub fn formated_value(value: &str, max_lines: usize, max_chars: usize) -> Cow<str> {
    let value = value.trim();

    if value.lines().count() <= max_lines && value.len() <= max_chars {
        Cow::from(value)
    } else {
        let mut str = String::with_capacity(min(value.len(), max_chars + 7));

        let mut lines = value.lines();

        let mut lines_count = 0;

        while lines_count < max_lines && str.len() < max_chars {
            let Some(line) = lines.next() else {
                break;
            };

            if lines_count > 0 {
                str.push('\n');
            }

            str.push_str(split_at(line.trim(), max_chars - str.len()));

            lines_count += 1;
        }

        str.push_str("...");

        Cow::from(str)
    }
}

fn split_at(str: &str, n: usize) -> &str {
    if str.len() > n {
        let mut i = n;
        loop {
            if let Some((left, _)) = split_at_checked(str, i) {
                return left;
            }
            i -= 1;
        }
    } else {
        str
    }
}

// https://github.com/rust-lang/rust/issues/119128
pub fn split_at_checked(s: &str, mid: usize) -> Option<(&str, &str)> {
    // is_char_boundary checks that the index is in [0, .len()]
    if s.is_char_boundary(mid) {
        // SAFETY: just checked that `mid` is on a char boundary.
        Some(unsafe { (s.get_unchecked(0..mid), s.get_unchecked(mid..s.len())) })
    } else {
        None
    }
}

pub fn horizontal_padding(value: f32) -> Padding {
    Padding {
        top: 0f32,
        right: value,
        bottom: 0f32,
        left: value,
    }
}

pub fn vertical_padding(value: f32) -> Padding {
    Padding {
        top: value,
        right: 0f32,
        bottom: value,
        left: 0f32,
    }
}

pub fn command_message<M: Send + 'static>(message: M) -> Command<Message<M>> {
    Command::single(Action::Future(Box::pin(async {
        cosmic::app::Message::App(message)
    })))
}

pub fn now_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_millis()
}
