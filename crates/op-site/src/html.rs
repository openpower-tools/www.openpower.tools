//! Small helpers for building shadow-DOM markup from Rust strings.

/// Escapes text for safe inclusion in HTML text or attribute values.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::escape;

    #[test]
    fn escapes_every_html_significant_character() {
        assert_eq!(
            escape(r#"<a href="x">Tom & Jerry's</a>"#),
            "&lt;a href=&quot;x&quot;&gt;Tom &amp; Jerry&#39;s&lt;/a&gt;"
        );
    }

    #[test]
    fn leaves_plain_text_and_non_ascii_untouched() {
        let text = "Talos II — POWER9 (ppc64le) · ünïcödé";
        assert_eq!(escape(text), text);
    }

    #[test]
    fn output_contains_no_raw_significant_characters() {
        let escaped = escape("&&<<>>\"\"''");
        for c in ['<', '>', '"', '\''] {
            assert!(!escaped.contains(c), "{c:?} survived escaping: {escaped}");
        }
        assert!(
            !escaped.contains("&&") && !escaped.contains("& "),
            "bare ampersand survived: {escaped}"
        );
    }
}
