//! `<opt-chart for="film">`: a time-series chart that follows a film's
//! clock. It never advances time itself; it is a projection of the film's
//! timeline, as the machine diagram is, so the only thing that moves per
//! tick is the playhead group, its readout and the played bar.
//!
//! The markup is built by pure functions ([`shadow_markup`],
//! [`prerender`]) that the page build calls natively, so a page ships the
//! chart finished: the SVG, a caption with a one-paragraph summary, and
//! the data as a real table behind a disclosure. Nothing of that needs a
//! script, which is what keeps the chart complete where WebAssembly is
//! unavailable (Firefox on ppc64le).
//!
//! A pre-render scales with its viewBox, so a 640 unit box at a 360 px
//! viewport would shrink its 12 px labels to about 6 px. The build
//! therefore emits two SVGs, `chart wide` and `chart narrow`, and a
//! container query at [`NARROW_AT`] chooses between them with no script at
//! all.
//!
//! On upgrade the element compares the block's hash with the `data-hash`
//! the build wrote. When they agree it keeps the markup and sets the
//! custom state `hydrated`, so a report can witness that nothing was
//! rewritten; otherwise it renders. Either way the `ResizeObserver`
//! delivers a notification as soon as it observes the host, and that first
//! frame is treated like any other resize: at the width one of the
//! pre-rendered charts was drawn for, the pair collapses to that chart and
//! it stays exactly as the build wrote it, `data-rendered-by="op-pages"`
//! and all; at any other width the element draws one SVG of its own,
//! marked `data-rendered-by="op-site"`. Either way the shadow root then
//! holds exactly one `svg.chart` whose viewBox width is its CSS width and
//! whose text is at its token size.
//!
//! The first tick from the bound film sets the custom state `following`,
//! and `playing` mirrors the film's own.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use op_chart::{Data, Layout};
use op_webc::{CustomElement, ElementDefinition, set_state};
use wasm_bindgen::prelude::*;
use web_sys::{Element, Event, HtmlElement, ShadowRoot};

use super::chart_style::chart_rules;
use super::machine_diagram::film_time_of;
use super::{BASE_CSS, shadow_root};
use crate::html::escape;

pub const DEFINITION: ElementDefinition = ElementDefinition {
    source: op_webc::here!(),
    tag: "opt-chart",
    observed_attributes: &["for", "initial-width", "ratio"],
    properties: &["data", "forElement"],
    create: |host| {
        Box::new(Chart {
            host,
            block: String::new(),
            data_value: None,
            for_element: None,
            wiring: None,
        })
    },
};

/// The width in CSS px a pre-render is drawn at when the element does not
/// say otherwise.
pub const DEFAULT_WIDTH: f64 = 640.0;
/// Width over height of the chart box when the element does not say.
pub const DEFAULT_RATIO: f64 = 16.0 / 6.0;
/// The width the second, narrow pre-render is drawn at.
pub const NARROW_WIDTH: f64 = 360.0;
/// Where the wide pre-render gives way to the narrow one: below this
/// container width its 12 px labels, scaled by the viewBox, fall under the
/// 10 px floor.
pub const NARROW_AT: f64 = 532.0;
/// Below this container width every second time label and the chapter cues
/// are dropped: they cannot be read at that size and they overprint.
const DROP_AT: f64 = 480.0;

/// What the caption says drew the markup: the build, or the element.
const BY_PAGES: &str = "op-pages";
const BY_SITE: &str = "op-site";

// ---- pure markup ------------------------------------------------------

/// Whether an upgrading element may keep the markup it found. Only a
/// shadow root whose `data-hash` still names this block can be kept; the
/// absence of either is a re-render.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Hydrate,
    Render,
}

/// See [`Action`].
pub fn hydrate_or_render(has_root: bool, hash_attr: Option<&str>, block_hash: &str) -> Action {
    if has_root && hash_attr == Some(block_hash) {
        Action::Hydrate
    } else {
        Action::Render
    }
}

/// Which of the pre-render's two charts is meant: the one drawn at the
/// element's `initial-width`, or the one drawn at [`NARROW_WIDTH`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Variant {
    Wide,
    Narrow,
}

/// Which pre-rendered chart the element may keep at the measured width,
/// [`None`] to draw one of its own. The container query decides which of
/// the pair is on screen, so that is the only one worth keeping: it is kept
/// when it was drawn at the width it ended up in, to within a pixel, and
/// then its height matches too, since the build and the element divide that
/// same width by the same `ratio`. Keeping it is the point of the
/// pre-render: the markup a re-render would produce is already there,
/// carrying the mark that says the build drew it.
pub fn keep_prerender(hydrated: bool, measured: f64, wide: f64, narrow: f64) -> Option<Variant> {
    if !hydrated {
        return None;
    }
    let (variant, drawn) = if measured <= NARROW_AT {
        (Variant::Narrow, narrow)
    } else {
        (Variant::Wide, wide)
    };
    ((measured - drawn).abs() <= 1.0).then_some(variant)
}

/// Whether a tick is worth touching the DOM for: half a pixel of travel,
/// or a changed readout. Below that the playhead would not move and the
/// text would not change, and an attribute write per frame is not free.
pub fn tick_change(prev_x: f64, x: f64, prev_label: &str, label: &str) -> bool {
    !((x - prev_x).abs() < 0.5 && prev_label == label)
}

/// The playhead's readout, in the format the film's own chart uses.
pub fn readout(t: f64) -> String {
    format!("{t:.2}s")
}

/// Names as a sentence joins them: commas between all but the last two,
/// "and" before the last.
fn join(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [head @ .., last] => format!("{} and {last}", head.join(", ")),
    }
}

/// `1 chapter` / `4 chapters`, for a noun whose plural is the `s` form.
fn count(n: usize, noun: &str) -> String {
    if n == 1 {
        format!("1 {noun}")
    } else {
        format!("{n} {noun}s")
    }
}

/// The caption's heading: every series the chart draws, in order.
fn title_of(d: &Data) -> String {
    let labels: Vec<String> = d.series.iter().map(|s| s.label.clone()).collect();
    join(&labels)
}

/// The caption's paragraph: what the chart shows, said once in prose, so
/// the figure carries its own description for a reader who cannot see it.
fn summary_of(d: &Data) -> String {
    let mut parts: Vec<String> = Vec::new();
    let labels: Vec<String> = d.series.iter().map(|s| s.label.clone()).collect();
    let series = if d.series.len() == 1 {
        "1 series".to_owned()
    } else {
        format!("{} series", d.series.len())
    };
    parts.push(if labels.is_empty() {
        series
    } else {
        format!("{series} ({})", join(&labels))
    });
    let mut units: Vec<String> = Vec::new();
    for s in &d.series {
        if !s.unit.is_empty() && !units.contains(&s.unit) {
            units.push(s.unit.clone());
        }
    }
    if !units.is_empty() {
        parts.push(format!("measured in {}", join(&units)));
    }
    parts.push(match (d.rows.first(), d.rows.last()) {
        (Some(first), Some(last)) => format!(
            "{} from {:.2} s to {:.2} s",
            count(d.rows.len(), "sample"),
            first.t,
            last.t
        ),
        _ => "no samples".to_owned(),
    });
    parts.push(if d.chapters.is_empty() {
        "no chapters".to_owned()
    } else {
        let titles: Vec<String> = d.chapters.iter().map(|c| c.label.clone()).collect();
        format!("{} ({})", count(d.chapters.len(), "chapter"), join(&titles))
    });
    if !d.marks.is_empty() {
        let marks: Vec<String> = d
            .marks
            .iter()
            .map(|m| format!("{} at {:.2} s", m.label, m.t))
            .collect();
        parts.push(format!(
            "{} ({})",
            count(d.marks.len(), "mark"),
            join(&marks)
        ));
    }
    if let Some(band) = &d.band {
        parts.push(format!(
            "a band ({} from {:.2} s to {:.2} s)",
            band.label, band.t0, band.t1
        ));
    }
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for value in d.rows.iter().flat_map(|r| r.values.iter().flatten()) {
        lo = lo.min(*value);
        hi = hi.max(*value);
    }
    if lo <= hi {
        parts.push(format!("values from {lo} to {hi}"));
    }
    format!("{}.", parts.join("; "))
}

/// The title of the chapter a time falls in: the latest one at or before
/// it, and nothing at all before the first.
fn chapter_at(d: &Data, t: f64) -> String {
    d.chapters
        .iter()
        .rfind(|c| c.t <= t + 1e-9)
        .map(|c| c.label.clone())
        .unwrap_or_default()
}

/// The data as a real table: one row per sample under the chapter it falls
/// in, with the marks, the band edges and the chapter starts interleaved on
/// the same time axis, so everything the chart draws can also be read.
fn table_of(d: &Data) -> String {
    let mut head =
        String::from("<tr><th scope=\"col\">time (s)</th><th scope=\"col\">chapter</th>");
    for s in &d.series {
        let name = if s.unit.is_empty() {
            s.label.clone()
        } else {
            format!("{} ({})", s.label, s.unit)
        };
        head.push_str(&format!("<th scope=\"col\">{}</th>", escape(&name)));
    }
    head.push_str("</tr>");

    // (time, 0 for a sample and 1 for an annotation, the cells): sorted
    // together so the table reads down the timeline, samples first where
    // an annotation shares their instant
    let mut rows: Vec<(f64, u8, String)> = Vec::new();
    for row in &d.rows {
        let mut cells = format!(
            "<td>{:.2}</td><td>{}</td>",
            row.t,
            escape(&chapter_at(d, row.t))
        );
        for j in 0..d.series.len() {
            match row.values.get(j).copied().flatten() {
                Some(v) => cells.push_str(&format!("<td>{v}</td>")),
                None => cells.push_str("<td></td>"),
            }
        }
        rows.push((row.t, 0, cells));
    }
    let blanks = "<td></td>".repeat(d.series.len());
    for mark in &d.marks {
        rows.push((
            mark.t,
            1,
            format!(
                "<td>{:.2}</td><td>mark: {}</td>{blanks}",
                mark.t,
                escape(&mark.label)
            ),
        ));
    }
    if let Some(band) = &d.band {
        for (t, edge) in [(band.t0, "start"), (band.t1, "end")] {
            rows.push((
                t,
                1,
                format!(
                    "<td>{t:.2}</td><td>band {edge}: {}</td>{blanks}",
                    escape(&band.label)
                ),
            ));
        }
    }
    // a row per chapter the chart draws a cue for, which is every one after
    // the first: the first names the start of the axis and draws nothing
    for chapter in d.chapters.iter().skip(1) {
        rows.push((
            chapter.t,
            1,
            format!(
                "<td>{:.2}</td><td>chapter: {}</td>{blanks}",
                chapter.t,
                escape(&chapter.label)
            ),
        ));
    }
    // an insertion sort: the samples arrive in time order and only the few
    // annotations move, and the standard library's generic stable sort would
    // cost several kilobytes of wasm for this one call
    for k in 1..rows.len() {
        let mut j = k;
        while j > 0 && (rows[j - 1].0, rows[j - 1].1) > (rows[j].0, rows[j].1) {
            rows.swap(j - 1, j);
            j -= 1;
        }
    }
    let body: String = rows
        .iter()
        .map(|(_, _, cells)| format!("<tr>{cells}</tr>"))
        .collect();
    format!(
        "<details class=\"data\"><summary>Data table</summary><table><thead>{head}</thead><tbody>{body}</tbody></table></details>"
    )
}

/// The id the caption's heading carries, so the figure can name itself.
/// Ids are scoped to the shadow root, so every chart on a page uses it.
const TITLE_ID: &str = "chart-title";

/// One chart, with the hooks the element adds to op-chart's markup: the
/// class that a container query switches on, the mark that says which side
/// drew it, and an opening tag that is not the film's slider.
///
/// The emitter opens every chart as the film's `role="slider"`, which is
/// right there: the film's chart is a control, and the film writes its
/// `aria-valuenow` on every tick. Nothing here drives a thumb - the
/// playhead follows the film's clock and keys are a later phase - so a
/// focusable slider frozen at zero would be a control that reports the
/// wrong value and answers no key. The tag is rewritten as a graphics
/// document labelled by the caption instead; `part="chart"` and the
/// viewBox are the emitter's own.
fn svg_of(spec: &op_chart::Spec, layout: Layout, class: &str, rendered_by: &str) -> String {
    let rendered = op_chart::render(spec, layout).svg;
    let (head, body) = match rendered.split_once('>') {
        Some(split) => split,
        None => return rendered,
    };
    let view = head
        .split_once("viewBox=\"")
        .and_then(|(_, rest)| rest.split_once('"'))
        .map_or("", |(value, _)| value);
    format!(
        "<svg class=\"{}\" part=\"chart\" data-rendered-by=\"{}\" viewBox=\"{view}\" role=\"graphics-document\" aria-labelledby=\"{TITLE_ID}\">{body}",
        escape(class),
        escape(rendered_by),
    )
}

/// The whole inner HTML of a chart's shadow root. With a `narrow` layout
/// the figure carries both pre-renders and the stylesheet chooses; with
/// none it carries the single chart the element just drew.
pub fn shadow_markup(
    data: &Data,
    wide: Layout,
    narrow: Option<Layout>,
    ratio: f64,
    rendered_by: &str,
) -> String {
    let spec = data.to_spec();
    let mut charts = svg_of(
        &spec,
        wide,
        if narrow.is_some() {
            "chart wide"
        } else {
            "chart"
        },
        rendered_by,
    );
    if let Some(narrow) = narrow {
        charts.push_str(&svg_of(&spec, narrow, "chart narrow", rendered_by));
    }
    format!(
        "<style>{BASE_CSS}{}</style><figure class=\"chart\">{charts}<figcaption><strong class=\"title\" id=\"{TITLE_ID}\">{}</strong><p class=\"summary\">{}</p></figcaption></figure>{}",
        stylesheet(ratio),
        escape(&title_of(data)),
        escape(&summary_of(data)),
        table_of(data)
    )
}

/// A chart the page build has finished: the hash it was drawn from, the
/// shadow root's markup, and the data block to put back in the light DOM.
#[derive(Clone, Debug)]
pub struct Prerender {
    pub hash: String,
    pub shadow: String,
    pub block: String,
}

/// Draw `block` at the build's two widths. The error is the reader's own
/// message, which names the field at fault.
pub fn prerender(block: &str, initial_width: f64, ratio: f64) -> Result<Prerender, String> {
    let data = op_chart::data::parse(block).map_err(|e| e.message)?;
    let end = data.end();
    let shadow = shadow_markup(
        &data,
        Layout::sized(initial_width, initial_width / ratio, end),
        Some(Layout::sized(NARROW_WIDTH, NARROW_WIDTH / ratio, end)),
        ratio,
        BY_PAGES,
    );
    // the element reads the text of the emitted script, which is the
    // escaped block, so that is the text the hash has to be over; the two
    // spellings differ only when the block can close a script element, and
    // there the raw hash would never match what the browser hands back
    let escaped = op_chart::escape_script(block);
    Ok(Prerender {
        hash: op_chart::hash_hex(&escaped),
        shadow,
        block: format!("<script type=\"application/json\">{escaped}</script>"),
    })
}

/// The chart's own rules. Colours come from the site's tokens, which
/// inherit across the shadow boundary; the shared blocks (the palette
/// table, forced colours, print) are the film's, included whole.
///
/// `ratio` is the element's own, so the CSS box and the viewBox the
/// geometry was drawn at are one thing: `--op-chart-ratio` overrides the
/// box, and with nothing set the box follows the attribute.
///
/// The gridlines are axis-aligned hairlines and take `crispEdges`, so they
/// land on a device pixel instead of being blurred across two. The swatch
/// keeps it as well, though it is a legend key rather than a rule: the
/// interaction report samples its colour a pixel at a time, and an
/// anti-aliased end would give the probe a blend of the swatch and the
/// surface behind it.
pub(crate) fn stylesheet(ratio: f64) -> String {
    let rules = chart_rules();
    // the default box is written as the fraction it was designed as
    let ratio = if ratio == DEFAULT_RATIO {
        "16 / 6".to_owned()
    } else {
        format!("{ratio}")
    };
    format!(
        "
:host {{ display: block; container-type: inline-size; }}
:host([hidden]) {{ display: none; }}
figure.chart {{ margin: 0; }}
svg.chart {{ display: block; width: 100%; height: auto; aspect-ratio: var(--op-chart-ratio, {ratio}); font-family: var(--op-font-sans); font-size: 12px; }}
svg.chart.narrow {{ display: none; }}
.chart .grid {{ stroke: var(--op-border); shape-rendering: crispEdges; }} .chart .tick {{ stroke: var(--op-border-strong); }} .chart .axis {{ fill: var(--op-muted); }}
.chart .mark {{ stroke: var(--op-accent); stroke-dasharray: 3 3; }} .chart .marklabel {{ fill: var(--op-accent); }}
.chart .endlabel {{ fill: var(--op-text); font-size: 12px; font-weight: 700; paint-order: stroke; stroke: var(--op-surface); stroke-width: 3; }}
.chart .swatch {{ stroke-width: 3; shape-rendering: crispEdges; }}
.chart .marker {{ display: none; fill: var(--op-surface); stroke-width: 1.5; stroke-dasharray: none; }} .chart .marker.shown {{ display: inline; }}
{rules}
@media (prefers-contrast: more) {{ .chart .grid {{ stroke: var(--op-border-strong); }} .chart path[class^=series] {{ stroke-width: 3; }} .chart .marker {{ display: inline; }} }}
.chart .band {{ fill: var(--op-band); opacity: 0.5; }} .chart .bar-bg {{ fill: var(--op-border); }} .chart .bar-played {{ fill: var(--op-band); }}
.chart .chapter {{ fill: var(--op-surface); stroke: var(--op-border-strong); stroke-width: 0.6; }}
.chart .peek-line {{ stroke: var(--op-muted); stroke-dasharray: 3 3; }}
.chart .head {{ stroke: var(--op-playhead); stroke-width: 1.5; }} .chart .head-dot {{ fill: var(--op-playhead); }}
.chart .head-t {{ fill: var(--op-playhead); font-weight: 700; paint-order: stroke; stroke: var(--op-surface); stroke-width: 4; }}
figcaption {{ margin-top: 0.4rem; font-size: 0.9rem; }}
figcaption .title {{ display: block; font-family: var(--op-font-heading); }}
figcaption .summary {{ color: var(--op-muted); margin: 0.2rem 0 0; }}
details.data {{ margin-top: 0.4rem; font-size: 0.85rem; }}
details.data summary {{ cursor: pointer; color: var(--op-muted); }}
details.data table {{ border-collapse: collapse; margin-top: 0.4rem; font-variant-numeric: tabular-nums; }}
details.data th, details.data td {{ border-bottom: 1px solid var(--op-border); padding: 0.1rem 0.5rem 0.1rem 0; text-align: left; }}
@container (max-width: {NARROW_AT}px) {{ svg.chart.wide {{ display: none; }} svg.chart.narrow {{ display: block; }} }}
@container (max-width: {DROP_AT}px) {{ .tick-label.alt {{ display: none; }} .chapters {{ display: none; }} }}"
    )
}

// ---- the element ------------------------------------------------------

/// The handles a tick and a re-layout act on, re-read whenever the SVG is
/// replaced.
#[derive(Default)]
struct Dom {
    svg: Option<Element>,
    playhead: Option<Element>,
    head_t: Option<Element>,
    played: Option<Element>,
    figure: Option<Element>,
}

/// What the element keeps between ticks.
struct Live {
    data: Data,
    /// The geometry the visible chart was drawn with; the playhead and a
    /// later hit-test both read it.
    pub(crate) layout: Layout,
    /// Width over height of the chart box.
    ratio: f64,
    /// The width to draw at when the host cannot be measured.
    fallback_width: f64,
    /// The clock the last tick delivered.
    time: f64,
    /// An animation frame is already asked for.
    pending: bool,
    /// Its handle, so a disconnect can take it back.
    raf: Option<i32>,
    /// The next frame must re-lay out before it draws.
    resize: bool,
    /// Where the playhead was last put, and what it last said.
    x: f64,
    label: String,
    /// The element whose ticks reach us, once it has been resolved.
    bound: Option<Element>,
}

/// The listener that reads a film's tick, shared so that the document-level
/// one can hand it to the element it just resolved.
type TickListener = Rc<Closure<dyn FnMut(Event)>>;

struct Follower {
    host: HtmlElement,
    root: ShadowRoot,
    dom: RefCell<Dom>,
    live: RefCell<Live>,
    /// The listener added to the bound element, kept so binding a second
    /// element can take the first one off.
    on_tick: RefCell<Option<TickListener>>,
    frame: RefCell<Option<Closure<dyn FnMut()>>>,
    following: Cell<bool>,
    /// The markup the build wrote is still on screen, so a re-layout at the
    /// width it was drawn for can keep it.
    hydrated: Cell<bool>,
}

fn document() -> Option<web_sys::Document> {
    web_sys::window()?.document()
}

impl Follower {
    /// Re-read the parts the effects act on out of `svg`.
    fn capture(&self, svg: Option<Element>) {
        let find = |selector: &str| {
            svg.as_ref()
                .and_then(|s| s.query_selector(selector).ok().flatten())
        };
        *self.dom.borrow_mut() = Dom {
            playhead: find("g.playhead"),
            head_t: find(".head-t"),
            played: find(".bar-played"),
            figure: self.root.query_selector("figure").ok().flatten(),
            svg,
        };
    }

    /// The host's width in CSS px, or the width the element was told to
    /// draw at when it has not been laid out yet.
    fn width(&self) -> f64 {
        let measured = self.host.get_bounding_client_rect().width();
        if measured > 0.0 {
            measured
        } else {
            self.live.borrow().fallback_width
        }
    }

    /// Write the whole shadow root: one chart at the measured width, its
    /// caption and its table. Used for a first render and after the data
    /// property replaces the block.
    fn render(&self) {
        let width = self.width();
        let (markup, layout) = {
            let live = self.live.borrow();
            let layout = Layout::sized(width, width / live.ratio, live.data.end());
            (
                shadow_markup(&live.data, layout, None, live.ratio, BY_SITE),
                layout,
            )
        };
        self.root.set_inner_html(&markup);
        self.live.borrow_mut().layout = layout;
        self.capture(self.root.query_selector("svg.chart").ok().flatten());
    }

    /// Keep the pre-render at `width` when it was drawn for that box: the
    /// chart the container query is not showing goes, the other stays as
    /// the build wrote it, and the element takes its handles and the
    /// geometry it was drawn with. False when there is nothing to keep, and
    /// the caller draws its own.
    fn keep(&self, width: f64) -> bool {
        let (ratio, wide, end) = {
            let live = self.live.borrow();
            (live.ratio, live.fallback_width, live.data.end())
        };
        let Some(variant) = keep_prerender(self.hydrated.get(), width, wide, NARROW_WIDTH) else {
            return false;
        };
        let (kept, unused, drawn) = match variant {
            Variant::Wide => ("svg.chart.wide", "svg.chart.narrow", wide),
            Variant::Narrow => ("svg.chart.narrow", "svg.chart.wide", NARROW_WIDTH),
        };
        let pick = |selector: &str| self.root.query_selector(selector).ok().flatten();
        let Some(svg) = pick(kept) else {
            return false;
        };
        if let Some(other) = pick(unused) {
            other.remove();
        }
        // the box the survivor was drawn in, not the measured width: they
        // agree to within a pixel, and the viewBox is in those units
        self.live.borrow_mut().layout = Layout::sized(drawn, drawn / ratio, end);
        self.capture(Some(svg));
        true
    }

    /// Re-draw the chart alone at the measured width. Whatever the
    /// pre-render left behind - the wide and narrow pair, or a single
    /// chart - is replaced by the one live chart, so from here on the
    /// viewBox is the CSS box and the text is at its token size.
    ///
    /// Unless the pre-render is already right: at the width it was drawn
    /// for, the pair collapses to the chart the container query was showing
    /// and nothing is drawn again.
    fn relayout(&self) {
        let width = self.width();
        if self.keep(width) {
            return;
        }
        let (markup, layout) = {
            let live = self.live.borrow();
            let layout = Layout::sized(width, width / live.ratio, live.data.end());
            (
                svg_of(&live.data.to_spec(), layout, "chart", BY_SITE),
                layout,
            )
        };
        self.hydrated.set(false);
        // the chart on screen is the one to replace; the other half of a
        // pre-rendered pair goes, so one live chart is left
        let target = self.dom.borrow().svg.clone();
        if let Ok(list) = self.root.query_selector_all("svg.chart") {
            for i in 0..list.length() {
                let Some(element) = list.item(i).and_then(|n| n.dyn_into::<Element>().ok()) else {
                    continue;
                };
                if target.as_ref() != Some(&element) {
                    element.remove();
                }
            }
        }
        match target {
            Some(element) => element.set_outer_html(&markup),
            // nothing to replace: a shadow root that lost its chart still
            // gets one, so a tick is never dropped on the floor
            None => {
                if let Some(figure) = &self.dom.borrow().figure {
                    let _ = figure.insert_adjacent_html("afterbegin", &markup);
                }
            }
        }
        self.live.borrow_mut().layout = layout;
        self.capture(self.root.query_selector("svg.chart").ok().flatten());
    }

    /// Put the playhead, its readout and the played bar at the stored
    /// time. `force` re-applies them after a re-layout, where the geometry
    /// changed under an unchanged clock.
    fn apply(&self, force: bool) {
        let (t, layout, prev_x, prev_label) = {
            let live = self.live.borrow();
            (live.time, live.layout, live.x, live.label.clone())
        };
        let x = layout.x_of(t.clamp(0.0, layout.end));
        let label = readout(t);
        if !force && !tick_change(prev_x, x, &prev_label, &label) {
            return;
        }
        let dom = self.dom.borrow();
        if let Some(playhead) = &dom.playhead {
            let _ = playhead.set_attribute("transform", &format!("translate({x:.1} 0)"));
        }
        if let Some(head_t) = &dom.head_t {
            head_t.set_text_content(Some(&label));
        }
        if let Some(played) = &dom.played {
            let _ = played.set_attribute("width", &format!("{:.1}", (x - layout.left).max(0.0)));
        }
        drop(dom);
        let mut live = self.live.borrow_mut();
        live.x = x;
        live.label = label;
    }

    /// One animation frame: re-lay out when a resize asked for it, then
    /// move the playhead. A hidden document draws nothing and asks for the
    /// frame again without clearing the flag, so the work stays armed and
    /// no second request piles up behind it: browsers do not run animation
    /// frames while a document is hidden, and the one asked for here is
    /// the one that runs when the document comes back.
    fn frame(&self) {
        if document().is_some_and(|d| d.hidden()) {
            self.arm();
            return;
        }
        {
            let mut live = self.live.borrow_mut();
            live.pending = false;
            live.raf = None;
        }
        let resize = std::mem::take(&mut self.live.borrow_mut().resize);
        if resize {
            self.relayout();
        }
        self.apply(resize);
    }

    /// Ask the browser for a frame, whatever is already pending, and keep
    /// the handle so a disconnect can take it back.
    fn arm(&self) {
        let held = self.frame.borrow();
        let (Some(window), Some(frame)) = (web_sys::window(), held.as_ref()) else {
            return;
        };
        if let Ok(handle) = window.request_animation_frame(frame.as_ref().unchecked_ref()) {
            let mut live = self.live.borrow_mut();
            live.pending = true;
            live.raf = Some(handle);
        }
    }

    fn request_frame(&self) {
        if self.live.borrow().pending {
            return;
        }
        self.arm();
    }

    /// A tick from the film: the clock, and the flags the chart mirrors.
    fn on_time(&self, event: &Event) {
        let Some(custom) = event.dyn_ref::<web_sys::CustomEvent>() else {
            return;
        };
        let detail = custom.detail();
        // Reflect::get throws on a number, a null or an undefined detail,
        // which reads here as no field at all
        let field = |key: &str| js_sys::Reflect::get(&detail, &JsValue::from_str(key)).ok();
        let Some(t) = film_time_of(detail.as_f64(), field("time").and_then(|v| v.as_f64())) else {
            return;
        };
        if !self.following.replace(true) {
            set_state(&self.host, "following", true);
        }
        if let Some(playing) = field("playing").and_then(|v| v.as_bool()) {
            set_state(&self.host, "playing", playing);
        }
        self.live.borrow_mut().time = t;
        self.request_frame();
    }

    /// Follow `element`, taking the listener off whatever was followed
    /// before.
    fn bind(&self, element: Element) {
        let Some(listener) = self.on_tick.borrow().clone() else {
            return;
        };
        let callback = listener.as_ref().as_ref().unchecked_ref();
        let previous = self.live.borrow_mut().bound.replace(element.clone());
        if let Some(previous) = previous {
            let _ = previous.remove_event_listener_with_callback("opt-film-time", callback);
        }
        let _ = element.add_event_listener_with_callback("opt-film-time", callback);
    }
}

/// Everything that must live as long as the element, and the targets its
/// listeners have to come off again.
struct Wiring {
    follower: Rc<Follower>,
    on_tick: TickListener,
    on_document: Closure<dyn FnMut(Event)>,
    observer: Option<web_sys::ResizeObserver>,
    _on_resize: Closure<dyn FnMut(js_sys::Array)>,
}

struct Chart {
    host: HtmlElement,
    /// The data block, whether it came from the light DOM or a property.
    block: String,
    /// The object a `data` assignment carried, handed back as it was
    /// given; absent when the block is the source of truth.
    data_value: Option<JsValue>,
    /// An element assigned to `forElement` before the wiring existed.
    for_element: Option<Element>,
    wiring: Option<Wiring>,
}

impl Chart {
    fn attr(&self, name: &str, default: f64) -> f64 {
        self.host
            .get_attribute(name)
            .and_then(|v| v.trim().parse::<f64>().ok())
            .filter(|v| v.is_finite() && *v > 0.0)
            .unwrap_or(default)
    }

    /// The block a `<script type="application/json">` child carries.
    fn block_in_light_dom(&self) -> String {
        self.host
            .query_selector("script[type=\"application/json\"]")
            .ok()
            .flatten()
            .and_then(|s| s.text_content())
            .unwrap_or_default()
    }

    fn fail(&self, message: &str) {
        shadow_root(&self.host).set_inner_html(&format!(
            "<style>{BASE_CSS}</style><p>opt-chart: {}</p>",
            escape(message)
        ));
    }

    /// Re-read the block and draw it again, always as a fresh render: the
    /// `data-hash` the build wrote no longer describes this data.
    fn replace_data(&mut self) {
        let Some(wiring) = &self.wiring else {
            return;
        };
        let data = match op_chart::data::parse(&self.block) {
            Ok(data) => data,
            Err(e) => {
                // the shadow root says what is wrong, and that is all: the
                // wiring owns the listeners the document and the observer
                // hold, so dropping it here would leave them calling
                // closures that no longer exist. It stays, and a later
                // `data` that reads recovers the chart.
                self.fail(&e.message);
                wiring.follower.hydrated.set(false);
                return;
            }
        };
        let _ = self.host.remove_attribute("data-hash");
        wiring.follower.hydrated.set(false);
        set_state(&self.host, "hydrated", false);
        {
            let mut live = wiring.follower.live.borrow_mut();
            live.data = data;
            live.x = f64::NEG_INFINITY;
            live.label.clear();
        }
        wiring.follower.render();
        wiring.follower.apply(true);
    }
}

impl CustomElement for Chart {
    fn connected(&mut self) {
        if self.wiring.is_some() {
            return;
        }
        // a `data` property set before the upgrade wins over the block in
        // the light DOM, which the page may not have been able to update
        if self.block.is_empty() {
            self.block = self.block_in_light_dom();
        }
        let initial_width = self.attr("initial-width", DEFAULT_WIDTH);
        let ratio = self.attr("ratio", DEFAULT_RATIO);
        let data = match op_chart::data::parse(&self.block) {
            Ok(data) => data,
            Err(e) => {
                self.fail(&e.message);
                return;
            }
        };
        let hash = op_chart::hash_hex(&self.block);
        let action = hydrate_or_render(
            self.host.shadow_root().is_some(),
            self.host.get_attribute("data-hash").as_deref(),
            &hash,
        );
        let end = data.end();
        let live = Live {
            data,
            layout: Layout::sized(initial_width, initial_width / ratio, end),
            ratio,
            fallback_width: initial_width,
            time: 0.0,
            pending: false,
            raf: None,
            // the observer's first notification is a resize like any
            // other, and it is what puts the live chart in place
            resize: false,
            x: f64::NEG_INFINITY,
            label: String::new(),
            bound: None,
        };
        let follower = Rc::new(Follower {
            host: self.host.clone(),
            root: shadow_root(&self.host),
            dom: RefCell::new(Dom::default()),
            live: RefCell::new(live),
            on_tick: RefCell::new(None),
            frame: RefCell::new(None),
            following: Cell::new(false),
            hydrated: Cell::new(false),
        });
        match action {
            Action::Hydrate => {
                // the pre-render ships a wide chart and a narrow one; the
                // one on screen is the one this box's container query
                // chose, and its geometry is what a hit-test must use
                let measured = self.host.get_bounding_client_rect().width();
                let narrow = measured > 0.0 && measured <= NARROW_AT;
                let pick = |selector: &str| follower.root.query_selector(selector).ok().flatten();
                let (svg, width) = if narrow {
                    (pick("svg.chart.narrow"), NARROW_WIDTH)
                } else {
                    (pick("svg.chart.wide"), initial_width)
                };
                let svg = svg.or_else(|| pick("svg.chart"));
                follower.live.borrow_mut().layout = Layout::sized(width, width / ratio, end);
                follower.capture(svg);
                follower.hydrated.set(true);
                set_state(&self.host, "hydrated", true);
            }
            Action::Render => follower.render(),
        }

        {
            let f = follower.clone();
            *follower.frame.borrow_mut() = Some(Closure::new(move || f.frame()));
        }
        let on_tick = {
            let f = follower.clone();
            Rc::new(Closure::<dyn FnMut(Event)>::new(move |e: Event| {
                f.on_time(&e)
            }))
        };
        *follower.on_tick.borrow_mut() = Some(on_tick.clone());

        // `forElement` binds directly; `for` names an id, resolved first
        // in this element's own tree scope and then in the document
        let for_id = self.host.get_attribute("for").unwrap_or_default();
        if let Some(element) = self.for_element.clone() {
            follower.bind(element);
        } else if let Some(element) = resolve(&self.host, &for_id) {
            follower.bind(element);
        }
        // one listener on the document catches the film that upgrades
        // after us: it binds on the first tick it sees from that id, and
        // every later tick then arrives through the element itself
        let on_document = {
            let f = follower.clone();
            let want = for_id.clone();
            Closure::<dyn FnMut(Event)>::new(move |e: Event| {
                if want.is_empty() || f.live.borrow().bound.is_some() {
                    return;
                }
                let Some(element) = e.target().and_then(|t| t.dyn_into::<Element>().ok()) else {
                    return;
                };
                if element.id() != want {
                    return;
                }
                f.bind(element);
                f.on_time(&e);
            })
        };
        if let Some(document) = document() {
            let _ = document.add_event_listener_with_callback(
                "opt-film-time",
                on_document.as_ref().unchecked_ref(),
            );
        }

        let on_resize = {
            let f = follower.clone();
            Closure::<dyn FnMut(js_sys::Array)>::new(move |_: js_sys::Array| {
                f.live.borrow_mut().resize = true;
                f.request_frame();
            })
        };
        let observer = web_sys::ResizeObserver::new(on_resize.as_ref().unchecked_ref()).ok();
        if let Some(observer) = &observer {
            observer.observe(&self.host);
        }

        self.wiring = Some(Wiring {
            follower,
            on_tick,
            on_document,
            observer,
            _on_resize: on_resize,
        });
    }

    fn disconnected(&mut self) {
        let Some(wiring) = self.wiring.take() else {
            return;
        };
        // an animation frame still owed to us would run against a shadow
        // root nothing is watching, and would call a closure this method is
        // about to drop; it goes back first
        let raf = {
            let mut live = wiring.follower.live.borrow_mut();
            live.pending = false;
            live.raf.take()
        };
        if let (Some(window), Some(handle)) = (web_sys::window(), raf) {
            let _ = window.cancel_animation_frame(handle);
        }
        // both of these closures hold the Follower they are stored on, so
        // until the slots are emptied its strong count never reaches zero
        // and every handle, the data and the layout are held with it
        *wiring.follower.frame.borrow_mut() = None;
        *wiring.follower.on_tick.borrow_mut() = None;
        let bound = wiring.follower.live.borrow_mut().bound.take();
        if let Some(bound) = bound {
            let _ = bound.remove_event_listener_with_callback(
                "opt-film-time",
                wiring.on_tick.as_ref().as_ref().unchecked_ref(),
            );
        }
        if let Some(document) = document() {
            let _ = document.remove_event_listener_with_callback(
                "opt-film-time",
                wiring.on_document.as_ref().unchecked_ref(),
            );
        }
        if let Some(observer) = &wiring.observer {
            observer.disconnect();
        }
    }

    fn property(&self, name: &str) -> JsValue {
        match name {
            "data" => match &self.data_value {
                Some(value) => value.clone(),
                None if self.block.is_empty() => JsValue::UNDEFINED,
                None => js_sys::JSON::parse(&self.block).unwrap_or(JsValue::UNDEFINED),
            },
            "forElement" => self
                .wiring
                .as_ref()
                .and_then(|w| w.follower.live.borrow().bound.clone())
                .or_else(|| self.for_element.clone())
                .map_or(JsValue::NULL, JsValue::from),
            _ => JsValue::UNDEFINED,
        }
    }

    fn set_property(&mut self, name: &str, value: JsValue) {
        match name {
            "data" => {
                match value.as_string() {
                    // a string is the block itself, and the getter parses
                    // it back; anything else is handed back as it came
                    Some(text) => {
                        self.block = text;
                        self.data_value = None;
                    }
                    None => {
                        self.block = js_sys::JSON::stringify(&value)
                            .ok()
                            .and_then(|s| s.as_string())
                            .unwrap_or_default();
                        self.data_value = Some(value);
                    }
                }
                self.replace_data();
            }
            "forElement" => {
                let element = value.dyn_ref::<Element>().cloned();
                self.for_element = element.clone();
                if let (Some(wiring), Some(element)) = (&self.wiring, element) {
                    wiring.follower.bind(element);
                }
            }
            _ => {}
        }
    }
}

/// The element `id` names: this element's own tree scope first (a shadow
/// root it sits in), then the document.
fn resolve(host: &HtmlElement, id: &str) -> Option<Element> {
    if id.is_empty() {
        return None;
    }
    let root = host.get_root_node();
    root.dyn_ref::<web_sys::DocumentFragment>()
        .and_then(|fragment| fragment.get_element_by_id(id))
        .or_else(|| document()?.get_element_by_id(id))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The block on /component/chart/, cut down: two series with units, a
    /// chapter, a mark, a band and a null.
    const BLOCK: &str = r#"{
        "duration": 3.3,
        "series": [
            {"id": "palette", "label": "palette", "unit": "%"},
            {"id": "thumb", "label": "solid thumb", "unit": "%"}
        ],
        "rows": [[0, 0, 0], [1.65, 43.5, null], [3.3, 100, 100]],
        "marks": [{"t": 1.65, "label": "half"}],
        "band": {"t0": 2.36, "t1": 3.3, "label": "settle"},
        "chapters": [{"t": 0, "title": "flight"}, {"t": 1.65, "title": "settle"}]
    }"#;

    fn data() -> Data {
        op_chart::data::parse(BLOCK).expect("the block is valid")
    }

    fn built() -> Prerender {
        prerender(BLOCK, DEFAULT_WIDTH, DEFAULT_RATIO).expect("the block is valid")
    }

    #[test]
    fn a_matching_hash_over_a_real_root_is_the_only_way_to_keep_the_markup() {
        assert_eq!(hydrate_or_render(true, Some("abc"), "abc"), Action::Hydrate);
        assert_eq!(hydrate_or_render(true, Some("abc"), "def"), Action::Render);
        assert_eq!(hydrate_or_render(false, Some("abc"), "abc"), Action::Render);
        assert_eq!(hydrate_or_render(false, None, "abc"), Action::Render);
        // and a root with no hash at all is a render, not a guess
        assert_eq!(hydrate_or_render(true, None, "abc"), Action::Render);
    }

    #[test]
    fn a_tick_moves_the_playhead_only_past_half_a_pixel_or_a_changed_readout() {
        assert!(!tick_change(100.0, 100.49, "1.00s", "1.00s"));
        assert!(!tick_change(100.0, 99.51, "1.00s", "1.00s"));
        assert!(tick_change(100.0, 100.5, "1.00s", "1.00s"));
        assert!(tick_change(100.0, 99.5, "1.00s", "1.00s"));
        // a changed readout alone is enough, however still the playhead
        assert!(tick_change(100.0, 100.1, "1.00s", "1.01s"));
        // and the readout is the film's own format
        assert_eq!(readout(0.0), "0.00s");
        assert_eq!(readout(3.297), "3.30s");
    }

    /// The text between the emitted script's tags, as a browser hands it to
    /// the element.
    fn block_text(markup: &str) -> &str {
        const OPEN: &str = "<script type=\"application/json\">";
        let start = markup.find(OPEN).expect("an emitted block") + OPEN.len();
        let end = markup[start..].find("</script>").expect("its end") + start;
        &markup[start..end]
    }

    #[test]
    fn a_prerender_carries_the_hash_the_chart_and_the_block_it_was_drawn_from() {
        let built = built();
        assert_eq!(built.hash, op_chart::hash_hex(BLOCK));
        assert_eq!(built.hash.len(), 16);
        // the figure, the caption and the disclosure
        assert!(built.shadow.contains("<figure class=\"chart\">"));
        assert!(built.shadow.contains("data-rendered-by=\"op-pages\""));
        assert!(built.shadow.contains("part=\"chart\""));
        assert!(built.shadow.contains("<figcaption>"));
        assert!(built.shadow.contains("class=\"summary\""));
        assert!(built.shadow.contains("<details class=\"data\">"));
        assert!(built.shadow.contains("<summary>Data table</summary>"));
        // the block goes back in a script element that it cannot end
        assert!(
            built
                .block
                .starts_with("<script type=\"application/json\">")
        );
        assert!(built.block.ends_with("</script>"));
        let closer = prerender(
            &BLOCK.replace("\"half\"", "\"</script> half\""),
            DEFAULT_WIDTH,
            DEFAULT_RATIO,
        )
        .expect("valid");
        assert!(closer.block.contains("<\\/script>"));
        assert_eq!(closer.block.matches("</script").count(), 1);
        // the hash is over the text the browser reads back, which is the
        // escaped block: hashing what the author wrote would leave a page
        // whose block can close a script element unable to ever hydrate
        assert_eq!(closer.hash, op_chart::hash_hex(block_text(&closer.block)));
        assert_eq!(built.hash, op_chart::hash_hex(block_text(&built.block)));
        // a block the reader rejects comes back as its own message
        let error = prerender("{\"series\": []}", DEFAULT_WIDTH, DEFAULT_RATIO)
            .expect_err("no rows, no duration");
        assert!(error.contains("the data has no rows"), "{error}");
    }

    #[test]
    fn the_prerender_ships_a_wide_chart_and_a_narrow_one_and_the_query_that_chooses() {
        let shadow = built().shadow;
        assert!(shadow.contains("<svg class=\"chart wide\""));
        assert!(shadow.contains("<svg class=\"chart narrow\""));
        assert_eq!(shadow.matches("<svg ").count(), 2);
        // both are marked, and both keep the part a page styles through
        assert_eq!(shadow.matches("data-rendered-by=\"op-pages\"").count(), 2);
        assert_eq!(shadow.matches("part=\"chart\"").count(), 2);
        // the wide one is the build's width and the narrow one is 360
        assert!(shadow.contains(&format!("viewBox=\"0 0 {DEFAULT_WIDTH} ")));
        assert!(shadow.contains(&format!("viewBox=\"0 0 {NARROW_WIDTH} ")));
        // and the stylesheet switches between them without a script
        assert!(shadow.contains("svg.chart.narrow { display: none; }"));
        assert!(shadow.contains(&format!(
            "@container (max-width: {NARROW_AT}px) {{ svg.chart.wide {{ display: none; }} svg.chart.narrow {{ display: block; }} }}"
        )));
        // the element's own render draws one chart at the measured width
        let d = data();
        let one = shadow_markup(
            &d,
            Layout::sized(800.0, 300.0, d.end()),
            None,
            DEFAULT_RATIO,
            BY_SITE,
        );
        assert_eq!(one.matches("<svg ").count(), 1);
        assert!(one.contains("<svg class=\"chart\" part=\"chart\" data-rendered-by=\"op-site\""));
        assert!(!one.contains("chart wide") && !one.contains("chart narrow"));
    }

    /// Decision 15: the element's chart is not a control. The renderer opens
    /// every chart as the film's slider, because the film drives one; this
    /// chart has no thumb and answers no key, so a focusable slider frozen
    /// at zero would announce a value that is never the chart's own.
    #[test]
    fn an_element_drawn_chart_is_a_labelled_graphics_document_and_not_a_slider() {
        let d = data();
        let one = shadow_markup(
            &d,
            Layout::sized(800.0, 300.0, d.end()),
            None,
            DEFAULT_RATIO,
            BY_SITE,
        );
        let pair = built().shadow;
        for markup in [&one, &pair] {
            assert!(markup.contains("role=\"graphics-document\""), "{markup}");
            assert!(
                markup.contains("aria-labelledby=\"chart-title\""),
                "{markup}"
            );
            // the caption's heading is what names it, by an id scoped to
            // this shadow root
            assert!(
                markup.contains("<strong class=\"title\" id=\"chart-title\">"),
                "{markup}"
            );
            for frozen in [
                "role=\"slider\"",
                "tabindex",
                "aria-valuenow",
                "aria-valuemin",
                "aria-valuemax",
                "aria-valuetext",
                "aria-label=\"playhead\"",
            ] {
                assert!(!markup.contains(frozen), "{frozen} survived: {markup}");
            }
            // and the box the emitter drew is untouched
            assert!(markup.contains("part=\"chart\""));
            assert!(markup.contains("viewBox=\"0 0 "));
        }
        // one role per chart, wide and narrow alike
        assert_eq!(pair.matches("role=\"graphics-document\"").count(), 2);
        // the film's own chart keeps the renderer's slider: it has a thumb,
        // and it writes aria-valuenow on every tick
        let film = op_chart::render(&d.to_spec(), Layout::film(d.end())).svg;
        assert!(film.contains("role=\"slider\"") && film.contains("aria-valuenow=\"0\""));
    }

    #[test]
    fn a_pre_render_is_kept_only_at_the_width_it_was_drawn_for() {
        let (wide, narrow) = (DEFAULT_WIDTH, NARROW_WIDTH);
        // the box it was drawn for, to within the pixel a measurement can
        // land either side of
        assert_eq!(
            keep_prerender(true, wide, wide, narrow),
            Some(Variant::Wide)
        );
        assert_eq!(
            keep_prerender(true, wide - 0.8, wide, narrow),
            Some(Variant::Wide)
        );
        assert_eq!(
            keep_prerender(true, narrow, wide, narrow),
            Some(Variant::Narrow)
        );
        assert_eq!(
            keep_prerender(true, narrow + 1.0, wide, narrow),
            Some(Variant::Narrow)
        );
        // any other width is a chart drawn again at that width
        assert_eq!(keep_prerender(true, 500.0, wide, narrow), None);
        assert_eq!(keep_prerender(true, wide - 1.1, wide, narrow), None);
        assert_eq!(keep_prerender(true, narrow + 1.1, wide, narrow), None);
        // an element that did not hydrate has nothing of the build's to keep
        assert_eq!(keep_prerender(false, wide, wide, narrow), None);
        // and the chart the container query hides is never the one kept: a
        // build whose wide chart is narrower than the switch is drawn again
        // rather than kept invisible
        assert_eq!(keep_prerender(true, 400.0, 400.0, narrow), None);
        const {
            assert!(NARROW_WIDTH < NARROW_AT && NARROW_AT < DEFAULT_WIDTH);
        }
    }

    #[test]
    fn the_box_the_stylesheet_asks_for_is_the_box_the_geometry_was_drawn_at() {
        // a chart at 4:1 draws a 4:1 viewBox, and its CSS box is 4:1 too,
        // so the art fills it rather than letterboxing inside 16 / 6
        let squat = prerender(BLOCK, 320.0, 4.0).expect("valid").shadow;
        assert!(
            squat.contains("aspect-ratio: var(--op-chart-ratio, 4)"),
            "{squat}"
        );
        assert!(squat.contains("viewBox=\"0 0 320 80\""), "{squat}");
        // the default box keeps the fraction it was designed as
        assert!(
            built()
                .shadow
                .contains("aspect-ratio: var(--op-chart-ratio, 16 / 6)")
        );
        // and a ratio that is not a whole number is still a CSS number
        let thin = prerender(BLOCK, 640.0, 2.5).expect("valid").shadow;
        assert!(
            thin.contains("aspect-ratio: var(--op-chart-ratio, 2.5)"),
            "{thin}"
        );
        assert!(stylesheet(16.0 / 6.0).contains("var(--op-chart-ratio, 16 / 6)"));
    }

    #[test]
    fn the_caption_names_every_series_the_units_the_samples_and_the_annotations() {
        let d = data();
        assert_eq!(title_of(&d), "palette and solid thumb");
        let summary = summary_of(&d);
        for label in ["palette", "solid thumb"] {
            assert!(summary.contains(label), "{summary}");
        }
        assert!(summary.contains("2 series"), "{summary}");
        assert!(summary.contains("measured in %"), "{summary}");
        assert!(
            summary.contains("3 samples from 0.00 s to 3.30 s"),
            "{summary}"
        );
        assert!(
            summary.contains("2 chapters (flight and settle)"),
            "{summary}"
        );
        assert!(summary.contains("1 mark (half at 1.65 s)"), "{summary}");
        assert!(
            summary.contains("a band (settle from 2.36 s to 3.30 s)"),
            "{summary}"
        );
        assert!(summary.contains("values from 0 to 100"), "{summary}");
        assert!(summary.ends_with('.'), "{summary}");
        // three or more labels read as a sentence, and one is just itself
        assert_eq!(
            title_of(&op_chart::data::parse(
                r#"{"series": [{"id": "a", "label": "palette"}, {"id": "b", "label": "solid thumb"}, {"id": "c", "label": "progress ghost"}], "rows": [], "duration": 1}"#
            ).expect("valid")),
            "palette, solid thumb and progress ghost"
        );
        // an empty block still says what it has, in the singular
        let bare = op_chart::data::parse(
            r#"{"series": [{"id": "a", "label": "A"}], "rows": [], "duration": 2}"#,
        )
        .expect("valid");
        let summary = summary_of(&bare);
        assert!(summary.contains("1 series (A)"), "{summary}");
        assert!(summary.contains("no samples"), "{summary}");
        assert!(summary.contains("no chapters"), "{summary}");
        assert!(!summary.contains("values from"), "{summary}");
    }

    /// The value of the first attribute `attr` after `head` in the markup.
    fn cells(row: &str) -> Vec<String> {
        row.split("<td>")
            .skip(1)
            .map(|cell| cell.split("</td>").next().unwrap_or_default().to_owned())
            .collect()
    }

    fn body_rows(markup: &str) -> Vec<String> {
        let body = markup
            .split("<tbody>")
            .nth(1)
            .and_then(|rest| rest.split("</tbody>").next())
            .expect("a table body");
        body.split("<tr>")
            .skip(1)
            .map(|row| row.split("</tr>").next().unwrap_or_default().to_owned())
            .collect()
    }

    #[test]
    fn the_table_carries_every_sample_the_marks_and_the_band_down_the_timeline() {
        let d = data();
        let markup = shadow_markup(
            &d,
            Layout::sized(640.0, 240.0, d.end()),
            None,
            DEFAULT_RATIO,
            BY_SITE,
        );
        let head = markup
            .split("<thead>")
            .nth(1)
            .and_then(|rest| rest.split("</thead>").next())
            .expect("a table head");
        assert!(head.contains("<th scope=\"col\">time (s)</th>"));
        assert!(head.contains("<th scope=\"col\">chapter</th>"));
        for name in ["palette (%)", "solid thumb (%)"] {
            assert!(
                head.contains(&format!("<th scope=\"col\">{name}</th>")),
                "{head}"
            );
        }
        let rows = body_rows(&markup);
        // three samples, one mark, two band edges, and a row for the one
        // chapter the chart draws a cue for
        let chapters = d.chapters.len() - 1;
        assert_eq!(
            rows.len(),
            d.rows.len() + d.marks.len() + 2 + chapters,
            "{rows:#?}"
        );
        assert_eq!(cells(&rows[0]), ["0.00", "flight", "0", "0"]);
        // the sample comes first where an annotation shares its instant, the
        // chapter column names the latest chapter at or before the row, and
        // a null is an empty cell rather than a zero
        assert_eq!(cells(&rows[1]), ["1.65", "settle", "43.5", ""]);
        assert_eq!(cells(&rows[2]), ["1.65", "mark: half", "", ""]);
        assert_eq!(cells(&rows[3]), ["1.65", "chapter: settle", "", ""]);
        assert_eq!(cells(&rows[4]), ["2.36", "band start: settle", "", ""]);
        assert_eq!(cells(&rows[5]), ["3.30", "settle", "100", "100"]);
        assert_eq!(cells(&rows[6]), ["3.30", "band end: settle", "", ""]);
        // the first chapter starts the axis and draws no cue, so it has no
        // row of its own either
        assert_eq!(markup.matches("chapter: ").count(), chapters);
        assert!(!markup.contains("chapter: flight"));
    }

    #[test]
    fn a_label_can_never_break_out_of_the_markup() {
        let block = r#"{"series": [{"id": "a", "label": "<img src=x> & \"quoted\"", "unit": "<b>"}],
            "rows": [[0, 1]], "marks": [{"t": 0, "label": "</span>"}],
            "chapters": [{"t": 0, "title": "<script>"}], "duration": 1}"#;
        let d = op_chart::data::parse(block).expect("valid");
        let markup = shadow_markup(
            &d,
            Layout::sized(640.0, 240.0, d.end()),
            None,
            DEFAULT_RATIO,
            BY_SITE,
        );
        for raw in ["<img", "</span>", "<b>"] {
            assert!(!markup.contains(raw), "{raw} survived into the markup");
        }
        assert!(markup.contains("&lt;img src=x&gt; &amp; &quot;quoted&quot;"));
        assert!(markup.contains("mark: &lt;/span&gt;"));
        // the chapter title reaches the caption and the table alike
        assert!(markup.contains("&lt;script&gt;"));
        // and the only script-looking thing left is nothing at all
        assert!(!markup.to_ascii_lowercase().contains("<script"));
    }

    #[test]
    fn the_stylesheet_owns_the_box_the_tokens_and_the_shared_chart_rules() {
        let css = stylesheet(DEFAULT_RATIO);
        assert!(css.contains(":host { display: block; container-type: inline-size; }"));
        assert!(css.contains(":host([hidden]) { display: none; }"));
        assert!(css.contains("figure.chart { margin: 0; }"));
        assert!(css.contains(
            "svg.chart { display: block; width: 100%; height: auto; aspect-ratio: var(--op-chart-ratio, 16 / 6);"
        ));
        // no text in the chart is smaller than 12 px, and the width the
        // narrow pre-render takes over at is where 12 px in a wide viewBox
        // would fall under the 10 px floor
        for size in css.split("font-size: ").skip(1) {
            let size = size.split("px").next().unwrap_or_default();
            let Ok(px) = size.parse::<f64>() else {
                continue;
            };
            assert!(px >= 12.0, "{px} px is under the chart's floor");
        }
        // the container width at which the wide pre-render's 12 px text
        // reaches the 10 px floor; below it the narrow chart must already
        // have taken over, and the narrow chart is drawn narrower still
        let floor = DEFAULT_WIDTH * 10.0 / 12.0;
        let widths = [NARROW_WIDTH, NARROW_AT, floor, DEFAULT_WIDTH];
        assert!(
            widths.windows(2).all(|pair| pair[0] < pair[1]),
            "the narrow chart, the width it takes over at, the 10 px floor \
             and the wide chart must be in that order: {widths:?}"
        );
        assert!(css.contains(&format!(
            "@container (max-width: {DROP_AT}px) {{ .tick-label.alt {{ display: none; }} .chapters {{ display: none; }} }}"
        )));
        // the tokens the brief names, each on the part it paints
        assert!(css.contains(".chart .head { stroke: var(--op-playhead); stroke-width: 1.5; }"));
        assert!(css.contains(".chart .head-dot { fill: var(--op-playhead); }"));
        assert!(css.contains(".chart .bar-played { fill: var(--op-band); }"));
        assert!(css.contains(".chart .axis { fill: var(--op-muted); }"));
        assert!(
            css.contains(".chart .grid { stroke: var(--op-border); shape-rendering: crispEdges; }")
        );
        // and the film's shared blocks arrive whole, not copied
        let rules = chart_rules();
        assert!(
            css.contains(&rules),
            "the shared rules are not included whole"
        );
        for block in [
            ".chart .series-6",
            "@media (forced-colors: active)",
            "@media print {",
        ] {
            assert_eq!(css.matches(block).count(), 1, "{block}");
        }
    }
}
