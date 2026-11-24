//! Python string semantics the reference relies on: `str.isspace`, `str.strip`, and the line
//! breaks of `str.splitlines`.

/// Python's `str.isspace` for one character: Unicode whitespace plus `\x1c`..`\x1f`.
pub fn is_space(c: char) -> bool {
    c.is_whitespace() || ('\x1c'..='\x1f').contains(&c)
}

/// Python's `str.strip()`.
pub fn strip(text: &str) -> &str {
    text.trim_matches(is_space)
}

/// Python's `str.lstrip()`.
pub fn lstrip(text: &str) -> &str {
    text.trim_start_matches(is_space)
}

/// The first line of `text` including its line break, and the rest, by `str.splitlines` rules
/// (`\n`, `\r`, `\r\n`, `\x0b`, `\x0c`, `\x1c`-`\x1e`, `\x85`, ` `, ` `).
pub fn split_first_line(text: &str) -> (&str, &str) {
    let mut chars = text.char_indices().peekable();
    while let Some((index, c)) = chars.next() {
        let end = match c {
            '\r' => match chars.peek() {
                Some((_, '\n')) => index + 2,
                _ => index + 1,
            },
            '\n' | '\x0b' | '\x0c' | '\x1c' | '\x1d' | '\x1e' | '\u{85}' | '\u{2028}'
            | '\u{2029}' => index + c.len_utf8(),
            _ => continue,
        };
        return text.split_at_checked(end).unwrap_or((text, ""));
    }
    (text, "")
}

#[cfg(test)]
mod tests {
    use super::{split_first_line, strip};

    #[test]
    fn splitlines_separators() {
        assert_eq!(split_first_line("a\r\nb"), ("a\r\n", "b"));
        assert_eq!(split_first_line("a\rb"), ("a\r", "b"));
        assert_eq!(split_first_line("a\u{2028}b"), ("a\u{2028}", "b"));
        assert_eq!(split_first_line("a"), ("a", ""));
        assert_eq!(
            split_first_line("a\x1fb"),
            ("a\x1fb", ""),
            "\\x1f is whitespace but not a line break"
        );
    }

    #[test]
    fn strip_like_python() {
        assert_eq!(strip("\x1c\t a \u{a0}\n"), "a");
    }
}
