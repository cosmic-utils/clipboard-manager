use std::borrow::Cow;
use std::cmp::min;

use chrono::Utc;
use cosmic::{Action, Task};

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

pub fn task_message<M: Send + 'static>(message: M) -> Task<Action<M>> {
    Task::done(cosmic::action::app(message))
}

pub fn now_millis() -> i64 {
    Utc::now().timestamp_millis()
}
