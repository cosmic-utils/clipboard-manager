use std::borrow::Cow;

pub fn formated_value(value: &str, max_lines: usize, max_chars: usize) -> Cow<str> {
    if value.lines().count() <= max_lines && value.len() <= max_chars {
        Cow::from(value.trim())
    } else {
        let mut str = String::with_capacity(max_chars + 3);

        let mut lines = value.trim().lines();

        let mut current_ligne = 0;

        while current_ligne < max_lines && str.len() < max_chars {
            let Some(line) = lines.next() else {
                break;
            };

            if current_ligne > 0 {
                str.push('\n');
            }

            str.push_str(split_at(line.trim(), max_chars - str.len()));

            current_ligne += 1;
        }

        str.push_str("...");

        Cow::from(str)
    }
}

fn split_at(str: &str, n: usize) -> &str {
    if str.len() > n {
        str.split_at(n).0
    } else {
        str
    }
}
