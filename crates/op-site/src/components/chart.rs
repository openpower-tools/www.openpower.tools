//! `<opt-chart for="film">`: a time-series chart that follows a film's
//! clock. It never advances time itself; it is a projection of the film's
//! timeline, as the machine diagram is, so the only thing that moves per
//! tick is the playhead group, its readout and the played bar.
//!
//! The markup is built by pure functions (`shadow_markup`,
//! [`prerender`]) that the page build calls natively, so a page ships the
//! chart finished: the SVG, a caption with a one-paragraph summary, and
//! the data as a real table behind a disclosure. Nothing of that needs a
//! script, which is what keeps the chart complete where WebAssembly is
//! unavailable (Firefox on ppc64le).
//!
//! A pre-render scales with its viewBox, so a 640 unit box at a 360 px
//! viewport would shrink its 12 px labels to about 6 px. The build
//! therefore emits two SVGs, `chart wide` and `chart narrow`, and a
//! container query at `NARROW_AT` chooses between them with no script at
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
//!
//! Pointers and keys never move the playhead either. Every gesture and
//! every key emits an intent in seconds - `opt-chart-seek`,
//! `opt-chart-peek` or `opt-chart-toggle`, composed and bubbling - and the
//! film applies it, so each projection of the timeline still moves only on
//! `opt-film-time`. Hovering a pointer that can hover peeks at the nearest
//! sample, or at the position's own time where no sample is near enough; a
//! press that does not move seeks on release; a press that moves, or one
//! held on a coarse pointer, aims a seek with the peek as its preview,
//! committed on release and cancelled by Escape or by coming back to where
//! it started and letting go there. The SVG is the one tab stop that
//! reaches the chart itself - the data table's disclosure is a tab stop of
//! its own - and it answers decision 17's key table while it has focus.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use op_chart::{Data, Layout};
use op_webc::{CustomElement, ElementDefinition, set_state};
use wasm_bindgen::prelude::*;
use web_sys::{Element, Event, HtmlElement, ShadowRoot};

use super::chart_style::{CHART_CUE_CSS, CHART_SHAPE_CSS, chart_rules};
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
pub(crate) const NARROW_WIDTH: f64 = 360.0;
/// Where the wide pre-render gives way to the narrow one: below this
/// container width its 12 px labels, scaled by the viewBox, fall under the
/// 10 px floor.
pub(crate) const NARROW_AT: f64 = 532.0;
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
pub(crate) enum Action {
    Hydrate,
    Render,
}

/// See [`Action`].
pub(crate) fn hydrate_or_render(
    has_root: bool,
    hash_attr: Option<&str>,
    block_hash: &str,
) -> Action {
    if has_root && hash_attr == Some(block_hash) {
        Action::Hydrate
    } else {
        Action::Render
    }
}

/// Which of the pre-render's two charts is meant: the one drawn at the
/// element's `initial-width`, or the one drawn at [`NARROW_WIDTH`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Variant {
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
pub(crate) fn keep_prerender(
    hydrated: bool,
    measured: f64,
    wide: f64,
    narrow: f64,
) -> Option<Variant> {
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
pub(crate) fn tick_change(prev_x: f64, x: f64, prev_label: &str, label: &str) -> bool {
    !((x - prev_x).abs() < 0.5 && prev_label == label)
}

/// The playhead's readout, in the format the film's own chart uses.
pub(crate) fn readout(t: f64) -> String {
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
    // "series" is its own plural, so the count needs no help from `count`
    let series = format!("{} series", d.series.len());
    parts.push(if d.series.is_empty() {
        series
    } else {
        // the same list the caption's heading carries
        format!("{series} ({})", title_of(d))
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
/// playhead follows the film's clock and this chart only asks it to move -
/// so a focusable slider frozen at zero would be a control that reports
/// the wrong value. The tag is rewritten as a graphics document labelled
/// by the caption instead, carrying the one `tabindex` that puts a tab
/// stop on the chart itself and, since it answers decision 17's keys, an
/// operable one. That is not the shadow root's only tab stop: the data
/// table's disclosure takes the focus too, and the key handler relies on
/// it. `part="chart"` and the viewBox are the emitter's own.
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
        "<svg class=\"{}\" part=\"chart\" data-rendered-by=\"{}\" viewBox=\"{view}\" tabindex=\"0\" role=\"graphics-document\" aria-labelledby=\"{TITLE_ID}\">{body}",
        escape(class),
        escape(rendered_by),
    )
}

/// The whole inner HTML of a chart's shadow root. With a `narrow` layout
/// the figure carries both pre-renders and the stylesheet chooses; with
/// none it carries the single chart the element just drew.
pub(crate) fn shadow_markup(
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
/// inherit across the shadow boundary; the blocks it shares with the film
/// are included whole.
///
/// `ratio` is the element's own, so the CSS box and the viewBox the
/// geometry was drawn at are one thing: `--op-chart-ratio` overrides the
/// box, and with nothing set the box follows the attribute.
///
/// The gridlines and the swatch take `crispEdges` for the reasons the
/// film's own stylesheet gives: a hairline on a device pixel, and a swatch
/// the interaction report can sample a pixel at a time.
///
/// The peek band takes its wash here, beside the element's own band; the
/// edge that belongs to the annotation band alone is in
/// [`CHART_SHAPE_CSS`]. [`chart_rules`] comes last, for the reason given
/// there. Nothing here paints a pointer target: the hit rects carry the
/// class `target` alone, and no rule names it.
///
/// The chart is a tab stop, so it carries the site's focus ring, on
/// `:focus-visible` alone: a press must not ring the chart it is scrubbing.
/// `touch-action: pan-y` leaves the page its vertical scroll and claims
/// the horizontal drag, which is the one this element reads.
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
svg.chart {{ display: block; width: 100%; height: auto; aspect-ratio: var(--op-chart-ratio, {ratio}); font-family: var(--op-font-sans); font-size: 12px; touch-action: pan-y; }}
svg.chart:focus {{ outline: none; }}
svg.chart:focus-visible {{ outline: 2px solid var(--op-focus); outline-offset: 2px; }}
svg.chart.narrow {{ display: none; }}
{CHART_SHAPE_CSS}
.chart .band, .chart .peek-band {{ fill: var(--op-band); fill-opacity: 0.5; }}
.chart .bar-bg {{ fill: var(--op-border); }} .chart .bar-played {{ fill: var(--op-band); }}
{CHART_CUE_CSS}
.chart .head {{ stroke: var(--op-playhead); stroke-width: 1.5; }} .chart .head-dot {{ fill: var(--op-playhead); }}
.chart .head-t {{ fill: var(--op-playhead); font-weight: 700; paint-order: stroke; stroke: var(--op-surface); stroke-width: 4; }}
{rules}
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

// ---- pointer and key intents ------------------------------------------

/// How far along x a pointer may be from a sample and still mean it, in
/// CSS px. y is ignored: a time series is read along its time axis
/// (decision 19).
pub(crate) const SNAP_RADIUS: f64 = 40.0;
/// How far a press may travel and still be a tap, in CSS px.
pub(crate) const DRAG_PX: f64 = 3.0;
/// How long a press on a coarse pointer is held before it becomes a
/// pending seek, in seconds.
pub(crate) const LONG_PRESS: f64 = 0.5;
/// How near its origin a pending drag has to come back to be cancelled, in
/// CSS px. Wider than [`DRAG_PX`], so the band that cancels a drag can
/// never be the same edge as the one that started it.
pub(crate) const SNAPBACK_PX: f64 = 4.0;

/// The sample nearest `x` and its time, when one is within
/// [`SNAP_RADIUS`] of it.
pub(crate) fn snap(layout: &Layout, x: f64, times: &[f64]) -> Option<(usize, f64)> {
    layout.nearest(x, times, SNAP_RADIUS)
}

/// The sample times the chart drew: what a pointer snaps to and what a
/// key steps between.
fn times_of(data: &Data) -> Vec<f64> {
    data.rows.iter().map(|r| r.t).collect()
}

/// Where each chapter starts, in the order they are drawn.
fn chapter_starts(data: &Data) -> Vec<f64> {
    data.chapters.iter().map(|c| c.t).collect()
}

/// What a gesture or a key asks for, in seconds. The chart never moves its
/// own playhead: it says what it wants, the film's clock answers, and every
/// projection of that clock moves together.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Intent {
    /// Put the clock here.
    Seek(f64),
    /// Preview this time without moving the clock; [`None`] clears the
    /// preview.
    Peek(Option<f64>),
    /// Play or pause, whichever the film is not doing.
    Toggle,
    /// Drop a pending seek without committing it.
    Cancel,
}

/// What a key press is read against: the sample times the chart drew, the
/// duration it announces, and the chapter starts a page key steps between.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Keys<'a> {
    pub times: &'a [f64],
    /// The whole timeline, which [`key_duration`] takes from the block and
    /// not from the axis. Stepping between samples is bounded by `times`.
    pub duration: f64,
    pub chapters: &'a [f64],
}

/// The duration End, a digit and a one-second jump are read against: the
/// stated one where it runs past the last row, which is exactly what the
/// renderer writes into `aria-valuemax` (`Data::to_spec`). Reading the axis
/// end instead would leave End short of the announced maximum and make a
/// digit a tenth of the wrong span, on any block whose `duration` outlives
/// its rows.
pub(crate) fn key_duration(data: &Data, end: f64) -> f64 {
    data.duration.max(end)
}

/// Times this close are the same instant: a chapter may name a sample
/// exactly, and both come from the same decimal block.
const T_EPS: f64 = 1e-9;

impl Keys<'_> {
    /// The sample at or before `t`, and the first sample when `t` is
    /// before all of them. A linear scan: the reader does not promise the
    /// rows arrive in time order, and there are tens to hundreds of them.
    fn index_at(&self, t: f64) -> usize {
        self.times
            .iter()
            .rposition(|s| *s <= t + T_EPS)
            .unwrap_or(0)
    }

    /// `n` samples along from `t`, held at either end. Nothing at all when
    /// nothing was sampled: there is no sample to step onto.
    fn step(&self, t: f64, n: i64) -> Option<f64> {
        if self.times.is_empty() {
            return None;
        }
        let last = self.times.len() as i64 - 1;
        let j = (self.index_at(t) as i64 + n).clamp(0, last);
        self.times.get(j as usize).copied()
    }

    /// The next chapter start after `t`, or the end of the announced
    /// timeline when there is none: Page Down never stops short of the
    /// announced maximum, which is [`key_duration`] and so `aria-valuemax`,
    /// and not the last row's time.
    fn chapter_next(&self, t: f64) -> f64 {
        self.chapters
            .iter()
            .copied()
            .find(|c| *c > t + T_EPS)
            .unwrap_or(self.duration)
    }

    /// The latest chapter start before `t`, or the start of the axis. Page
    /// Up goes back to the chapter's own start first and from there to the
    /// chapter before it, as the film's does.
    fn chapter_prev(&self, t: f64) -> f64 {
        self.chapters
            .iter()
            .copied()
            .rfind(|c| *c < t - T_EPS)
            .unwrap_or(0.0)
    }
}

/// The intent a key press carries, [`None`] for a key the chart does not
/// answer. Decision 17's whole set: the film's own table, less the speed
/// keys the film owns, plus the digits.
///
/// `at` is the clock the chart last saw and `chapter_mod` is Ctrl or Alt,
/// which alias the page keys onto the arrows. Every time returned is
/// inside the axis, so no key walks past 0 or the duration.
pub(crate) fn key_intent(
    key: &str,
    shift: bool,
    chapter_mod: bool,
    at: f64,
    ctx: &Keys,
) -> Option<Intent> {
    let seek = |t: f64| Some(Intent::Seek(t.clamp(0.0, ctx.duration)));
    let step = |n: i64| {
        ctx.step(at, n)
            .map(|t| Intent::Seek(t.clamp(0.0, ctx.duration)))
    };
    match key {
        "Escape" => Some(Intent::Cancel),
        " " | "k" | "K" => Some(Intent::Toggle),
        "," => step(-1),
        "." => step(1),
        // the chapter alias is read before the one-second jump, so Ctrl
        // and Shift together still step by chapter
        "ArrowLeft" if chapter_mod => seek(ctx.chapter_prev(at)),
        "ArrowRight" if chapter_mod => seek(ctx.chapter_next(at)),
        "ArrowLeft" if shift => seek(at - 1.0),
        "ArrowRight" if shift => seek(at + 1.0),
        "ArrowLeft" => step(-5),
        "ArrowRight" => step(5),
        "j" | "J" => step(-10),
        "l" | "L" => step(10),
        "PageUp" => seek(ctx.chapter_prev(at)),
        "PageDown" => seek(ctx.chapter_next(at)),
        "Home" => seek(0.0),
        "End" => seek(ctx.duration),
        // 0 to 9: tenths of the timeline, as every player's digits are
        d if d.len() == 1 && d.as_bytes()[0].is_ascii_digit() => {
            seek(ctx.duration * f64::from(d.as_bytes()[0] - b'0') / 10.0)
        }
        _ => None,
    }
}

/// A press in progress: where it went down, whether it has already become
/// a pending seek, and whether the pointer has come up. The element keeps
/// one of these between events and [`pointer_phase`] reads it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Down {
    /// Where the press went down, in the chart's own units.
    pub x0: f64,
    /// It has already passed a threshold, so a return to the origin is the
    /// snap-back and not a tap that has yet to move.
    pub pending: bool,
    /// The pointer has come up.
    pub released: bool,
    /// The pointer has been further from the origin than the snap-back band
    /// at some point, so a return to it is a change of mind rather than a
    /// press that simply never moved.
    pub travelled: bool,
}

/// What a press has become.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Phase {
    /// Nothing yet: no press at all, or one still inside the slop a tap is
    /// allowed. The caller shows nothing and waits for the release.
    Idle,
    /// A seek is being aimed here; the caller previews it.
    Pending(f64),
    /// A seek is being aimed here and this is where it started, so letting
    /// go now cancels. The press is still alive: the caller says so and
    /// keeps previewing, and aiming out again is [`Phase::Pending`] once
    /// more.
    WillCancel(f64),
    /// The press is over and means this position.
    Commit(f64),
    /// The press is over and means nothing.
    Cancel,
}

/// Whether a gesture is a coarse one, asked of the pointer that made it
/// rather than of the device it ran on: `(pointer: fine)` describes the
/// primary pointing device, so it answers for the trackpad on a touchscreen
/// laptop and for the mouse plugged into a tablet, and neither answer is
/// about the finger or the mouse actually pressing. Everything that is not
/// a mouse is coarse here, a pen with the rest: what the coarse path
/// provides is the long press, and only a mouse arrives with a hover to
/// have aimed with already (decision 19).
pub(crate) fn coarse_pointer(pointer_type: &str) -> bool {
    pointer_type != "mouse"
}

/// Which phase a press is in, from where it went down, where the pointer is
/// now (`x`, in the chart's own units), how long it has been held
/// (`elapsed`, in seconds) and whether the pointer that made it is a coarse
/// one.
///
/// The two thresholds are deliberately not the same comparison: the travel
/// has to exceed the slop a tap is allowed, while the hold only has to
/// reach the delay, so a finger that never moves still gets its pending
/// seek at exactly [`LONG_PRESS`]. A mouse has no long press at all; held
/// still, it stays a tap however long it waits.
///
/// The snap-back is a state and not an event: coming back inside
/// [`SNAPBACK_PX`] of the origin says the release would cancel, and the
/// release is what cancels. A scrub that crosses its own start and goes on
/// is one gesture throughout, which is what decision 19 describes. Coming
/// back means having left: a held press that never moves, which is how a
/// long press aims on a coarse pointer, sits at its origin from the start
/// and must not be read as a change of mind.
///
/// The phases carry positions, not times. The caller snaps them, so the
/// preview and the commit land on the same sample.
pub(crate) fn pointer_phase(down: Option<Down>, x: f64, elapsed: f64, coarse: bool) -> Phase {
    let Some(down) = down else {
        return Phase::Idle;
    };
    let travel = (x - down.x0).abs();
    // back where it started, with a seek already being aimed and the pointer
    // having been away: a change of mind, until it either leaves again or
    // comes up
    let aborting = down.pending && down.travelled && travel <= SNAPBACK_PX;
    if down.released {
        // a tap and a drag resolve alike: the press means where the
        // pointer left it, unless it was let go inside the snap-back band
        return if aborting {
            Phase::Cancel
        } else {
            Phase::Commit(x)
        };
    }
    if down.pending {
        if aborting {
            Phase::WillCancel(x)
        } else {
            Phase::Pending(x)
        }
    } else if travel > DRAG_PX || (coarse && elapsed >= LONG_PRESS) {
        Phase::Pending(x)
    } else {
        Phase::Idle
    }
}

/// What a press records about itself around a phase: that it has been
/// outside the snap-back band, which is what makes a later return to the
/// origin a change of mind rather than a press that never moved, and that
/// it has become a seek, which is what makes that return worth reading at
/// all. A press is advanced twice at the same position, once before the
/// phase is asked and once with the answer, because the travel is what
/// the question is asked about while the seek is what the answer says.
/// The element and its tests both record a press through here: a rule
/// kept inside the element would be tested by a copy of itself.
pub(crate) fn advance(down: &mut Down, x: f64, phase: Option<Phase>) {
    if (x - down.x0).abs() > SNAPBACK_PX {
        down.travelled = true;
    }
    if matches!(phase, Some(Phase::Pending(_))) {
        down.pending = true;
    }
}

/// Whether an event was made by some pointer other than the one the live
/// press belongs to. A second pointer going down takes the press over, and
/// the first one's moves and its release must not be read against the
/// second one's origin: they would commit a seek at a position the second
/// pointer never visited. No press stored means nothing to disagree with,
/// so a hover is never ignored.
pub(crate) fn should_ignore(stored: Option<i32>, event: i32) -> bool {
    stored.is_some_and(|id| id != event)
}

/// The intents the chart emits. All three are composed and bubbling, and
/// all three come from a user's own gesture: nothing here is dispatched
/// because a property was set or because the clock ticked (decision 4).
pub(crate) const SEEK_EVENT: &str = "opt-chart-seek";
pub(crate) const PEEK_EVENT: &str = "opt-chart-peek";
pub(crate) const TOGGLE_EVENT: &str = "opt-chart-toggle";
/// The one field a seek or a peek detail carries; the peek that clears
/// carries it as null. The toggle carries no detail at all.
pub(crate) const TIME_FIELD: &str = "time";

/// The custom states this phase sets: a preview is showing, a press is
/// aiming a seek, and that press is back where it started, so letting go
/// now would drop it (decision 5). A page styles them and the interaction
/// report witnesses them, so the names live here and nowhere else.
pub(crate) const PEEKING_STATE: &str = "peeking";
pub(crate) const PENDING_STATE: &str = "pending";
pub(crate) const CANCELLING_STATE: &str = "cancelling";

/// What the hover peek is gated on. The feature is hovering, so `hover`
/// carries the question and `pointer` only says the primary device is
/// precise enough to mean a sample: a pen-driven screen answers
/// `pointer: fine` and cannot hover at all (decision 19, amended). Asked as
/// a live [`web_sys::MediaQueryList`], so a device that docks a mouse is
/// followed rather than answered once at upgrade.
pub(crate) const HOVER_QUERY: &str = "(hover: hover) and (pointer: fine)";

// ---- the element ------------------------------------------------------

/// The handles a tick and a re-layout act on, re-read whenever the SVG is
/// replaced.
#[derive(Default)]
struct Dom {
    svg: Option<Element>,
    playhead: Option<Element>,
    head_t: Option<Element>,
    played: Option<Element>,
    /// The renderer's peek rule, drawn hidden and shown at the peeked time.
    peek_line: Option<Element>,
    figure: Option<Element>,
}

/// What the element keeps between ticks.
struct Live {
    data: Data,
    /// The times in `data`'s rows, read out once so a pointer move does
    /// not walk the rows again for every frame it travels.
    times: Vec<f64>,
    /// Where each chapter in `data` starts, for the chapter keys.
    chapters: Vec<f64>,
    /// The geometry the visible chart was drawn with; the playhead and a
    /// later hit-test both read it.
    layout: Layout,
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

/// One pointer listener with the event it answers, kept as a pair so that
/// a re-layout can put it back on the chart it just drew.
type Gesture = (&'static str, Closure<dyn FnMut(Event)>);

/// The press the element is following: which pointer it captured, what
/// [`pointer_phase`] is told about it, where that pointer is now, when it
/// went down and the long-press wake-up a coarse pointer needs.
struct Press {
    id: i32,
    down: Down,
    x: f64,
    started: f64,
    /// Whether this gesture's own pointer is a coarse one, read off the
    /// press that started it and kept for the moves that follow: the
    /// pointer answers for itself, and the device it ran on does not.
    coarse: bool,
    timer: Option<i32>,
}

struct Follower {
    host: HtmlElement,
    root: ShadowRoot,
    dom: RefCell<Dom>,
    live: RefCell<Live>,
    /// The listener added to the bound element, kept so binding a second
    /// element can take the first one off.
    on_tick: RefCell<Option<TickListener>>,
    frame: RefCell<Option<Closure<dyn FnMut()>>>,
    /// The pointer listeners. They are added to each SVG as it is
    /// captured: the element replaces its chart on every re-layout, and
    /// the gestures have to follow it onto the new one.
    gestures: RefCell<Vec<Gesture>>,
    /// The wake-up that turns a held press on a coarse pointer into a
    /// pending seek, since a finger that does not move sends no events.
    long_press: RefCell<Option<Closure<dyn FnMut()>>>,
    /// Whether hovering means anything here, asked of the browser rather
    /// than answered once, so a device that changes answers for itself.
    hover: Option<web_sys::MediaQueryList>,
    press: RefCell<Option<Press>>,
    /// The time the preview is showing, and the record of whether one is:
    /// a peek is announced only when it changes.
    peek: Cell<Option<f64>>,
    following: Cell<bool>,
    /// The markup the build wrote is still on screen, so a re-layout at the
    /// width it was drawn for can keep it.
    hydrated: Cell<bool>,
}

fn document() -> Option<web_sys::Document> {
    web_sys::window()?.document()
}

/// The page's clock in seconds, for the long press. A page with no
/// performance clock never reaches the delay, so a press there stays a tap
/// until it moves.
fn now() -> f64 {
    web_sys::window()
        .and_then(|w| w.performance())
        .map_or(0.0, |p| p.now() / 1000.0)
}

impl Follower {
    /// Re-read the parts the effects act on out of `svg`, move the
    /// gestures onto it, and put the preview back where it was: a
    /// re-layout draws a fresh chart, whose peek rule starts hidden.
    fn capture(&self, svg: Option<Element>) {
        let find = |selector: &str| {
            svg.as_ref()
                .and_then(|s| s.query_selector(selector).ok().flatten())
        };
        if let Some(svg) = &svg {
            for (name, closure) in self.gestures.borrow().iter() {
                let _ =
                    svg.add_event_listener_with_callback(name, closure.as_ref().unchecked_ref());
            }
        }
        *self.dom.borrow_mut() = Dom {
            playhead: find("g.playhead"),
            head_t: find(".head-t"),
            played: find(".bar-played"),
            peek_line: find(".peek-line"),
            figure: self.root.query_selector("figure").ok().flatten(),
            svg,
        };
        self.place_peek();
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
        // the build's markup leaves the screen here, so the host's state
        // says so too: hydrated means the pre-render is what is shown
        self.hydrated.set(false);
        set_state(&self.host, "hydrated", false);
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

    /// Put the playhead, its readout and the played bar at the stored time.
    /// `force` re-applies them after a re-layout, where the geometry
    /// changed under an unchanged clock.
    ///
    /// Nothing here announces a value. The chart's `graphics-document`
    /// role does not carry the `aria-value` family, so a value written on
    /// it would be written for nobody: no assistive technology reads it,
    /// and the attribute is invalid where it lands. Announcing the time
    /// wants the thumb and the slider role of decision 15, with decision
    /// 18's wording, and both are phase 4; until then the chart is
    /// operable and announces no value.
    fn apply(&self, force: bool) {
        let (t, layout, prev_x, prev_label) = {
            let live = self.live.borrow();
            (live.time, live.layout, live.x, live.label.clone())
        };
        let x = layout.x_of(t.clamp(0.0, layout.end));
        // the readout follows the preview while one is showing: the
        // playhead stays on the film's clock, and the number says what a
        // release would land on
        let label = readout(self.peek.get().unwrap_or(t));
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

    // ---- intents ------------------------------------------------------

    /// Whether hovering means anything on this device: [`HOVER_QUERY`],
    /// asked again each time rather than answered once. It gates the hover
    /// peek and nothing else - whether a gesture takes the long press is
    /// [`coarse_pointer`]'s answer about that gesture's own pointer.
    fn hover_peeks(&self) -> bool {
        self.hover.as_ref().is_some_and(|q| q.matches())
    }

    /// A pointer's x in the chart's own units. The viewBox is the CSS box
    /// to within the pixel a kept pre-render may differ by, so the scale is
    /// one either way and the thresholds stay in CSS px.
    fn x_of_pointer(&self, event: &web_sys::PointerEvent) -> Option<f64> {
        let rect = self.dom.borrow().svg.as_ref()?.get_bounding_client_rect();
        if rect.width() <= 0.0 {
            return None;
        }
        let width = self.live.borrow().layout.width;
        Some((f64::from(event.client_x()) - rect.left()) * (width / rect.width()))
    }

    /// The time a position means: the nearest sample within
    /// [`SNAP_RADIUS`], and the position's own time when no sample is near
    /// enough. A preview and the commit that follows it read this same
    /// function, so the preview never points at a time the release misses.
    fn time_at(&self, x: f64) -> f64 {
        let live = self.live.borrow();
        snap(&live.layout, x, &live.times).map_or_else(|| live.layout.t_at(x), |(_, t)| t)
    }

    /// Dispatch one intent from the host. Composed and bubbling, so a film
    /// in another tree still hears it.
    fn emit(&self, name: &str, detail: Option<&JsValue>) {
        let init = web_sys::CustomEventInit::new();
        init.set_bubbles(true);
        init.set_composed(true);
        if let Some(detail) = detail {
            init.set_detail(detail);
        }
        if let Ok(event) = web_sys::CustomEvent::new_with_event_init_dict(name, &init) {
            let _ = self.host.dispatch_event(&event);
        }
    }

    /// The same, with a `{ time }` detail; a peek that clears carries a
    /// null time rather than no field.
    fn emit_time(&self, name: &str, time: Option<f64>) {
        let detail = js_sys::Object::new();
        let _ = js_sys::Reflect::set(
            &detail,
            &JsValue::from_str(TIME_FIELD),
            &time.map_or(JsValue::NULL, JsValue::from_f64),
        );
        self.emit(name, Some(detail.as_ref()));
    }

    /// Put the renderer's peek rule at the previewed time, or hide it.
    fn place_peek(&self) {
        let dom = self.dom.borrow();
        let Some(line) = &dom.peek_line else {
            return;
        };
        match self.peek.get() {
            Some(t) => {
                let x = format!("{:.1}", self.live.borrow().layout.x_of(t));
                let _ = line.set_attribute("x1", &x);
                let _ = line.set_attribute("x2", &x);
                let _ = line.set_attribute("visibility", "visible");
            }
            None => {
                let _ = line.set_attribute("visibility", "hidden");
            }
        }
    }

    /// Show or clear the preview: the rule at that time, the readout
    /// reading it, the state, and the intent. Only a change is announced,
    /// so a pointer crossing one sample does not emit a peek per frame.
    fn set_peek(&self, time: Option<f64>) {
        if self.peek.get() == time {
            return;
        }
        self.peek.set(time);
        set_state(&self.host, PEEKING_STATE, time.is_some());
        self.place_peek();
        self.apply(true);
        self.emit_time(PEEK_EVENT, time);
    }

    /// Let a press go: the capture, the wake-up, both press states and the
    /// preview all come back. Whether it committed is the caller's to say.
    fn end_press(&self) {
        let press = self.press.borrow_mut().take();
        if let Some(press) = press {
            if let (Some(window), Some(timer)) = (web_sys::window(), press.timer) {
                window.clear_timeout_with_handle(timer);
            }
            if let Some(svg) = &self.dom.borrow().svg {
                let _ = svg.release_pointer_capture(press.id);
            }
        }
        set_state(&self.host, PENDING_STATE, false);
        set_state(&self.host, CANCELLING_STATE, false);
        self.set_peek(None);
    }

    /// Carry out an intent. Nothing here moves the playhead: the film's
    /// clock answers, and the chart follows that answer like any other
    /// tick.
    fn act(&self, intent: Intent) {
        match intent {
            Intent::Seek(t) => {
                // a seek commits at once and never leaves a press behind
                self.end_press();
                self.emit_time(SEEK_EVENT, Some(t));
            }
            Intent::Peek(t) => self.set_peek(t),
            Intent::Toggle => self.emit(TOGGLE_EVENT, None),
            Intent::Cancel => self.end_press(),
        }
    }

    /// Run the press through [`pointer_phase`] and do what it says.
    fn resolve(&self, x: f64) {
        let (down, elapsed, coarse) = {
            let mut press = self.press.borrow_mut();
            let Some(press) = press.as_mut() else {
                return;
            };
            press.x = x;
            advance(&mut press.down, x, None);
            (press.down, now() - press.started, press.coarse)
        };
        let phase = pointer_phase(Some(down), x, elapsed, coarse);
        if let Some(press) = self.press.borrow_mut().as_mut() {
            advance(&mut press.down, x, Some(phase));
        }
        match phase {
            Phase::Idle => {}
            Phase::Pending(x) => {
                set_state(&self.host, PENDING_STATE, true);
                set_state(&self.host, CANCELLING_STATE, false);
                self.act(Intent::Peek(Some(self.time_at(x))));
            }
            // the press is left exactly as it was: aiming out again is a
            // pending seek once more, and only the release drops this one
            Phase::WillCancel(x) => {
                set_state(&self.host, CANCELLING_STATE, true);
                self.act(Intent::Peek(Some(self.time_at(x))));
            }
            Phase::Commit(x) => self.act(Intent::Seek(self.time_at(x))),
            Phase::Cancel => self.act(Intent::Cancel),
        }
    }

    /// Ask for the wake-up that turns a held press into a pending seek.
    fn arm_long_press(&self) {
        let timer = {
            let held = self.long_press.borrow();
            let (Some(window), Some(callback)) = (web_sys::window(), held.as_ref()) else {
                return;
            };
            window
                .set_timeout_with_callback_and_timeout_and_arguments_0(
                    callback.as_ref().unchecked_ref(),
                    // a millisecond past the delay, not exactly on it: a timer
                    // set for the delay itself wakes at the instant the phase
                    // compares against, and whether the comparison sees the
                    // press as held depends on how the clock rounds that
                    // instant, which on a synthetic frame clock it does not
                    // reliably do in the same direction twice
                    (LONG_PRESS * 1000.0) as i32 + 1,
                )
                .ok()
        };
        if let (Some(timer), Some(press)) = (timer, self.press.borrow_mut().as_mut()) {
            press.timer = Some(timer);
        }
    }

    /// A press: capture the pointer so a drag that leaves the chart still
    /// resolves, and start the clock the long press runs on.
    fn on_down(&self, event: &Event) {
        let Some(pointer) = event.dyn_ref::<web_sys::PointerEvent>() else {
            return;
        };
        let Some(x) = self.x_of_pointer(pointer) else {
            return;
        };
        // a second pointer going down before the first came up takes the
        // press over, so the press it replaces is ended and not dropped:
        // its capture is released, its wake-up cleared, and its states and
        // its preview taken back. A press left lying here would have its
        // own origin read against the new pointer's positions
        let live = self.press.borrow().is_some();
        if live {
            self.end_press();
        }
        // preventing the default suppresses the compatibility mouse
        // events, which are what would select the chart's labels under a
        // drag; it suppresses the focus a press would have given it too,
        // so the focus is taken here instead and Escape can then cancel
        // the very drag this press is starting. The ring is
        // `:focus-visible` only, so a press does not draw one.
        event.prevent_default();
        if let Some(svg) = &self.dom.borrow().svg {
            let _ = svg.set_pointer_capture(pointer.pointer_id());
            if let Some(svg) = svg.dyn_ref::<web_sys::SvgElement>() {
                let _ = svg.focus();
            }
        }
        // the pointer that made this press is what says whether it takes
        // the long press, and it says so once: the moves that follow carry
        // the same type, and a second reading of the device would not be
        // about this gesture at all
        let coarse = coarse_pointer(&pointer.pointer_type());
        *self.press.borrow_mut() = Some(Press {
            id: pointer.pointer_id(),
            down: Down {
                x0: x,
                pending: false,
                released: false,
                travelled: false,
            },
            x,
            started: now(),
            coarse,
            timer: None,
        });
        if coarse {
            self.arm_long_press();
        }
    }

    /// The pointer moved: a press aims, and a pointer that can hover, with
    /// nothing pressed, peeks. A button still held with no press of ours is
    /// a gesture this element has already let go of, not a hover.
    ///
    /// A hover past [`SNAP_RADIUS`] of every sample still previews: what it
    /// previews is the position's own time, which is what a release there
    /// would seek to, since `time_at` is the one function both read.
    ///
    /// A move made by a pointer that is not the live press's is not this
    /// gesture and is ignored ([`should_ignore`]).
    fn on_move(&self, event: &Event) {
        let Some(pointer) = event.dyn_ref::<web_sys::PointerEvent>() else {
            return;
        };
        let Some(x) = self.x_of_pointer(pointer) else {
            return;
        };
        let held = self.press.borrow().as_ref().map(|p| p.id);
        if should_ignore(held, pointer.pointer_id()) {
            return;
        }
        if self.press.borrow().is_some() {
            self.resolve(x);
        } else if pointer.buttons() == 0 && self.hover_peeks() {
            self.act(Intent::Peek(Some(self.time_at(x))));
        }
    }

    /// The pointer came up: whatever the press was, it resolves now, unless
    /// the pointer is not the one the press was made with
    /// ([`should_ignore`]). A first finger lifting after a second took the
    /// press over resolves nothing: its position means nothing against the
    /// second one's origin.
    fn on_up(&self, event: &Event) {
        let Some(pointer) = event.dyn_ref::<web_sys::PointerEvent>() else {
            return;
        };
        let Some(x) = self.x_of_pointer(pointer) else {
            return;
        };
        let held = self.press.borrow().as_ref().map(|p| p.id);
        if should_ignore(held, pointer.pointer_id()) {
            return;
        }
        if let Some(press) = self.press.borrow_mut().as_mut() {
            press.down.released = true;
        }
        self.resolve(x);
    }

    /// The browser took the gesture away: the press ends where it stands,
    /// giving back the capture, the wake-up and the states. A cancel made
    /// by a pointer that is not the live press's is not this gesture and is
    /// ignored ([`should_ignore`]): with two fingers down and the second
    /// owning the press, the first one's cancel would end a gesture that is
    /// still under a finger.
    fn on_cancel(&self, event: &Event) {
        let Some(pointer) = event.dyn_ref::<web_sys::PointerEvent>() else {
            return;
        };
        let held = self.press.borrow().as_ref().map(|p| p.id);
        if should_ignore(held, pointer.pointer_id()) {
            return;
        }
        self.act(Intent::Cancel);
    }

    /// The pointer left the chart: the preview goes with it. A press is
    /// captured and resolves wherever it ends, so it is left alone.
    fn on_leave(&self) {
        if self.press.borrow().is_none() {
            self.act(Intent::Peek(None));
        }
    }

    /// A key, acted on only while the chart itself has the focus. The
    /// chart is not the shadow root's only tab stop: the data table's
    /// disclosure takes the focus too and keeps its own keys, so Space
    /// there opens the table and seeks nothing.
    fn on_key(&self, event: &Event) {
        let Some(key) = event.dyn_ref::<web_sys::KeyboardEvent>() else {
            return;
        };
        if self.root.active_element() != self.dom.borrow().svg {
            return;
        }
        let intent = {
            let live = self.live.borrow();
            key_intent(
                &key.key(),
                key.shift_key(),
                key.ctrl_key() || key.alt_key(),
                live.time,
                &Keys {
                    times: &live.times,
                    duration: key_duration(&live.data, live.layout.end),
                    chapters: &live.chapters,
                },
            )
        };
        let Some(intent) = intent else {
            return;
        };
        // the chart answered it, so the page does not also scroll
        key.prevent_default();
        self.act(intent);
    }
}

/// Everything that must live as long as the element, and the targets its
/// listeners have to come off again.
struct Wiring {
    follower: Rc<Follower>,
    on_tick: TickListener,
    on_document: Closure<dyn FnMut(Event)>,
    /// Keys ride on the host, which outlives every re-layout; the handler
    /// asks the shadow root whether the chart is the focused thing.
    on_key: Closure<dyn FnMut(Event)>,
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
        // a preview or a press aimed at the data that has just gone is
        // about nothing now. It is dropped quietly: no user gesture ended
        // it, so nothing is announced for it either
        wiring.follower.peek.set(None);
        *wiring.follower.press.borrow_mut() = None;
        set_state(&self.host, PEEKING_STATE, false);
        set_state(&self.host, PENDING_STATE, false);
        set_state(&self.host, CANCELLING_STATE, false);
        {
            let mut live = wiring.follower.live.borrow_mut();
            live.times = times_of(&data);
            live.chapters = chapter_starts(&data);
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
            times: times_of(&data),
            chapters: chapter_starts(&data),
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
            gestures: RefCell::new(Vec::new()),
            long_press: RefCell::new(None),
            hover: web_sys::window().and_then(|w| w.match_media(HOVER_QUERY).ok().flatten()),
            press: RefCell::new(None),
            peek: Cell::new(None),
            following: Cell::new(false),
            hydrated: Cell::new(false),
        });

        // the gestures are built before anything is drawn, because it is
        // `capture` that puts them on whichever chart is on screen
        {
            let f = follower.clone();
            *follower.long_press.borrow_mut() = Some(Closure::new(move || {
                let x = f.press.borrow().as_ref().map(|p| p.x);
                if let Some(x) = x {
                    f.resolve(x);
                }
            }));
            let mut gestures = follower.gestures.borrow_mut();
            let mut on = |name: &'static str, closure: Closure<dyn FnMut(Event)>| {
                gestures.push((name, closure));
            };
            let f = follower.clone();
            on("pointerdown", Closure::new(move |e: Event| f.on_down(&e)));
            let f = follower.clone();
            on("pointermove", Closure::new(move |e: Event| f.on_move(&e)));
            let f = follower.clone();
            on("pointerup", Closure::new(move |e: Event| f.on_up(&e)));
            let f = follower.clone();
            on(
                "pointercancel",
                Closure::new(move |e: Event| f.on_cancel(&e)),
            );
            let f = follower.clone();
            on("pointerleave", Closure::new(move |_| f.on_leave()));
        }

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

        let on_key = {
            let f = follower.clone();
            Closure::<dyn FnMut(Event)>::new(move |e: Event| f.on_key(&e))
        };
        let _ = self
            .host
            .add_event_listener_with_callback("keydown", on_key.as_ref().unchecked_ref());

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
            on_key,
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
        // a long press still owed to us would resolve against a chart
        // nothing is watching, and would emit an intent no gesture asked
        // for; it goes back with the press it belongs to
        let press = wiring.follower.press.borrow_mut().take();
        if let (Some(window), Some(timer)) = (web_sys::window(), press.and_then(|p| p.timer)) {
            window.clear_timeout_with_handle(timer);
        }
        // every one of these closures holds the Follower they are stored
        // on, so until the slots are emptied its strong count never reaches
        // zero and every handle, the data and the layout are held with it
        *wiring.follower.frame.borrow_mut() = None;
        *wiring.follower.on_tick.borrow_mut() = None;
        *wiring.follower.long_press.borrow_mut() = None;
        wiring.follower.gestures.borrow_mut().clear();
        let _ = self
            .host
            .remove_event_listener_with_callback("keydown", wiring.on_key.as_ref().unchecked_ref());
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
    use crate::components::chart_style::{FORCED_COLOURS_CSS, PRINT_CSS};

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
    /// chart has no thumb and no value of its own, so a focusable slider
    /// frozen at zero would announce a value that is never the chart's.
    /// It is still a tab stop: it answers decision 17's keys.
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
            // a tab stop on the chart itself, and an operable one: it
            // answers keys. The data table's disclosure is a tab stop of
            // its own, so the shadow root has two
            assert!(markup.contains("tabindex=\"0\""), "{markup}");
            assert!(markup.contains("<summary>Data table</summary>"), "{markup}");
            for frozen in [
                "role=\"slider\"",
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
        // and no tick puts a value back on that role, for the reason
        // `Follower::apply` gives
        let source = include_str!("chart.rs");
        let write = ["set_attribute(\"aria", "-value"].concat();
        assert!(
            !source.contains(&write),
            "a tick must not write an aria value onto the document role"
        );
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

    /// The text of every cell in a table row.
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
        // the chart is a tab stop now, so it carries the site's focus
        // token, and only where a ring belongs: a press must not ring the
        // chart it is scrubbing
        assert!(css.contains("svg.chart:focus { outline: none; }"), "{css}");
        assert!(
            css.contains(
                "svg.chart:focus-visible { outline: 2px solid var(--op-focus); outline-offset: 2px; }"
            ),
            "{css}"
        );
        // and the page keeps its vertical scroll over a chart that reads
        // horizontal drags
        assert!(css.contains("touch-action: pan-y;"), "{css}");
        // every colour is a token, on the part it paints
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

    /// One attribute of an element's opening tag. The tag is read with a
    /// space in front of it, so the first attribute is found like the rest
    /// and no key can match the tail of a longer one.
    fn attr(head: &str, key: &str) -> String {
        format!(" {head}")
            .split_once(&format!(" {key}=\""))
            .map(|(_, rest)| rest.split_once('"').expect("a closing quote").0.to_owned())
            .unwrap_or_default()
    }

    /// The opening tag of every element in `svg` that opens with `tag`.
    fn heads<'a>(svg: &'a str, tag: &str) -> Vec<&'a str> {
        svg.split(&format!("<{tag} "))
            .skip(1)
            .map(|e| e.split_once('>').expect("the tag ends").0)
            .collect()
    }

    /// Every rule in a stylesheet, as the at-rule it was read inside (empty
    /// at the top level), its selector and its declarations. An at-rule
    /// opens no rule of its own: its block is read for the rules in it, so
    /// a rule under `@media` is enumerated like any other and still says
    /// which query it answers.
    fn rules_of(css: &str) -> Vec<(String, String, String)> {
        let mut out: Vec<(String, String, String)> = Vec::new();
        let mut head = String::new();
        let mut at = String::new();
        let mut open: Option<(String, String)> = None;
        for c in css.chars() {
            match c {
                '{' => {
                    let selector = head.trim().to_owned();
                    head.clear();
                    if selector.starts_with('@') {
                        at = selector;
                    } else {
                        open = Some((selector, String::new()));
                    }
                }
                '}' => {
                    match open.take() {
                        Some((selector, body)) => out.push((at.clone(), selector, body)),
                        // the at-rule's own block ends here
                        None => at.clear(),
                    }
                    head.clear();
                }
                _ => match &mut open {
                    Some((_, body)) => body.push(c),
                    None => head.push(c),
                },
            }
        }
        out
    }

    /// Every class a selector names, in any of its compounds. A descendant
    /// selector names its ancestors as well, so this is read generously:
    /// one of these matching an element's class list puts the element in
    /// the rule's reach, whether as the subject or as an ancestor of it.
    fn classes_named(selector: &str) -> Vec<String> {
        selector
            .split('.')
            .skip(1)
            .map(|rest| {
                rest.chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
                    .collect()
            })
            .collect()
    }

    /// Whether a rule with this selector could reach an element whose class
    /// attribute is `class`, by a class of its own or by an attribute
    /// selector over the class attribute.
    fn could_reach(selector: &str, class: &str) -> bool {
        let named = classes_named(selector);
        let by_class = class
            .split_whitespace()
            .any(|token| named.iter().any(|c| c == token));
        let by_attribute = selector.split("[class").skip(1).any(|rest| {
            let value: String = rest
                .trim_start_matches(['^', '$', '*', '~', '|', '='])
                .chars()
                .take_while(|c| *c != ']')
                .collect();
            let value = value.trim_matches(['"', '\'']);
            !value.is_empty() && class.contains(value)
        });
        by_class || by_attribute
    }

    /// The whole stylesheet a chart's shadow root carries.
    fn shadow_css() -> String {
        format!("{BASE_CSS}{}", stylesheet(DEFAULT_RATIO))
    }

    /// Both stylesheets that draw op-chart's markup: this element's own,
    /// and the film's copy for the chart it holds.
    fn sheets() -> Vec<String> {
        vec![shadow_css(), super::super::film::chart_css()]
    }

    /// The classes the emitter writes that carry no paint of their own: the
    /// groups, which exist to be z-ordered and hidden whole, and `target`,
    /// which no rule may reach at all.
    const UNPAINTED: &[&str] = &[
        "axes", "bands", "marks", "series", "track", "cursor", "playhead", "targets", "target",
    ];

    /// The class of every label the emitter draws inside the plot, where a
    /// label lies over the gridlines, the band and the series lines. The
    /// labels in the margins - the value axis, the time axis and the
    /// readout - are over nothing and are not in this set.
    fn labels_over_the_plot() -> Vec<String> {
        let mut inside: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        let mut outside: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for block in [BLOCK, EVERY_LABEL] {
            let d = op_chart::data::parse(block).expect("the block is valid");
            let l = Layout::sized(DEFAULT_WIDTH, DEFAULT_WIDTH / DEFAULT_RATIO, d.end());
            let svg = op_chart::render(&d.to_spec(), l).svg;
            for head in heads(&svg, "text") {
                let x: f64 = attr(head, "x").parse().expect("a number");
                let y: f64 = attr(head, "y").parse().expect("a number");
                let over_the_plot =
                    x >= l.left && x <= l.width - l.right && y >= l.top && y <= l.plot_bottom();
                let class = attr(head, "class");
                if over_the_plot {
                    inside.insert(class);
                } else {
                    outside.insert(class);
                }
            }
        }
        // every kind of label the emitter draws over the plot is here, so
        // a caller can never be checking an empty set, and the axis text
        // that is not one of them is where it belongs
        let over: Vec<String> = inside.into_iter().collect();
        assert_eq!(over, ["band-label", "endlabel", "mark-label", "marklabel"]);
        assert!(outside.contains("axis"), "{outside:?}");
        over
    }

    /// Every class the emitter writes for the test blocks.
    fn emitted_classes() -> std::collections::BTreeSet<String> {
        let mut out = std::collections::BTreeSet::new();
        for block in [BLOCK, EVERY_LABEL] {
            let d = op_chart::data::parse(block).expect("the block is valid");
            let svg = op_chart::render(
                &d.to_spec(),
                Layout::sized(DEFAULT_WIDTH, DEFAULT_WIDTH / DEFAULT_RATIO, d.end()),
            )
            .svg;
            for tag in ["svg", "rect", "line", "path", "text", "circle", "g"] {
                for head in heads(&svg, tag) {
                    for token in attr(head, "class").split_whitespace() {
                        out.insert(token.to_owned());
                    }
                }
            }
        }
        out
    }

    /// A hit target is hit, never seen. It carries one class, `target`,
    /// and no rule in the stylesheet names it: the defect this pins is a
    /// target classed with the cue it stands for, which is the class the
    /// drawn rule or tick is painted by, so every target drew a dashed box.
    #[test]
    fn no_rule_in_the_stylesheet_can_paint_a_pointer_target() {
        let rules: Vec<(String, String, String)> =
            sheets().iter().flat_map(|css| rules_of(css)).collect();
        // the reader found the stylesheets, at-rules and all
        assert!(rules.len() > 40, "only {} rules read", rules.len());
        assert!(
            rules
                .iter()
                .any(|(_, s, b)| s == ".chart .mark" && b.contains("stroke-dasharray")),
            "the dashed rule is not among the rules read"
        );
        assert!(
            rules
                .iter()
                .any(|(at, s, _)| at.contains("forced-colors") && s.contains(".chart .chapter")),
            "the forced-colours block was not descended into"
        );
        // the detector itself: the class list the emitter used to write
        // would be reached by the rule that dashes a mark, and the one it
        // writes now is reached by nothing
        assert!(could_reach(".chart .mark", "target mark"));
        assert!(could_reach(".chart .chapter", "target chapter"));
        assert!(!could_reach(".chart .mark", "target"));
        assert!(could_reach(".chart path[class^=series]", "series-2"));

        let d = data();
        let svg = op_chart::render(
            &d.to_spec(),
            Layout::sized(DEFAULT_WIDTH, DEFAULT_WIDTH / DEFAULT_RATIO, d.end()),
        )
        .svg;
        let targets: Vec<String> = heads(&svg, "rect")
            .into_iter()
            .filter(|head| attr(head, "part") == "target")
            .map(|head| attr(head, "class"))
            .collect();
        // the block draws a mark and a chapter, so both kinds are here
        assert_eq!(targets.len(), 2, "{targets:?}");
        for class in &targets {
            for (at, selector, body) in &rules {
                assert!(
                    !could_reach(selector, class),
                    "the rule `{at} {selector} {{{body}}}` reaches a hit target classed `{class}`"
                );
            }
        }
    }

    /// The positive counterpart: a class the emitter writes and no rule
    /// names is a thing drawn in whatever the user agent chose, which is
    /// how `mark-label` and `band-label` came to paint black on the dark
    /// theme. The group wrappers are the exception, and `target` is the
    /// one class that must be reached by nothing at all.
    #[test]
    fn every_class_the_emitter_writes_is_painted_or_is_a_group() {
        let sheets = sheets();
        let emitted = emitted_classes();
        // the fixtures really do draw the classes this is here to guard
        for want in [
            "band",
            "band-label",
            "mark-label",
            "marklabel",
            "endlabel",
            "peek-band",
            "chapter",
            "shown",
            "alt",
            "target",
        ] {
            assert!(emitted.contains(want), "the fixtures draw no {want}");
        }
        for class in emitted.iter().filter(|c| !UNPAINTED.contains(&c.as_str())) {
            assert!(
                sheets.iter().any(|css| rules_of(css)
                    .iter()
                    .any(|(_, selector, _)| classes_named(selector).iter().any(|c| c == class))),
                "no rule in either stylesheet names `{class}`"
            );
        }
    }

    /// Decision 24's gap where the band meets a series line: a 1 px edge in
    /// the surface colour, which the markup asks for with a width and the
    /// stylesheet has to paint, since `stroke` is none until a rule says
    /// otherwise. The film's peek band is the same shape and takes no edge,
    /// so the rule may not be written to reach both.
    #[test]
    fn the_annotation_band_carries_the_surface_edge_and_the_peek_band_does_not() {
        for css in sheets() {
            let rules = rules_of(&css);
            assert!(
                rules.iter().any(|(at, selector, body)| {
                    let named = classes_named(selector);
                    at.is_empty()
                        && named.iter().any(|c| c == "band")
                        && !named.iter().any(|c| c == "peek-band")
                        && body.contains("stroke: var(--op-surface)")
                }),
                "no surface edge for the annotation band in {css}"
            );
            // and nothing outside the forced-colours and print blocks,
            // where both bands become outlines, strokes the peek band
            for (at, selector, body) in &rules {
                if at.is_empty() && classes_named(selector).iter().any(|c| c == "peek-band") {
                    assert!(
                        !body.contains("stroke:"),
                        "`{selector}` strokes the peek band"
                    );
                }
            }
        }
    }

    /// The compound each part of a selector list ends in: the element the
    /// rule paints, rather than the ancestors that qualify it. `.chart`
    /// stands in front of nearly every selector in these stylesheets and
    /// tells no two of them apart, so the cascade's question - do these two
    /// rules meet on one element - is asked of the subjects alone.
    fn subjects(selector: &str) -> Vec<&str> {
        selector
            .split(',')
            .filter_map(|part| part.split_whitespace().next_back())
            .collect()
    }

    /// The properties a declaration block sets.
    fn properties(body: &str) -> Vec<&str> {
        body.split(';')
            .filter_map(|declaration| declaration.split_once(':'))
            .map(|(property, _)| property.trim())
            .filter(|property| !property.is_empty())
            .collect()
    }

    /// Forced colours and print take the palette away, and they can only do
    /// that where nothing written after them paints the same property on
    /// the same element again: at equal specificity the later rule wins,
    /// media query or not. The shared blocks were interpolated halfway up
    /// both stylesheets, so the band's fill, the track, the played bar, the
    /// chapter tick, the peek rule, the playhead, its dot and its readout
    /// were all written after them and kept their tokens on a forced
    /// palette and in print. The blocks are the last word now and this
    /// holds them there: a later rule is a failure at any specificity,
    /// since the ordering is the fix and a specificity hack is not.
    #[test]
    fn nothing_written_after_the_shared_blocks_repaints_what_they_map() {
        let shared = chart_rules();
        let classes = emitted_classes();
        // the (class, property) pairs actually put to the test, so this can
        // never be a loop over nothing
        let mut checked: std::collections::BTreeSet<(&str, &str)> =
            std::collections::BTreeSet::new();
        let blocks = rules_of(&shared);
        for css in sheets() {
            let head = css
                .split(shared.as_str())
                .next()
                .expect("a split yields at least one piece");
            assert!(
                head.len() < css.len(),
                "the shared blocks are not in this stylesheet whole"
            );
            let rules = rules_of(&css);
            let after = &rules[rules_of(head).len() + blocks.len()..];
            for (at, selector, body) in &blocks {
                if !(at.contains("forced-colors") || at.contains("print")) {
                    continue;
                }
                for property in properties(body) {
                    for class in &classes {
                        if !subjects(selector).iter().any(|s| could_reach(s, class)) {
                            continue;
                        }
                        checked.insert((class, property));
                        for (later_at, later_selector, later_body) in after {
                            assert!(
                                !(properties(later_body).contains(&property)
                                    && subjects(later_selector)
                                        .iter()
                                        .any(|s| could_reach(s, class))),
                                "`{later_at} {later_selector}` is written after the shared blocks \
                                 and sets `{property}` on `{class}` again, undoing \
                                 `{at} {selector}`"
                            );
                        }
                    }
                }
            }
        }
        // every paint the ordering defect put back: if one of these stops
        // being mapped, the loop above stops testing it and says nothing
        for pair in [
            ("head", "stroke"),
            ("head-dot", "fill"),
            ("head-t", "fill"),
            ("bar-bg", "fill"),
            ("bar-played", "fill"),
            ("chapter", "stroke"),
            ("peek-line", "stroke"),
            ("band", "fill"),
        ] {
            assert!(
                checked.contains(&pair),
                "the shared blocks no longer map `{}` on `{}`",
                pair.1,
                pair.0
            );
        }
    }

    /// `--op-peek` is the peek rule's own token: declared per theme beside
    /// the playhead's, registered as an `@property` so the theme change can
    /// interpolate it, blended by that transition and held to 3:1 on all
    /// three backdrops by the palette test (decisions 6, 22 and 23). Both
    /// stylesheets painted the rule with `--op-muted`, which carries the
    /// same value in both themes, so the rule looked right while the token
    /// drew nothing at all and no test could tell. This ties the two ends
    /// together: the rule names the token, and the token is a real one.
    #[test]
    fn the_peek_rule_is_painted_with_its_own_token() {
        for css in sheets() {
            let painted: Vec<String> = rules_of(&css)
                .into_iter()
                .filter(|(at, selector, _)| {
                    at.is_empty() && classes_named(selector).iter().any(|c| c == "peek-line")
                })
                .map(|(_, _, body)| body)
                .collect();
            assert_eq!(painted.len(), 1, "{painted:?}");
            assert!(
                painted[0].contains("stroke: var(--op-peek)"),
                "the peek rule is painted with something else: `{}`",
                painted[0]
            );
        }
        const THEME: &str = include_str!("../../../../styles/theme.css");
        let declared = THEME.matches("--op-peek:").count();
        assert!(declared > 1, "the peek token is declared {declared} times");
        assert_eq!(
            declared,
            THEME.matches("--op-playhead:").count(),
            "the peek token is not declared in every theme the playhead's is"
        );
        assert!(THEME.contains("@property --op-peek {"));
        let blended = THEME
            .split("transition-property:")
            .nth(1)
            .and_then(|list| list.split(';').next())
            .expect("the theme transition names the tokens it blends");
        assert!(blended.contains("--op-peek"), "{blended}");
    }

    /// A block that draws one of every label the emitter has: a chapter
    /// past the start, a mark, a band with room for its own label, and a
    /// series with a direct end label.
    const EVERY_LABEL: &str = r#"{
        "duration": 3.3,
        "series": [{"id": "a", "label": "a", "unit": "%"}],
        "rows": [[0, 0], [3.3, 100]],
        "marks": [{"t": 1.0, "label": "half"}],
        "band": {"t0": 1.5, "t1": 2.2, "label": "warm"},
        "chapters": [{"t": 0, "title": "start"}, {"t": 0.5, "title": "one"}]
    }"#;

    /// Decision 24's halo: a label drawn over the plot is a label over a
    /// series line, a gridline or the band, so every one of them carries a
    /// stroke in the surface colour under its glyphs. The labels in the
    /// margins - the value axis, the time axis and the readout - are over
    /// nothing and are not asked for one.
    #[test]
    fn every_label_drawn_inside_the_plot_carries_the_halo() {
        // in the element's stylesheet and in the film's copy alike: the
        // two draw the same markup and must read the same
        for css in sheets() {
            for class in labels_over_the_plot() {
                assert!(
                    rules_of(&css).iter().any(|(_, selector, body)| {
                        classes_named(selector).contains(&class)
                            && body.contains("paint-order: stroke")
                            && body.contains("stroke: var(--op-surface)")
                    }),
                    "no halo rule for a label classed `{class}` in {css}"
                );
            }
        }
    }

    /// Forced colours and print take the palette away and map every paint
    /// the chart uses to a system or a print colour. A label left out of
    /// those lists keeps an author colour that the mode was meant to
    /// replace, so each of them names every label drawn over the plot.
    #[test]
    fn every_label_drawn_inside_the_plot_is_mapped_where_the_palette_goes() {
        for (block, mode) in [(FORCED_COLOURS_CSS, "forced colours"), (PRINT_CSS, "print")] {
            for class in labels_over_the_plot() {
                assert!(
                    rules_of(block)
                        .iter()
                        .any(|(_, selector, _)| classes_named(selector).contains(&class)),
                    "no {mode} rule for a label classed `{class}`"
                );
            }
        }
    }

    // ---- intents ------------------------------------------------------

    #[test]
    fn a_pointer_takes_the_sample_within_forty_pixels_of_it_and_no_other() {
        let d = data();
        let layout = Layout::sized(640.0, 240.0, d.end());
        let times = times_of(&d);
        assert_eq!(times, [0.0, 1.65, 3.3]);
        // the chapter starts a page key steps between are read off the
        // same block
        assert_eq!(chapter_starts(&d), [0.0, 1.65]);
        // on a sample, and at the radius either side of it
        let x = layout.x_of(1.65);
        assert_eq!(snap(&layout, x, &times), Some((1, 1.65)));
        assert_eq!(snap(&layout, x - SNAP_RADIUS, &times), Some((1, 1.65)));
        assert_eq!(snap(&layout, x + SNAP_RADIUS, &times), Some((1, 1.65)));
        // and a hair past it, where no sample is near enough to mean
        assert_eq!(snap(&layout, x + SNAP_RADIUS + 0.1, &times), None);
        assert_eq!(snap(&layout, x - SNAP_RADIUS - 0.1, &times), None);
        // the samples here are 290 px apart, so the radius can never
        // reach two of them at once
        assert!((layout.x_of(3.3) - x) > 2.0 * SNAP_RADIUS);
        // nothing sampled is nothing to snap to
        assert_eq!(snap(&layout, x, &[]), None);
    }

    /// A timeline for the key table: 61 samples 0.05 s apart, so five
    /// samples, ten samples and a second are three different distances,
    /// and three chapters, the last of them not on a round second.
    fn key_times() -> Vec<f64> {
        (0..=60).map(|i| f64::from(i) * 0.05).collect()
    }
    const KEY_CHAPTERS: [f64; 3] = [0.0, 1.0, 2.25];
    const KEY_END: f64 = 3.0;

    /// The time a key asked for, without minding the last bit of a tenth.
    #[track_caller]
    fn seeks(got: Option<Intent>, want: f64) {
        match got {
            Some(Intent::Seek(t)) => assert!((t - want).abs() < 1e-9, "{t} is not {want}"),
            other => panic!("{other:?} is not a seek to {want}"),
        }
    }

    #[test]
    fn every_key_of_decision_seventeen_carries_its_own_intent() {
        let times = key_times();
        let k = Keys {
            times: &times,
            duration: KEY_END,
            chapters: &KEY_CHAPTERS,
        };
        // the middle of the timeline, on the thirtieth sample and inside
        // the second chapter
        let at = 1.5;
        assert_eq!(times[30], at);
        // shift changes nothing but the arrows, so every other key is
        // asked both ways
        for shift in [false, true] {
            // one sample
            seeks(key_intent(",", shift, false, at, &k), times[29]);
            seeks(key_intent(".", shift, false, at, &k), times[31]);
            // ten samples, in either case the key arrives in
            for (key, want) in [("j", times[20]), ("J", times[20]), ("l", times[40])] {
                seeks(key_intent(key, shift, false, at, &k), want);
            }
            seeks(key_intent("L", shift, false, at, &k), times[40]);
            // by chapter, and the same by the modifier that aliases it
            seeks(key_intent("PageUp", shift, false, at, &k), 1.0);
            seeks(key_intent("PageDown", shift, false, at, &k), 2.25);
            seeks(key_intent("ArrowLeft", shift, true, at, &k), 1.0);
            seeks(key_intent("ArrowRight", shift, true, at, &k), 2.25);
            // the ends of the timeline
            seeks(key_intent("Home", shift, false, at, &k), 0.0);
            seeks(key_intent("End", shift, false, at, &k), KEY_END);
            // tenths of the duration, all ten of them
            for digit in 0..=9u8 {
                let key = char::from(b'0' + digit).to_string();
                seeks(
                    key_intent(&key, shift, false, at, &k),
                    KEY_END * f64::from(digit) / 10.0,
                );
            }
            // play, pause and the cancel that emits no seek
            for key in [" ", "k", "K"] {
                assert_eq!(key_intent(key, shift, false, at, &k), Some(Intent::Toggle));
            }
            assert_eq!(
                key_intent("Escape", shift, false, at, &k),
                Some(Intent::Cancel)
            );
            // and a key the chart does not answer is left to the page,
            // Tab above all
            for key in ["Tab", "Enter", "a", "<", ">", "F5", "Shift", "ArrowUp"] {
                assert_eq!(key_intent(key, shift, false, at, &k), None, "{key}");
            }
        }
        // five samples without shift, a second with it
        seeks(key_intent("ArrowLeft", false, false, at, &k), times[25]);
        seeks(key_intent("ArrowRight", false, false, at, &k), times[35]);
        seeks(key_intent("ArrowLeft", true, false, at, &k), 0.5);
        seeks(key_intent("ArrowRight", true, false, at, &k), 2.5);
        // five samples, ten samples and a second are three distances, so
        // none of those assertions could have passed for another key
        for pair in [times[25], times[20], 0.5].windows(2) {
            assert!(pair[0] > pair[1], "{pair:?}");
        }
        // a block with nothing sampled has no sample to step onto, and
        // still answers every key that is not a step
        let bare = Keys {
            times: &[],
            duration: KEY_END,
            chapters: &KEY_CHAPTERS,
        };
        for key in [",", ".", "ArrowLeft", "ArrowRight", "j", "l"] {
            assert_eq!(key_intent(key, false, false, at, &bare), None, "{key}");
        }
        seeks(key_intent("End", false, false, at, &bare), KEY_END);
        seeks(key_intent("ArrowRight", true, false, at, &bare), 2.5);
    }

    #[test]
    fn no_key_walks_past_either_end_of_the_timeline() {
        let times = key_times();
        let k = Keys {
            times: &times,
            duration: KEY_END,
            chapters: &KEY_CHAPTERS,
        };
        // at the start every backward key holds at zero
        for key in [",", "ArrowLeft", "j", "Home", "PageUp", "0"] {
            seeks(key_intent(key, false, false, 0.0, &k), 0.0);
        }
        seeks(key_intent("ArrowLeft", true, false, 0.0, &k), 0.0);
        seeks(key_intent("ArrowLeft", false, true, 0.0, &k), 0.0);
        // and at the end every forward key holds at the duration
        for key in [".", "ArrowRight", "l", "End", "PageDown", "9"] {
            let want = if key == "9" { KEY_END * 0.9 } else { KEY_END };
            seeks(key_intent(key, false, false, KEY_END, &k), want);
        }
        seeks(key_intent("ArrowRight", true, false, KEY_END, &k), KEY_END);
        seeks(key_intent("ArrowRight", false, true, KEY_END, &k), KEY_END);
        // a clock already past either end is brought back inside it
        seeks(key_intent(".", false, false, -5.0, &k), times[1]);
        seeks(key_intent(",", false, false, 99.0, &k), times[59]);
        seeks(key_intent("ArrowRight", true, false, 99.0, &k), KEY_END);
    }

    #[test]
    fn a_chapter_key_stops_at_the_first_chapter_and_at_the_last() {
        let times = key_times();
        let k = Keys {
            times: &times,
            duration: KEY_END,
            chapters: &KEY_CHAPTERS,
        };
        // inside the first chapter: forward to the second, back to the
        // start of the axis, since there is no chapter before this one
        seeks(key_intent("PageDown", false, false, 0.4, &k), 1.0);
        seeks(key_intent("PageUp", false, false, 0.4, &k), 0.0);
        // inside the last: forward to the end of the axis, back to its
        // own start
        seeks(key_intent("PageDown", false, false, 2.6, &k), KEY_END);
        seeks(key_intent("PageUp", false, false, 2.6, &k), 2.25);
        // standing exactly on a chapter start goes to the one before it,
        // as the film's page key does, and never to itself
        seeks(key_intent("PageUp", false, false, 2.25, &k), 1.0);
        seeks(key_intent("PageUp", false, false, 1.0, &k), 0.0);
        seeks(key_intent("PageDown", false, false, 1.0, &k), 2.25);
        // a block with one chapter has only the two ends to offer
        let one = Keys {
            times: &times,
            duration: KEY_END,
            chapters: &KEY_CHAPTERS[..1],
        };
        seeks(key_intent("PageDown", false, false, 1.5, &one), KEY_END);
        seeks(key_intent("PageUp", false, false, 1.5, &one), 0.0);
    }

    /// The schema lets a block state a duration past its last row, and the
    /// renderer writes that stated duration into `aria-valuemax`. The keys
    /// are read against the same number, so End reaches the maximum the
    /// chart announces and a digit is a tenth of the span it announced.
    #[test]
    fn end_and_the_digits_reach_the_duration_the_chart_announces() {
        let block = r#"{"series": [{"id": "a", "label": "A"}],
            "rows": [[0, 0], [2, 1]], "duration": 5}"#;
        let d = op_chart::data::parse(block).expect("the block is valid");
        // the axis stops at the last row; the block promises three seconds
        // more, and both numbers are real
        assert_eq!((d.end(), d.duration), (2.0, 5.0));
        let duration = key_duration(&d, d.end());
        // what the keys read is what the renderer announces
        assert_eq!(duration, d.to_spec().duration);
        assert!(duration > d.end(), "{duration} is not past {}", d.end());
        let times = times_of(&d);
        let k = Keys {
            times: &times,
            duration,
            chapters: &[],
        };
        // End lands on the announced maximum, and a digit is a tenth of it
        seeks(key_intent("End", false, false, 0.0, &k), 5.0);
        seeks(key_intent("5", false, false, 0.0, &k), 2.5);
        seeks(key_intent("9", false, false, 0.0, &k), 4.5);
        // and Page Down, with no chapter left to reach, stops nowhere short
        // of that maximum either: the fallback is the announced timeline's
        // end and not the axis, which stops three seconds earlier here
        seeks(key_intent("PageDown", false, false, 0.0, &k), 5.0);
        // a one-second jump inside that span is not clipped to the axis
        seeks(key_intent("ArrowRight", true, false, 4.2, &k), 5.0);
        // while stepping stays the rows' business and stops on the last of
        // them, wherever the stated duration ends
        seeks(key_intent(".", false, false, 0.0, &k), 2.0);
        seeks(key_intent("ArrowRight", false, false, 0.0, &k), 2.0);
        // and where the rows outlive the stated duration the axis wins, so
        // no key asks for a time the chart never drew
        let past = op_chart::data::parse(
            r#"{"series": [{"id": "a", "label": "A"}], "rows": [[0, 0], [9, 1]], "duration": 1}"#,
        )
        .expect("the block is valid");
        assert_eq!(key_duration(&past, past.end()), 9.0);
        assert_eq!(key_duration(&past, past.end()), past.to_spec().duration);
    }

    #[test]
    fn a_press_that_does_not_move_is_a_tap_until_it_is_released() {
        let down = Down {
            x0: 100.0,
            pending: false,
            released: false,
            travelled: false,
        };
        let press =
            |x: f64, elapsed: f64, coarse: bool| pointer_phase(Some(down), x, elapsed, coarse);
        // inside the slop, exactly on it, and past it
        assert_eq!(press(100.0 + 2.9, 10.0, false), Phase::Idle);
        assert_eq!(press(100.0 + DRAG_PX, 10.0, false), Phase::Idle);
        let out = 100.0 + 3.1;
        assert_eq!(press(out, 10.0, false), Phase::Pending(out));
        // and the same either way from the origin
        let back = 100.0 - 3.1;
        assert_eq!(press(100.0 - DRAG_PX, 10.0, false), Phase::Idle);
        assert_eq!(press(back, 10.0, false), Phase::Pending(back));
        // a fine pointer has no long press: held still, it stays a tap
        // however long it waits
        assert_eq!(press(100.0, LONG_PRESS, false), Phase::Idle);
        assert_eq!(press(100.0, 60.0, false), Phase::Idle);
        // no press at all is nothing to resolve, on either pointer
        assert_eq!(pointer_phase(None, 100.0, 0.0, false), Phase::Idle);
        assert_eq!(pointer_phase(None, 100.0, 9.0, true), Phase::Idle);
        // the release resolves a tap and a drag alike, at the position the
        // pointer left it
        let released = Down {
            released: true,
            ..down
        };
        assert_eq!(
            pointer_phase(Some(released), 101.0, 0.02, false),
            Phase::Commit(101.0)
        );
        assert_eq!(
            pointer_phase(
                Some(Down {
                    pending: true,
                    ..released
                }),
                300.0,
                2.0,
                true
            ),
            Phase::Commit(300.0)
        );
    }

    #[test]
    fn a_coarse_press_becomes_pending_once_it_has_been_held() {
        let down = Down {
            x0: 40.0,
            pending: false,
            released: false,
            travelled: false,
        };
        let press = |elapsed: f64, coarse: bool| pointer_phase(Some(down), 40.0, elapsed, coarse);
        // under the delay, exactly at it, and past it
        assert_eq!(press(0.49, true), Phase::Idle);
        assert_eq!(press(LONG_PRESS, true), Phase::Pending(40.0));
        // the wake-up is armed a millisecond later than the delay it tests, so
        // the callback always arrives on the far side of the comparison
        assert!(
            (LONG_PRESS * 1000.0) as i32 + 1 > (LONG_PRESS * 1000.0) as i32,
            "the long press must wake after the delay it compares against"
        );
        assert_eq!(
            press(LONG_PRESS + 0.001, true),
            Phase::Pending(40.0),
            "a press woken a millisecond late is held"
        );
        assert_eq!(press(0.51, true), Phase::Pending(40.0));
        // the same holds on a fine pointer are still taps: the delay is
        // the coarse path's alone
        assert_eq!(press(0.49, false), Phase::Idle);
        assert_eq!(press(LONG_PRESS, false), Phase::Idle);
        assert_eq!(press(0.51, false), Phase::Idle);
        // and a coarse pointer that travels does not wait for the delay
        let moved = 40.0 + 3.1;
        assert_eq!(
            pointer_phase(Some(down), moved, 0.0, true),
            Phase::Pending(moved)
        );
    }

    /// Which gestures take that path is the pressing pointer's answer, not
    /// the device's. `(pointer: fine)` describes the primary pointing
    /// device: it is true of a touchscreen laptop, whose finger would then
    /// never get a long press, and false of a tablet with a mouse plugged
    /// in, whose mouse would get one it never asked for.
    #[test]
    fn a_gesture_is_coarse_by_its_own_pointer_and_not_by_the_device() {
        assert!(!coarse_pointer("mouse"));
        assert!(coarse_pointer("touch"));
        // a pen is coarse here though its tip is not: what the coarse path
        // gives is the long press, and only a mouse arrives having hovered
        assert!(coarse_pointer("pen"));
        // an unnamed pointer takes the long press too. It costs a mouse
        // nothing it can see, and its absence costs a finger the only way
        // it has to aim
        assert!(coarse_pointer(""));
        // the press reads its own pointer once and keeps the answer; the
        // media query is left to the hover gate, which is the one place
        // that asks about the device at all
        let source = include_str!("chart.rs");
        let asked = ["coarse_pointer(&pointer.pointer", "_type())"].concat();
        assert!(source.contains(&asked), "a press must read its own pointer");
        let gate = ["self.hover_", "peeks()"].concat();
        assert_eq!(
            source.matches(&gate).count(),
            1,
            "the hover gate is the only thing that may ask the device"
        );
    }

    #[test]
    fn a_pending_drag_returned_to_where_it_started_says_it_will_cancel() {
        // a drag that has been away from its origin: only such a press can
        // come back to it, and a long press that never moved is covered by
        // its own test
        let down = Down {
            x0: 200.0,
            pending: true,
            released: false,
            travelled: true,
        };
        let press = |x: f64| pointer_phase(Some(down), x, 1.0, false);
        // inside the snap-back band, exactly on it, and outside it
        assert_eq!(press(200.0 + 3.9), Phase::WillCancel(200.0 + 3.9));
        assert_eq!(
            press(200.0 + SNAPBACK_PX),
            Phase::WillCancel(200.0 + SNAPBACK_PX)
        );
        let out = 200.0 + 4.1;
        assert_eq!(press(out), Phase::Pending(out));
        // and the same on the other side of the origin
        assert_eq!(
            press(200.0 - SNAPBACK_PX),
            Phase::WillCancel(200.0 - SNAPBACK_PX)
        );
        let back = 200.0 - 4.1;
        assert_eq!(press(back), Phase::Pending(back));
        // on a coarse pointer too: the snap-back is not the mouse's alone
        assert_eq!(
            pointer_phase(Some(down), 200.0, 9.0, true),
            Phase::WillCancel(200.0)
        );
        // the band that cancels a drag is wider than the one that starts
        // it, so no position can sit on both thresholds at once
        const {
            assert!(DRAG_PX < SNAPBACK_PX);
        }
        // a press that has not become a seek yet has no snap-back to fall
        // into: back at its origin it is still the tap it always was
        assert_eq!(
            pointer_phase(
                Some(Down {
                    pending: false,
                    ..down
                }),
                200.0,
                1.0,
                false
            ),
            Phase::Idle
        );
    }

    /// Decision 19: the snap-back is a state held until the release, so a
    /// scrub that crosses its own start is one gesture and not two. The
    /// element carries the press across all of it; these are the phases it
    /// reads at each step.
    #[test]
    fn a_press_that_never_left_its_origin_is_not_a_change_of_mind() {
        // a long press aims without moving, so it sits in the snap-back band
        // from the first instant: releasing it must seek, not cancel
        let held = Down {
            x0: 100.0,
            pending: true,
            released: false,
            travelled: false,
        };
        assert_eq!(
            pointer_phase(Some(held), 100.0, 1.0, true),
            Phase::Pending(100.0)
        );
        let released = Down {
            released: true,
            ..held
        };
        assert_eq!(
            pointer_phase(Some(released), 100.0, 1.0, true),
            Phase::Commit(100.0),
            "a held press that never moved commits where it was held"
        );
        // the same press, once it has been away and come back, does cancel
        let returned = Down {
            travelled: true,
            ..held
        };
        assert_eq!(
            pointer_phase(Some(returned), 100.0, 1.0, true),
            Phase::WillCancel(100.0)
        );
        assert_eq!(
            pointer_phase(
                Some(Down {
                    released: true,
                    ..returned
                }),
                100.0,
                1.0,
                true
            ),
            Phase::Cancel
        );
    }

    #[test]
    fn a_drag_across_its_own_origin_resumes_and_only_the_release_cancels() {
        let x0 = 200.0;
        let mut down = Down {
            x0,
            pending: false,
            released: false,
            travelled: false,
        };
        // the press records itself through `advance`, once before the phase
        // is asked and once with the answer, which is what the element
        // does: the rule is the element's own here and not a copy of it
        let mut step = |x: f64, released: bool| {
            down.released = released;
            advance(&mut down, x, None);
            let phase = pointer_phase(Some(down), x, 1.0, false);
            advance(&mut down, x, Some(phase));
            phase
        };
        // aim out, come back over the origin, aim out the other way, let go
        assert_eq!(step(x0 + 40.0, false), Phase::Pending(x0 + 40.0));
        assert_eq!(step(x0, false), Phase::WillCancel(x0));
        assert_eq!(step(x0 - 40.0, false), Phase::Pending(x0 - 40.0));
        assert_eq!(step(x0 - 40.0, true), Phase::Commit(x0 - 40.0));
        // the same drag let go inside the band instead: that is the cancel,
        // and it happens on the release rather than on the way past
        let mut down = Down {
            x0,
            pending: true,
            released: false,
            travelled: true,
        };
        assert_eq!(
            pointer_phase(Some(down), x0 + 1.0, 1.0, false),
            Phase::WillCancel(x0 + 1.0)
        );
        down.released = true;
        assert_eq!(
            pointer_phase(Some(down), x0 + 1.0, 1.0, false),
            Phase::Cancel
        );
        // a hair outside the band, the same release commits: the two
        // outcomes differ by where the pointer was let go and nothing else
        assert_eq!(
            pointer_phase(Some(down), x0 + SNAPBACK_PX + 0.1, 1.0, false),
            Phase::Commit(x0 + SNAPBACK_PX + 0.1)
        );
        // and a tap is untouched by any of it: never pending, so its
        // release commits where it started
        assert_eq!(
            pointer_phase(
                Some(Down {
                    x0,
                    pending: false,
                    released: true,
                    travelled: false
                }),
                x0,
                0.02,
                false
            ),
            Phase::Commit(x0)
        );
        // the travel `advance` records is that same band, read strictly: on
        // the edge the press has not been away, a hair beyond it has. The
        // drag above crosses at 40 px and would survive a comparison
        // relaxed to `>=`, or a band of another width, so both sides of it
        // are asked for here
        for (x, away) in [
            (x0 + SNAPBACK_PX, false),
            (x0 - SNAPBACK_PX, false),
            (x0 + SNAPBACK_PX + 0.1, true),
            (x0 - SNAPBACK_PX - 0.1, true),
        ] {
            let mut edge = Down {
                x0,
                pending: true,
                released: false,
                travelled: false,
            };
            advance(&mut edge, x, None);
            assert_eq!(
                edge.travelled,
                away,
                "{} px from the origin",
                (x - x0).abs()
            );
        }
        // the element's own press goes through the same two calls: without
        // them the flags above are never set on a real gesture and the
        // snap-back is dead in the browser with this test still green
        let source = include_str!("chart.rs");
        let call = ["advance(&mut press", ".down, x, "].concat();
        assert_eq!(
            source.matches(&call).count(),
            2,
            "resolve must record its press before the phase and after it"
        );
        // and the arm that reads a release inside the band must act on it.
        // Deleting the call leaves the snap-back deciding to cancel and
        // doing nothing: the pending and cancelling states stay set, the
        // preview freezes and the capture is never given back, all of it
        // invisible to a test that asks only for the phase
        let arm = ["Phase::Cancel =>", " self.act(Intent::Cancel)"].concat();
        assert_eq!(
            source.matches(&arm).count(),
            1,
            "the cancel phase must end the press through the cancel intent"
        );
    }

    /// One press at a time, each event answered by the pointer that made
    /// it. A second pointer going down takes the press over, and the first
    /// one's moves and its release would otherwise be read against an
    /// origin that pointer never had.
    #[test]
    fn a_press_ignores_every_pointer_but_its_own() {
        // no press stored: nothing to disagree with, so a hover is never
        // ignored
        assert!(!should_ignore(None, 1));
        assert!(!should_ignore(Some(1), 1));
        assert!(should_ignore(Some(1), 2));
        // an id is the browser's own number, and it may be zero or negative
        assert!(!should_ignore(Some(-3), -3));
        assert!(should_ignore(Some(0), -3));
    }

    #[test]
    fn a_second_press_mid_drag_neither_inherits_the_first_nor_commits_at_it() {
        let (a_id, b_id) = (1, 2);
        // finger A presses at 100 and drags to 140: a pending seek, and one
        // that has left the snap-back band
        let mut a = Down {
            x0: 100.0,
            pending: false,
            released: false,
            travelled: false,
        };
        advance(&mut a, 140.0, None);
        let phase = pointer_phase(Some(a), 140.0, 0.0, true);
        advance(&mut a, 140.0, Some(phase));
        assert_eq!(phase, Phase::Pending(140.0));
        assert!(a.pending && a.travelled);
        // finger B goes down at 500 while A is still down. One press is
        // kept, so B's replaces A's and starts clean
        let b = Down {
            x0: 500.0,
            pending: false,
            released: false,
            travelled: false,
        };
        // what reading every pointer's events would do: A lifting at 150
        // releases the press that B owns and commits a seek 350 px from
        // where B went down, at a position B never visited, having
        // recorded A's travel against B on the way
        let mut confused = b;
        confused.released = true;
        advance(&mut confused, 150.0, None);
        assert_eq!(
            pointer_phase(Some(confused), 150.0, 0.0, true),
            Phase::Commit(150.0)
        );
        assert!(confused.travelled);
        // so A's move and A's release are refused: they are not this
        // press's
        assert!(should_ignore(Some(b_id), a_id));
        // and B's own release, where B is, is the one that resolves
        let mut mine = b;
        mine.released = true;
        assert!(!should_ignore(Some(b_id), b_id));
        assert_eq!(
            pointer_phase(Some(mine), 500.0, 0.0, true),
            Phase::Commit(500.0)
        );
        // the wiring is not reachable from a native test: there is no DOM
        // here to dispatch a second `pointerdown` into. All three readers
        // are pinned by the source instead, and so is the press being ended
        // rather than dropped, which is what gives back its capture, its
        // wake-up and its states. The cancel is the third: a `pointercancel`
        // for A, which the browser sends whenever it takes a touch for a
        // scroll, would otherwise end the gesture B still has a finger on
        let source = include_str!("chart.rs");
        let guard = ["should_ignore(held, pointer.pointer", "_id())"].concat();
        assert_eq!(
            source.matches(&guard).count(),
            3,
            "a move, an up and a cancel must each answer to their own pointer"
        );
        // and the cancel gesture must reach that reader: a closure that
        // throws the event away has no id to disagree with, and the guard
        // above would still count three with the handler left unreachable
        let at = source
            .find(&["\"pointer", "cancel\","].concat())
            .expect("the cancel gesture");
        let wiring = &source[at..];
        let end = wiring.find("pointerleave").unwrap_or(wiring.len());
        assert!(
            wiring[..end].contains(&["f.on_", "cancel(&e)"].concat()),
            "the cancel gesture must go through the handler that reads its pointer"
        );
        let at = source
            .find(&["fn on_", "down(&self"].concat())
            .expect("the down handler");
        let after = &source[at..];
        let end = after
            .find(&["fn on_", "move(&self"].concat())
            .unwrap_or(after.len());
        assert!(
            after[..end].contains(&["self.end_", "press();"].concat()),
            "a press taken over must be ended and not dropped"
        );
    }

    /// The three event names and the one detail field a listener binds
    /// to. They are the element's public surface, so they are pinned here
    /// rather than left to a rename.
    #[test]
    fn the_intents_are_three_named_events_carrying_one_time_field() {
        assert_eq!(SEEK_EVENT, "opt-chart-seek");
        assert_eq!(PEEK_EVENT, "opt-chart-peek");
        assert_eq!(TOGGLE_EVENT, "opt-chart-toggle");
        // the seek and the peek carry `{ time }`, spelled as the film's
        // own clock event spells it, so one reader serves both
        assert_eq!(TIME_FIELD, "time");
        let names = std::collections::BTreeSet::from([SEEK_EVENT, PEEK_EVENT, TOGGLE_EVENT]);
        assert_eq!(names.len(), 3);
        for name in names {
            assert!(name.starts_with("opt-chart-"), "{name}");
        }
    }

    /// Decision 19 as amended on 2026-09-04: the hover peek is gated on
    /// hovering, and precision alone is the wrong half of the capability.
    /// A pen-driven screen answers `pointer: fine` and cannot hover at all,
    /// so the old gate offered it a peek it can never take.
    #[test]
    fn the_hover_peek_is_gated_on_hovering_and_not_on_precision_alone() {
        assert_eq!(HOVER_QUERY, "(hover: hover) and (pointer: fine)");
        // both halves are asked, and of the primary device: `any-hover`
        // would serve a mouse docked to a tablet at the price of peeking
        // on a touchscreen laptop, where a tap synthesises the move
        assert!(HOVER_QUERY.contains("hover: hover") && HOVER_QUERY.contains("pointer: fine"));
        assert!(!HOVER_QUERY.contains("any-hover") && !HOVER_QUERY.contains("any-pointer"));
        // and the element asks the browser this, from this constant, as a
        // live query, so a device that docks a mouse is followed
        let source = include_str!("chart.rs");
        let call = ["match", "_media("].concat();
        assert!(
            source.contains(&format!("{call}HOVER_QUERY)")),
            "the gate must be asked from the constant"
        );
    }

    /// The custom states the element writes, which a page styles and the
    /// interaction report witnesses (decision 5).
    #[test]
    fn the_element_names_the_states_it_sets() {
        assert_eq!(PEEKING_STATE, "peeking");
        assert_eq!(PENDING_STATE, "pending");
        assert_eq!(CANCELLING_STATE, "cancelling");
        // they join the three phase 2 already writes, and all six differ
        let states = std::collections::BTreeSet::from([
            PEEKING_STATE,
            PENDING_STATE,
            CANCELLING_STATE,
            "following",
            "playing",
            "hydrated",
        ]);
        assert_eq!(states.len(), 6);
        // and the cancelling one is written in the arm that decides it. It
        // is the snap-back's whole visible answer - a page styles it and
        // the report witnesses it - so an arm that decided a will-cancel
        // and wrote nothing would cancel silently
        let source = include_str!("chart.rs");
        let arm = ["Phase::WillCancel(x) =>", " {"].concat();
        let at = source.find(&arm).expect("the will-cancel arm");
        let body = &source[at + arm.len()..];
        let end = body.find("Phase::").unwrap_or(body.len());
        let write = ["set_state(&self.host, CANCELLING", "_STATE, true)"].concat();
        assert!(
            body[..end].contains(&write),
            "the will-cancel arm must set the cancelling state"
        );
    }
}
