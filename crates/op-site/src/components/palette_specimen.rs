//! `<opt-palette-specimen>`: every palette token and the site's elements,
//! rendered in both themes side by side for design preview. It is the body
//! of <https://www.openpower.tools/specimen/> (`specimen/index.html`, a
//! second Trunk target).
//!
//! It renders into the light DOM rather than a shadow root so that the
//! `.opt-theme-dark` and `.opt-theme-light` token scopes in `styles/theme.css`
//! apply to its two columns; the real site elements placed inside each column
//! pick up that column's tokens through inheritance.
//!
//! Two questions, two views. The swatch columns answer "what is the
//! palette": every token with its name, its role, its value in both themes
//! and the contrast it holds. The applied figures answer "does it work":
//! the series tokens drawn by the real renderer ([`op_chart`]) under the
//! real chart stylesheet ([`super::chart::stylesheet`]), and the status
//! tokens as `<opt-term>` projects them from a controlled vocabulary. A
//! chart is where a categorical palette is actually judged, so the charts
//! come first.

use op_chart::{Band, Layout, Mark, Series, Spec};
use op_webc::{CustomElement, ElementDefinition};
use wasm_bindgen::JsCast;
use web_sys::{Element, HtmlElement};

use super::chart::{DEFAULT_RATIO, stylesheet as chart_stylesheet};
use crate::colour::{self, Rgb};
use crate::html::escape;

pub const DEFINITION: ElementDefinition = ElementDefinition {
    source: op_webc::here!(),
    tag: "opt-palette-specimen",
    observed_attributes: &[],
    properties: &[],
    create: |host| {
        Box::new(Specimen {
            host,
            on_fonts_loadingdone: None,
        })
    },
};

struct Specimen {
    host: HtmlElement,
    /// Kept alive for as long as the element exists.
    on_fonts_loadingdone: Option<wasm_bindgen::prelude::Closure<dyn FnMut(web_sys::Event)>>,
}

/// How a token's contrast is reported.
#[derive(Clone, Copy)]
enum Role {
    /// Text: measured against the page background and the surface.
    Text,
    /// UI boundary or indicator: measured against background and surface.
    Ui,
    /// Something drawn behind text: measured with body text on it.
    Backdrop,
    /// Decoration only: no requirement, no ratio shown.
    Decoration,
}

/// Which collection of the palette a token belongs to. The chart tokens
/// are fitted together, tested together (`palette.rs`'s `chart_series`) and
/// read together, so the specimen lists them as one set rather than sorting
/// them in among the interface tokens by role; the status tokens likewise,
/// since one meaning maps to one hue in both themes.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Group {
    Interface,
    Status,
    Chart,
}

impl Group {
    /// In the order the columns list them.
    const ALL: [Group; 3] = [Group::Interface, Group::Status, Group::Chart];

    fn heading(self) -> &'static str {
        match self {
            Group::Interface => "Surfaces, text and interface",
            Group::Status => "Status",
            Group::Chart => "Chart",
        }
    }
}

const TOKENS: &[(&str, &str, Role, Group)] = &[
    (
        "--op-bg",
        "page background",
        Role::Backdrop,
        Group::Interface,
    ),
    (
        "--op-surface",
        "controls and panels",
        Role::Backdrop,
        Group::Interface,
    ),
    (
        "--op-code-bg",
        "code background",
        Role::Backdrop,
        Group::Interface,
    ),
    (
        "--op-raised",
        "raised background (callouts)",
        Role::Backdrop,
        Group::Interface,
    ),
    (
        "--op-pole-base",
        "barberpole mix base",
        Role::Decoration,
        Group::Interface,
    ),
    ("--op-text", "body text", Role::Text, Group::Interface),
    ("--op-muted", "secondary text", Role::Text, Group::Interface),
    ("--op-link", "links", Role::Text, Group::Interface),
    (
        "--op-link-hover",
        "links on hover",
        Role::Text,
        Group::Interface,
    ),
    (
        "--op-accent",
        "hover borders and rules",
        Role::Ui,
        Group::Interface,
    ),
    ("--op-focus", "focus ring", Role::Ui, Group::Interface),
    (
        "--op-border-strong",
        "control borders",
        Role::Ui,
        Group::Interface,
    ),
    (
        "--op-border",
        "separators",
        Role::Decoration,
        Group::Interface,
    ),
    (
        "--op-highlight",
        "title rule",
        Role::Decoration,
        Group::Interface,
    ),
    (
        "--op-status-neutral",
        "neutral status marker",
        Role::Ui,
        Group::Status,
    ),
    (
        "--op-status-info",
        "note and info markers",
        Role::Ui,
        Group::Status,
    ),
    ("--op-status-ok", "success markers", Role::Ui, Group::Status),
    (
        "--op-status-warning",
        "warning markers",
        Role::Ui,
        Group::Status,
    ),
    (
        "--op-status-danger",
        "danger markers",
        Role::Ui,
        Group::Status,
    ),
    (
        "--op-series-1",
        "chart series 1: orange",
        Role::Ui,
        Group::Chart,
    ),
    (
        "--op-series-2",
        "chart series 2: bluish green",
        Role::Ui,
        Group::Chart,
    ),
    (
        "--op-series-3",
        "chart series 3: blue",
        Role::Ui,
        Group::Chart,
    ),
    (
        "--op-series-4",
        "chart series 4: reddish purple",
        Role::Ui,
        Group::Chart,
    ),
    (
        "--op-series-5",
        "chart series 5: sky blue",
        Role::Ui,
        Group::Chart,
    ),
    (
        "--op-series-6",
        "chart series 6: olive",
        Role::Ui,
        Group::Chart,
    ),
    ("--op-playhead", "chart playhead", Role::Ui, Group::Chart),
    ("--op-peek", "chart peek rule", Role::Ui, Group::Chart),
    (
        "--op-band",
        "chart chapter band",
        Role::Backdrop,
        Group::Chart,
    ),
];

const STYLE: &str = "
opt-palette-specimen { display: block; margin-top: 1.5rem; }
opt-palette-specimen .columns {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(19rem, 1fr));
  gap: 1rem;
}
opt-palette-specimen .column {
  background: var(--op-bg);
  color: var(--op-text);
  padding: 1rem;
  border: 1px solid var(--op-border-strong);
  border-radius: 0.5rem;
}
opt-palette-specimen h2 { margin: 0 0 0.25rem; font-size: 1.125rem; }
opt-palette-specimen .muted { color: var(--op-muted); }
opt-palette-specimen h3.group {
  margin: 1.1rem 0 0;
  padding-bottom: 0.15rem;
  font-size: 0.75rem;
  font-weight: 600;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  color: var(--op-muted);
  border-bottom: 1px solid var(--op-border);
}
opt-palette-specimen h3.group:first-of-type { margin-top: 0.75rem; }
opt-palette-specimen .swatches {
  list-style: none;
  margin: 0.45rem 0 0;
  padding: 0;
  display: grid;
  gap: 0.45rem;
}
opt-palette-specimen .swatch {
  display: flex;
  align-items: center;
  gap: 0.6rem;
  font-size: 0.8rem;
  line-height: 1.3;
}
opt-palette-specimen .chip {
  flex: none;
  width: 2.25rem;
  height: 2.25rem;
  border-radius: 0.3rem;
  border: 1px solid var(--op-border-strong);
}
opt-palette-specimen .meta { display: grid; }
opt-palette-specimen .meta .role { color: var(--op-muted); }
opt-palette-specimen .meta .ratio { color: var(--op-muted); }
opt-palette-specimen .meta .values {
  display: flex;
  flex-wrap: wrap;
  gap: 0 0.6rem;
  font-family: var(--op-font-mono);
}
opt-palette-specimen .meta .other { color: var(--op-muted); }
opt-palette-specimen pre,
opt-palette-specimen code {
  font-family: var(--op-font-mono);
  background: var(--op-code-bg);
  border-radius: 0.3rem;
}
opt-palette-specimen code { padding: 0 0.25em; }
opt-palette-specimen pre { padding: 0.5rem 0.75rem; overflow-x: auto; }
opt-palette-specimen .sample-button {
  font: inherit;
  font-size: 0.875rem;
  color: var(--op-text);
  background: var(--op-surface);
  border: 1px solid var(--op-border-strong);
  border-radius: 0.375rem;
  padding: 0.35rem 0.7rem;
  cursor: pointer;
}
opt-palette-specimen .sample-button:hover { border-color: var(--op-accent); }
opt-palette-specimen .focus-sample {
  outline: 2px solid var(--op-focus);
  outline-offset: 2px;
  padding: 0 0.25rem;
}
opt-palette-specimen .surface-sample {
  background: var(--op-surface);
  padding: 0.5rem 0.75rem;
  border-radius: 0.375rem;
}
opt-palette-specimen hr.rule { border: 0; border-top: 1px solid var(--op-border); margin: 0.75rem 0; }
opt-palette-specimen hr.rule.strong { border-top-color: var(--op-border-strong); }
";

/// One of the two themes the specimen pins: the class that scopes its
/// tokens to a subtree, the heading it is shown under, the short name the
/// *other* column uses when it prints this theme's value beside its own,
/// and where the colours came from.
struct Theme {
    class: &'static str,
    title: &'static str,
    short: &'static str,
    source: &'static str,
}

const THEMES: [Theme; 2] = [
    Theme {
        class: "opt-theme-dark",
        title: "Dark (default)",
        short: "dark",
        source: "Derived from the Worcester palette.",
    },
    Theme {
        class: "opt-theme-light",
        title: "Light",
        short: "light",
        source: "Derived from the Nottingham palette.",
    },
];

fn column(theme: &Theme, other: &str) -> String {
    let (class, title, source) = (theme.class, theme.title, theme.source);
    let mut groups = String::new();
    for group in Group::ALL {
        let swatches: String = TOKENS
            .iter()
            .filter(|(_, _, _, g)| *g == group)
            .map(|(name, role, _, _)| {
                format!(
                    "<li class=\"swatch\" data-token=\"{name}\">\
<span class=\"chip\" style=\"background: var({name})\"></span>\
<span class=\"meta\"><code>{name}</code><span class=\"role\">{role}</span>\
<span class=\"values\"><span class=\"hex\"></span><span class=\"other\"></span></span>\
<span class=\"ratio\"></span></span></li>"
                )
            })
            .collect();
        groups.push_str(&format!(
            "<h3 class=\"group\">{}</h3><ul class=\"swatches\">{swatches}</ul>",
            group.heading()
        ));
    }
    format!(
        "<section class=\"{class} column\">
<h2>{title}</h2>
<p class=\"muted\">{source} Each row carries this column's computed value and, in grey, the same token in the {other} theme.</p>
{groups}
<opt-site-header heading=\"Heading\" tagline=\"Tagline in secondary text under the title rule.\"></opt-site-header>
<p>Body text with a <a href=\"#\">link</a>, <code>inline code</code> and <span class=\"muted\">secondary text</span>.</p>
<pre>preformatted block
on the code background</pre>
<p><button type=\"button\" class=\"sample-button\">Button</button> <span class=\"focus-sample\">focus ring</span></p>
<p class=\"surface-sample\">Text on a surface, as in controls and panels.</p>
<hr class=\"rule\"><hr class=\"rule strong\">
</section>"
    )
}

/// Reads a custom property from an element's computed style as a colour.
fn token_colour(element: &Element, token: &str) -> Option<Rgb> {
    let style = web_sys::window()?.get_computed_style(element).ok()??;
    let value = style.get_property_value(token).ok()?;
    let value = value.trim();
    Rgb::from_hex(value).or_else(|| Rgb::from_css_rgb(value))
}

fn ratio_text(role: Role, colour: Rgb, bg: Rgb, surface: Rgb, text: Rgb) -> String {
    match role {
        Role::Text | Role::Ui => {
            let need = if matches!(role, Role::Text) { 4.5 } else { 3.0 };
            format!(
                "{:.2}:1 on bg, {:.2}:1 on surface (needs {need}:1)",
                colour::contrast(colour, bg),
                colour::contrast(colour, surface)
            )
        }
        Role::Backdrop => format!(
            "body text on it {:.2}:1 (needs 4.5:1)",
            colour::contrast(text, colour)
        ),
        Role::Decoration => "decoration only".to_owned(),
    }
}

/// Fills in the hex value and contrast ratios for every swatch in `column`
/// from the column's computed tokens, and the same token's value in the
/// other column beside it. Both numbers are read from the live DOM, not
/// from a table in this file, so neither can drift from `theme.css`: the
/// other column is pinned to the other theme and is on the page already,
/// which is what makes a two-theme readout honest here.
fn annotate(column: &Element, other: &Element, other_name: &str) {
    let Some(bg) = token_colour(column, "--op-bg") else {
        return;
    };
    let Some(surface) = token_colour(column, "--op-surface") else {
        return;
    };
    let Some(text) = token_colour(column, "--op-text") else {
        return;
    };
    for (name, _, role, _) in TOKENS {
        let Some(colour) = token_colour(column, name) else {
            continue;
        };
        let Ok(Some(swatch)) = column.query_selector(&format!("[data-token=\"{name}\"]")) else {
            continue;
        };
        if let Ok(Some(hex)) = swatch.query_selector(".hex") {
            hex.set_text_content(Some(&colour.to_hex()));
        }
        if let Ok(Some(el)) = swatch.query_selector(".other") {
            el.set_text_content(
                token_colour(other, name)
                    .map(|c| format!("{other_name} {}", c.to_hex()))
                    .as_deref(),
            );
        }
        if let Ok(Some(ratio)) = swatch.query_selector(".ratio") {
            ratio.set_text_content(Some(&ratio_text(*role, colour, bg, surface, text)));
        }
    }
}

fn render(host: &HtmlElement) {
    host.set_inner_html(&format!(
        "<style>{STYLE}</style>
<div class=\"columns\">{}{}</div>
{}",
        column(&THEMES[0], THEMES[1].short),
        column(&THEMES[1], THEMES[0].short),
        applied_section(),
    ));
    let find = |theme: &Theme| {
        host.query_selector(&format!("section.{}.column", theme.class))
            .ok()
            .flatten()
    };
    if let (Some(dark), Some(light)) = (find(&THEMES[0]), find(&THEMES[1])) {
        annotate(&dark, &light, THEMES[1].short);
        annotate(&light, &dark, THEMES[0].short);
    }
    render_typography(host);
}

impl CustomElement for Specimen {
    fn connected(&mut self) {
        // Re-annotate whenever the stylesheet finishes loading faces (they
        // arrive lazily as glyphs demand them), so the badges track reality.
        if self.on_fonts_loadingdone.is_none()
            && let Some(document) = web_sys::window().and_then(|w| w.document())
        {
            let closure =
                wasm_bindgen::prelude::Closure::<dyn FnMut(web_sys::Event)>::new(move |_event| {
                    annotate_face_status()
                });
            let _ = document
                .fonts()
                .add_event_listener_with_callback("loadingdone", closure.as_ref().unchecked_ref());
            self.on_fonts_loadingdone = Some(closure);
        }
        render(&self.host);
    }
}

// ---- the applied views -------------------------------------------------
//
// A swatch is a colour with nothing asked of it. These figures ask
// something: the series tokens have to stay apart from each other while
// six lines cross, and the status tokens have to carry a meaning they did
// not choose. Two collections earn a place here and no more. Charting is
// first because the series palette exists for it and is fitted to it
// (`palette.rs`'s `chart_series`, and decisions 22 to 24 of
// `docs/research/chart-element-2026-09.md`). The status set is second
// because `<opt-term>` is the one place on the site where a colour is
// derived from data rather than written down, which is the thing a swatch
// can never show. The interface tokens are not repeated here: the columns
// above already put them to work in a header, a link, a button, a focus
// ring, a code block and a surface, and a second row of the same would be
// a wall rather than evidence.

/// How many samples every figure's series carries.
const SAMPLES: usize = 13;
/// The interval between samples, in seconds.
const SAMPLE_STEP: f64 = 0.5;
/// The end of every figure's time axis, in seconds.
const FIGURE_END: f64 = (SAMPLES - 1) as f64 * SAMPLE_STEP;
/// The box every applied chart is drawn in, in CSS px. It is the site's
/// default 16 / 6 chart at about the width the specimen's measure leaves
/// inside a panel, so the renderer's 12 px labels arrive at 12 px instead
/// of being scaled down to a size a reader cannot judge them at.
const FIGURE_WIDTH: f64 = 600.0;
const FIGURE_HEIGHT: f64 = 225.0;
/// Stroke width for a series: the palette's design width, the one the dash
/// table and the dark theme's APCA floor were both fitted for.
const SERIES_WIDTH: f64 = 2.0;
/// The vocabulary the status figure is drawn from: the can-i-use matrix's
/// answers, the one scheme whose terms reach all five severities.
const TERM_SCHEME: &str = "support";

/// One series of a figure: which palette slot it takes, the direct end
/// label it carries, and its samples at [`SAMPLE_STEP`] apart from zero.
struct Line {
    index: usize,
    label: &'static str,
    values: &'static [f64],
}

/// The four-series robust set: palette slots 1 to 4, which decision 22
/// makes the default for any chart that needs no more, because four hues
/// are the largest set that survives one lightness band under every
/// simulated deficiency. The numbers are a test pattern and not a
/// measurement; they were chosen so that all six pairs cross at least
/// once, since a crossing is where a categorical palette is read hardest.
const ROBUST_SET: &[Line] = &[
    Line {
        index: 1,
        label: "orange",
        values: &[
            8.0, 19.0, 32.0, 46.0, 59.0, 70.0, 78.0, 84.0, 88.0, 90.0, 92.0, 93.0, 93.0,
        ],
    },
    Line {
        index: 2,
        label: "bluish green",
        values: &[
            78.0, 66.0, 53.0, 42.0, 35.0, 33.0, 38.0, 47.0, 57.0, 66.0, 73.0, 78.0, 82.0,
        ],
    },
    Line {
        index: 3,
        label: "blue",
        values: &[
            92.0, 86.0, 79.0, 71.0, 62.0, 53.0, 44.0, 36.0, 29.0, 24.0, 20.0, 17.0, 16.0,
        ],
    },
    Line {
        index: 4,
        label: "reddish purple",
        values: &[
            30.0, 45.0, 57.0, 63.0, 61.0, 52.0, 42.0, 35.0, 35.0, 43.0, 54.0, 62.0, 68.0,
        ],
    },
];

/// The six-series two-band set. The end values are chosen so that the two
/// lightness bands alternate down the right-hand edge: slots 1, 4 and 5
/// (orange, reddish purple, sky blue) are the darker band and slots 2, 3
/// and 6 (bluish green, blue, olive) the lighter, so a reader running an
/// eye down the labels meets dark, light, dark, light. Same test pattern,
/// same six seconds.
const TWO_BAND_SET: &[Line] = &[
    Line {
        index: 1,
        label: "orange",
        values: &[
            20.0, 33.0, 45.0, 56.0, 66.0, 74.0, 81.0, 86.0, 90.0, 93.0, 95.0, 96.0, 96.0,
        ],
    },
    Line {
        index: 2,
        label: "bluish green",
        values: &[
            88.0, 80.0, 71.0, 62.0, 54.0, 48.0, 45.0, 47.0, 53.0, 61.0, 70.0, 78.0, 84.0,
        ],
    },
    Line {
        index: 3,
        label: "blue",
        values: &[
            62.0, 58.0, 52.0, 45.0, 38.0, 33.0, 30.0, 31.0, 35.0, 41.0, 47.0, 52.0, 55.0,
        ],
    },
    Line {
        index: 4,
        label: "reddish purple",
        values: &[
            44.0, 52.0, 61.0, 68.0, 72.0, 73.0, 71.0, 67.0, 63.0, 61.0, 62.0, 66.0, 70.0,
        ],
    },
    Line {
        index: 5,
        label: "sky blue",
        values: &[
            75.0, 68.0, 60.0, 52.0, 45.0, 40.0, 36.0, 34.0, 33.0, 34.0, 35.0, 37.0, 38.0,
        ],
    },
    Line {
        index: 6,
        label: "olive",
        values: &[
            96.0, 88.0, 78.0, 67.0, 56.0, 46.0, 37.0, 29.0, 23.0, 18.0, 15.0, 13.0, 12.0,
        ],
    },
];

/// One applied chart figure: what it draws, what it annotates it with,
/// where its playhead and peek rule are parked, and what to look at.
struct ChartFigure {
    lines: &'static [Line],
    /// Start, end and label of the chapter band, when the figure has one.
    band: Option<(f64, f64, &'static str)>,
    /// Time and label of the one mark, when the figure has one.
    mark: Option<(f64, &'static str)>,
    /// Where the playhead is parked, in seconds.
    head: f64,
    /// Where the peek rule is parked, in seconds; `None` leaves it hidden
    /// as the renderer emitted it, which is a chart nobody is pointing at.
    peek: Option<f64>,
    /// Whether the latent markers are shown. The chart draws them only for
    /// a series with no end label, and under forced colours and high
    /// contrast (decision 24), so one figure reveals them on purpose and
    /// says so in its caption; the other is left exactly as the site draws
    /// it, which is also the difference in weight between the two.
    markers: bool,
    /// The drawing's accessible name.
    alt: &'static str,
    /// The caption's opening words, and the caption itself.
    lead: &'static str,
    caption: &'static str,
}

const CHART_FIGURES: &[ChartFigure] = &[
    ChartFigure {
        lines: ROBUST_SET,
        band: None,
        mark: None,
        head: 1.5,
        peek: None,
        markers: true,
        alt: "Four crossing series over six seconds, labelled orange, bluish green, blue and reddish purple",
        lead: "The robust four.",
        caption: "Slots 1 to 4, the default for a chart that needs no more. Look at the crossings, where a categorical palette is read hardest: the dash pattern and the direct end label separate the lines with no help from colour at all. Markers are the cue that comes back when colour goes, so the chart draws them only for a series with no end label and under forced colours or high contrast. They are shown here anyway, so the six shapes can be compared.",
    },
    ChartFigure {
        lines: TWO_BAND_SET,
        band: Some((2.0, 3.5, "the band")),
        mark: Some((4.5, "a mark")),
        head: 3.0,
        peek: Some(1.2),
        markers: false,
        alt: "Six series over six seconds, labelled orange, bluish green, blue, reddish purple, sky blue and olive, with a band from two to three and a half seconds and the playhead at three seconds",
        lead: "All six, in two bands.",
        caption: "Six hues will not stay apart at one lightness, so the palette fits them in two bands, and the end labels alternate between them down the right-hand edge. Desaturated, the six fall into two levels of three: a pair from different bands separates by lightness, a pair from the same band by its dash and its label. This figure also carries the tokens a swatch says nothing about: the chapter band behind the plot from 2.0 to 3.5 s, the playhead parked at 3.00 s with its readout and the played part of the track under it, and the peek rule at 1.2 s where a pointer would be resting. The peek rule takes <code>--op-peek</code>, which until this figure was drawn was declared, blended and contrast-tested while both chart stylesheets painted the rule with <code>--op-muted</code>, a colour that carries the same value in both themes and so looked right.",
    },
];

/// The spec a figure draws: one shared percent scale, so the gridlines are
/// the quarters every chart on the site is read at.
fn spec_of(figure: &ChartFigure) -> Spec {
    Spec {
        end: FIGURE_END,
        duration: FIGURE_END,
        y: op_chart::layout::PERCENT,
        ylabel: "per cent".to_owned(),
        chapters: Vec::new(),
        marks: figure
            .mark
            .iter()
            .map(|(t, label)| Mark {
                t: *t,
                label: (*label).to_owned(),
            })
            .collect(),
        band: figure.band.map(|(t0, t1, label)| Band {
            t0,
            t1,
            label: label.to_owned(),
        }),
        series: figure
            .lines
            .iter()
            .map(|line| Series {
                label: line.label.to_owned(),
                index: line.index,
                points: line
                    .values
                    .iter()
                    .enumerate()
                    .map(|(i, v)| Some((i as f64 * SAMPLE_STEP, *v)))
                    .collect(),
                width: SERIES_WIDTH,
            })
            .collect(),
    }
}

/// The renderer opens every chart as a `graphics-document` with a tab stop
/// on it, because a chart is a thing to read and to drive. A specimen's
/// chart follows no clock and answers no key: it is a picture of a chart,
/// and one already described by the figure's own caption. The opening tag
/// is rewritten as a named image, which collapses the subtree the emitter
/// exposed, and the tab stop is dropped.
fn as_image(svg: &str, name: &str) -> String {
    let Some((head, body)) = svg.split_once('>') else {
        return svg.to_owned();
    };
    let view = head
        .split_once("viewBox=\"")
        .and_then(|(_, rest)| rest.split_once('"'))
        .map_or("", |(value, _)| value);
    format!(
        "<svg class=\"chart\" viewBox=\"{view}\" role=\"img\" aria-label=\"{}\">{body}",
        escape(name)
    )
}

/// The writes the live chart makes on every tick: the playhead group's
/// transform, its readout and the width of the played bar, plus the peek
/// rule a pointer moves. op-chart parks all of them at zero, which is the
/// truth for a chart nobody has touched and no use to a figure that exists
/// to show the playhead and peek tokens doing something. They have to move
/// together or the figure contradicts itself (a playhead at 3 s over an
/// unplayed track), so they are one edit list, each string rebuilt from the
/// same layout the renderer drew with rather than guessed at;
/// `applied_charts_park_the_playhead_and_the_peek_rule` fails if any of
/// them stops matching what op-chart emits.
fn park_edits(l: &Layout, figure: &ChartFigure) -> Vec<(String, String)> {
    let head = l.x_of(figure.head);
    let mut edits = vec![
        (
            format!("transform=\"translate({:.1} 0)\"", l.left),
            format!("transform=\"translate({head:.1} 0)\""),
        ),
        (
            // the readout is hidden from the accessibility tree, since the
            // thumb speaks its own value, so the edit carries that attribute
            format!(
                "y=\"{:.1}\" aria-hidden=\"true\">0.00s</text>",
                l.readout_y()
            ),
            format!(
                "y=\"{:.1}\" aria-hidden=\"true\">{:.2}s</text>",
                l.readout_y(),
                figure.head
            ),
        ),
        (
            format!(
                "<rect class=\"bar-played\" x=\"{}\" y=\"{}\" width=\"0\"",
                l.left,
                l.track_y()
            ),
            format!(
                "<rect class=\"bar-played\" x=\"{}\" y=\"{}\" width=\"{:.1}\"",
                l.left,
                l.track_y(),
                head - l.left
            ),
        ),
    ];
    if let Some(peek) = figure.peek {
        let x = l.x_of(peek);
        edits.push((
            format!(
                "<line class=\"peek-line\" x1=\"{}\" x2=\"{}\"",
                l.left, l.left
            ),
            format!("<line class=\"peek-line\" x1=\"{x:.1}\" x2=\"{x:.1}\""),
        ));
        edits.push((
            "visibility=\"hidden\"".to_owned(),
            "visibility=\"visible\"".to_owned(),
        ));
    }
    edits
}

/// One figure's chart, drawn by the real renderer and parked.
fn chart_svg(figure: &ChartFigure, theme: &Theme) -> String {
    let spec = spec_of(figure);
    let rendered = op_chart::render(&spec, Layout::sized(FIGURE_WIDTH, FIGURE_HEIGHT, spec.end));
    let mut svg = as_image(
        &rendered.svg,
        &format!("{}; {} theme.", figure.alt, theme.short),
    );
    for (old, new) in park_edits(&rendered.layout, figure) {
        svg = svg.replace(&old, &new);
    }
    svg
}

/// One theme's panel of a figure. The panel carries the theme's token
/// scope, so everything inside it, chart included, resolves against that
/// theme however the page's own toggle is set.
fn chart_panel(figure: &ChartFigure, theme: &Theme) -> String {
    format!(
        "<div class=\"panel {}{}\"><span class=\"theme-tag\">{}</span>{}</div>",
        theme.class,
        if figure.markers { " show-markers" } else { "" },
        theme.title,
        chart_svg(figure, theme),
    )
}

/// A figure: the same chart in both themes, stacked, with one caption
/// under the pair. Stacked and not side by side because a 600 unit chart
/// squeezed into half the measure would draw its 12 px labels at 5 px, and
/// a palette nobody can read is not evidence of anything.
fn chart_figure_markup(figure: &ChartFigure) -> String {
    let panels: String = THEMES.iter().map(|t| chart_panel(figure, t)).collect();
    format!(
        "<figure class=\"applied-figure\">{panels}<figcaption><strong>{}</strong> {}</figcaption></figure>",
        figure.lead, figure.caption
    )
}

/// The status collection: the support vocabulary as `<opt-term>` renders
/// it. The markup names a scheme and a value and no colour whatever; the
/// element looks the term up in `op_terms` and paints the severity that
/// term is contained in. That is why this earns its place over a row of
/// badges or callouts, which take their severity straight from an
/// attribute: the terms show the whole path from meaning to token, and the
/// support scheme's six terms reach all five status colours.
fn terms_panel(theme: &Theme) -> String {
    let terms: String = op_terms::scheme(TERM_SCHEME)
        .map(|term| {
            format!(
                "<opt-term scheme=\"{TERM_SCHEME}\" value=\"{}\"></opt-term>",
                escape(term.value)
            )
        })
        .collect();
    format!(
        "<div class=\"panel {}\"><span class=\"theme-tag\">{}</span><div class=\"terms\">{terms}</div></div>",
        theme.class, theme.title
    )
}

/// The specimen's own rules for the applied views. They come after the
/// chart's stylesheet so they can override it, and they are the only rules
/// here that are not the chart's own.
const APPLIED_STYLE: &str = "
opt-palette-specimen .applied { margin-top: 2rem; }
opt-palette-specimen .applied-figure { margin: 0.75rem 0 1.75rem; }
opt-palette-specimen .panel {
  background: var(--op-bg);
  color: var(--op-text);
  border: 1px solid var(--op-border);
  border-radius: 0.5rem;
  padding: 0.6rem 0.75rem 0.75rem;
  container-type: inline-size;
}
opt-palette-specimen .panel + .panel { margin-top: 0.4rem; }
opt-palette-specimen .panel .theme-tag {
  display: block;
  margin-bottom: 0.3rem;
  font-size: 0.75rem;
  color: var(--op-muted);
}
opt-palette-specimen .applied-figure.pair {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(14rem, 1fr));
  gap: 0.4rem;
}
opt-palette-specimen .applied-figure.pair .panel + .panel { margin-top: 0; }
opt-palette-specimen .applied-figure.pair figcaption { grid-column: 1 / -1; }
opt-palette-specimen .applied-figure figcaption {
  margin-top: 0.5rem;
  font-size: 0.85rem;
  color: var(--op-muted);
}
opt-palette-specimen .applied-figure figcaption strong { color: var(--op-text); font-weight: 600; }
opt-palette-specimen .terms { display: flex; flex-wrap: wrap; gap: 0.35rem 0.5rem; }
/* The markers a figure shows on purpose; its caption says that the chart
   itself keeps them for the cases where colour is gone. */
opt-palette-specimen .panel.show-markers .chart .marker { display: inline; }
";

/// The whole applied section. The chart figures are drawn with the very
/// stylesheet `<opt-chart>` ships (`chart.rs`'s `stylesheet`), not a copy
/// of it, so the class-to-token mapping, the dash table, and the forced
/// colours and print rules a reader may check are the ones the site
/// actually uses. Its `:host` rules match nothing out here in the light
/// DOM, which costs a few bytes and keeps the rest honest.
fn applied_section() -> String {
    let charts: String = CHART_FIGURES.iter().map(chart_figure_markup).collect();
    let terms: String = THEMES.iter().map(terms_panel).collect();
    format!(
        "<section class=\"applied\">
<style>{}{APPLIED_STYLE}</style>
<h2>The palette at work</h2>
<p class=\"muted\">A swatch says what a colour is, not whether it works. These figures put the same tokens into the elements that use them, pinned to each theme as the columns above are. The chart data is a test pattern rather than a measurement: the curves were chosen so that every pair of series crosses at least once.</p>
{charts}
<figure class=\"applied-figure pair\">{terms}<figcaption><strong>Status, derived rather than chosen.</strong> The support vocabulary, as <code>&lt;opt-term&gt;</code> draws it. Nothing in the markup names a colour: the element looks the term up and paints the severity that term is contained in. Six terms, five colours, because <code>broken</code> and <code>unsupported</code> both sit under danger. Two answers wear one colour and only the word tells them apart, which is the whole reason the status set is five hues and not one per term.</figcaption></figure>
</section>",
        chart_stylesheet(DEFAULT_RATIO)
    )
}

/// One typography candidate: label, note, and the three stacks.
struct TypeOption {
    id: &'static str,
    label: &'static str,
    note: &'static str,
    heading: &'static str,
    body: &'static str,
    mono: &'static str,
    /// Families whose availability the badge reports.
    check: &'static [&'static str],
}

const TYPE_OPTIONS: &[TypeOption] = &[
    TypeOption {
        id: "site",
        label: "Site rendering: the fitted embedded faces",
        note: "Barlow Semi Condensed fitted to Sys 2.0 for headings, IBM Plex Sans body, Iosevka SS08 fitted to PragmataPro for code; identical on every machine.",
        heading: "'Barlow Semi Condensed', system-ui, sans-serif",
        body: "'IBM Plex Sans', system-ui, sans-serif",
        mono: "'Iosevka SS08', ui-monospace, monospace",
        check: &["Barlow Semi Condensed", "IBM Plex Sans", "Iosevka SS08"],
    },
    TypeOption {
        id: "fallback",
        label: "Before the pack arrives, and wherever it never can",
        note: "Metric-fitted local() faces: the swap to the embedded faces changes letterforms without moving layout.",
        heading: "'opt-heading-fallback', system-ui, sans-serif",
        body: "'opt-body-fallback', system-ui, sans-serif",
        mono: "'opt-mono-fallback', ui-monospace, monospace",
        check: &[],
    },
];

const TYPE_STYLE: &str = "
opt-palette-specimen .type-option {
  background: var(--op-surface);
  border: 1px solid var(--op-border-strong);
  border-radius: 0.5rem;
  padding: 0.75rem 1rem;
  margin: 0.75rem 0;
}
opt-palette-specimen .type-option h3 { margin: 0; font-size: 1rem; }
opt-palette-specimen .type-option .t-h {
  font-size: 1.4rem;
  font-weight: 700;
  margin: 0.5rem 0 0.25rem;
}
opt-palette-specimen .type-option .t-b { margin: 0.25rem 0; }
opt-palette-specimen .type-option .t-m {
  font-variant-ligatures: contextual;
  margin: 0.25rem 0 0;
  background: var(--op-code-bg);
  padding: 0.3rem 0.5rem;
  border-radius: 0.3rem;
  white-space: pre-wrap;
}
";

fn type_option_markup(option: &TypeOption) -> String {
    format!(
        "<article class=\"type-option\" id=\"type-{}\">
<h3>{}</h3>
<p class=\"muted\">{} <span class=\"face-status\" data-families=\"{}\"></span></p>
<p class=\"t-h\" style=\"font-family: {}\">OpenPOWER firmware, ports and tools</p>
<p class=\"t-b\" style=\"font-family: {}\">Owner-controlled POWER9 systems: the Talos II and Blackbird boot from fully inspectable firmware. 0123456789 Il1 O0 — <em>italic</em>, <strong>bold</strong>.</p>
<p class=\"t-m\" style=\"font-family: {}\">pflash -E -p /tmp/talos.pnor &amp;&amp; echo ok  # =&gt; != === 0xDEADBEEF fi ffi</p>
</article>",
        option.id,
        option.label,
        option.note,
        option.check.join("|"),
        option.heading,
        option.body,
        option.mono,
    )
}

fn annotate_face_status() {
    let Some(document) = web_sys::window().and_then(|w| w.document()) else {
        return;
    };
    let Ok(nodes) = document.query_selector_all("opt-palette-specimen .face-status") else {
        return;
    };
    for index in 0..nodes.length() {
        let Some(el) = nodes.item(index).and_then(|n| n.dyn_into::<Element>().ok()) else {
            continue;
        };
        let families = el.get_attribute("data-families").unwrap_or_default();
        if families.is_empty() {
            continue;
        }
        let status: Vec<String> = families
            .split('|')
            .map(|family| {
                let resolves = crate::fontprobe::family_resolves(family);
                format!(
                    "{family}: {}",
                    if resolves { "resolves" } else { "not here" }
                )
            })
            .collect();
        el.set_text_content(Some(&status.join(" / ")));
    }
}

fn render_typography(host: &HtmlElement) {
    let cards: String = TYPE_OPTIONS.iter().map(type_option_markup).collect();
    let section = format!(
        "<style>{TYPE_STYLE}</style>
<h2>Typography</h2>
<p class=\"muted\">The typography as everyone sees it: the fitted embedded faces, and the metric-fitted fallbacks that hold layout until the font pack arrives. Locally installed fonts are deliberately not used.</p>
{cards}"
    );
    let current = host.inner_html();
    host.set_inner_html(&format!("{current}{section}"));
    let document = web_sys::window()
        .and_then(|w| w.document())
        .expect("document");
    let ready = document.fonts().ready().expect("fonts.ready");
    wasm_bindgen_futures::spawn_local(async move {
        let _ = wasm_bindgen_futures::JsFuture::from(ready).await;
        annotate_face_status();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fonts arrive only through the pack and the generated fallback
    /// stylesheet; the source stylesheet declares no font sources at all.
    #[test]
    fn stylesheet_declares_no_font_sources() {
        let css = include_str!("../../../../styles/theme.css");
        assert!(
            !css.contains("@font-face"),
            "unexpected @font-face in theme.css"
        );
        assert!(!css.contains("url("), "unexpected url() in theme.css");
        assert!(
            !css.contains(".woff"),
            "unexpected font file reference in theme.css"
        );
    }

    #[test]
    fn type_options_reference_families_from_their_own_stacks() {
        assert!(TYPE_OPTIONS.len() >= 2);
        for option in TYPE_OPTIONS {
            for family in option.check {
                let quoted = format!("'{family}'");
                assert!(
                    option.heading.contains(&quoted)
                        || option.body.contains(&quoted)
                        || option.mono.contains(&quoted),
                    "{}: checked family {family} is not in any stack",
                    option.id
                );
            }
            for stack in [option.heading, option.body, option.mono] {
                assert!(
                    stack.ends_with("sans-serif")
                        || stack.ends_with("monospace")
                        || stack.ends_with("serif"),
                    "{}: stack {stack:?} lacks a generic fallback",
                    option.id
                );
            }
        }
        let markup = type_option_markup(&TYPE_OPTIONS[0]);
        assert!(markup.contains("face-status"));
        assert!(markup.contains(TYPE_OPTIONS[0].label));
    }

    #[test]
    fn every_css_token_has_a_specimen_entry_and_vice_versa() {
        let css = include_str!("../../../../styles/theme.css");
        let dark_block = &css[css.find(":root,").unwrap()..];
        let dark_block = &dark_block[..dark_block.find('}').unwrap()];
        let mut declared: Vec<&str> = dark_block
            .lines()
            .filter_map(|l| l.trim().split_once(':'))
            .map(|(name, _)| name.trim())
            .filter(|name| name.starts_with("--op-"))
            .collect();
        let mut listed: Vec<&str> = TOKENS.iter().map(|(name, _, _, _)| *name).collect();
        declared.sort_unstable();
        listed.sort_unstable();
        assert_eq!(
            declared, listed,
            "styles/theme.css tokens and TOKENS differ"
        );
    }

    /// Every token belongs to exactly one group and every group is drawn,
    /// so no token can be added to the table and land nowhere.
    #[test]
    fn the_swatch_table_is_grouped_and_every_group_is_drawn() {
        let markup = column(&THEMES[0], THEMES[1].short);
        for group in Group::ALL {
            assert!(
                markup.contains(&format!("<h3 class=\"group\">{}</h3>", group.heading())),
                "{} has no heading",
                group.heading()
            );
            assert!(
                TOKENS.iter().any(|(_, _, _, g)| *g == group),
                "{} has no tokens",
                group.heading()
            );
        }
        // the six series, the playhead, the peek rule and the band are one
        // collection, listed together
        let chart: Vec<&str> = TOKENS
            .iter()
            .filter(|(_, _, _, g)| *g == Group::Chart)
            .map(|(name, _, _, _)| *name)
            .collect();
        assert_eq!(
            chart,
            [
                "--op-series-1",
                "--op-series-2",
                "--op-series-3",
                "--op-series-4",
                "--op-series-5",
                "--op-series-6",
                "--op-playhead",
                "--op-peek",
                "--op-band",
            ]
        );
    }

    #[test]
    fn ratio_text_reports_the_right_requirement_per_role() {
        let black = Rgb(0, 0, 0);
        let white = Rgb(255, 255, 255);
        let grey = Rgb(0x76, 0x76, 0x76);
        assert_eq!(
            ratio_text(Role::Text, black, white, white, black),
            "21.00:1 on bg, 21.00:1 on surface (needs 4.5:1)"
        );
        assert!(ratio_text(Role::Ui, grey, white, white, black).ends_with("(needs 3:1)"));
        assert_eq!(
            ratio_text(Role::Backdrop, white, white, white, black),
            "body text on it 21.00:1 (needs 4.5:1)"
        );
        assert_eq!(
            ratio_text(Role::Decoration, grey, white, white, black),
            "decoration only"
        );
    }

    #[test]
    fn columns_render_every_token_once() {
        let markup = column(&THEMES[0], THEMES[1].short);
        for (name, role, _, _) in TOKENS {
            assert_eq!(
                markup.matches(&format!("data-token=\"{name}\"")).count(),
                1,
                "{name}"
            );
            assert!(markup.contains(role), "{role}");
            // both readouts are present and empty; `annotate` fills them
            // from the two live columns
            assert!(markup.contains("<span class=\"hex\"></span>"));
            assert!(markup.contains("<span class=\"other\"></span>"));
        }
        assert!(markup.contains("<opt-site-header"));
        assert!(markup.starts_with("<section class=\"opt-theme-dark column\">"));
        // the column says whose value the second readout carries
        assert!(markup.contains("in the light theme"));
    }

    // ---- the applied views ---------------------------------------------

    /// The point of the applied section: a series token must be somewhere
    /// other than its own swatch. The figures never name a token, they
    /// name the class the renderer writes, so both ends are asserted: the
    /// class is drawn, and the stylesheet the figures are styled with is
    /// what maps that class to the token.
    #[test]
    fn every_series_token_is_at_work_and_not_only_in_the_swatch_table() {
        let applied = applied_section();
        let table = column(&THEMES[0], THEMES[1].short);
        for n in 1..=super::super::chart_style::SERIES_TOKENS {
            let token = format!("--op-series-{n}");
            assert!(
                TOKENS.iter().any(|(name, _, _, _)| *name == token),
                "{token} is not in the swatch table"
            );
            assert!(table.contains(&token), "{token} has no swatch");
            assert!(
                applied.contains(&format!("class=\"series-{n}\"")),
                "series {n} draws nothing in an applied figure"
            );
            assert!(
                applied.contains(&format!(".chart .series-{n} {{ stroke: var({token}); }}")),
                "nothing maps series-{n} to {token}"
            );
        }
    }

    /// Every chart token the specimen lists must be reachable from the
    /// stylesheet its figures are drawn with, so that changing the token
    /// changes the figure. Writing this test is what found `--op-peek`
    /// declared, blended by the theme transition and contrast-tested while
    /// both chart stylesheets painted the peek rule with `--op-muted`,
    /// which carries the same value in both themes and so looked right.
    /// The rule now takes its own token, and the list is empty.
    #[test]
    fn every_chart_token_reaches_the_applied_figures() {
        let applied = applied_section();
        let unreferenced: Vec<&str> = TOKENS
            .iter()
            .filter(|(_, _, _, group)| *group == Group::Chart)
            .map(|(name, _, _, _)| *name)
            .filter(|name| !applied.contains(&format!("var({name})")))
            .collect();
        assert!(
            unreferenced.is_empty(),
            "chart tokens nothing paints with: {unreferenced:?}"
        );
    }

    /// The figures are styled by the chart's own rules, included whole,
    /// not by a copy: the dash table, the forced-colours mapping and the
    /// print mapping a reader may check are the ones the site ships.
    #[test]
    fn the_applied_figures_carry_the_charts_own_rules() {
        let applied = applied_section();
        for (what, rules) in [
            ("the dash table", super::super::chart_style::SERIES_CSS),
            (
                "forced colours",
                super::super::chart_style::FORCED_COLOURS_CSS,
            ),
            ("print", super::super::chart_style::PRINT_CSS),
        ] {
            assert!(applied.contains(rules), "{what} is not included whole");
        }
    }

    /// Each figure names the palette slot of every series it draws, in the
    /// class and in the exported part, and gives each one a direct end
    /// label, once per theme. Dashes and labels are decision 24's cues
    /// that survive without colour, so a figure that lost its end labels
    /// would be showing colour alone.
    #[test]
    fn every_chart_figure_names_its_series_and_labels_them_directly() {
        assert!(CHART_FIGURES.len() >= 2);
        for figure in CHART_FIGURES {
            let markup = chart_figure_markup(figure);
            for line in figure.lines {
                let n = line.index;
                assert!(
                    markup.contains(&format!("<path class=\"series-{n}\"")),
                    "no line for series {n}"
                );
                assert!(
                    markup.contains(&format!("part=\"series-{n}\"")),
                    "series {n} is not exported as a part"
                );
                assert_eq!(
                    markup.matches(&format!(">{}</text>", line.label)).count(),
                    THEMES.len(),
                    "the end label {:?} is not drawn once per theme",
                    line.label
                );
            }
            assert_eq!(
                markup.matches("class=\"endlabel\"").count(),
                figure.lines.len() * THEMES.len()
            );
            // both themes, each pinned to its own token scope
            for theme in &THEMES {
                assert!(markup.contains(&format!("<div class=\"panel {}", theme.class)));
            }
        }
        // the two sets decision 22 defines: the robust four and all six
        let sets: Vec<usize> = CHART_FIGURES.iter().map(|f| f.lines.len()).collect();
        assert_eq!(sets, [4, super::super::chart_style::SERIES_TOKENS]);
        for (n, line) in TWO_BAND_SET.iter().enumerate() {
            assert_eq!(line.index, n + 1, "the six-series figure skips a slot");
        }
    }

    /// Every series of every figure is sampled over the same axis, so the
    /// two figures are read at one scale and the end labels land where the
    /// data ends rather than short of it.
    #[test]
    fn every_figure_series_spans_the_whole_time_axis() {
        for figure in CHART_FIGURES {
            for line in figure.lines {
                assert_eq!(line.values.len(), SAMPLES, "{}", line.label);
            }
        }
        assert_eq!(FIGURE_END, (SAMPLES - 1) as f64 * SAMPLE_STEP);
        assert!((FIGURE_WIDTH / FIGURE_HEIGHT - DEFAULT_RATIO).abs() < 1e-9);
    }

    /// The playhead, its readout, the played bar and the peek rule are
    /// parked together. Each edit is rebuilt from the layout the renderer
    /// drew with, so this fails the moment op-chart emits any of them
    /// differently, rather than silently leaving a figure at zero.
    #[test]
    fn applied_charts_park_the_playhead_and_the_peek_rule() {
        for figure in CHART_FIGURES {
            let spec = spec_of(figure);
            let layout = Layout::sized(FIGURE_WIDTH, FIGURE_HEIGHT, spec.end);
            let markup = chart_figure_markup(figure);
            let edits = park_edits(&layout, figure);
            assert_eq!(edits.len(), if figure.peek.is_some() { 5 } else { 3 });
            for (old, new) in edits {
                assert!(!markup.contains(&old), "{old:?} was never rewritten");
                assert_eq!(
                    markup.matches(&new).count(),
                    THEMES.len(),
                    "{new:?} is not in both themes"
                );
            }
            // the readout agrees with where the playhead was parked
            assert!(markup.contains(&format!(">{:.2}s</text>", figure.head)));
        }
    }

    /// The figure that carries the annotation tokens draws all three of
    /// the things a swatch cannot show, in both themes.
    #[test]
    fn one_figure_draws_the_band_the_playhead_and_the_peek_rule() {
        let figure = CHART_FIGURES
            .iter()
            .find(|f| f.band.is_some())
            .expect("a figure with a band");
        assert!(figure.peek.is_some(), "the band figure must peek too");
        assert!(figure.mark.is_some());
        let markup = chart_figure_markup(figure);
        for class in [
            "band",
            "peek-line",
            "head",
            "head-dot",
            "bar-played",
            "mark",
        ] {
            assert_eq!(
                markup.matches(&format!("class=\"{class}\"")).count(),
                THEMES.len(),
                "{class} is not drawn once per theme"
            );
        }
        // the other figure is left exactly as the chart draws it, which is
        // where the difference in weight between the two comes from
        let plain = CHART_FIGURES
            .iter()
            .find(|f| f.band.is_none())
            .expect("a figure without a band");
        assert!(!chart_figure_markup(plain).contains("class=\"band\""));
        assert!(plain.markers && !figure.markers);
    }

    /// The status collection: every status token is reached by a term of
    /// the vocabulary the figure draws, and the figure names no colour at
    /// all, because the element derives it.
    #[test]
    fn the_support_terms_reach_every_status_token_without_naming_one() {
        let panel = terms_panel(&THEMES[0]);
        let mut reached: Vec<&str> = Vec::new();
        for term in op_terms::scheme(TERM_SCHEME) {
            assert!(
                panel.contains(&format!("value=\"{}\"", term.value)),
                "{} is missing from the figure",
                term.value
            );
            let severity = term.broader.name();
            if !reached.contains(&severity) {
                reached.push(severity);
            }
        }
        for (name, _, _, group) in TOKENS {
            if *group != Group::Status {
                continue;
            }
            let severity = name
                .strip_prefix("--op-status-")
                .expect("a status token is named after its severity");
            assert!(
                reached.contains(&severity),
                "no {TERM_SCHEME} term projects to {name}"
            );
        }
        assert!(
            !panel.contains("--op-"),
            "the terms figure must name no token: the vocabulary chooses it"
        );
        // two terms, one colour: the case the caption is about
        assert_eq!(
            op_terms::severity_of(TERM_SCHEME, "broken").name(),
            op_terms::severity_of(TERM_SCHEME, "unsupported").name()
        );
    }

    /// Every applied figure says what to look for, in a caption of its
    /// own, and each panel is pinned to a theme.
    #[test]
    fn every_applied_figure_carries_a_caption_and_both_themes() {
        let applied = applied_section();
        assert_eq!(
            applied.matches("<figure class=\"applied-figure").count(),
            CHART_FIGURES.len() + 1,
            "one figure per chart, plus the status collection"
        );
        assert_eq!(
            applied.matches("<figcaption>").count(),
            CHART_FIGURES.len() + 1
        );
        for figure in CHART_FIGURES {
            assert!(applied.contains(figure.lead));
            assert!(applied.contains(figure.caption));
            assert!(applied.contains(figure.alt));
        }
        for theme in &THEMES {
            assert_eq!(
                applied.matches(&format!("panel {}", theme.class)).count(),
                CHART_FIGURES.len() + 1
            );
        }
        // a chart that is not a control: no tab stop, no frozen slider.
        // The cue buttons the emitter writes carry `tabindex="-1"`, which
        // is programmatic focus and no tab stop at all, and a named image
        // hides them along with everything else it holds.
        assert!(!applied.contains("role=\"slider\""));
        assert!(!applied.contains("tabindex=\"0\""));
        assert!(!applied.contains("aria-valu"));
        assert_eq!(
            applied.matches("role=\"img\"").count(),
            CHART_FIGURES.len() * THEMES.len()
        );
    }
}
