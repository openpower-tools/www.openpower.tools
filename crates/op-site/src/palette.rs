//! Contrast checks for `styles/theme.css`, run by `cargo test`.
//!
//! WCAG 2.x contrast ratio: (L1 + 0.05) / (L2 + 0.05) with relative luminance
//! L from sRGB. Text pairs must reach 4.5:1 (AA); UI boundaries and focus
//! indicators 3:1 (AA non-text contrast). `--op-highlight` and `--op-border`
//! are decoration only and are not checked. The colour maths lives in
//! `crate::colour`, shared with the specimen element.

use std::collections::BTreeMap;

use crate::colour::{self, Rgb};

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
    // status colours label callouts as text on the page background (4.5:1)
    // and mark stripes and dots against both backgrounds (3:1 non-text)
    ("--op-status-neutral", "--op-bg", 3.0),
    ("--op-status-neutral", "--op-surface", 3.0),
    ("--op-status-info", "--op-bg", 4.5),
    ("--op-status-info", "--op-surface", 3.0),
    ("--op-status-ok", "--op-bg", 4.5),
    ("--op-status-ok", "--op-surface", 3.0),
    ("--op-status-warning", "--op-bg", 4.5),
    ("--op-status-warning", "--op-surface", 3.0),
    ("--op-status-danger", "--op-bg", 4.5),
    ("--op-status-danger", "--op-surface", 3.0),
    // callouts fill with the raised background; their labels, text and any
    // links or dots inside must hold their grade on it
    ("--op-text", "--op-raised", 4.5),
    ("--op-muted", "--op-raised", 4.5),
    ("--op-link", "--op-raised", 4.5),
    ("--op-link-hover", "--op-raised", 4.5),
    ("--op-status-info", "--op-raised", 4.5),
    ("--op-status-ok", "--op-raised", 4.5),
    ("--op-status-warning", "--op-raised", 4.5),
    ("--op-status-danger", "--op-raised", 4.5),
    ("--op-status-neutral", "--op-raised", 3.0),
];

fn contrast(a: &str, b: &str) -> f64 {
    let parse = |c: &str| Rgb::from_hex(c).unwrap_or_else(|| panic!("expected #RRGGBB, got {c}"));
    colour::contrast(parse(a), parse(b))
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
    tokens_after(":root,\n.op-theme-dark {")
}

/// Light, when explicitly chosen (`data-theme="light"`).
fn light() -> BTreeMap<String, String> {
    tokens_after(":root[data-theme=\"light\"],\n.op-theme-light {")
}

/// Light, when no explicit choice is stored and the system prefers light.
fn light_media() -> BTreeMap<String, String> {
    tokens_after(":root:not([data-theme=\"dark\"]) {")
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
