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
    // chart paints are graphical objects on every backdrop (WCAG 1.4.11)
    ("--op-series-1", "--op-bg", 3.0),
    ("--op-series-1", "--op-surface", 3.0),
    ("--op-series-1", "--op-raised", 3.0),
    ("--op-series-2", "--op-bg", 3.0),
    ("--op-series-2", "--op-surface", 3.0),
    ("--op-series-2", "--op-raised", 3.0),
    ("--op-series-3", "--op-bg", 3.0),
    ("--op-series-3", "--op-surface", 3.0),
    ("--op-series-3", "--op-raised", 3.0),
    ("--op-series-4", "--op-bg", 3.0),
    ("--op-series-4", "--op-surface", 3.0),
    ("--op-series-4", "--op-raised", 3.0),
    ("--op-series-5", "--op-bg", 3.0),
    ("--op-series-5", "--op-surface", 3.0),
    ("--op-series-5", "--op-raised", 3.0),
    ("--op-series-6", "--op-bg", 3.0),
    ("--op-series-6", "--op-surface", 3.0),
    ("--op-series-6", "--op-raised", 3.0),
    ("--op-playhead", "--op-bg", 3.0),
    ("--op-playhead", "--op-surface", 3.0),
    ("--op-playhead", "--op-raised", 3.0),
    ("--op-peek", "--op-bg", 3.0),
    ("--op-peek", "--op-surface", 3.0),
    ("--op-peek", "--op-raised", 3.0),
    // the chapter band is drawn behind the axis readout and mark labels
    ("--op-text", "--op-band", 4.5),
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
pub(crate) fn dark() -> BTreeMap<String, String> {
    tokens_after(":root,\n.opt-theme-dark {")
}

/// Light, when explicitly chosen (`data-theme="light"`).
pub(crate) fn light() -> BTreeMap<String, String> {
    tokens_after(":root[data-theme=\"light\"],\n.opt-theme-light {")
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

/// The chart series palette: six Okabe-Ito hues fitted in two OKLCH
/// lightness bands per theme by `op-colour`'s `fit_series`. These tests
/// regenerate the numbers from the tokens rather than trusting the fit.
///
/// The floors are project conventions, not standards: the CIEDE2000
/// separations follow the Palettailor and PaletteGuard practice (10 in
/// normal vision, 8 after simulation), the APCA levels are the author's
/// drafts (45 as the floor for 2 px strokes on the dark surfaces, 60 the
/// target), and WCAG 3's draft names no algorithm. A few units of slack
/// are built into the CVD floor because Machado and Brettel differ at
/// the margins.
#[cfg(test)]
mod chart_series {
    use super::{dark, light};
    use op_colour::{Deficiency, Lab, Oklch, Srgb, apca_lc, ciede2000, simulate};
    use std::collections::BTreeMap;

    const SERIES: usize = 6;
    const MIN_PAIR_NORMAL: f64 = 10.0;
    const MIN_PAIR_CVD: f64 = 8.0;
    const MIN_TO_SURFACE: f64 = 20.0;
    const DARK_APCA_FLOOR: f64 = 45.0;
    /// Okabe-Ito hue angles in OKLCH, in token order, and the band each sits in.
    const HUES: [(f64, char); SERIES] = [
        (77.0, 'A'),
        (166.0, 'B'),
        (244.0, 'B'),
        (346.0, 'A'),
        (236.0, 'A'),
        (105.0, 'B'),
    ];
    const HUE_TOLERANCE: f64 = 3.0;

    fn series(tokens: &BTreeMap<String, String>) -> Vec<Srgb> {
        (1..=SERIES)
            .map(|n| {
                let hex = &tokens[&format!("--op-series-{n}")];
                Srgb::from_hex(hex)
                    .unwrap_or_else(|| panic!("--op-series-{n} is not #RRGGBB: {hex}"))
            })
            .collect()
    }

    fn themes() -> [(&'static str, BTreeMap<String, String>); 2] {
        [("dark", dark()), ("light", light())]
    }

    #[test]
    fn every_pair_stays_apart_in_normal_and_deficient_vision() {
        for (theme, tokens) in themes() {
            let s = series(&tokens);
            for i in 0..SERIES {
                for j in i + 1..SERIES {
                    let d = ciede2000(Lab::from_srgb(s[i]), Lab::from_srgb(s[j]));
                    assert!(
                        d >= MIN_PAIR_NORMAL,
                        "{theme}: series {} vs {} only {d:.1} apart",
                        i + 1,
                        j + 1
                    );
                    for dfc in Deficiency::ALL {
                        let e = ciede2000(
                            Lab::from_srgb(simulate(s[i], dfc)),
                            Lab::from_srgb(simulate(s[j], dfc)),
                        );
                        assert!(
                            e >= MIN_PAIR_CVD,
                            "{theme}: series {} vs {} only {e:.1} apart under {}",
                            i + 1,
                            j + 1,
                            dfc.name()
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn every_series_stands_off_the_surface() {
        for (theme, tokens) in themes() {
            let surface = Srgb::from_hex(&tokens["--op-surface"]).unwrap();
            for (n, c) in series(&tokens).iter().enumerate() {
                let d = ciede2000(Lab::from_srgb(*c), Lab::from_srgb(surface));
                assert!(
                    d >= MIN_TO_SURFACE,
                    "{theme}: series {} is only {d:.1} from the surface",
                    n + 1
                );
            }
        }
    }

    /// 2 px strokes on the dark surfaces: WCAG 3:1 alone lands at Lc 31 to
    /// 37 there, the thin-line case the WCAG text says to exceed.
    #[test]
    fn dark_theme_series_reach_apca_45_on_every_backdrop() {
        let tokens = dark();
        for backdrop in ["--op-bg", "--op-surface", "--op-raised"] {
            let b = Srgb::from_hex(&tokens[backdrop]).unwrap();
            for (n, c) in series(&tokens).iter().enumerate() {
                let lc = apca_lc(*c, b).abs();
                assert!(
                    lc >= DARK_APCA_FLOOR,
                    "dark: series {} on {backdrop} is Lc {lc:.0}",
                    n + 1
                );
            }
        }
    }

    /// The design: each token keeps its Okabe-Ito hue, and the six sit in
    /// exactly two lightness bands with the intended membership.
    #[test]
    fn hues_are_okabe_ito_and_lightness_forms_two_bands() {
        for (theme, tokens) in themes() {
            let oklch: Vec<Oklch> = series(&tokens)
                .iter()
                .map(|c| Oklch::from_srgb(*c))
                .collect();
            for (n, (o, (hue, _))) in oklch.iter().zip(HUES).enumerate() {
                let diff = (o.h - hue).abs().min(360.0 - (o.h - hue).abs());
                assert!(
                    diff <= HUE_TOLERANCE,
                    "{theme}: series {} hue {:.0} is not {hue:.0}",
                    n + 1,
                    o.h
                );
            }
            let band = |ch: char| {
                oklch
                    .iter()
                    .zip(HUES)
                    .filter(|(_, (_, b))| *b == ch)
                    .map(|(o, _)| o.l)
                    .collect::<Vec<_>>()
            };
            let (a, b) = (band('A'), band('B'));
            let spread = |v: &[f64]| {
                v.iter().cloned().fold(f64::MIN, f64::max)
                    - v.iter().cloned().fold(f64::MAX, f64::min)
            };
            assert!(
                spread(&a) < 0.02 && spread(&b) < 0.02,
                "{theme}: bands are not flat: {a:?} {b:?}"
            );
            let (ma, mb) = (
                a.iter().sum::<f64>() / a.len() as f64,
                b.iter().sum::<f64>() / b.len() as f64,
            );
            assert!(
                (ma - mb).abs() >= 0.08,
                "{theme}: the two bands are too close: {ma:.3} {mb:.3}"
            );
        }
    }

    #[test]
    fn tokens_are_the_quantised_srgb_form_of_their_oklch_fit() {
        for (theme, tokens) in themes() {
            for (n, c) in series(&tokens).iter().enumerate() {
                // a token sits at the gamut edge by construction, so the round trip may
                // overshoot by float error: anything under half a quantisation step is
                // the same hex value, and that is the check
                let back = Oklch::from_srgb(*c).to_srgb();
                let step = 0.5 / 255.0;
                for ch in [back.r, back.g, back.b] {
                    assert!(
                        (-step..=1.0 + step).contains(&ch),
                        "{theme}: series {} leaves the gamut by more than a quantisation step: {back:?}",
                        n + 1
                    );
                }
                assert_eq!(
                    back.quantised().to_hex(),
                    c.to_hex(),
                    "{theme}: series {} does not round-trip",
                    n + 1
                );
            }
        }
    }
}
