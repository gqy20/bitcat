/// Build a short, single-line preview for logs.
///
/// The returned string is character-safe and normalizes line breaks so log
/// records stay one event per line.
pub fn log_preview(s: &str, max_chars: usize) -> String {
    let char_count = s.chars().count();
    let mut out: String = s.chars().take(max_chars).collect();
    if char_count > max_chars {
        out.push('…');
    }
    out.replace('\r', "\\r").replace('\n', "\\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_keeps_short_text() {
        assert_eq!(log_preview("hello", 10), "hello");
    }

    #[test]
    fn preview_truncates_by_chars() {
        assert_eq!(log_preview("你好世界", 2), "你好…");
    }

    #[test]
    fn preview_escapes_newlines() {
        assert_eq!(log_preview("a\nb\rc", 10), "a\\nb\\rc");
    }
}
