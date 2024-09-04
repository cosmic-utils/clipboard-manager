use std::cmp::min;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{borrow::Cow, time::Duration};
use std::{fs, io};

use chrono::Utc;
use cosmic::{app::Message, iced::Padding, iced_runtime::command::Action, Command};

use crate::app::APPID;

pub fn formatted_value(value: &str, max_lines: usize, max_chars: usize) -> Cow<str> {
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
            if let Some((left, _)) = str.split_at_checked(i) {
                return left;
            }
            i -= 1;
        }
    } else {
        str
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

pub fn now_millis() -> i64 {
    Utc::now().timestamp_millis()
}

pub fn remove_dir_contents(dir: &Path) {
    pub fn inner(dir: &Path) -> Result<(), io::Error> {
        for entry in fs::read_dir(dir)?.flatten() {
            let path = entry.path();

            if path.is_dir() {
                let _ = fs::remove_dir_all(&path);
            } else {
                let _ = fs::remove_file(&path);
            }
        }
        Ok(())
    }

    let _ = inner(dir);
}

pub fn find_x_scheme_handler<'a>(a: &'a str) -> Option<&'a str> {
    // Use memchr to find the first occurrence of ':' in the input string.
    if let Some(colon_index) = memchr::memchr(b':', a.as_bytes()) {
        // Check if the colon is followed by "//" to validate the scheme.
        if a[colon_index..].starts_with("://") {
            // If valid, return the scheme as a slice from the start up to the colon.
            return Some(&a[..colon_index]);
        }
    }
    // If no scheme is found, return None.
    None
}

#[test]
fn find_x_scheme_handler_test() {
    assert_eq!(
        find_x_scheme_handler("https://github.com/wiiznokes/clipboard-manager"),
        Some("https")
    );
    
    assert_eq!(
        find_x_scheme_handler("ddg://query%20terms"),
        Some("ddg")
    );
}
