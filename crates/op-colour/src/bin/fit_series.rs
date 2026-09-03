//! Fit the chart series palette: six Okabe-Ito hues in two OKLCH lightness
//! bands per theme, chosen to clear the site's contrast floors and stay as
//! far apart as possible in normal and deficient colour vision.
//!
//!     cargo run -p op-colour --bin fit_series
//!
//! Prints the hex tokens and the numbers the palette test will re-check.

use op_colour::{Deficiency, Lab, Oklch, Srgb, apca_lc, ciede2000, simulate, wcag_contrast};

/// Okabe-Ito hues as OKLCH hue angles (computed from the published sRGB
/// values), in token order: the first four are the one-band set that is
/// robust on its own, the last two complete the six.
const HUES: [(&str, f64, Band); 6] = [
    ("orange", 77.0, Band::A),
    ("bluish green", 166.0, Band::B),
    ("blue", 244.0, Band::B),
    ("reddish purple", 346.0, Band::A),
    ("sky blue", 236.0, Band::A),
    ("olive (yellow)", 105.0, Band::B),
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Band {
    A,
    B,
}

struct Theme {
    name: &'static str,
    bg: &'static str,
    surface: &'static str,
    raised: &'static str,
    /// Lightness search ranges for the two bands (A darker on light
    /// themes, lighter on dark themes only by convention of the search).
    range_a: (f64, f64),
    range_b: (f64, f64),
    /// Minimum WCAG ratio against every backdrop.
    wcag_floor: f64,
    /// Minimum APCA Lc magnitude against every backdrop (0 to ignore).
    apca_floor: f64,
    /// Below this chroma a hue reads as grey, brown or pastel rather than
    /// as its colour, which is the drift a pure separation objective
    /// produces. Light blues are gamut-limited, so the dark theme's floor
    /// is lower.
    min_chroma: f64,
    /// Pairwise CIEDE2000 floors: normal vision, and after simulation.
    min_pair_normal: f64,
    min_pair_cvd: f64,
}

const THEMES: [Theme; 2] = [
    Theme {
        name: "light",
        bg: "#E9F0F8",
        surface: "#FFFFFF",
        raised: "#FFFFFF",
        range_a: (0.45, 0.62),
        range_b: (0.55, 0.72),
        wcag_floor: 3.0,
        apca_floor: 0.0,
        min_chroma: 0.11,
        min_pair_normal: 10.0,
        min_pair_cvd: 8.0,
    },
    Theme {
        name: "dark",
        bg: "#020202",
        surface: "#2B415F",
        raised: "#131A26",
        range_a: (0.66, 0.84),
        range_b: (0.74, 0.92),
        wcag_floor: 3.0,
        apca_floor: 45.0,
        // a light blue at Lc 45 on this navy cannot exceed about 0.07 in sRGB
        min_chroma: 0.07,
        min_pair_normal: 10.0,
        min_pair_cvd: 8.0,
    },
];

const CHROMA_CAP: f64 = 0.16;
const MIN_TO_SURFACE: f64 = 20.0;
/// Colour-vision margin beyond this earns no further credit; vividness
/// decides instead.
const CVD_CREDIT_CAP: f64 = 11.0;

struct Fit {
    la: f64,
    lb: f64,
    colours: Vec<Srgb>,
    min_normal: f64,
    min_cvd: f64,
    mean_chroma: f64,
    worst_pair: String,
    worst_normal: String,
}

impl Fit {
    /// Lexicographic: colour-vision margin up to the cap, then vividness,
    /// then the normal-vision margin.
    fn score(&self) -> (f64, f64, f64) {
        (
            self.min_cvd.min(CVD_CREDIT_CAP),
            self.mean_chroma,
            self.min_normal,
        )
    }
}

use std::cell::RefCell;
thread_local! { static TALLY: RefCell<[usize; 6]> = const { RefCell::new([0; 6]) }; }
const REASONS: [&str; 6] = [
    "chroma floor",
    "wcag floor",
    "apca floor",
    "distance to surface",
    "pairwise normal",
    "pairwise cvd",
];
fn tally(i: usize) {
    TALLY.with(|t| t.borrow_mut()[i] += 1);
}

fn colours_for(la: f64, lb: f64) -> Vec<Srgb> {
    HUES.iter()
        .map(|(_, h, band)| {
            let l = if *band == Band::A { la } else { lb };
            let c = Oklch::max_in_gamut_chroma(l, *h, CHROMA_CAP);
            Oklch { l, c, h: *h }.to_srgb().quantised()
        })
        .collect()
}

fn evaluate(theme: &Theme, la: f64, lb: f64) -> Option<Fit> {
    let colours = colours_for(la, lb);
    let chromas: Vec<f64> = colours.iter().map(|c| Oklch::from_srgb(*c).c).collect();
    if chromas.iter().any(|c| *c < theme.min_chroma) {
        tally(0);
        return None;
    }
    let mean_chroma = chromas.iter().sum::<f64>() / chromas.len() as f64;
    let backdrops = [theme.bg, theme.surface, theme.raised].map(|h| Srgb::from_hex(h).unwrap());
    let surface = Srgb::from_hex(theme.surface).unwrap();
    for c in &colours {
        for b in backdrops {
            if wcag_contrast(*c, b) < theme.wcag_floor {
                tally(1);
                return None;
            }
            if apca_lc(*c, b).abs() < theme.apca_floor {
                tally(2);
                return None;
            }
        }
        if ciede2000(Lab::from_srgb(*c), Lab::from_srgb(surface)) < MIN_TO_SURFACE {
            tally(3);
            return None;
        }
    }
    let mut min_normal = f64::INFINITY;
    let mut min_cvd = f64::INFINITY;
    let mut worst_pair = String::new();
    let mut worst_normal = String::new();
    for i in 0..colours.len() {
        for j in i + 1..colours.len() {
            let d = ciede2000(Lab::from_srgb(colours[i]), Lab::from_srgb(colours[j]));
            if d < min_normal {
                min_normal = d;
                worst_normal = format!("{} vs {}", HUES[i].0, HUES[j].0);
            }
            for dfc in Deficiency::ALL {
                let e = ciede2000(
                    Lab::from_srgb(simulate(colours[i], dfc)),
                    Lab::from_srgb(simulate(colours[j], dfc)),
                );
                if e < min_cvd {
                    min_cvd = e;
                    worst_pair = format!("{} vs {} under {}", HUES[i].0, HUES[j].0, dfc.name());
                }
            }
        }
    }
    if min_normal < theme.min_pair_normal {
        tally(4);
        return None;
    }
    if min_cvd < theme.min_pair_cvd {
        tally(5);
        return None;
    }
    Some(Fit {
        la,
        lb,
        colours,
        min_normal,
        min_cvd,
        mean_chroma,
        worst_pair,
        worst_normal,
    })
}

fn main() {
    for theme in &THEMES {
        let mut best: Option<Fit> = None;
        let steps = |(lo, hi): (f64, f64)| {
            (0..=((hi - lo) / 0.005).round() as usize).map(move |i| lo + i as f64 * 0.005)
        };
        for la in steps(theme.range_a) {
            for lb in steps(theme.range_b) {
                if let Some(fit) = evaluate(theme, la, lb) {
                    // prefer the widest CVD margin, then the widest normal margin
                    let better = best.as_ref().is_none_or(|b| fit.score() > b.score());
                    if better {
                        best = Some(fit);
                    }
                }
            }
        }
        let counts = TALLY.with(|t| std::mem::take(&mut *t.borrow_mut()));
        let Some(fit) = best else {
            println!(
                "{}: no fit clears the floors; rejections by first failing check:",
                theme.name
            );
            for (r, n) in REASONS.iter().zip(counts) {
                println!("  {r}: {n}");
            }
            continue;
        };
        println!(
            "theme {}  bands L {:.3} / {:.3}  min pairwise dE00 normal {:.1} ({})  cvd {:.1} ({})  mean chroma {:.3}",
            theme.name,
            fit.la,
            fit.lb,
            fit.min_normal,
            fit.worst_normal,
            fit.min_cvd,
            fit.worst_pair,
            fit.mean_chroma
        );
        let backdrops = [theme.bg, theme.surface, theme.raised].map(|h| Srgb::from_hex(h).unwrap());
        for (i, ((name, hue, _), c)) in HUES.iter().zip(&fit.colours).enumerate() {
            let o = Oklch::from_srgb(*c);
            let ratios: Vec<String> = backdrops
                .iter()
                .map(|b| format!("{:.2}", wcag_contrast(*c, *b)))
                .collect();
            let lcs: Vec<String> = backdrops
                .iter()
                .map(|b| format!("{:.0}", apca_lc(*c, *b).abs()))
                .collect();
            println!(
                "  --op-series-{}: {};  /* {} oklch({:.3} {:.3} {:.0}) hue {:.0}  wcag bg/surface/raised {}  apca {} */",
                i + 1,
                c.to_hex(),
                name,
                o.l,
                o.c,
                o.h,
                hue,
                ratios.join("/"),
                lcs.join("/")
            );
        }
    }
}
