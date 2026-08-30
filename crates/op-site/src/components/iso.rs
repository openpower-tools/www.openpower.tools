//! Stroke-drawn icon glyphs following the ISO 3864 shape grammar as
//! registered in ISO 7010 (24x24 viewBox), shared by the components that
//! signal severity (currently the op-callout frames).

/// The icon for a status or severity name, as stroke elements.
pub(super) fn glyph(variant: &str) -> &'static str {
    match variant {
        // Safe condition (ISO 3864 square) carrying a check.
        "tip" => {
            r#"<rect x="3.5" y="3.5" width="17" height="17" rx="1"/><path d="M7.5 12.4l3.1 3.1 5.9-5.9"/>"#
        }
        // General warning, after ISO 7010 W001: triangle and exclamation.
        "warning" => {
            r#"<path d="M12 3.4 21.5 19.9H2.5Z"/><path d="M12 9.8v4.4"/><path d="M12 17.3v.01" stroke-width="2.6"/>"#
        }
        // General prohibition, after ISO 7010 P001: circle and diagonal bar.
        "danger" => r#"<circle cx="12" cy="12" r="9.2"/><path d="M5.5 5.5l13 13"/>"#,
        // Information: a circled i.
        _ => {
            r#"<circle cx="12" cy="12" r="9.2"/><path d="M12 11v5.4"/><path d="M12 7.6v.01" stroke-width="2.6"/>"#
        }
    }
}
