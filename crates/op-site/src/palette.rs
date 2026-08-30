//! Contrast checks for `styles/theme.css`, run by `cargo test`.
//!
//! WCAG 2.x contrast ratio: (L1 + 0.05) / (L2 + 0.05) with relative luminance
//! L from sRGB. Text pairs must reach 4.5:1 (AA); UI boundaries and focus
//! indicators 3:1 (AA non-text contrast). `--op-highlight` and `--op-border`
//! are decoration only and are not checked.

use std::collections::BTreeMap;

const CSS: &str = include_str!("../../../styles/theme.css");

/// The Worcester colours the dark theme was derived from; each must appear
/// verbatim in the dark token set. Elgar Bronze #5C2119 is omitted on purpose:
/// it is 1.7:1 against Pear Black, so its only dark-theme role would be a
/// background, and it was rejected as one.
const DARK_SOURCE_COLOURS: &[&str] = &["#020202", "#334D70", "#D7BD44", "#DCCAA4", "#EB6424"];

/// The six Nottingham colours the light theme was derived from; each must
/// appear verbatim in the light token set.
const LIGHT_SOURCE_COLOURS: &[&str] = &[
    "#30544A", "#1E8477", "#8CB531", "#E9F0F8", "#C6B49E", "#E75019",
];

/// (foreground token, background token, minimum ratio)
const REQUIRED_PAIRS: &[(&str, &str, f64)] = &[
    ("--op-text", "--op-bg", 4.5),
    ("--op-text", "--op-surface", 4.5),
    ("--op-text", "--op-code-bg", 4.5),
    ("--op-muted", "--op-bg", 4.5),
    ("--op-muted", "--op-surface", 4.5),
    ("--op-link", "--op-bg", 4.5),
    ("--op-link", "--op-surface", 4.5),
    ("--op-link-hover", "--op-bg", 4.5),
    ("--op-link-hover", "--op-surface", 4.5),
    // accent is used for hover borders and rules, so non-text contrast applies
    ("--op-accent", "--op-bg", 3.0),
    ("--op-accent", "--op-surface", 3.0),
    ("--op-focus", "--op-bg", 3.0),
    ("--op-focus", "--op-surface", 3.0),
    ("--op-border-strong", "--op-bg", 3.0),
    ("--op-border-strong", "--op-surface", 3.0),
];

fn channel(hex: &str) -> f64 {
    let c = f64::from(u8::from_str_radix(hex, 16).expect("hex channel")) / 255.0;
    if c <= 0.03928 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn luminance(colour: &str) -> f64 {
    let hex = colour.strip_prefix('#').expect("colour starts with #");
    assert_eq!(hex.len(), 6, "expected #RRGGBB, got {colour}");
    0.2126 * channel(&hex[0..2]) + 0.7152 * channel(&hex[2..4]) + 0.0722 * channel(&hex[4..6])
}

fn contrast(a: &str, b: &str) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// Returns the `--op-*` declarations inside the first `{ ... }` block that
/// follows `selector`.
fn tokens_after(selector: &str) -> BTreeMap<String, String> {
    let start = CSS
        .find(selector)
        .unwrap_or_else(|| panic!("selector {selector:?} not found"));
    let open = start + CSS[start..].find('{').expect("block start");
    let mut depth = 0usize;
    let mut close = open;
    for (offset, c) in CSS[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    close = open + offset;
                    break;
                }
            }
            _ => {}
        }
    }
    assert!(close > open, "unterminated block after {selector:?}");
    CSS[open + 1..close]
        .lines()
        .filter_map(|line| {
            let line = line.trim().strip_suffix(';')?;
            let (name, value) = line.split_once(':')?;
            name.starts_with("--op-")
                .then(|| (name.trim().to_owned(), value.trim().to_owned()))
        })
        .collect()
}

/// Dark is the default: the tokens in the first `:root` block.
fn dark() -> BTreeMap<String, String> {
    tokens_after(":root {")
}

/// Light, when explicitly chosen (`data-theme="light"`).
fn light() -> BTreeMap<String, String> {
    tokens_after(":root[data-theme=\"light\"] {\n  --op-bg")
}

/// Light, when `data-theme="auto"` and the system prefers light.
fn light_media() -> BTreeMap<String, String> {
    tokens_after(":root[data-theme=\"auto\"] {\n    --op-bg")
}

#[test]
fn luminance_and_contrast_match_reference_values() {
    assert!((luminance("#FFFFFF") - 1.0).abs() < 1e-9);
    assert!(luminance("#000000").abs() < 1e-9);
    assert!((contrast("#000000", "#FFFFFF") - 21.0).abs() < 1e-9);
    // Published reference points: #767676 is the lightest grey that passes AA
    // on white at 4.54:1, and #777777 just fails at 4.48:1.
    assert!((contrast("#767676", "#FFFFFF") - 4.54).abs() < 0.01);
    assert!((contrast("#777777", "#FFFFFF") - 4.48).abs() < 0.01);
}

#[test]
fn both_themes_define_the_same_tokens() {
    let (light, dark) = (light(), dark());
    assert!(!light.is_empty());
    assert_eq!(
        light.keys().collect::<Vec<_>>(),
        dark.keys().collect::<Vec<_>>()
    );
    for (name, _, _) in REQUIRED_PAIRS {
        assert!(light.contains_key(*name), "missing token {name}");
    }
    for name in ["--op-highlight", "--op-border"] {
        assert!(light.contains_key(name), "missing token {name}");
    }
}

#[test]
fn media_query_light_block_equals_data_theme_light_block() {
    assert_eq!(light_media(), light());
}

#[test]
fn every_required_pair_meets_wcag_aa_in_both_themes() {
    for (theme, tokens) in [("dark", dark()), ("light", light())] {
        for (fg, bg, minimum) in REQUIRED_PAIRS {
            let ratio = contrast(&tokens[*fg], &tokens[*bg]);
            assert!(
                ratio >= *minimum,
                "{theme}: {fg} {} on {bg} {} is {ratio:.2}:1, needs {minimum}:1",
                tokens[*fg],
                tokens[*bg]
            );
        }
    }
}

#[test]
fn each_theme_uses_every_colour_of_its_source_palette() {
    for (theme, tokens, sources) in [
        ("dark", dark(), DARK_SOURCE_COLOURS),
        ("light", light(), LIGHT_SOURCE_COLOURS),
    ] {
        let values: Vec<&String> = tokens.values().collect();
        for colour in sources {
            assert!(
                values.iter().any(|v| v.eq_ignore_ascii_case(colour)),
                "{theme}: source colour {colour} is not used"
            );
        }
    }
}
