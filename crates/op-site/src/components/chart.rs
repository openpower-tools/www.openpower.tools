//! `<opt-chart for="film">`: a time-series chart that follows a film's
//! clock. It never advances time itself; it is a projection of the film's
//! timeline, as the machine diagram is, so the only thing that moves per
//! tick is the playhead group, its readout and the played bar.
//!
//! The markup is built by pure functions (`shadow_markup`,
//! [`prerender`]) that the page build calls natively, so a page ships the
//! chart finished: the SVG, a caption with a one-paragraph summary and a
//! line naming the keys, and the data as a real table behind a
//! disclosure. Nothing of that needs a script, which is what keeps the
//! chart complete where WebAssembly is unavailable (Firefox on ppc64le).
//!
//! Decision 15's structure is that markup and the host's own internals:
//! the host is a `group` named by the caption's heading and described by
//! the summary and the key line, the SVG is a `graphics-document` named by
//! the same heading, the drawing that only decorates it is hidden from the
//! accessibility tree, each series and each cue is exposed as itself, and
//! the playhead is the slider. Its value is the film's clock, written by
//! the same tick that moves it. A chart inside a film's own shadow tree
//! keeps all of that but the slider: the film's native range input is the
//! one control there, so the chart's thumb is decoration and is hidden
//! from the accessibility tree (decision 15).
//!
//! What is announced is decision 18's. The thumb's `aria-valuetext` says
//! the time in words with the chapter appended, adding the whole duration
//! and the frame index whenever the thumb has not got the focus, so they
//! are spoken once when it is focused and left out of every step after
//! that; clock-driven writes wait for the focus and a 300 ms debounce, and
//! a key or a pointer seek writes at once. A polite status region sits in
//! the shadow root from the first render, empty, and receives one short
//! message on a committed seek, on play, on pause, and on a chapter
//! entered while the film plays. A peek and a seek being aimed announce
//! nothing.
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
//! it started and letting go there. Tab reaches the playhead that is the
//! chart's slider, then the data table's disclosure, and the element
//! answers decision 17's key table while the focus is anywhere in the
//! chart itself. Which node the root delegates a focus to is the engine's
//! own choice and is not the thumb, so an arrival on the svg or on the
//! host is moved onto it here ([`takes_the_stop`]). A re-layout draws a
//! new thumb and gives it the focus the old one had, so a chart that
//! reflows under a reader's hands does not turn them out of it.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use op_chart::{Data, Layout};
use op_webc::{CustomElement, ElementDefinition, set_state};
use wasm_bindgen::prelude::*;
use web_sys::{Element, Event, HtmlElement, ShadowRoot};

use super::BASE_CSS;
use super::chart_style::{CHART_CUE_CSS, CHART_SHAPE_CSS, chart_rules};
use super::machine_diagram::film_time_of;
use crate::html::escape;

pub const DEFINITION: ElementDefinition = ElementDefinition {
    source: op_webc::here!(),
    tag: "opt-chart",
    // decision 1's four primitives, which are everything an attribute may
    // carry here: the rich data is a block and a property. `step` is
    // decision 17's key unit in seconds ([`step_attr`]), read where it is
    // used so an attribute set after the upgrade takes effect at the next
    // key rather than at the next render
    observed_attributes: &["for", "initial-width", "ratio", "step"],
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
/// are dropped: they cannot be read at that size and they overprint. The
/// cue's button goes with its tick, so a chapter that is not drawn is not
/// named to a reader and not hittable by a pointer either.
const DROP_AT: f64 = 480.0;
/// What a chapter cue's rect and its button both carry in `data-cue`, and
/// what the rule that drops them below [`DROP_AT`] names them by. One
/// spelling, so the rule that hides a cue and the set the roving walks
/// ([`cue_is_drawn`]) cannot come to mean different cues.
const CHAPTER_CUE: &str = "chapter";

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

/// A clock reading as this project draws one, on the chart's playhead and
/// on the film's own labels: seconds to a hundredth, which is finer than
/// any film it draws is sampled. It is drawn, not spoken; [`in_words`] is
/// what a listener hears.
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
        parts.push(format!(
            "values from {} to {}",
            op_chart::announced(lo),
            op_chart::announced(hi)
        ));
    }
    let takeaway = takeaway_of(d);
    format!("{}.{takeaway}", parts.join("; "))
}

/// Decision 15's takeaway: the sentence that says what the chart shows
/// rather than what it contains.
///
/// A generated summary cannot have an opinion, so this states the one
/// thing the samples decide on their own: where each series ended up
/// against where it started. A reader who is being told rather than shown
/// gets the shape of the line, which is what a sighted reader takes from
/// the picture in a glance and what the list of counts above never says.
///
/// Empty where the block gives it nothing to compare: a chart with no
/// series, or one whose series carry no samples, has no shape to report
/// and an invented one would be worse than none.
fn takeaway_of(d: &Data) -> String {
    let mut moves: Vec<String> = Vec::new();
    for (i, series) in d.series.iter().enumerate() {
        let present: Vec<f64> = d
            .rows
            .iter()
            .filter_map(|r| r.values.get(i).copied().flatten())
            .collect();
        let (Some(first), Some(last)) = (present.first(), present.last()) else {
            continue;
        };
        let unit = if series.unit.is_empty() {
            String::new()
        } else {
            format!(" {}", series.unit)
        };
        let label = if series.label.is_empty() {
            format!("series {}", i + 1)
        } else {
            series.label.clone()
        };
        let (from, to) = (op_chart::announced(*first), op_chart::announced(*last));
        moves.push(if first == last {
            format!("{label} holds at {from}{unit}")
        } else {
            let verb = if last > first { "rises" } else { "falls" };
            format!("{label} {verb} from {from} to {to}{unit}")
        });
    }
    if moves.is_empty() {
        String::new()
    } else {
        format!(" Overall, {}.", join(&moves))
    }
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

// ---- announcements ----------------------------------------------------

/// A time as a listener should hear it: "1 minute 32 seconds", not "92"
/// (decision 18).
///
/// Seconds carry a tenth where they have one. A film of a few seconds is
/// the case this chart is drawn for, and whole seconds there would give
/// every position on the track the same words; over a minute the tenth
/// falls out of its own accord, because a minute of film is not sampled
/// that finely.
pub(crate) fn in_words(t: f64) -> String {
    let tenths = (t.max(0.0) * 10.0).round() as i64;
    let minutes = tenths / 600;
    let rest = tenths % 600;
    let mut said: Vec<String> = Vec::new();
    if minutes > 0 {
        said.push(count(minutes as usize, "minute"));
    }
    // seconds are said whenever there are any, and on a whole minute they
    // are not said at all; a time of zero is "0 seconds" and not silence
    if rest > 0 || minutes == 0 {
        let number = if rest % 10 == 0 {
            format!("{}", rest / 10)
        } else {
            format!("{}.{}", rest / 10, rest % 10)
        };
        let noun = if rest == 10 { "second" } else { "seconds" };
        said.push(format!("{number} {noun}"));
    }
    said.join(" ")
}

/// What the status region has to say. Decision 18's whole list: a seek
/// that committed, play, pause, and a chapter entered while the film
/// plays. Nothing is ever said for a peek or for a seek still being aimed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Said {
    Seeked,
    Playing,
    Paused,
    Chapter,
}

/// The one short message the status region receives. `chapter` is the
/// title of the chapter `t` falls in, empty where the block names none and
/// then left out rather than said as an empty name.
pub(crate) fn message(said: Said, t: f64, chapter: &str) -> String {
    match said {
        Said::Seeked if chapter.is_empty() => format!("Seeked to {}", in_words(t)),
        Said::Seeked => format!("Seeked to {}, chapter {chapter}", in_words(t)),
        // play and pause say what happened and no more: where the clock is
        // stands in the thumb's own value, and repeating it here would be
        // the double announcement decision 18 is written against
        Said::Playing => "Playing".to_owned(),
        Said::Paused => "Paused".to_owned(),
        Said::Chapter if chapter.is_empty() => format!("Chapter at {}", in_words(t)),
        Said::Chapter => format!("Chapter {chapter}"),
    }
}

/// The time a committed seek announces: the clock's own answer where the
/// emit moved it, and the time asked for where it did not.
///
/// The two are the same on the docs page and differ wherever the announced
/// duration outlives the film. The chart clamps a key to
/// [`key_duration`], which is the block's `duration` or the axis end,
/// whichever is later; the film clamps to its own last frame. End on such
/// a block asks for a time the clock never reaches, and the status region
/// used to name the request while the thumb's value named the arrival.
/// Where the film is elsewhere in the document and answers later, there is
/// no arrival yet and the request is the only time there is to say.
pub(crate) fn seeked_time(asked: f64, before: f64, landed: f64) -> f64 {
    if landed == before { asked } else { landed }
}

/// What the thumb's value says beyond the time and the chapter: the whole
/// timeline, and which sample the time sits on.
///
/// It is written at initialisation and again on blur, so it is waiting to
/// be spoken when the thumb next gains focus, and is left out of every
/// change while the thumb has focus (decision 18): a listener stepping
/// along the track is told the total once, not on every step.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Whole {
    /// The announced timeline, which is what `aria-valuemax` carries.
    pub duration: f64,
    /// The sample the time sits on, counted from one.
    pub frame: usize,
    /// How many samples there are. No frame is named where there are none.
    pub frames: usize,
}

/// The thumb's `aria-valuetext`: the time in words with the chapter title
/// appended, and, where `whole` is given, the duration and the frame index
/// as well.
pub(crate) fn valuetext(t: f64, chapter: &str, whole: Option<Whole>) -> String {
    let mut said = match whole {
        Some(whole) => format!("{} of {}", in_words(t), in_words(whole.duration)),
        None => in_words(t),
    };
    if let Some(whole) = whole.filter(|whole| whole.frames > 0) {
        said.push_str(&format!(", frame {} of {}", whole.frame, whole.frames));
    }
    if !chapter.is_empty() {
        said.push_str(&format!(", chapter {chapter}"));
    }
    said
}

/// The chapter entered between `prev` and `now`, and [`None`] where no
/// boundary falls between them.
///
/// Forwards only, and by the shape of the question rather than by a guard
/// in front of it: a start can be past `prev` and at or before `now` only
/// where `now` is the later of the two, so a clock that stands still or
/// runs back crosses nothing. What this rate-limits is the status region,
/// which speaks on a boundary and never per sample (decision 18). A
/// boundary exactly at `prev` was already entered; one exactly at `now` has
/// just been. A tick that steps over several names the chapter it ends in.
pub(crate) fn chapter_crossed(prev: f64, now: f64, starts: &[f64]) -> Option<usize> {
    starts
        .iter()
        .rposition(|start| *start > prev + T_EPS && *start <= now + T_EPS)
}

/// How long a clock-driven `aria-valuetext` write waits behind the last
/// one: decision 18's 300 ms, in the seconds [`now`] counts.
pub(crate) const VALUETEXT_WAIT: f64 = 0.3;

/// Whether a clock tick may write the thumb's `aria-valuetext`: only while
/// the thumb has the focus, which is when its value is being spoken at
/// all, and only [`VALUETEXT_WAIT`] after the last write, so a playing film
/// does not queue a phrase per frame. A key or a pointer seek never asks
/// this: it writes at once.
pub(crate) fn valuetext_due(last: f64, now: f64, focused: bool) -> bool {
    focused && now - last >= VALUETEXT_WAIT
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
/// The id of the caption's one-paragraph summary.
const SUMMARY_ID: &str = "chart-summary";
/// The id of the visible line naming the keys the chart answers.
const KEYS_ID: &str = "chart-keys";
/// What the host is described by, in the order a reader hears it: what the
/// chart holds, and then how to drive it (decision 15).
const DESCRIBED_BY: [&str; 2] = [SUMMARY_ID, KEYS_ID];

/// The status region, after the svg and before the table where decision 15
/// puts it: in the shadow root from the first render and empty, never made
/// when a message arrives, because a live region has to be in the tree
/// before the text lands in it for the change to be announced at all.
/// Polite and never assertive, and out of the way visually as the film's
/// own is: what it holds is said, not read.
const STATUS: &str = "<div class=\"live\" role=\"status\" aria-live=\"polite\"></div>";

/// The element a chart would find as the host of the shadow tree it sits
/// in, where that tree is a film's.
const FILM_TAG: &str = "opt-film";

/// The chart's shadow root, attached with `delegatesFocus` where it has to
/// be attached at all: decision 17 wants one tab stop into a chart, so a
/// click on the host or a `focus()` on it belongs on the control and not
/// on the host.
///
/// Which node it is delegated to is the engine's to choose, and the engine
/// does not choose the thumb. A `focus()` on a host whose root holds this
/// drawing leaves the root's active element on the outermost svg, which
/// carries no `tabindex`, is not a tab stop and answers no keys, even
/// though the `role="slider"` group inside it is all three. So the
/// attribute says where a focus enters and never where it settles, and
/// [`takes_the_stop`] is what puts it on the control.
///
/// A pre-rendered chart is hydrated instead, and the root it is hydrated
/// into carries `shadowrootdelegatesfocus` from the template the build
/// wrote (`op-pages`), which is the only way that root can have it:
/// `attachShadow` on an element that already has a declarative shadow root
/// does not apply the init it was handed. It empties the root it found and
/// returns it, which would throw away the very markup this element is here
/// to keep, and would leave `delegatesFocus` as the parser set it either
/// way. So the two paths agree by each stating it at the one moment its
/// own root is made, and a pre-render written without the attribute cannot
/// be corrected after the fact: it is a template to redraw, not a flag to
/// set.
///
/// `delegatesFocus` is not settable through web-sys 0.3's
/// [`web_sys::ShadowRootInit`], which exposes `mode` alone; the dictionary
/// is a plain JS object, so the field is written onto it.
fn delegating_shadow_root(host: &HtmlElement) -> ShadowRoot {
    if let Some(existing) = host.shadow_root() {
        return existing;
    }
    let init = web_sys::ShadowRootInit::new(web_sys::ShadowRootMode::Open);
    let _ = js_sys::Reflect::set(
        &init,
        &JsValue::from_str("delegatesFocus"),
        &JsValue::from_bool(true),
    );
    host.attach_shadow(&init).expect("attach shadow root")
}

/// Whether this chart's thumb is the widget's slider, from the tag of the
/// element whose shadow tree the chart sits in ([`None`] for a chart in a
/// document, which is where every chart the build writes lands).
///
/// One slider per widget (decision 15). The film owns the time, and a
/// chart inside the film's own shadow tree is a second picture of the
/// timeline the film's native range input already controls, so the chart's
/// thumb is decoration there: no slider role, no tab stop, no value, and
/// hidden from the accessibility tree. Inside anything else - a page's own
/// wrapper element - the chart is still the only control over its
/// timeline and keeps the slider.
pub(crate) fn owns_slider(host_tag: Option<&str>) -> bool {
    !host_tag.is_some_and(|tag| tag.eq_ignore_ascii_case(FILM_TAG))
}

/// Which node this chart's focus is about, from [`owns_slider`]'s answer:
/// the thumb where the chart owns the widget's slider, and the svg where
/// it does not.
///
/// One answer, because three things have to agree about it and once did
/// not: the node a press moves the focus to, the node the record of the
/// focus is about, and so the node whose focus decides the words the
/// value takes (decision 18). It follows the markup, where the thumb
/// carries `tabindex="0"` and the svg gives up its own exactly when the
/// thumb is the slider, and a test holds the two together.
/// [`Follower::focus_target`] is where this becomes an element.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Stop {
    Thumb,
    Svg,
}

/// See [`Stop`].
pub(crate) const fn focus_stop(slider: bool) -> Stop {
    if slider { Stop::Thumb } else { Stop::Svg }
}

/// What the shadow root says about a focus that has just arrived in this
/// chart, as the three facts [`takes_the_stop`] turns into a decision.
/// [`Follower::arrival`] reads it out of the tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Arrival {
    /// The root has an active element at all. False is the focus on the
    /// host itself, which is where it stays when an engine asked to
    /// delegate finds nothing inside to delegate to.
    pub(crate) inside: bool,
    /// That element is the node this element's focus is about
    /// ([`focus_stop`]).
    pub(crate) stop: bool,
    /// It is the svg the drawing sits in.
    pub(crate) svg: bool,
}

/// Whether the element has to move a focus that has just arrived onto its
/// own stop.
///
/// `delegatesFocus` is a request and not a promise: the root is attached
/// with it ([`delegating_shadow_root`]) and the engine still picks the
/// delegate, which here is the outermost svg rather than the thumb inside
/// it. The widget cannot leave the focus where that lands, since the svg
/// is not a tab stop and the words the value takes are about the thumb
/// (decision 18), so it moves it.
///
/// Only where the engine chose. The two nodes it can leave a focus on are
/// the host, which a root reports by having no active element at all, and
/// the svg. Everywhere else in the root is somewhere a reader went on
/// purpose, a cue button during the roving or the data table's disclosure,
/// and taking a focus off those would be the widget arguing with the
/// person using it. The stop keeps the focus whichever node the stop is,
/// which is also what stops a chart inside a film, whose stop is that same
/// svg (decision 15), from chasing itself.
pub(crate) const fn takes_the_stop(at: Arrival) -> bool {
    !at.stop && (!at.inside || at.svg)
}

/// What the emitter's accessible structure is told about this chart
/// (decision 15): the caption's heading is the visible title that names
/// the svg and the thumb, each series is measured in the unit its block
/// gave, and the thumb is this widget's slider. A standalone chart owns
/// the one control that answers for its timeline; a chart inside the
/// film's shadow tree does not, and the film's own range input is the
/// slider there.
fn aria_of(data: &Data, slider: bool) -> op_chart::Aria {
    op_chart::Aria {
        title: TITLE_ID.to_owned(),
        units: data.series.iter().map(|s| s.unit.clone()).collect(),
        slider,
    }
}

/// The opening of the thumb group the emitter draws, which is the one
/// group carrying that part (a test in op-chart holds it to one).
const THUMB_OPEN: &str = "<g class=\"playhead\" part=\"playhead\"";

/// One chart, with the two hooks the element adds to op-chart's markup:
/// the class a container query switches on, in place of the emitter's own,
/// and the mark that says which side drew it. Everything else in the
/// opening tag is the emitter's, including decision 15's
/// `graphics-document` role and the caption heading it is named by, so the
/// role the chart carries is written in one place and not two.
fn svg_of(
    spec: &op_chart::Spec,
    aria: &op_chart::Aria,
    layout: Layout,
    class: &str,
    rendered_by: &str,
) -> String {
    let rendered = op_chart::render_with(spec, layout, aria).svg;
    // A thumb that is not the widget's slider is decoration, and
    // decoration is out of the accessibility tree. The emitter already
    // leaves the group with no role, no tab stop and no value; the
    // `aria-hidden` that says the playhead is a picture of the film's own
    // control, and not a second control, is written here (decision 15).
    let rendered = if aria.slider {
        rendered
    } else {
        rendered.replace(THUMB_OPEN, &format!("{THUMB_OPEN} aria-hidden=\"true\""))
    };
    const OPEN: &str = "<svg class=\"chart\"";
    let Some(rest) = rendered.strip_prefix(OPEN) else {
        return rendered;
    };
    format!(
        "<svg class=\"{}\" data-rendered-by=\"{}\"{rest}",
        escape(class),
        escape(rendered_by),
    )
}

/// The whole inner HTML of a chart's shadow root. With a `narrow` layout
/// the figure carries both pre-renders and the stylesheet chooses; with
/// none it carries the single chart the element just drew.
///
/// `slider` is [`owns_slider`]'s answer: it decides the thumb alone, and
/// everything else about the chart is the same either way.
pub(crate) fn shadow_markup(
    data: &Data,
    wide: Layout,
    narrow: Option<Layout>,
    ratio: f64,
    rendered_by: &str,
    slider: bool,
) -> String {
    let spec = data.to_spec();
    let aria = aria_of(data, slider);
    let mut charts = svg_of(
        &spec,
        &aria,
        wide,
        if narrow.is_some() {
            "chart wide"
        } else {
            "chart"
        },
        rendered_by,
    );
    if let Some(narrow) = narrow {
        charts.push_str(&svg_of(&spec, &aria, narrow, "chart narrow", rendered_by));
    }
    // the caption is the chart's name and its description in one place:
    // the heading names it, the paragraph says what it holds, and the line
    // after that says what it answers. All three are visible, which
    // decision 15 prefers to a hidden description, and the host points at
    // them through its internals.
    format!(
        "<style>{BASE_CSS}{}</style><figure class=\"chart\">{charts}<figcaption><strong class=\"title\" id=\"{TITLE_ID}\">{}</strong><p class=\"summary\" id=\"{SUMMARY_ID}\">{}</p><p class=\"keys\" id=\"{KEYS_ID}\">{}</p></figcaption></figure>{STATUS}{}",
        stylesheet(ratio),
        escape(&title_of(data)),
        escape(&summary_of(data)),
        escape(&instructions()),
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
        // the build writes a chart into a page's own tree, never into a
        // film's shadow tree, so a pre-rendered chart always owns its
        // slider; the embedded case can only arise at runtime, and the
        // element decides it there ([`owns_slider`])
        true,
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
/// The thumb is the chart's first tab stop, and it takes the ring the
/// emitter drew around it, 2 px outside its target, rather than an
/// outline: an outline on an SVG node is laid out in user space, so it
/// scales with the viewBox and is clipped by the viewport, and the 2 px
/// the criterion asks for is whatever the box happens to be drawn at
/// (decision 20). No rule here puts an outline on any node of the drawing;
/// the site's own `:focus-visible` ring, which reaches every element in
/// the tree, is turned off on the thumb for the same reason and the ring
/// stands in its place. The ring is `stroke: currentColor` over a `color`
/// of the focus token: a forced palette adjusts `color` and leaves SVG
/// paints alone, so the ring follows the forced text colour with no
/// mapping of its own, where `stroke: var(--op-focus)` would keep the
/// theme's colour on a forced one. Decision 20 asks for a stronger stroke
/// on the thumb as well as the ring, which is the `head` rule beneath it.
///
/// The cue buttons take the same treatment for the same reason, and they
/// need it more: they carry no class at all, so the site's ring was the
/// only indicator they had ([`cue_indicator_css`]).
///
/// Below [`DROP_AT`] the chapter ticks are hidden, and the container
/// query takes their buttons with them: a cue that is not drawn is not
/// named and not hittable either, where hiding the ticks alone left an
/// invisible chapter with a 24 px target and an announced button.
///
/// The svg carries no focus rules at all now. Where this chart owns its
/// slider the svg is not a tab stop and never has the focus; where it does
/// not, the svg is the only thing here a keyboard can reach and takes the
/// site's own ring, which is the same 2 px of the same token this
/// stylesheet used to write out a second time.
///
/// `touch-action: pan-y` leaves the page its vertical scroll and claims
/// the horizontal drag, which is the one this element reads.
pub(crate) fn stylesheet(ratio: f64) -> String {
    let rules = chart_rules();
    let cues = cue_indicator_css();
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
.live {{ position: absolute; width: 1px; height: 1px; overflow: hidden; clip: rect(0 0 0 0); white-space: nowrap; }}
svg.chart {{ display: block; width: 100%; height: auto; aspect-ratio: var(--op-chart-ratio, {ratio}); font-family: var(--op-font-sans); font-size: 12px; touch-action: pan-y; }}
svg.chart.narrow {{ display: none; }}
{CHART_SHAPE_CSS}
.chart .band, .chart .peek-band {{ fill: var(--op-band); fill-opacity: 0.5; }}
.chart .bar-bg {{ fill: var(--op-border); }} .chart .bar-played {{ fill: var(--op-band); }}
{CHART_CUE_CSS}
.chart .head {{ stroke: var(--op-playhead); stroke-width: 1.5; }} .chart .head-dot {{ fill: var(--op-playhead); }}
.chart .head-t {{ fill: var(--op-playhead); font-weight: 700; paint-order: stroke; stroke: var(--op-surface); stroke-width: 4; }}
.chart .playhead:focus {{ outline: none; }}
.chart .playhead:focus-visible .head-ring {{ color: var(--op-focus); stroke: currentColor; stroke-width: 2; }}
.chart .playhead:focus-visible .head {{ stroke-width: 2.5; }}
{cues}
{rules}
figcaption {{ margin-top: 0.4rem; font-size: 0.9rem; }}
figcaption .title {{ display: block; font-family: var(--op-font-heading); }}
figcaption .summary, figcaption .keys {{ color: var(--op-muted); margin: 0.2rem 0 0; }}
details.data {{ margin-top: 0.4rem; font-size: 0.85rem; }}
details.data summary {{ cursor: pointer; color: var(--op-muted); }}
details.data table {{ border-collapse: collapse; margin-top: 0.4rem; font-variant-numeric: tabular-nums; }}
details.data th, details.data td {{ border-bottom: 1px solid var(--op-border); padding: 0.1rem 0.5rem 0.1rem 0; text-align: left; }}
@container (max-width: {NARROW_AT}px) {{ svg.chart.wide {{ display: none; }} svg.chart.narrow {{ display: block; }} }}
@container (max-width: {DROP_AT}px) {{ .tick-label.alt {{ display: none; }} .chapters {{ display: none; }} .chart .targets g[data-cue=\"{CHAPTER_CUE}\"] {{ display: none; }} }}"
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

/// Whether an intent ends the press that is in flight.
///
/// A seek commits at once, whatever aimed it: a key seek therefore never
/// leaves a press aiming and never enters the pending state, which is
/// decision 17's answer, and a pointer seek is the
/// release of the press it ends. A cancel drops the press. A peek and a
/// toggle leave it exactly as it was, so a scrub that crosses its own
/// origin is one gesture throughout.
pub(crate) fn ends_press(intent: Intent) -> bool {
    matches!(intent, Intent::Seek(_) | Intent::Cancel)
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
    /// The `step` attribute, in seconds, where the element carries a
    /// usable one ([`step_attr`]). It overrides the sample step alone
    /// (decision 17): comma and full stop, the bare arrows and J and L
    /// count in it instead of in samples, and every other key is
    /// untouched. The chapter keys walk the chapters, Home and End and the
    /// digits are fractions of the announced duration, and Shift with an
    /// arrow was already a second and is not a sample step at all.
    pub step: Option<f64>,
}

/// The `step` attribute as seconds, and [`None`] where the element carries
/// none or carries nonsense. A step must be a finite positive number of
/// seconds: an empty attribute, a word, a negative and an infinity all
/// leave the keys counting in samples rather than moving the playhead to
/// somewhere no arithmetic can name.
pub(crate) fn step_attr(value: Option<&str>) -> Option<f64> {
    value
        .and_then(|v| v.trim().parse::<f64>().ok())
        .filter(|v| v.is_finite() && *v > 0.0)
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

    /// `n` steps along from `t`, held at either end by the caller's clamp.
    /// A step is one sample, or `step` seconds where the element states one
    /// (decision 17): a chart whose rows are irregular, or which carries no
    /// rows at all, can then say what a key is worth. Nothing at all when
    /// nothing was sampled and nothing was stated: there is no sample to
    /// step onto and no distance to step by.
    fn stepped(&self, t: f64, n: i64) -> Option<f64> {
        if let Some(step) = self.step {
            return Some(n as f64 * step + t);
        }
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

/// The keys the instructions line names: for each clause, the key values
/// [`key_intent`] answers for it and the words the line spells it with.
///
/// One table, so the visible line and the key handling cannot drift apart.
/// A second hand-written list would go stale the first time a key moved,
/// and a reader would be told to press something the chart refuses;
/// `the_instructions_line_names_every_key_the_element_answers_and_no_other`
/// holds the two ends together by walking a keyboard's worth of keys
/// through [`key_intent`].
const KEY_LINE: &[(&[&str], &str)] = &[
    (&[",", "."], "comma and full stop step one sample"),
    (&["ArrowLeft", "ArrowRight"], "the arrow keys five"),
    (&["j", "J", "l", "L"], "J and L ten"),
    (
        &["ArrowLeft", "ArrowRight"],
        "Shift with an arrow one second",
    ),
    (
        &["PageUp", "PageDown"],
        "Page Up and Page Down, or Ctrl with an arrow, step by chapter",
    ),
    (&["Home", "End"], "Home and End go to either end"),
    (
        &["0", "1", "2", "3", "4", "5", "6", "7", "8", "9"],
        "a digit seeks to that tenth",
    ),
    (&[" ", "k", "K"], "Space or K plays and pauses"),
    (&["Escape"], "Escape drops a seek being aimed"),
];

/// The roving's own keys in words, in [`KEY_LINE`]'s shape: the entry
/// gesture above all, which decision 17 leaves unstated and which a reader
/// cannot guess. The key table never sees any of these while a cue has the
/// focus, and [`roving_key`] answers them; the same test that holds
/// [`KEY_LINE`] to the key table holds this to that function.
const ROVE_LINE: &[(&[&str], &str)] = &[
    (
        &["ArrowDown"],
        "Down from the playhead reaches the marks and the chapters",
    ),
    (&["ArrowLeft", "ArrowRight"], "the arrows step between them"),
    (
        &["Enter", " "],
        "Enter or Space seeks to the one that has the focus",
    ),
    (
        &["ArrowUp", "Escape"],
        "Up or Escape returns to the playhead",
    ),
];

/// The visible line naming the keys, which decision 15 asks for beside the
/// summary: Chartability rates missing instructions as critical, and a
/// sighted keyboard user needs them as much as a screen reader does, so
/// this is text on the page and not a hidden description.
///
/// Two sentences, because there are two places to stand: the thumb, which
/// moves the clock, and a cue, which does not. The second is where the
/// entry gesture is documented, since a set nothing announces the way into
/// is a set no keyboard reader finds.
fn instructions() -> String {
    let clauses: Vec<&str> = KEY_LINE.iter().map(|(_, words)| *words).collect();
    let roving: Vec<&str> = ROVE_LINE.iter().map(|(_, words)| *words).collect();
    format!("Keys: {}. {}.", clauses.join("; "), roving.join("; "))
}

/// The intent a key press carries, [`None`] for a key the chart does not
/// answer. Decision 17's whole set: the film's own table, less the speed
/// keys the film owns, plus the digits. [`KEY_LINE`] is the same set in
/// words, and a test holds them together.
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
        ctx.stepped(at, n)
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

/// The cue buttons the emitter draws, which are the marks and the chapter
/// ticks: one `role="button"` apiece, each holding the hit rect that names
/// its time. Asked of the svg, which is the drawing itself. The film reads
/// its own chart's cues back by these same words.
pub(crate) const CUE_BUTTONS: &str = ".targets g[role=\"button\"]";

/// The same buttons as a stylesheet has to name them: a rule is written
/// against the whole shadow tree and has to say which drawing it means,
/// where the query above is already asked of one. They are named by their
/// role because they carry no class: a class on a cue would be the class
/// the drawn rule or tick is painted by, and a class of the button's own
/// is what the z-ordered groups are read by. A test holds the two
/// spellings together.
pub(crate) const CUE_RULE: &str = ".chart .targets g[role=\"button\"]";

/// Decision 20's indicator on a cue button, which both stylesheets that
/// draw cues include whole ([`stylesheet`] and the film's `chart_css`).
///
/// [`BASE_CSS`] is inlined above every drawing and its `:focus-visible`
/// reaches the emitter's cue buttons; an outline on an SVG node is laid
/// out in user space, so it scales with the viewBox and is clipped by the
/// viewport, which here is a hairline around a `<g>` at either end of the
/// track. So the ring is turned off and the indicator is a stroke on the
/// hit rect that is already there, painted off `color` so a forced palette
/// adjusts it with the text. Hover and focus paint the same stroke off the
/// same token, which is the same decision's focus mirrors hover.
///
/// One function rather than a copy apiece: a reader who meets a cue in a
/// chart and in a film meets one indicator, and the day it changes it
/// changes in both.
pub(crate) fn cue_indicator_css() -> String {
    format!(
        "{CUE_RULE}:focus {{ outline: none; }}
{CUE_RULE}:hover {{ color: var(--op-accent); }}
{CUE_RULE}:focus-visible {{ color: var(--op-focus); }}
{CUE_RULE}:hover .target, {CUE_RULE}:focus-visible .target {{ stroke: currentColor; stroke-width: 2; }}"
    )
}

/// Where the focus stands in decision 17's roving set: on the thumb, or on
/// the cue at this position in time order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Roving {
    Thumb,
    Cue(usize),
}

/// What a key does while the roving set has the focus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Roved {
    /// Take the tab stop and the focus to this member of the set.
    To(Roving),
    /// Seek to the focused cue's own time. A `role="button"` answers Enter
    /// and Space or it is not a button, and Space on a cue reached the
    /// key table before this and played the film.
    Press,
}

/// Decision 17's roving tabindex, as the decision behind it: where the
/// focus goes from where it is, given the key, and [`None`] for every key
/// that is somebody else's. `count` is how many cues the drawing has.
///
/// One tab stop moves over the whole set. Down from the thumb is the entry
/// gesture the decision leaves unstated, and it is free: no key in
/// [`KEY_LINE`] answers either vertical arrow, and the chart's timeline
/// runs along x, so down is out of the clock's own axis and into the
/// things standing on it. Up and Escape both leave, because a reader who
/// stepped in by one of them will try the other.
///
/// Between the cues the list never wraps: an arrow at either end leaves
/// the focus where it is, so the reader learns the end of the timeline by
/// the focus not moving rather than by arriving at the other end of it.
///
/// Tab is not here and never can be: decision 17 says a roving tabindex
/// never intercepts it, and one that did would shut the reader inside the
/// cues. Escape on the thumb is not here either: there it is the key that
/// drops a seek being aimed, which is the key table's answer.
pub(crate) fn roving_key(at: Roving, count: usize, key: &str) -> Option<Roved> {
    let to = |where_: Roving| Some(Roved::To(where_));
    match (at, key) {
        // in, at the first cue along the timeline, where there is one
        (Roving::Thumb, "ArrowDown") => (count > 0).then_some(Roved::To(Roving::Cue(0))),
        // and out, by either key that means "back where I came from"
        (Roving::Cue(_), "ArrowUp" | "Escape") => to(Roving::Thumb),
        (Roving::Cue(i), "ArrowRight") if i + 1 < count => to(Roving::Cue(i + 1)),
        (Roving::Cue(i), "ArrowLeft") if i > 0 => to(Roving::Cue(i - 1)),
        (Roving::Cue(_), "Enter" | " ") => Some(Roved::Press),
        _ => None,
    }
}

/// Whether the drawing shows this cue at this container width, which is
/// whether a reader can reach it at all: the roving walks what is drawn
/// and not what the markup carries.
///
/// The stylesheet drops every chapter cue below [`DROP_AT`], its button
/// with its tick, and a `display: none` node refuses the focus. Counting
/// one took the element's only tab stop somewhere nobody can stand: the
/// stop left the thumb, the hidden chapter took it, and the focus that
/// should have followed went nowhere, so the widget stopped being
/// reachable until something else restored a stop.
///
/// The threshold is consulted rather than the layout. This answer and the
/// rule are written from the one constant and the one spelling, so they
/// cannot come to disagree, where a per-cue rect read would ask the engine
/// the same question in a second language and would force a synchronous
/// layout inside the re-layout that calls it ([`Follower::capture`]) as
/// well as on every key press. The width is the host's own, which is the
/// box `container-type: inline-size` makes the query container, and it is
/// the measurement this element already draws itself from
/// ([`Follower::width`]).
pub(crate) fn cue_is_drawn(kind: &str, width: f64) -> bool {
    kind != CHAPTER_CUE || width > DROP_AT
}

/// Where the one tab stop lands, given how many cues the drawing shows and
/// which of them the caller asked for, with [`None`] for the element's own
/// stop and for a node the set does not hold.
///
/// A cue the drawing does not show cannot be focused, so it cannot hold
/// the stop either: the write would put the element's only `tabindex="0"`
/// on a node that then refuses the focus meant to follow it. The stop goes
/// to the thumb instead, which is the one member of the set that is always
/// drawn, so a set that shrank between the read and the write lands
/// somewhere a reader can stand rather than nowhere.
pub(crate) fn stop_lands(drawn: usize, want: Option<usize>) -> Roving {
    match want {
        Some(i) if i < drawn => Roving::Cue(i),
        _ => Roving::Thumb,
    }
}

/// The cues in time order, as positions in the order the emitter wrote
/// them. The markup carries every mark first and every chapter tick after,
/// each group in its own order, so document order stops being time order
/// the moment a chart has both kinds; the roving walks a timeline, so the
/// order it walks is made here. Equal times keep the order they were
/// written in, which is a stable sort's promise.
pub(crate) fn in_time_order(times: &[f64]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..times.len()).collect();
    order.sort_by(|a, b| times[*a].total_cmp(&times[*b]));
    order
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
    /// The status region, which sits beside the figure rather than in it
    /// and so outlives every re-layout.
    live: Option<Element>,
}

/// One cue button in the drawing as the element reads it back: what its
/// own hit rect says it stands for, and the node the focus goes on.
struct Cue {
    /// `data-cue`: `mark` or `chapter`.
    kind: String,
    /// `data-t` exactly as the markup carries it. A re-layout matches on
    /// the written text and never on the number, because the same block
    /// drawn again writes the same three decimals and a parsed float
    /// compared for equality would depend on that being exactly true.
    stamp: String,
    /// The same instant as a number, which is what the roving orders the
    /// cues by and what a press on one seeks to.
    t: f64,
    node: Element,
}

/// What had the focus in the drawing before its markup was replaced.
///
/// The element's own tab stop is one node whatever the markup, so it needs
/// no more than saying so; a cue is one of many and is named by the pair
/// its rect carries, which survives the re-layout the node does not.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Held {
    Stop,
    Cue(String, String),
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
    /// Whether this chart's thumb is the widget's slider ([`owns_slider`]),
    /// answered once from the tree the host was connected into.
    slider: bool,
    /// Whether the node this element's focus is about ([`focus_stop`]) has
    /// it, which for a chart that owns its slider is the thumb, and is the
    /// only time a clock tick writes its `aria-valuetext` (decision 18).
    /// Written from the tree ([`Follower::stop_has_focus`]) and from the
    /// focus events, never assumed.
    focused: Cell<bool>,
    /// A move onto the stop is in flight. Focusing a node raises a
    /// `focusin` of its own, and raises it synchronously, so the move that
    /// answers one arrival runs inside the handler for the next; this is
    /// what makes that second arrival the end of the first move rather
    /// than the start of another.
    moving_focus: Cell<bool>,
    /// When that value was last written, on the page's own clock.
    value_written: Cell<f64>,
    /// The next write skips the wait: a key or a pointer seek says the new
    /// value at once rather than on the debounce.
    value_at_once: Cell<bool>,
    /// The film's play state as the last tick reported it, so play and
    /// pause are announced when they change and not once a frame.
    playing: Cell<Option<bool>>,
}

fn document() -> Option<web_sys::Document> {
    web_sys::window()?.document()
}

/// Put the focus on an SVG node the element moved it to, without letting
/// the page scroll to it: `preventScroll` is decision 20's rule for every
/// focus this element moves itself. A chart is a wide thing in a narrow
/// column, and a roving tabindex that scrolled the page each time it
/// stepped along the timeline would be the widget deciding where the
/// reader looks.
fn give_focus(element: &Element) {
    let Some(node) = element.dyn_ref::<web_sys::SvgElement>() else {
        return;
    };
    let options = web_sys::FocusOptions::new();
    options.set_prevent_scroll(true);
    let _ = node.focus_with_options(&options);
}

/// The page's clock in seconds, which is the unit [`LONG_PRESS`] and
/// [`VALUETEXT_WAIT`] are counted in, here and in the film that keeps the
/// same wait ([`valuetext_due`]). A page with no performance clock reads
/// zero for ever, so neither delay is ever reached there: a press stays a
/// tap until it moves, and a tick never comes due.
pub(crate) fn now() -> f64 {
    web_sys::window()
        .and_then(|w| w.performance())
        .map_or(0.0, |p| p.now() / 1000.0)
}

impl Follower {
    /// Re-read the parts the effects act on out of `svg`, move the
    /// gestures onto it, and put the preview back where it was: a
    /// re-layout draws a fresh chart, whose peek rule starts hidden.
    ///
    /// `refocus` is what had the focus before the markup it stood in went,
    /// which is the whole roving set and not the tab stop alone: a resize
    /// with a cue focused destroyed that cue and dropped the reader onto
    /// the page. The caller asks the tree for it, because it cannot be
    /// asked here and the record cannot answer either: a browser removing
    /// the focused node fires `focusout` as it goes and leaves the shadow
    /// root with no active element, so by now both say the focus has left.
    /// It has not; the node it was on has. The focus goes back on the cue
    /// this markup drew for the same instant, and on the stop where that
    /// cue is gone from the data as well as from the markup.
    fn capture(&self, svg: Option<Element>, refocus: Option<Held>) {
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
            live: self.root.query_selector("[role=\"status\"]").ok().flatten(),
            svg,
        };
        self.place_peek();
        if let Some(held) = refocus {
            let back = match &held {
                Held::Stop => None,
                Held::Cue(kind, stamp) => self
                    .cues()
                    .into_iter()
                    .find(|cue| &cue.kind == kind && &cue.stamp == stamp)
                    .map(|cue| cue.node),
            };
            if let Some(target) = back.or_else(|| self.focus_target()) {
                self.move_stop(&target);
            }
        }
        // the record is what the tree says once that is done and never a
        // constant: it decides the words the value takes, and a value
        // written as though the focus had gone would tell a reader who is
        // still on the thumb the whole timeline again (decision 18)
        self.focused.set(self.stop_has_focus());
        // this thumb is new markup, carrying the value the emitter drew it
        // at, so the words for the clock the element is actually on go in
        // now: at a first render, a re-layout, and the upgrade of a chart
        // the build wrote
        let t = self.live.borrow().time;
        self.write_value(t);
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
        // asked before the markup goes, which is the only moment the tree
        // can still answer it ([`Follower::capture`])
        let refocus = self.holds_focus();
        let (markup, layout) = {
            let live = self.live.borrow();
            let layout = Layout::sized(width, width / live.ratio, live.data.end());
            (
                shadow_markup(&live.data, layout, None, live.ratio, BY_SITE, self.slider),
                layout,
            )
        };
        self.root.set_inner_html(&markup);
        self.live.borrow_mut().layout = layout;
        self.capture(
            self.root.query_selector("svg.chart").ok().flatten(),
            refocus,
        );
        // the caption is new markup, so the host's references to it are
        // stale until they are taken again
        self.describe();
    }

    /// The host's own semantics, through its internals rather than stamped
    /// on the element as attributes: the chart is a group, named by the
    /// caption's heading and described by the summary and the instructions
    /// line (decision 15). The three sit in the shadow tree, where no id in
    /// the page's scope can reach them, so the host points at the elements
    /// themselves, which is what internals are for. A browser without the
    /// element references loses the naming and keeps the rest, and a page
    /// that writes its own `role` or `aria-label` still wins: internals are
    /// the default semantics and the attribute is the author's word.
    fn describe(&self) {
        let Some(internals) = op_webc::internals(&self.host) else {
            return;
        };
        let set = |key: &str, value: &JsValue| {
            let _ = js_sys::Reflect::set(&internals, &JsValue::from_str(key), value);
        };
        set("role", &JsValue::from_str("group"));
        let by_id = |id: &str| self.root.query_selector(&format!("#{id}")).ok().flatten();
        if let Some(title) = by_id(TITLE_ID) {
            set("ariaLabelledByElements", &js_sys::Array::of1(&title));
        }
        let described = js_sys::Array::new();
        for id in DESCRIBED_BY {
            if let Some(element) = by_id(id) {
                described.push(&element);
            }
        }
        if described.length() > 0 {
            set("ariaDescribedByElements", &described);
        }
    }

    /// Keep the pre-render at `width` when it was drawn for that box: the
    /// chart the container query is not showing goes, the other stays as
    /// the build wrote it, and the element takes its handles and the
    /// geometry it was drawn with. False when there is nothing to keep, and
    /// the caller draws its own.
    fn keep(&self, width: f64) -> bool {
        let refocus = self.holds_focus();
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
        self.capture(Some(svg), refocus);
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
                svg_of(
                    &live.data.to_spec(),
                    &aria_of(&live.data, self.slider),
                    layout,
                    "chart",
                    BY_SITE,
                ),
                layout,
            )
        };
        // asked before the markup goes, which is the only moment the tree
        // can still answer it ([`Follower::capture`])
        let refocus = self.holds_focus();
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
        self.capture(
            self.root.query_selector("svg.chart").ok().flatten(),
            refocus,
        );
    }

    /// Put the playhead, its readout and the played bar at the stored time,
    /// and say the time as the thumb's own value. `force` re-applies them
    /// after a re-layout, where the geometry changed under an unchanged
    /// clock.
    ///
    /// The value goes on the playhead group and nowhere else: that group
    /// is the slider (decision 15), and the svg around it is a
    /// `graphics-document`, a role with no value to carry. Where the thumb
    /// is not this widget's slider it carries no value at all, and only
    /// the geometry here moves. The readout follows a preview while one is
    /// showing and the value never does: nothing is announced for a peek
    /// (decision 18).
    ///
    /// The number and the words for it are written on different clocks.
    /// `aria-valuenow` is the machine-readable value and follows every
    /// tick; `aria-valuetext` is what a listener hears, so it waits for
    /// the focus and the debounce ([`valuetext_due`]) unless a gesture
    /// has just asked for it. When the words do go in, the number goes in
    /// beside them ([`Follower::write_value`]) rather than a frame later,
    /// so the two never name different instants: a committed seek writes
    /// both in the frame the key was handled in, and the animation frame
    /// after it finds nothing to change.
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
        if self.value_at_once.replace(false)
            || valuetext_due(self.value_written.get(), now(), self.focused.get())
        {
            self.write_value(t);
        } else {
            self.write_valuenow(t);
        }
        let mut live = self.live.borrow_mut();
        live.x = x;
        live.label = label;
    }

    /// Write the thumb's `aria-valuenow` now: the machine-readable value,
    /// which follows every tick whether or not the words do.
    fn write_valuenow(&self, t: f64) {
        let Some(playhead) = self.thumb() else {
            return;
        };
        let _ = playhead.set_attribute("aria-valuenow", &format!("{t:.2}"));
    }

    /// Write the thumb's whole value now: the number, and the words for it
    /// in the form the focus asks for (decision 18).
    ///
    /// The two go in together. `aria-valuenow` on its own follows the
    /// clock every frame, but a frame in which the words are written is a
    /// frame in which the number has to agree with them: a seek used to
    /// write the words in the handler and the number in the animation
    /// frame after it, so for one frame the value said one instant and
    /// read another.
    ///
    /// The long form, with the whole timeline and the frame index, goes in
    /// whenever the thumb does not have the focus: that is initialisation,
    /// the blur the decision names, and a re-layout that had no focus to
    /// put back, so the total is there to be spoken when focus next
    /// arrives. While the thumb has the focus every change is the short
    /// form, so a listener stepping along the track is not told the total
    /// again on every step.
    ///
    /// A chart whose thumb is not the widget's slider writes nothing here:
    /// [`Follower::thumb`] gives it nothing to write on.
    fn write_value(&self, t: f64) {
        let Some(playhead) = self.thumb() else {
            return;
        };
        self.write_valuenow(t);
        let text = {
            let live = self.live.borrow();
            let whole = (!self.focused.get()).then(|| {
                let keys = Keys {
                    times: &live.times,
                    duration: key_duration(&live.data, live.layout.end),
                    chapters: &live.chapters,
                    // nothing steps here: the frame the words name is a
                    // position among the rows, whatever a key is worth
                    step: None,
                };
                Whole {
                    duration: keys.duration,
                    frame: keys.index_at(t) + 1,
                    frames: keys.times.len(),
                }
            });
            valuetext(t, &chapter_at(&live.data, t), whole)
        };
        let _ = playhead.set_attribute("aria-valuetext", &text);
        self.value_written.set(now());
    }

    /// The thumb, where it is this widget's slider and so has a value to
    /// carry at all. A chart inside a film's shadow tree gets nothing here:
    /// its playhead is decoration, hidden from the accessibility tree, and
    /// the film's own range input is what announces the time (decision 15).
    /// Both aria value writes go through this, so the rule is asked once.
    fn thumb(&self) -> Option<Element> {
        self.slider
            .then(|| self.dom.borrow().playhead.clone())
            .flatten()
    }

    /// Put one short message in the status region, which has been in the
    /// shadow root since the first render and is never made on demand
    /// (decision 18). Four things reach it: a seek that committed, play,
    /// pause, and a chapter entered while the film plays. Nothing about a
    /// peek or a seek still being aimed is ever said, and the region is
    /// polite.
    fn say(&self, text: &str) {
        if let Some(live) = &self.dom.borrow().live {
            live.set_text_content(Some(text));
        }
    }

    /// The node this element's focus is about ([`Follower::focus_target`])
    /// gained or lost it, which is what the value's words follow. While it
    /// has the focus a clock tick writes the short form on the debounce;
    /// when it goes, the long form goes in at once, so the duration and
    /// the frame index are waiting to be spoken when the thumb is next
    /// focused (decision 18). Focus on anything else in the chart - a cue
    /// button the roving moved to - says nothing about the thumb.
    ///
    /// Read in the bubbling spelling, `focusin` and `focusout`: it is the
    /// thumb inside the svg that takes the focus, and `focus` does not
    /// bubble.
    fn on_focus(&self, event: &Event, gained: bool) {
        let stop = self.focus_target();
        let target = event.target().and_then(|t| t.dyn_into::<Element>().ok());
        if stop.is_none() || stop != target {
            return;
        }
        self.focused.set(gained);
        if !gained {
            let t = self.live.borrow().time;
            self.write_value(t);
        }
    }

    /// Where the tree says a focus that has just arrived stands, for
    /// [`takes_the_stop`] to judge.
    ///
    /// The stop is asked for in the one place that answers for it
    /// ([`Follower::focus_target`]), and the svg is the one
    /// [`Follower::capture`] kept, so neither can be a node of the chart
    /// the element threw away: a pre-render ships a wide drawing and a
    /// narrow one, with a thumb in each, until the width the host was laid
    /// out at picks one and the other is removed.
    fn arrival(&self) -> Arrival {
        let Some(active) = self.root.active_element() else {
            return Arrival {
                inside: false,
                stop: false,
                svg: false,
            };
        };
        let stop = self.focus_target();
        Arrival {
            inside: true,
            stop: stop.as_ref() == Some(&active),
            svg: self.dom.borrow().svg.as_ref() == Some(&active),
        }
    }

    /// A focus has arrived somewhere in this element. Where the engine put
    /// it there rather than the reader ([`takes_the_stop`]), it is moved
    /// onto the stop, with the tab stop and without scrolling the page
    /// ([`Follower::move_stop`]).
    ///
    /// Heard on the host and in the bubbling spelling, because one of the
    /// two nodes an engine can leave the focus on is the host itself, and
    /// no listener inside the root is told about that one.
    fn on_arrival(&self) {
        if self.moving_focus.get() || !takes_the_stop(self.arrival()) {
            return;
        }
        let Some(stop) = self.focus_target() else {
            return;
        };
        self.moving_focus.set(true);
        self.move_stop(&stop);
        self.moving_focus.set(false);
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
    ///
    /// Two of decision 18's four messages are decided here, and both are
    /// events rather than states: the play flag turning over, and a chapter
    /// boundary crossed while the film plays. A tick that only advances the
    /// clock says nothing at all, which is what "rate-limited to chapter
    /// boundaries and never per sample" means.
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
        let mut said: Option<String> = None;
        if let Some(playing) = field("playing").and_then(|v| v.as_bool()) {
            set_state(&self.host, "playing", playing);
            // the first tick teaches the chart what the film is doing; it
            // is not the film starting or stopping, so it announces nothing
            if self.playing.replace(Some(playing)) == Some(!playing) {
                let said_now = if playing { Said::Playing } else { Said::Paused };
                said = Some(message(said_now, t, ""));
            }
        }
        let prev = std::mem::replace(&mut self.live.borrow_mut().time, t);
        if said.is_none() && self.playing.get() == Some(true) {
            let live = self.live.borrow();
            if let Some(entered) = chapter_crossed(prev, t, &live.chapters) {
                let title = live
                    .data
                    .chapters
                    .get(entered)
                    .map(|c| c.label.clone())
                    .unwrap_or_default();
                drop(live);
                said = Some(message(Said::Chapter, t, &title));
            }
        }
        if let Some(said) = said {
            self.say(&said);
        }
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
        if ends_press(intent) {
            self.end_press();
        }
        match intent {
            Intent::Seek(t) => {
                // the seek has committed, so the thumb says its new value
                // at once instead of waiting for the debounce; the peek
                // that aimed it said nothing (decision 18)
                self.value_at_once.set(true);
                let before = self.live.borrow().time;
                self.emit_time(SEEK_EVENT, Some(t));
                // a film in this document answers a seek where it stands,
                // so the words for the time it landed on go in here, in
                // the frame the key or the release was handled in, rather
                // than in the animation frame that moves the drawing after
                // it: "at once" is the decision's word. Where the answer
                // comes later the flag is still set, and the tick that
                // carries it writes without the debounce instead
                let landed = self.live.borrow().time;
                if landed != before && self.value_at_once.replace(false) {
                    self.write_value(landed);
                }
                // and the message goes in last, after the emit and not
                // before it. A seek made while the film plays pauses it,
                // and the film's pause renders and dispatches a tick of
                // its own with the play flag turned over; this element
                // hears that tick inside the emit above and says "Paused"
                // on it. A message written before the emit is the one that
                // is overwritten, in exactly the case a reader seeks most
                let spoken = seeked_time(t, before, landed);
                let chapter = chapter_at(&self.live.borrow().data, spoken);
                self.say(&message(Said::Seeked, spoken, &chapter));
            }
            Intent::Peek(t) => self.set_peek(t),
            Intent::Toggle => self.emit(TOGGLE_EVENT, None),
            // the press is already back; a cancel says nothing, because
            // nothing happened to the clock
            Intent::Cancel => {}
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
        // the very drag this press is starting. It goes to the thumb, as a
        // press on a range input's track puts the focus on that input, and
        // the thumb is this chart's tab stop; the ring is `:focus-visible`
        // only, so a press does not draw one.
        event.prevent_default();
        if let Some(svg) = &self.dom.borrow().svg {
            let _ = svg.set_pointer_capture(pointer.pointer_id());
        }
        if let Some(target) = self.focus_target() {
            self.move_stop(&target);
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

    /// The node this chart's focus is about ([`focus_stop`]) as an
    /// element: the tab stop the element hands the focus to, where an
    /// embedded chart has no thumb of its own and the svg is the only
    /// thing here a keyboard can reach. [`Follower::thumb`] answers for
    /// the value and says no to a decoration; this answers for the focus,
    /// and a decoration is still where a press on it belongs.
    fn focus_target(&self) -> Option<Element> {
        let dom = self.dom.borrow();
        match focus_stop(self.slider) {
            Stop::Thumb => dom.playhead.clone(),
            Stop::Svg => dom.svg.clone(),
        }
    }

    /// Whether the tree says that node has the focus now. Everything about
    /// the focus is written from this rather than assumed: the record
    /// [`Follower::focused`] keeps, and whether a re-layout has a focus to
    /// put back.
    fn stop_has_focus(&self) -> bool {
        let target = self.focus_target();
        target.is_some() && target == self.root.active_element()
    }

    /// The cue buttons a reader can reach in the drawing, in the order the
    /// markup carries them, each with the instant and the kind its own
    /// rect names.
    ///
    /// Read out of the markup rather than off the data, because the markup
    /// is what has the focus and what a re-layout replaces: a chart the
    /// build wrote and a chart the element drew both answer here.
    ///
    /// The set is what the container query leaves drawn ([`cue_is_drawn`])
    /// and not every button in the tree, and that is asked here alone: the
    /// roving's keys, the lookup that says where the focus stands, the
    /// record a re-layout puts back and the tab stop's own write all walk
    /// this one set, so none of them can count a cue another cannot reach.
    fn cues(&self) -> Vec<Cue> {
        let width = self.width();
        let dom = self.dom.borrow();
        let Some(nodes) = dom
            .svg
            .as_ref()
            .and_then(|svg| svg.query_selector_all(CUE_BUTTONS).ok())
        else {
            return Vec::new();
        };
        (0..nodes.length())
            .filter_map(|i| nodes.get(i)?.dyn_into::<Element>().ok())
            .map(|node| {
                let rect = node.query_selector("[data-t]").ok().flatten();
                let stamp = rect
                    .as_ref()
                    .and_then(|rect| rect.get_attribute("data-t"))
                    .unwrap_or_default();
                let kind = rect
                    .as_ref()
                    .and_then(|rect| rect.get_attribute("data-cue"))
                    .unwrap_or_default();
                // a cue whose rect says nothing sorts to the end of the
                // timeline rather than to the start of it, where it would
                // take the entry gesture's first cue
                let t = stamp.parse::<f64>().unwrap_or(f64::INFINITY);
                Cue {
                    kind,
                    stamp,
                    t,
                    node,
                }
            })
            .filter(|cue| cue_is_drawn(&cue.kind, width))
            .collect()
    }

    /// Which member of the roving set the tree says has the focus, and
    /// [`None`] where the focus is somewhere else entirely: the data
    /// table's disclosure, or another element on the page. `order` is
    /// [`in_time_order`] over `cues`, so a cue's position is the one the
    /// arrows move over.
    fn roving_at(&self, cues: &[Cue], order: &[usize]) -> Option<Roving> {
        if self.stop_has_focus() {
            return Some(Roving::Thumb);
        }
        let active = self.root.active_element()?;
        order
            .iter()
            .position(|i| active.is_same_node(Some(cues[*i].node.as_ref())))
            .map(Roving::Cue)
    }

    /// What the tree says has the focus in this drawing, as something that
    /// outlives the markup it was read from: a re-layout throws every node
    /// away, so a cue is remembered by the pair its rect carries and found
    /// again in the markup that replaces it.
    fn holds_focus(&self) -> Option<Held> {
        if self.stop_has_focus() {
            return Some(Held::Stop);
        }
        let active = self.root.active_element()?;
        self.cues()
            .into_iter()
            .find(|cue| active.is_same_node(Some(cue.node.as_ref())))
            .map(|cue| Held::Cue(cue.kind, cue.stamp))
    }

    /// Put the one tab stop on `to`, and the focus with it.
    ///
    /// Decision 17's roving tabindex is an attribute that moves. The set
    /// is the element's own stop and every cue button the drawing shows;
    /// exactly one of them is in the tab order at a time and the rest are
    /// at `-1`, so Tab leaves the chart from wherever the reader stands in
    /// it. Shift with Tab comes back to the thumb rather than to the cue
    /// it left from: that is `delegatesFocus` entering at the delegate,
    /// which [`Follower::on_arrival`] then normalises onto the stop. The
    /// emitter draws the set at rest, with the stop at `0` and every cue
    /// at `-1`, so fresh markup needs nothing done to it until the focus
    /// moves off the stop.
    ///
    /// Where `to` is not in that set the stop falls back to the thumb
    /// ([`stop_lands`]) and the write follows the decision rather than
    /// running ahead of it: a `0` on a node the drawing is not showing
    /// would be the element's only tab stop, and the focus written after
    /// it would silently go nowhere.
    fn move_stop(&self, to: &Element) {
        let stop = self.focus_target();
        let cues = self.cues();
        let want = cues
            .iter()
            .position(|cue| cue.node.is_same_node(Some(to.as_ref())));
        let onto = match stop_lands(cues.len(), want) {
            Roving::Cue(i) => Some(&cues[i].node),
            Roving::Thumb => stop.as_ref(),
        };
        let Some(onto) = onto else {
            return;
        };
        for node in stop.iter().chain(cues.iter().map(|cue| &cue.node)) {
            let at = if node.is_same_node(Some(onto.as_ref())) {
                "0"
            } else {
                "-1"
            };
            let _ = node.set_attribute("tabindex", at);
        }
        give_focus(onto);
    }

    /// Decision 17's roving tabindex as the element runs it, answered
    /// before the key table: the entry gesture from the thumb, the arrows
    /// between the cues, the two keys that leave, and the press that seeks
    /// to the focused cue's own time. Says whether the key was the
    /// roving's own, so the key table never sees one it answered.
    ///
    /// Every key the roving answers leaves by the one `true` at the end,
    /// so a key cannot be acted on and left to the key table as well:
    /// Space on a cue both sought and played the film before, because the
    /// answer and the claim were two decisions instead of one.
    fn roved(&self, key: &str) -> bool {
        let cues = self.cues();
        let times: Vec<f64> = cues.iter().map(|cue| cue.t).collect();
        let order = in_time_order(&times);
        let Some(at) = self.roving_at(&cues, &order) else {
            return false;
        };
        let Some(answer) = roving_key(at, order.len(), key) else {
            return false;
        };
        match answer {
            Roved::To(Roving::Thumb) => {
                if let Some(stop) = self.focus_target() {
                    self.move_stop(&stop);
                }
            }
            Roved::To(Roving::Cue(i)) => self.move_stop(&cues[order[i]].node),
            Roved::Press => {
                if let Roving::Cue(i) = at {
                    let t = cues[order[i]].t;
                    let duration = {
                        let live = self.live.borrow();
                        key_duration(&live.data, live.layout.end)
                    };
                    self.act(Intent::Seek(t.clamp(0.0, duration)));
                }
            }
        }
        true
    }

    /// A key, acted on only while the focus is on the chart: the svg, or
    /// anything inside it, which is the thumb that is its slider and the
    /// cue buttons the roving moves between. The chart is not the shadow
    /// root's only tab stop: the data table's disclosure takes the focus
    /// too and keeps its own keys, so Space there opens the table and seeks
    /// nothing.
    fn on_key(&self, event: &Event) {
        let Some(key) = event.dyn_ref::<web_sys::KeyboardEvent>() else {
            return;
        };
        let active = self.root.active_element();
        let ours = match (&self.dom.borrow().svg, &active) {
            (Some(svg), Some(active)) => svg.contains(Some(active.as_ref())),
            _ => false,
        };
        if !ours {
            return;
        }
        // the roving answered it: the way into the cues, a step between
        // them, the way back, or a press on the one that has the focus.
        // The key table never sees a key the roving took, which is what
        // kept Space on a chapter from playing the film as well
        if self.roved(&key.key()) {
            key.prevent_default();
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
                    // read at the key, so an attribute set after the
                    // upgrade is answered by the next press of one
                    step: step_attr(self.host.get_attribute("step").as_deref()),
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
    /// A focus arriving rides on the host for the same reason, and for one
    /// more: the arrival it answers can be on the host itself.
    on_focus_in: Closure<dyn FnMut(Event)>,
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
        delegating_shadow_root(&self.host).set_inner_html(&format!(
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
            root: delegating_shadow_root(&self.host),
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
            // one slider per widget: a chart in a film's shadow tree leaves
            // the slider to the film's own range input (decision 15)
            slider: owns_slider(host_tag(&self.host).as_deref()),
            focused: Cell::new(false),
            moving_focus: Cell::new(false),
            value_written: Cell::new(f64::NEG_INFINITY),
            value_at_once: Cell::new(false),
            playing: Cell::new(None),
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
            // focus is read in its bubbling spelling, because it is the
            // thumb inside the svg that takes it and `focus` does not
            // bubble; the handler asks whether the target is the node this
            // element's focus is about ([`Follower::focus_target`])
            let f = follower.clone();
            on(
                "focusin",
                Closure::new(move |e: Event| f.on_focus(&e, true)),
            );
            let f = follower.clone();
            on(
                "focusout",
                Closure::new(move |e: Event| f.on_focus(&e, false)),
            );
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
                follower.capture(svg, None);
                // the build wrote the caption this names; a render would
                // have written its own
                follower.describe();
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

        let on_focus_in = {
            let f = follower.clone();
            Closure::<dyn FnMut(Event)>::new(move |_: Event| f.on_arrival())
        };
        let _ = self
            .host
            .add_event_listener_with_callback("focusin", on_focus_in.as_ref().unchecked_ref());

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
            on_focus_in,
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
        let _ = self.host.remove_event_listener_with_callback(
            "focusin",
            wiring.on_focus_in.as_ref().unchecked_ref(),
        );
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

/// The tag of the element whose shadow tree this host sits in, and nothing
/// at all for a host in a document. This is what [`owns_slider`] is asked,
/// and it is all the element can know about the widget it is part of: the
/// root node it was connected into.
fn host_tag(host: &HtmlElement) -> Option<String> {
    host.get_root_node()
        .dyn_ref::<ShadowRoot>()
        .map(|root| root.host().local_name())
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
            true,
        );
        assert_eq!(one.matches("<svg ").count(), 1);
        assert!(one.contains("<svg class=\"chart\" data-rendered-by=\"op-site\" part=\"chart\""));
        assert!(!one.contains("chart wide") && !one.contains("chart narrow"));
    }

    /// The markup of one chart at the element's own width, drawn as the
    /// standalone chart that owns its widget's slider.
    fn one_chart() -> String {
        chart_owning(true)
    }

    /// The same chart, drawn for whichever answer [`owns_slider`] gave.
    fn chart_owning(slider: bool) -> String {
        let d = data();
        shadow_markup(
            &d,
            Layout::sized(800.0, 300.0, d.end()),
            None,
            DEFAULT_RATIO,
            BY_SITE,
            slider,
        )
    }

    /// Decision 15: the svg is the document and the thumb is the control.
    /// A slider role on the svg would make everything under it
    /// presentational, and `role="img"` would collapse the subtree the
    /// series groups are exposed through; the value belongs to the thumb,
    /// which is the operable thing and the one that moves.
    #[test]
    fn the_chart_is_a_labelled_document_whose_thumb_is_the_slider() {
        let one = one_chart();
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
            // its own, so the shadow root has more than one
            assert!(markup.contains("tabindex=\"0\""), "{markup}");
            assert!(markup.contains("<summary>Data table</summary>"), "{markup}");
            // the svg is never the control and never an image
            for head in heads(markup, "svg") {
                for never in ["role=\"slider\"", "role=\"img\"", "aria-value"] {
                    assert!(!head.contains(never), "{never} on an svg: {head}");
                }
                assert_eq!(attr(head, "role"), "graphics-document", "{head}");
                assert_eq!(attr(head, "aria-labelledby"), TITLE_ID, "{head}");
            }
            // and the box the emitter drew is untouched
            assert!(markup.contains("part=\"chart\""));
            assert!(markup.contains("viewBox=\"0 0 "));
        }
        // one document and one slider per chart, wide and narrow alike
        assert_eq!(pair.matches("role=\"graphics-document\"").count(), 2);
        assert_eq!(pair.matches("role=\"slider\"").count(), 2);
        // the thumb is the slider, named by the same heading and carrying
        // the value the tick writes
        let thumb = heads(&one, "g")
            .into_iter()
            .find(|head| attr(head, "class") == "playhead")
            .expect("the thumb");
        assert_eq!(attr(thumb, "role"), "slider");
        assert_eq!(attr(thumb, "tabindex"), "0");
        // the control is named for what it moves. The caption's heading
        // names the document, which is what the chart is a chart of; a
        // slider named by that list announces the whole legend before its
        // own value on every step along the track
        assert_eq!(attr(thumb, "aria-label"), "Time");
        assert_eq!(attr(thumb, "aria-labelledby"), "");
        assert_eq!(attr(thumb, "aria-valuemin"), "0");
        assert_eq!(attr(thumb, "aria-valuemax"), format!("{:.2}", data().end()));
        assert_eq!(attr(thumb, "aria-valuenow"), "0");
        assert_eq!(attr(thumb, "aria-valuetext"), "0.00 seconds");
        // the readout is inside that slider, and written once, so the time
        // is not read again as loose text beside it
        assert_eq!(one.matches("class=\"head-t\"").count(), 1);
        let group = one
            .split_once("<g class=\"playhead\"")
            .expect("the thumb")
            .1
            .split_once("</g>")
            .expect("it closes")
            .0;
        assert!(group.contains("class=\"head-t\""), "{group}");
        // and the tick writes the value through that group and no other
        // handle: the svg's document role has no value to carry
        let source = include_str!("chart.rs");
        let writes: Vec<&str> = source
            .lines()
            .map(str::trim)
            .filter(|line| line.contains("set_attribute(\"aria-value"))
            .collect();
        assert_eq!(writes.len(), 2, "{writes:?}");
        for write in writes {
            assert!(write.starts_with("let _ = playhead."), "{write}");
        }
        // and the number is written beside the words whenever the words go
        // in, so the two can never name different instants: a seek used to
        // write the words in the handler and the number in the animation
        // frame after it, and for that frame the value said one time and
        // read another. `aria-valuenow` still follows every tick of
        // its own, which is the other half of decision 13
        let body_of = |signature: &str| {
            source
                .split_once(signature)
                .unwrap_or_else(|| panic!("{signature}"))
                .1
                .split_once("\n    }")
                .expect("it ends")
                .0
        };
        let write_value = body_of("fn write_value(&self, t: f64) {");
        assert!(
            write_value.contains("self.write_valuenow(t)"),
            "{write_value}"
        );
        assert!(write_value.contains("aria-valuetext"), "{write_value}");
        let now = body_of("fn write_valuenow(&self, t: f64) {");
        assert!(now.contains("aria-valuenow"), "{now}");
        let apply = body_of("fn apply(&self, force: bool) {");
        assert!(apply.contains("self.write_value(t)"), "{apply}");
        assert!(apply.contains("self.write_valuenow(t)"), "{apply}");
        // and the value is nowhere else at all: the animation frame no
        // longer writes the number on its own beside the transform
        assert!(!apply.contains("set_attribute(\"aria-"), "{apply}");
        // and that handle is `thumb`, which asks whether this chart owns
        // the widget's slider at all: an embedded chart's playhead is
        // decoration and no tick may write a value onto it. The needle is
        // assembled, so this line is not one of the occurrences it counts
        let handle = ["self.", "thumb()"].concat();
        assert_eq!(source.matches(&handle).count(), 2, "{source}");
        let body = source
            .split_once("fn thumb(&self)")
            .expect("the thumb accessor")
            .1;
        let body = body.split_once("\n    }").expect("it ends").0;
        assert!(body.contains("self.slider"), "{body}");
    }

    /// Decision 15's last clauses: a chart inside the film's own shadow
    /// tree is a second picture of a timeline the film's native range input
    /// already controls, so one slider per widget makes the chart's thumb
    /// decoration there. Anywhere else - a document, a page's own wrapper
    /// element - the chart is the only control over its timeline and keeps
    /// the slider.
    #[test]
    fn a_chart_inside_a_film_leaves_the_slider_to_the_film() {
        // the decision, as the element can ask it: what the root node's
        // host is
        assert!(owns_slider(None));
        assert!(!owns_slider(Some(FILM_TAG)));
        // the tag comes off the DOM, which spells it in whatever case the
        // page wrote
        assert!(!owns_slider(Some("OPT-FILM")));
        // every other host is somebody's wrapper, not the widget that owns
        // the time
        for host in ["opt-machine", "opt-scene", "div", "opt-film-strip"] {
            assert!(owns_slider(Some(host)), "{host}");
        }

        let embedded = chart_owning(false);
        let thumb = heads(&embedded, "g")
            .into_iter()
            .find(|head| attr(head, "class") == "playhead")
            .expect("the thumb");
        // no role, no tab stop, no value, and out of the accessibility
        // tree, which is what decoration is
        assert_eq!(attr(thumb, "role"), "");
        assert_eq!(attr(thumb, "tabindex"), "");
        assert_eq!(attr(thumb, "aria-hidden"), "true");
        assert!(!thumb.contains("aria-value"), "{thumb}");
        assert!(!embedded.contains("role=\"slider\""), "{embedded}");

        // and everything else about the chart is exactly as it was: the
        // document role and its name, the caption, the instructions, the
        // series groups, the cue buttons, the status region and the table
        let standalone = one_chart();
        for same in [
            "role=\"graphics-document\"",
            "<strong class=\"title\" id=\"chart-title\">",
            "<p class=\"summary\"",
            "<p class=\"keys\"",
            "role=\"graphics-object\"",
            "<g role=\"button\" tabindex=\"-1\"",
            STATUS,
            "<summary>Data table</summary>",
        ] {
            assert_eq!(
                embedded.matches(same).count(),
                standalone.matches(same).count(),
                "`{same}` differs between an embedded chart and a standalone one"
            );
        }
        // one tab stop either way, and it is whichever of the two can be
        // driven: the thumb where the chart owns the slider, and the svg
        // where it does not, since a decoration is no use to a keyboard
        // and the chart would otherwise be unreachable inside the film
        assert_eq!(embedded.matches("tabindex=\"0\"").count(), 1);
        assert_eq!(standalone.matches("tabindex=\"0\"").count(), 1);
        for head in heads(&embedded, "svg") {
            assert_eq!(attr(head, "tabindex"), "0", "{head}");
        }
        for head in heads(&standalone, "svg") {
            assert_eq!(attr(head, "tabindex"), "", "{head}");
        }
        // the caption's heading still names the document; the thumb is not
        // named at all, having nothing left to be named as
        for head in heads(&embedded, "svg") {
            assert_eq!(attr(head, "aria-labelledby"), TITLE_ID, "{head}");
        }
        assert_eq!(
            embedded.matches("aria-labelledby=\"chart-title\"").count(),
            1
        );
        // one reference per chart, wherever it stands: the document takes
        // the caption's heading and the thumb is named "Time"
        assert_eq!(
            standalone
                .matches("aria-labelledby=\"chart-title\"")
                .count(),
            1
        );
        assert_eq!(standalone.matches("aria-label=\"Time\"").count(), 1);
        assert_eq!(embedded.matches("aria-label=\"Time\"").count(), 0);
        // and the thumb the build writes is always the slider, because the
        // build writes charts into a page and never into a film
        assert!(built().shadow.contains("role=\"slider\""));
    }

    /// The one answer about the focus, held to the markup: the node
    /// [`focus_stop`] names is the node the emitter gives the tab stop to,
    /// for either answer of [`owns_slider`].
    ///
    /// This is the check the phase went without. Two stories moved the tab
    /// stop onto the thumb in the markup and left the element focusing and
    /// watching the svg, and nothing native said so: a press then focused a
    /// node that is no longer a stop, the record of the focus was about a
    /// node that never gained it, and the value said the whole timeline to
    /// a reader stepping along the track. The markup here is the emitter's
    /// own, so the two cannot drift apart again without this failing.
    #[test]
    fn the_focus_stop_is_the_node_the_markup_gives_the_tab_stop_to() {
        for slider in [true, false] {
            let markup = chart_owning(slider);
            let stops: Vec<&str> = heads(&markup, "g")
                .into_iter()
                .chain(heads(&markup, "svg"))
                .filter(|head| attr(head, "tabindex") == "0")
                .collect();
            assert_eq!(stops.len(), 1, "{stops:?}");
            // the class the element finds that node by, which is what
            // `capture` reads the two handles out of the markup with
            let class = match focus_stop(slider) {
                Stop::Thumb => "playhead",
                Stop::Svg => "chart",
            };
            assert_eq!(attr(stops[0], "class"), class, "{}", stops[0]);
        }
        // the two answers are different nodes, so the pass above is not one
        // shape checked twice
        assert_ne!(focus_stop(true), focus_stop(false));
        // and the answer is the decision the element can actually ask: what
        // the root node it was connected into is
        assert_eq!(focus_stop(owns_slider(None)), Stop::Thumb);
        assert_eq!(focus_stop(owns_slider(Some(FILM_TAG))), Stop::Svg);
    }

    /// The element focuses the node it treats as focusable, judges the
    /// focus by that same node, and puts the focus back on it after a
    /// re-layout. No native test can reach a shadow root's active element,
    /// so this is read off the source, as the other guards in this file
    /// are.
    #[test]
    fn the_element_focuses_and_watches_the_one_node_it_calls_its_stop() {
        let source = include_str!("chart.rs");
        let body_of = |signature: &str| {
            source
                .split_once(signature)
                .unwrap_or_else(|| panic!("{signature}"))
                .1
                .split_once("\n    }")
                .expect("it ends")
                .0
        };
        // `focus_stop` becomes an element in one place, so there is one
        // answer and not one per caller. The needle is assembled, so this
        // line is not the occurrence it counts
        let once = ["focus_", "stop(self.slider)"].concat();
        assert_eq!(source.matches(&once).count(), 1, "{source}");

        // everything that moves the focus by hand takes the node from
        // there. The roving moves it along the cues as well, and hands it
        // back to the stop when Up or Escape leaves them. That each of
        // them goes through the one mover is
        // `the_tab_stop_is_an_attribute_that_moves_over_the_whole_set`,
        // over this same list
        for mover in [
            "fn capture(&self, svg: Option<Element>, refocus: Option<Held>) {",
            "fn on_down(&self, event: &Event) {",
            "fn roved(&self, key: &str) -> bool {",
            "fn on_arrival(&self) {",
        ] {
            let body = body_of(mover);
            assert!(body.contains("self.focus_target()"), "{mover}: {body}");
        }
        // and everything that judges the focus asks about that same node,
        // directly or through the one place that does, and names none of
        // its own
        for judge in [
            "fn on_focus(&self, event: &Event, gained: bool) {",
            "fn stop_has_focus(&self) -> bool {",
            "fn holds_focus(&self) -> Option<Held> {",
            "fn roving_at(&self, cues: &[Cue], order: &[usize]) -> Option<Roving> {",
        ] {
            let body = body_of(judge);
            assert!(
                body.contains("self.focus_target()") || body.contains("self.stop_has_focus()"),
                "{judge}: {body}"
            );
            for own in ["playhead", "dom.svg", "self.slider"] {
                assert!(!body.contains(own), "{judge} names {own}: {body}");
            }
        }

        // a re-layout takes the record from the tree and never from a
        // constant: it had put the focus back a moment earlier, and a
        // record saying otherwise would tell a reader still on the thumb
        // the whole timeline again on the next tick
        let capture = body_of("fn capture(&self, svg: Option<Element>, refocus: Option<Held>) {");
        assert!(
            capture.contains("self.focused.set(self.stop_has_focus())"),
            "{capture}"
        );
        for constant in ["focused.set(false)", "focused.set(true)"] {
            assert!(!capture.contains(constant), "{capture}");
        }
        // and every caller asks the tree before the markup goes, which is
        // the only moment it can still answer: a browser removing the
        // focused node fires `focusout` and empties the root's active
        // element on its way out
        for caller in [
            "fn render(&self) {",
            "fn keep(&self, width: f64) -> bool {",
            "fn relayout(&self) {",
        ] {
            let body = body_of(caller);
            let asked = body.find("self.holds_focus()").expect(caller);
            let captured = body.find("self.capture(").expect(caller);
            assert!(asked < captured, "{caller}: {body}");
            for swap in ["set_inner_html", "set_outer_html", ".remove()"] {
                if let Some(at) = body.find(swap) {
                    assert!(asked < at, "{caller} asks after {swap}: {body}");
                }
            }
        }
    }

    /// A re-layout puts the focus back where it was, and where it was is
    /// the whole roving set and not the tab stop alone.
    ///
    /// The markup goes on a container width change, a font load and a
    /// window resize, and every node in it with it. The stop is one node
    /// whatever the markup, so it needs no more than saying so; a cue is
    /// one of many and is remembered by what its own rect says it stands
    /// for. That pair has to survive the redraw, which is what this pins:
    /// the same block at three widths writes the same cues with the same
    /// stamps, while every coordinate in the drawing moves.
    #[test]
    fn a_relayout_finds_the_cue_it_took_the_focus_from() {
        let d = data();
        let cues = |width: f64| -> Vec<(String, String, String)> {
            let svg = svg_of(
                &d.to_spec(),
                &aria_of(&d, true),
                Layout::sized(width, width / DEFAULT_RATIO, d.end()),
                "chart",
                BY_SITE,
            );
            heads(&svg, "rect")
                .into_iter()
                .filter(|head| attr(head, "part") == "target")
                .map(|head| {
                    (
                        attr(head, "data-cue"),
                        attr(head, "data-t"),
                        attr(head, "x"),
                    )
                })
                .collect()
        };
        let wide = cues(DEFAULT_WIDTH);
        // the block draws a mark and a chapter, so both kinds are here and
        // the pair really does have to tell them apart
        assert_eq!(wide.len(), 2, "{wide:?}");
        assert_eq!(wide[0].0, "mark");
        assert_eq!(wide[1].0, "chapter");
        for width in [NARROW_WIDTH, 900.0] {
            let drawn = cues(width);
            let pairs = |c: &[(String, String, String)]| -> Vec<(String, String)> {
                c.iter().map(|(k, t, _)| (k.clone(), t.clone())).collect()
            };
            assert_eq!(pairs(&drawn), pairs(&wide), "at {width}");
            // and the drawing really was laid out again, so the pair is
            // not steady because nothing moved
            assert_ne!(drawn[0].2, wide[0].2, "at {width}");
        }

        // the element asks the tree for the whole set before the markup
        // goes, and looks the pair up in the markup that replaces it,
        // falling back to the stop where the cue is gone from the data too
        let source = include_str!("chart.rs");
        let body_of = |signature: &str| {
            source
                .split_once(signature)
                .unwrap_or_else(|| panic!("{signature}"))
                .1
                .split_once("\n    }")
                .expect("it ends")
                .0
        };
        let held = body_of("fn holds_focus(&self) -> Option<Held> {");
        assert!(held.contains("Held::Stop"), "{held}");
        assert!(held.contains("Held::Cue("), "{held}");
        assert!(held.contains("self.cues()"), "{held}");
        let capture = body_of("fn capture(&self, svg: Option<Element>, refocus: Option<Held>) {");
        assert!(capture.contains("cue.kind == kind"), "{capture}");
        assert!(capture.contains("cue.stamp == stamp"), "{capture}");
        assert!(
            capture.contains("or_else(|| self.focus_target())"),
            "{capture}"
        );
    }

    /// Decision 18's "at once": a committed seek says the new time in the
    /// frame the key or the release was handled in, and not in the
    /// animation frame that moves the drawing after it. The debounce is
    /// for the clock's own ticks ([`valuetext_due`]) and never for a
    /// gesture, and a value one frame late is a value the reader hears
    /// after the seek they made.
    #[test]
    fn a_committed_seek_says_the_new_time_before_the_frame_that_draws_it() {
        let source = include_str!("chart.rs");
        let act = source
            .split_once("fn act(&self, intent: Intent) {")
            .expect("the act")
            .1;
        let act = act.split_once("\n    }").expect("it ends").0;
        // the film answers where it stands, so the words go in after the
        // seek is emitted and before `act` returns
        let emit = act
            .find("emit_time(SEEK_EVENT")
            .expect("the seek is emitted");
        let write = act
            .find("self.write_value(")
            .expect("act must say the new time itself");
        assert!(emit < write, "{act}");
        // what is said is the film's answer and not the time asked for: the
        // clock is the film's, and a value that named the request would
        // disagree with the readout beside it
        assert!(act.contains("self.live.borrow().time"), "{act}");
        // and the write takes the flag the animation frame reads, so one
        // seek is not announced twice
        assert!(act.contains("self.value_at_once.replace(false)"), "{act}");
    }

    /// Decision 18's commit message, and the order that decides whether a
    /// reader hears it.
    ///
    /// A seek made while the film plays pauses it. The film's pause
    /// renders, and rendering dispatches a tick with the play flag turned
    /// over, which this element hears and answers with "Paused" - inside
    /// the emit, before the emit returns. So a message written before the
    /// emit is the message that is overwritten, in exactly the case a
    /// reader seeks most: while watching.
    ///
    /// The order is all this can hold from here. That the region's last
    /// word is the seek and not the pause is a fact about two elements in
    /// a live document, and only the browser can say it: the check the
    /// interaction report needs is a seek driven while the film plays,
    /// reading the region afterwards.
    #[test]
    fn a_seek_is_announced_after_the_emit_and_names_the_time_the_clock_reached() {
        // the time the message names: the clock's own answer where the
        // emit moved it, and the request where it did not. The two differ
        // wherever the announced duration outlives the film, which is
        // where End asks for a time the film cannot reach
        assert_eq!(seeked_time(3.3, 0.0, 3.0), 3.0);
        assert_eq!(seeked_time(3.3, 1.0, 1.0), 3.3);
        // a seek to where the clock already stands is answered by no move
        // at all, and the time asked for is the time it is on
        assert_eq!(seeked_time(2.0, 2.0, 2.0), 2.0);
        assert_eq!(seeked_time(0.0, 1.5, 0.0), 0.0);

        let source = include_str!("chart.rs");
        let act = source
            .split_once("fn act(&self, intent: Intent) {")
            .expect("the act")
            .1;
        let act = act.split_once("\n    }").expect("it ends").0;
        let emit = act
            .find("emit_time(SEEK_EVENT")
            .expect("the seek is emitted");
        let said = act.find("Said::Seeked").expect("the seek is announced");
        assert!(
            emit < said,
            "the seek is announced before the emit that can overwrite it: {act}"
        );
        // and what it names is the decision above and not the request
        assert!(act.contains("seeked_time("), "{act}");
        assert!(
            !act.contains("message(Said::Seeked, t,"),
            "the message names the time asked for: {act}"
        );
    }

    /// Decision 18's status region: in the shadow root from the first
    /// render, empty, and never made when a message arrives. A live region
    /// created to hold a message is not observed changing, so nothing is
    /// announced at all.
    #[test]
    fn the_status_region_ships_empty_in_every_render_and_is_never_assertive() {
        for markup in [one_chart(), chart_owning(false), built().shadow] {
            assert_eq!(markup.matches(STATUS).count(), 1, "{markup}");
            assert_eq!(markup.matches("role=\"status\"").count(), 1, "{markup}");
            // it ships empty: the region is the tree the message lands in,
            // not the message
            assert!(markup.contains("aria-live=\"polite\"></div>"), "{markup}");
            // one region for a pre-render's two charts as much as for one
            assert_eq!(markup.matches("aria-live").count(), 1, "{markup}");
            // and nothing here interrupts a reader
            assert!(!markup.contains("assertive"), "{markup}");
            assert!(!markup.contains("aria-atomic"), "{markup}");
            // after the svg and before the table, where decision 15 puts it
            let at = markup.find(STATUS).expect("the region");
            assert!(
                at > markup.find("</figure>").expect("the figure"),
                "{markup}"
            );
            assert!(at < markup.find("<details").expect("the table"), "{markup}");
        }
        // it is said, not read: the region is out of sight, as the film's
        // own is
        let css = stylesheet(DEFAULT_RATIO);
        assert!(
            css.contains(
                ".live { position: absolute; width: 1px; height: 1px; overflow: hidden; \
                 clip: rect(0 0 0 0); white-space: nowrap; }"
            ),
            "{css}"
        );
    }

    /// A time as a listener hears it (decision 18). The tenth is kept
    /// because this chart is drawn for films of a few seconds, where whole
    /// seconds would give every position on the track the same words; over
    /// a minute it falls out on its own, since a minute of film is not
    /// sampled that finely.
    #[test]
    fn a_time_is_said_in_words_and_keeps_the_tenth_a_short_film_needs() {
        // decision 18's own example, and the total beside it
        assert_eq!(in_words(92.0), "1 minute 32 seconds");
        assert_eq!(in_words(243.0), "4 minutes 3 seconds");
        // the singular is said as a singular, at either scale
        assert_eq!(in_words(1.0), "1 second");
        assert_eq!(in_words(61.0), "1 minute 1 second");
        assert_eq!(in_words(60.0), "1 minute");
        assert_eq!(in_words(120.0), "2 minutes");
        // a whole minute says no seconds at all, and zero says seconds
        assert_eq!(in_words(0.0), "0 seconds");
        // the tenth the chart's own films need
        assert_eq!(in_words(3.3), "3.3 seconds");
        assert_eq!(in_words(0.1), "0.1 seconds");
        assert_eq!(in_words(92.5), "1 minute 32.5 seconds");
        // rounded to the tenth, and across the minute with it
        assert_eq!(in_words(3.34), "3.3 seconds");
        assert_eq!(in_words(59.97), "1 minute");
        // and a clock that ran below zero is still a time
        assert_eq!(in_words(-1.0), "0 seconds");
    }

    /// The four things the status region says, and no fifth: a peek and a
    /// seek being aimed say nothing at all (decision 18).
    #[test]
    fn each_thing_the_status_region_says_has_its_own_words() {
        assert_eq!(
            message(Said::Seeked, 92.0, "Boot"),
            "Seeked to 1 minute 32 seconds, chapter Boot"
        );
        // a block that names no chapter leaves the clause out rather than
        // saying an empty name
        assert_eq!(
            message(Said::Seeked, 92.0, ""),
            "Seeked to 1 minute 32 seconds"
        );
        // play and pause say what happened and no more: where the clock is
        // stands in the thumb's own value
        assert_eq!(message(Said::Playing, 92.0, "Boot"), "Playing");
        assert_eq!(message(Said::Paused, 92.0, "Boot"), "Paused");
        // the chapter message is the context response, a few words
        assert_eq!(message(Said::Chapter, 92.0, "Boot"), "Chapter Boot");
        assert_eq!(
            message(Said::Chapter, 92.0, ""),
            "Chapter at 1 minute 32 seconds"
        );
        // all four differ, so no two of them are the same announcement
        let all = [Said::Seeked, Said::Playing, Said::Paused, Said::Chapter];
        let said: std::collections::BTreeSet<String> = all
            .iter()
            .map(|said| message(*said, 92.0, "Boot"))
            .collect();
        assert_eq!(said.len(), all.len(), "{said:?}");
        // and none of them is about a preview
        for words in said {
            for never in ["peek", "pending", "aiming", "preview"] {
                assert!(!words.to_lowercase().contains(never), "{words}");
            }
        }
    }

    /// The thumb's `aria-valuetext`: the time in words with the chapter
    /// appended, and the whole timeline and the frame index only where the
    /// thumb has not got the focus (decision 18). A listener stepping along
    /// the track is told the total once, when focus arrives, and not again
    /// on every step.
    #[test]
    fn the_valuetext_carries_the_total_and_the_frame_only_off_focus() {
        let whole = Whole {
            duration: 243.0,
            frame: 12,
            frames: 40,
        };
        assert_eq!(
            valuetext(92.0, "Boot", Some(whole)),
            "1 minute 32 seconds of 4 minutes 3 seconds, frame 12 of 40, chapter Boot"
        );
        assert_eq!(
            valuetext(92.0, "Boot", None),
            "1 minute 32 seconds, chapter Boot"
        );
        // the chapter clause goes where the block names none, at either
        // length
        assert_eq!(valuetext(92.0, "", None), "1 minute 32 seconds");
        assert_eq!(
            valuetext(92.0, "", Some(whole)),
            "1 minute 32 seconds of 4 minutes 3 seconds, frame 12 of 40"
        );
        // and a chart with nothing sampled names no frame, having none
        assert_eq!(
            valuetext(
                0.0,
                "flight",
                Some(Whole {
                    duration: 3.3,
                    frame: 1,
                    frames: 0
                })
            ),
            "0 seconds of 3.3 seconds, chapter flight"
        );
    }

    /// A chapter boundary is crossed once, on the tick that passes it, and
    /// only going forwards: that is what makes the status message
    /// rate-limited to chapters and never one per sample (decision 18).
    #[test]
    fn a_chapter_is_entered_once_going_forwards_and_never_by_standing_still() {
        let starts = [0.0, 1.65, 2.36];
        // the tick that steps over the boundary, and the one that lands on
        // it exactly
        assert_eq!(chapter_crossed(1.0, 2.0, &starts), Some(1));
        assert_eq!(chapter_crossed(1.0, 1.65, &starts), Some(1));
        // and never again after that: a boundary at `prev` was entered on
        // the tick before, which is the rate limit itself
        assert_eq!(chapter_crossed(1.65, 2.0, &starts), None);
        assert_eq!(chapter_crossed(1.7, 2.0, &starts), None);
        // a tick that steps over two names the chapter it ends in
        assert_eq!(chapter_crossed(0.5, 3.0, &starts), Some(2));
        // the first chapter starts the axis: a clock at zero is already in
        // it and does not enter it
        assert_eq!(chapter_crossed(0.0, 0.5, &starts), None);
        // standing still crosses nothing, however long the film plays on
        // one sample
        assert_eq!(chapter_crossed(2.0, 2.0, &starts), None);
        // and neither does running back: a seek is announced as a seek
        assert_eq!(chapter_crossed(2.0, 1.0, &starts), None);
        assert_eq!(chapter_crossed(3.0, 0.0, &starts), None);
        // a film with no chapters announces none
        assert_eq!(chapter_crossed(0.0, 9.0, &[]), None);
    }

    /// A clock-driven value waits for the focus and for the debounce; a
    /// gesture does not wait at all (decision 18).
    #[test]
    fn a_clock_driven_value_waits_for_the_focus_and_for_the_debounce() {
        assert_eq!(VALUETEXT_WAIT, 0.3);
        // focused, and the wait is over
        assert!(valuetext_due(1.0, 1.3, true));
        assert!(valuetext_due(1.0, 9.0, true));
        // focused, and it is not
        assert!(!valuetext_due(1.0, 1.29, true));
        assert!(!valuetext_due(1.0, 1.0, true));
        // the wait being over is not enough: an unfocused thumb is not
        // being spoken, so a playing film writes nothing onto it
        assert!(!valuetext_due(1.0, 9.0, false));
        assert!(!valuetext_due(1.0, 1.3, false));
        // and the first write of all is due as soon as the thumb is
        // focused, having no last write to wait behind
        assert!(valuetext_due(f64::NEG_INFINITY, 0.0, true));
    }

    /// Decision 17's answer: a keyboard seek commits
    /// immediately and never enters the pending state. Three things hold
    /// it. No key previews, and the preview is what a pending seek is; a
    /// pending phase can only come out of a press, which a key has not
    /// made; and a seek ends whatever press is in flight before it emits,
    /// so even a key struck mid-drag commits rather than aiming.
    #[test]
    fn a_key_seek_commits_at_once_and_never_enters_the_pending_state() {
        assert!(ends_press(Intent::Seek(1.0)));
        assert!(ends_press(Intent::Cancel));
        assert!(!ends_press(Intent::Peek(Some(1.0))));
        assert!(!ends_press(Intent::Peek(None)));
        assert!(!ends_press(Intent::Toggle));

        // no key the element answers previews anything
        let times = key_times();
        let ctx = Keys {
            times: &times,
            duration: 3.3,
            chapters: &[0.0, 1.65],
            step: None,
        };
        let mut seeks = 0;
        for (keys, _) in KEY_LINE {
            for key in *keys {
                for shift in [false, true] {
                    for chapter_mod in [false, true] {
                        let Some(intent) = key_intent(key, shift, chapter_mod, 1.0, &ctx) else {
                            continue;
                        };
                        assert!(
                            !matches!(intent, Intent::Peek(_)),
                            "`{key}` previews: {intent:?}"
                        );
                        if matches!(intent, Intent::Seek(_)) {
                            seeks += 1;
                            assert!(ends_press(intent), "`{key}` seeks without committing");
                        }
                    }
                }
            }
        }
        assert!(seeks > 0, "no key seeks at all");

        // and with no press there is no phase to be pending in
        for x in [-10.0, 0.0, 7.0, 400.0] {
            for elapsed in [0.0, LONG_PRESS, 10.0] {
                for coarse in [false, true] {
                    assert_eq!(pointer_phase(None, x, elapsed, coarse), Phase::Idle);
                }
            }
        }

        // the element asks the rule before it acts on the intent, so the
        // press is back before the seek is emitted
        let source = include_str!("chart.rs");
        let act = source
            .split_once("fn act(&self, intent: Intent) {")
            .expect("the act")
            .1;
        let act = act.split_once("\n    }").expect("it ends").0;
        let guard = act
            .find("ends_press(intent)")
            .expect("act must ask whether the intent ends the press");
        let seek = act.find("Intent::Seek(t) =>").expect("the seek arm");
        assert!(guard < seek, "{act}");
    }

    /// Every id the element names itself and its description by is in the
    /// shadow root, once: a reference to a missing id names nothing, and a
    /// duplicate id makes which of them is named a matter of order.
    #[test]
    fn every_id_the_naming_relies_on_is_in_the_shadow_root_exactly_once() {
        for markup in [one_chart(), built().shadow] {
            // the host points at these three through its internals
            for id in [TITLE_ID, SUMMARY_ID, KEYS_ID] {
                assert_eq!(
                    markup.matches(&format!("id=\"{id}\"")).count(),
                    1,
                    "`{id}` is not in the shadow root exactly once"
                );
            }
            // and every id the markup itself names resolves to one of them
            let named: Vec<String> = markup
                .split("aria-labelledby=\"")
                .skip(1)
                .chain(markup.split("aria-describedby=\"").skip(1))
                .map(|rest| rest.split_once('"').expect("a closing quote").0.to_owned())
                .collect();
            assert!(!named.is_empty(), "nothing is named at all");
            for reference in named {
                for id in reference.split_whitespace() {
                    assert_eq!(
                        markup.matches(&format!("id=\"{id}\"")).count(),
                        1,
                        "`{id}` is named but is not in the shadow root once"
                    );
                }
            }
        }
        // the two the host describes itself by are the summary and the
        // instructions, in that order
        assert_eq!(DESCRIBED_BY, [SUMMARY_ID, KEYS_ID]);
    }

    /// Decision 15's instructions line, held to the keys the element really
    /// answers: every key named here is one `key_intent` acts on, and every
    /// key it acts on is named here. The alternative is two lists, and a
    /// reader told to press a key the chart refuses.
    #[test]
    fn the_instructions_line_names_every_key_the_element_answers_and_no_other() {
        let line = instructions();
        assert!(line.starts_with("Keys: ") && line.ends_with('.'), "{line}");
        for (_, words) in KEY_LINE {
            assert!(line.contains(words), "{words} is not in `{line}`");
        }
        // the line is in the markup as visible text, not a hidden
        // description
        let markup = one_chart();
        assert!(
            markup.contains(&format!("<p class=\"keys\" id=\"{KEYS_ID}\">{line}</p>")),
            "{markup}"
        );
        assert!(!markup.contains("aria-hidden=\"true\"><p"), "{markup}");

        let times = key_times();
        let ctx = Keys {
            times: &times,
            duration: KEY_END,
            chapters: &KEY_CHAPTERS,
            step: None,
        };
        // every key a keyboard can send, tried with and without each
        // modifier the table reads
        let mut answered: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        let named = [
            "ArrowLeft",
            "ArrowRight",
            "ArrowUp",
            "ArrowDown",
            "PageUp",
            "PageDown",
            "Home",
            "End",
            "Escape",
            "Enter",
            "Tab",
            "Backspace",
            "Delete",
            "Insert",
            "F1",
            "Shift",
            "Control",
            "Alt",
            "Meta",
            "Dead",
        ];
        let printable: Vec<String> = (0x20u8..0x7f).map(|c| (c as char).to_string()).collect();
        for key in printable.iter().map(String::as_str).chain(named) {
            for (shift, chapter) in [(false, false), (true, false), (false, true), (true, true)] {
                if key_intent(key, shift, chapter, 1.0, &ctx).is_some() {
                    answered.insert(key.to_owned());
                }
            }
        }
        let spelled: std::collections::BTreeSet<String> = KEY_LINE
            .iter()
            .flat_map(|(keys, _)| keys.iter().map(|k| (*k).to_owned()))
            .collect();
        assert_eq!(
            answered, spelled,
            "the keys the chart answers and the keys the line names differ"
        );
        // the probe really did reach the whole table, so this can never
        // pass over an empty set
        assert!(answered.len() > 20, "{answered:?}");

        // and the roving's own keys, which the key table never sees: the
        // same probe through the function that answers them. The entry
        // gesture is the one a reader cannot guess, so the line naming it
        // is held to the code the same way the thumb's keys are
        let mut roving: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for key in printable.iter().map(String::as_str).chain(named) {
            for at in [
                Roving::Thumb,
                Roving::Cue(0),
                Roving::Cue(1),
                Roving::Cue(2),
            ] {
                if roving_key(at, 3, key).is_some() {
                    roving.insert(key.to_owned());
                }
            }
        }
        let spelled: std::collections::BTreeSet<String> = ROVE_LINE
            .iter()
            .flat_map(|(keys, _)| keys.iter().map(|k| (*k).to_owned()))
            .collect();
        assert_eq!(
            roving, spelled,
            "the keys the roving answers and the keys the line names differ"
        );
        // the way in is named, in the sentence a reader standing on the
        // thumb is being told about
        assert!(roving.contains("ArrowDown"), "{roving:?}");
        assert!(line.contains("Down from the playhead"), "{line}");
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
        // the range is announced at a hundredth and not as `f64`'s own
        // Display: a block whose arithmetic left a long tail behind would
        // be read out a digit at a time
        let noisy = op_chart::data::parse(
            r#"{"duration": 1, "series": [{"id": "a", "label": "a"}],
                "rows": [[0, 0.30000000000000004], [1, 0.7000000000000001]]}"#,
        )
        .expect("valid");
        assert!(
            summary_of(&noisy).contains("values from 0.3 to 0.7."),
            "{}",
            summary_of(&noisy)
        );
        assert!(
            summary_of(&noisy).ends_with(" Overall, a rises from 0.3 to 0.7."),
            "{}",
            summary_of(&noisy)
        );
        assert!(summary.ends_with('.'), "{summary}");
        // decision 15's takeaway: what the chart shows, which the list of
        // counts above never says. It is the last sentence, so a reader
        // who stops at the counts has still heard them
        assert!(
            summary.ends_with(
                " Overall, palette rises from 0 to 100 % and solid thumb rises from 0 to 100 %."
            ),
            "{summary}"
        );
        // and it reads the samples rather than the domain: a falling
        // series falls, a flat one holds, and a gap is not an end
        let shapes = op_chart::data::parse(
            r#"{"duration": 2, "series": [
                {"id": "a", "label": "down", "unit": "%"},
                {"id": "b", "label": "flat"},
                {"id": "c", "label": "gappy"}
            ], "rows": [[0, 90, 7, 1], [1, 50, 7, null], [2, 12.5, 7, null]]}"#,
        )
        .expect("valid");
        assert!(
            summary_of(&shapes).ends_with(
                " Overall, down falls from 90 to 12.5 %, flat holds at 7 and gappy holds at 1."
            ),
            "{}",
            summary_of(&shapes)
        );
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
        // and no takeaway at all: a series with no samples has no shape to
        // report, and an invented one is worse than none
        assert!(!summary.contains("Overall"), "{summary}");
        assert!(summary.ends_with("no chapters."), "{summary}");
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
            true,
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
            true,
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
        // below the drop width the chapter ticks go, and their buttons go
        // with them: an invisible chapter with a name and a 24 px target
        // is a control a reader can reach and cannot see
        assert!(css.contains(&format!(
            "@container (max-width: {DROP_AT}px) {{ .tick-label.alt {{ display: none; }} .chapters {{ display: none; }} .chart .targets g[data-cue=\"chapter\"] {{ display: none; }} }}"
        )));
        // the thumb is the tab stop and its ring is a rect in the drawing,
        // shown on `:focus-visible` alone: a press must not ring the chart
        // it is scrubbing. `currentColor` over the focus token, so a
        // forced palette carries it without a mapping of its own
        assert!(
            css.contains(".chart .playhead:focus { outline: none; }"),
            "{css}"
        );
        assert!(
            css.contains(
                ".chart .playhead:focus-visible .head-ring { color: var(--op-focus); stroke: currentColor; stroke-width: 2; }"
            ),
            "{css}"
        );
        // decision 20 asks for the ring and a stronger stroke on the thumb
        // itself, which is a wider line under a fixed 1.5
        assert!(
            css.contains(".chart .playhead:focus-visible .head { stroke-width: 2.5; }"),
            "{css}"
        );
        // and the cue buttons take the same treatment: the site's ring
        // turned off, because a `<g>` carrying an outline is drawn in user
        // space, and an in-SVG indicator in its place, with hover and
        // focus painting the same stroke off the same `color` so focus
        // mirrors hover
        assert_eq!(CUE_RULE, format!(".chart {CUE_BUTTONS}"));
        for rule in [
            format!("{CUE_RULE}:focus {{ outline: none; }}"),
            format!("{CUE_RULE}:hover {{ color: var(--op-accent); }}"),
            format!("{CUE_RULE}:focus-visible {{ color: var(--op-focus); }}"),
            format!(
                "{CUE_RULE}:hover .target, {CUE_RULE}:focus-visible .target \
                 {{ stroke: currentColor; stroke-width: 2; }}"
            )
            .replace("                 ", ""),
        ] {
            assert!(css.contains(&rule), "`{rule}` is not in {css}");
        }
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
    /// and the only rules in either stylesheet that name it say which
    /// state they answer: the defect this pins is a target classed with
    /// the cue it stands for, which is the class the drawn rule or tick is
    /// painted by, so every target drew a dashed box, all the time.
    ///
    /// Decision 20's hover and focus styling is the one thing a target is
    /// allowed to show, because a cue's button carries no geometry of its
    /// own and the rect inside it is the only shape there is to indicate.
    /// A rule that reaches a target and says nothing about hover or focus
    /// paints it at rest, which is the defect either way round.
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
        let mut indicated = 0usize;
        for class in &targets {
            for (at, selector, body) in &rules {
                if !could_reach(selector, class) {
                    continue;
                }
                assert!(
                    selector.contains(":hover") || selector.contains(":focus-visible"),
                    "the rule `{at} {selector} {{{body}}}` paints a hit target \
                     classed `{class}` at rest"
                );
                indicated += 1;
            }
        }
        // and the indicator really is written, so the exception above is
        // not a hole kept open for nothing: decision 20's last clause is
        // the reason a target may be reached at all
        assert!(
            indicated > 0,
            "no rule shows a cue's target on hover or focus"
        );
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
            step: None,
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
            step: None,
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
            step: None,
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
            step: None,
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
            step: None,
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
            step: None,
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

    /// Decision 17's `step` attribute: a chart may state what a key is
    /// worth, in seconds, and it overrides the sample step and nothing
    /// else. The rows here are deliberately irregular, so that no sample
    /// step is the stated one by accident and every answer below tells the
    /// two apart.
    #[test]
    fn the_step_attribute_overrides_the_sample_step_and_no_other_key() {
        // the attribute as the element reads it: seconds, positive, finite
        assert_eq!(step_attr(Some("0.25")), Some(0.25));
        assert_eq!(step_attr(Some("  2  ")), Some(2.0));
        assert_eq!(step_attr(Some("1e-1")), Some(0.1));
        // and nonsense is ignored rather than carried into the arithmetic:
        // a step of zero would leave every key seeking where it already
        // is, and a negative one would send the arrows backwards
        for nonsense in [
            None,
            Some(""),
            Some("   "),
            Some("half a second"),
            Some("0"),
            Some("-0.5"),
            Some("inf"),
            Some("NaN"),
            Some("0,25"),
            Some("0.25s"),
        ] {
            assert_eq!(step_attr(nonsense), None, "{nonsense:?}");
        }

        let times = [0.0, 0.05, 0.3, 9.0, 10.0, 10.5, 24.0, 30.0];
        let chapters = [0.0, 12.0];
        let plain = Keys {
            times: &times,
            duration: 30.0,
            chapters: &chapters,
            step: None,
        };
        let stepped = Keys {
            step: Some(0.25),
            ..plain
        };
        let at = 10.0;
        // the sample step, which walks the rows however far apart they are
        for (key, want) in [
            (",", 9.0),
            (".", 10.5),
            ("ArrowLeft", 0.0),
            ("ArrowRight", 30.0),
            ("j", 0.0),
            ("l", 30.0),
        ] {
            seeks(key_intent(key, false, false, at, &plain), want);
        }
        // and the stated step, which counts in seconds: one, five and ten
        // of them, as the samples were one, five and ten rows
        for (key, want) in [
            (",", 9.75),
            (".", 10.25),
            ("ArrowLeft", 8.75),
            ("ArrowRight", 11.25),
            ("j", 7.5),
            ("l", 12.5),
        ] {
            seeks(key_intent(key, false, false, at, &stepped), want);
        }
        // every other key is untouched: the chapter keys and their alias
        // walk the chapters, Home and End and the digits are fractions of
        // the announced duration, and Shift with an arrow was a second
        // before the attribute and is a second after it
        for (key, shift, chapter_mod) in [
            ("PageUp", false, false),
            ("PageDown", false, false),
            ("ArrowLeft", false, true),
            ("ArrowRight", false, true),
            ("ArrowLeft", true, false),
            ("ArrowRight", true, false),
            ("Home", false, false),
            ("End", false, false),
            ("0", false, false),
            ("5", false, false),
            ("9", false, false),
            (" ", false, false),
            ("Escape", false, false),
        ] {
            assert_eq!(
                key_intent(key, shift, chapter_mod, at, &stepped),
                key_intent(key, shift, chapter_mod, at, &plain),
                "{key} answered differently with a step set"
            );
        }
        // the stated step is held inside the timeline like every other key
        seeks(key_intent("l", false, false, 29.0, &stepped), 30.0);
        seeks(key_intent(",", false, false, 0.1, &stepped), 0.0);
        // a chart with no rows at all can still be driven once it says
        // what a key is worth: without the attribute there is no sample to
        // step onto and the key is refused
        let bare = Keys {
            times: &[],
            duration: 30.0,
            chapters: &chapters,
            step: None,
        };
        assert_eq!(key_intent(".", false, false, at, &bare), None);
        seeks(
            key_intent(
                ".",
                false,
                false,
                at,
                &Keys {
                    step: Some(0.25),
                    ..bare
                },
            ),
            10.25,
        );
        // and a nonsense attribute leaves the whole table exactly as it
        // was, which is the point of ignoring it rather than crashing
        let ignored = Keys {
            step: step_attr(Some("half a second")),
            ..plain
        };
        for key in [",", ".", "ArrowLeft", "ArrowRight", "j", "l", "Home", "End"] {
            assert_eq!(
                key_intent(key, false, false, at, &ignored),
                key_intent(key, false, false, at, &plain),
                "{key}"
            );
        }
    }

    /// The attribute is on the element's own surface, so a page may write
    /// it: op-pages builds its vocabulary from the observed attributes, and
    /// an attribute the element does not observe is a lowering error.
    #[test]
    fn the_step_attribute_is_one_a_page_may_write() {
        assert!(
            DEFINITION.observed_attributes.contains(&"step"),
            "{:?}",
            DEFINITION.observed_attributes
        );
        let page = |attrs: &str| {
            format!(
                "<opt:body xmlns:opt=\"https://www.openpower.tools/ns/opt\">\
                 <opt:chart for=\"f\"{attrs}>\
                 <script type=\"application/json\">{BLOCK}</script>\
                 </opt:chart></opt:body>"
            )
        };
        let html = op_pages::lower(&page(" step=\"0.25\"")).expect("a page with a step lowers");
        assert!(html.contains("step=\"0.25\""), "{html}");
        // and the vocabulary really is the gate this passed through
        let errors = op_pages::lower(&page(" pace=\"0.25\"")).expect_err("an unknown attribute");
        assert!(errors.iter().any(|e| e.contains("\"pace\"")), "{errors:?}");
    }

    /// Decision 17's roving tabindex, as the decision behind it: where the
    /// focus goes from where it stands, given the key. One tab stop moves
    /// over the thumb and the cues together, which is what the decision
    /// says and what the criterion reads: a set of arrow-reachable buttons
    /// with no way in and no way to press them is not a roving tabindex,
    /// it is a fixed stop beside some unreachable markup.
    #[test]
    fn the_roving_moves_one_stop_between_the_thumb_and_the_cues() {
        use Roved::{Press, To};
        use Roving::{Cue, Thumb};
        // in, and only by the key the instructions line names: the entry
        // gesture decision 17 leaves unstated is the whole reason the cues
        // were reachable by nothing at all
        assert_eq!(roving_key(Thumb, 3, "ArrowDown"), Some(To(Cue(0))));
        // and there is nothing to step into where the block draws no cue
        assert_eq!(roving_key(Thumb, 0, "ArrowDown"), None);
        // along the timeline
        assert_eq!(roving_key(Cue(0), 3, "ArrowRight"), Some(To(Cue(1))));
        assert_eq!(roving_key(Cue(1), 3, "ArrowRight"), Some(To(Cue(2))));
        assert_eq!(roving_key(Cue(2), 3, "ArrowLeft"), Some(To(Cue(1))));
        assert_eq!(roving_key(Cue(1), 3, "ArrowLeft"), Some(To(Cue(0))));
        // both ends hold rather than wrapping: the focus not moving is how
        // the reader is told this is the last cue
        assert_eq!(roving_key(Cue(2), 3, "ArrowRight"), None);
        assert_eq!(roving_key(Cue(0), 3, "ArrowLeft"), None);
        // one cue is both ends at once
        assert_eq!(roving_key(Cue(0), 1, "ArrowRight"), None);
        assert_eq!(roving_key(Cue(0), 1, "ArrowLeft"), None);
        // out, by either key a reader who stepped in will try
        assert_eq!(roving_key(Cue(1), 3, "ArrowUp"), Some(To(Thumb)));
        assert_eq!(roving_key(Cue(1), 3, "Escape"), Some(To(Thumb)));
        // and pressed: a `role="button"` answers Enter and Space or it is
        // not a button. Space fell through to the key table before this
        // and played the film, which is the one thing on the chart a press
        // on a chapter must not do
        assert_eq!(roving_key(Cue(1), 3, "Enter"), Some(Press));
        assert_eq!(roving_key(Cue(1), 3, " "), Some(Press));
        // the thumb keeps its own table: Enter is nothing there, Space
        // plays, and Escape drops a seek being aimed
        for key in ["Enter", " ", "Escape", "ArrowLeft", "ArrowRight", "k", "."] {
            assert_eq!(roving_key(Thumb, 3, key), None, "{key} on the thumb");
        }
        // every key that must leave the focus where it is, on a cue
        for key in [
            "Tab", "Home", "End", "PageUp", "PageDown", "k", "j", "l", ",", ".", "0", "9",
        ] {
            assert_eq!(roving_key(Cue(1), 3, key), None, "{key} moved the focus");
        }
        // Tab reaches the key table too, and is refused there as well, so
        // nothing in the element ever prevents its default
        assert_eq!(
            key_intent(
                "Tab",
                false,
                false,
                1.0,
                &Keys {
                    times: &[0.0, 1.0, 2.0],
                    duration: 2.0,
                    chapters: &[0.0],
                    step: None,
                }
            ),
            None
        );

        // the order the roving moves over is the timeline and not the
        // markup: the emitter writes every mark and then every chapter
        // tick, so document order stops being time order as soon as a
        // chart has both
        assert_eq!(in_time_order(&[0.6, 2.1, 1.2]), vec![0, 2, 1]);
        assert_eq!(in_time_order(&[3.0, 2.0, 1.0]), vec![2, 1, 0]);
        // equal times keep the order they were written in
        assert_eq!(in_time_order(&[1.0, 1.0, 0.0]), vec![2, 0, 1]);
        assert_eq!(in_time_order(&[]), Vec::<usize>::new());

        // and the wiring asks the roving before the key table, so a key
        // the roving answers never reaches it
        let source = include_str!("chart.rs");
        let body_of = |signature: &str| {
            source
                .split_once(signature)
                .unwrap_or_else(|| panic!("{signature}"))
                .1
                .split_once("\n    }")
                .expect("it ends")
                .0
        };
        let body = body_of("fn on_key(&self, event: &Event)");
        let roved = ["self.", "roved("].concat();
        let asked = body.find(&roved).expect("the roving is asked");
        let table = body.find("key_intent(").expect("the key table is read");
        assert!(asked < table, "the key table is read before the roving");
        // and every key it answers leaves by one `true`, so a key cannot
        // be acted on and handed to the key table as well
        let body = body_of("fn roved(&self, key: &str) -> bool {");
        assert_eq!(body.matches("\n        true").count(), 1, "{body}");
        assert_eq!(body.matches("true").count(), 1, "{body}");
    }

    /// B2: the roving set is the cues the drawing shows, and the one tab
    /// stop goes nowhere else.
    ///
    /// Below [`DROP_AT`] the container query hides every chapter cue,
    /// button and all. A roving that counted them anyway walked a
    /// set larger than the drawing: one ArrowDown from the thumb wrote the
    /// thumb down to `-1`, wrote the `0` on an invisible chapter and then
    /// failed to focus it, because focus on a `display: none` node does
    /// nothing at all. The element was left with no tab stop a reader
    /// could reach until something else happened to restore one, and on a
    /// chart of chapters and no marks that is the whole widget gone from
    /// the keyboard at the width 1.4.10 and 400 % zoom put a keyboard
    /// reader at.
    ///
    /// Both halves of the answer are here: which cues can be reached now,
    /// and where the stop lands when that set has shrunk under it.
    #[test]
    fn the_roving_walks_only_the_cues_the_drawing_shows() {
        use Roved::To;
        use Roving::{Cue, Thumb};

        // which cues can be reached now, as the one answer every reader of
        // the set takes: a mark at every width, a chapter only above the
        // drop, and the width the rule names belongs to the rule
        assert!(cue_is_drawn(CHAPTER_CUE, DEFAULT_WIDTH));
        assert!(cue_is_drawn(CHAPTER_CUE, DROP_AT + 1.0));
        assert!(!cue_is_drawn(CHAPTER_CUE, DROP_AT));
        assert!(!cue_is_drawn(CHAPTER_CUE, NARROW_WIDTH));
        for width in [NARROW_WIDTH, DROP_AT, DROP_AT + 1.0, DEFAULT_WIDTH] {
            assert!(cue_is_drawn("mark", width), "a mark at {width}");
        }
        // and the threshold and the spelling are the stylesheet's own, so
        // the set cannot drift from the rule that empties it: the rule
        // that hides a chapter cue is found by what this answer is written
        // from, and it is the drop query it sits in
        let hides: Vec<(String, String, String)> = rules_of(&shadow_css())
            .into_iter()
            .filter(|(_, selector, _)| selector.contains(&format!("[data-cue=\"{CHAPTER_CUE}\"]")))
            .collect();
        assert_eq!(hides.len(), 1, "{hides:?}");
        let (at, _, body) = &hides[0];
        assert_eq!(at, &format!("@container (max-width: {DROP_AT}px)"), "{at}");
        assert_eq!(body.trim(), "display: none;", "{body}");

        // the set really does shrink. The block draws one mark and one
        // chapter, and below the drop the chapter is not there to be
        // reached
        let kinds_of = |block: &str| -> Vec<String> {
            let d = op_chart::data::parse(block).expect("the block is valid");
            let svg = svg_of(
                &d.to_spec(),
                &aria_of(&d, true),
                Layout::sized(DEFAULT_WIDTH, DEFAULT_WIDTH / DEFAULT_RATIO, d.end()),
                "chart",
                BY_SITE,
            );
            heads(&svg, "rect")
                .into_iter()
                .filter(|head| attr(head, "part") == "target")
                .map(|head| attr(head, "data-cue"))
                .collect()
        };
        let kinds = kinds_of(BLOCK);
        assert_eq!(kinds, ["mark", CHAPTER_CUE]);
        let shown = |kinds: &[String], width: f64| -> usize {
            kinds
                .iter()
                .filter(|kind| cue_is_drawn(kind, width))
                .count()
        };
        assert_eq!(shown(&kinds, DEFAULT_WIDTH), 2);
        assert_eq!(shown(&kinds, NARROW_WIDTH), 1);

        // where the stop lands, given that set and the member the caller
        // asked for. Both ends of it are members
        assert_eq!(stop_lands(3, Some(0)), Cue(0));
        assert_eq!(stop_lands(3, Some(2)), Cue(2));
        // the element's own stop is where nothing was asked for, and it is
        // all an empty set can answer: a drawing showing no cue has one
        // node in the tab order and it is the thumb
        assert_eq!(stop_lands(3, None), Thumb);
        assert_eq!(stop_lands(0, None), Thumb);
        assert_eq!(stop_lands(0, Some(0)), Thumb);
        // and a member the drawing does not have is the thumb as well,
        // never a `0` written on a node that would refuse the focus
        assert_eq!(stop_lands(3, Some(3)), Thumb);
        assert_eq!(stop_lands(1, Some(1)), Thumb);

        // the shrink itself: the reader stands on the chapter at the wide
        // width, the container narrows, and the stop lands back on the
        // thumb rather than on the cue that has just gone
        let chapter = kinds
            .iter()
            .position(|kind| kind == CHAPTER_CUE)
            .expect("the block draws a chapter");
        assert_eq!(
            stop_lands(shown(&kinds, DEFAULT_WIDTH), Some(chapter)),
            Cue(1)
        );
        assert_eq!(
            stop_lands(shown(&kinds, NARROW_WIDTH), Some(chapter)),
            Thumb
        );

        // and the trap as it was. A chart of chapters and no marks draws
        // no reachable cue at all below the drop, so the entry gesture has
        // nowhere to go and the stop stays where a reader can stand.
        // Counting the hidden ones answered ArrowDown with the first of
        // them, which is the tab stop leaving the thumb for a node that
        // cannot take the focus
        const CHAPTERS_ONLY: &str = r#"{
            "duration": 3.3,
            "series": [{"id": "palette", "label": "palette", "unit": "%"}],
            "rows": [[0, 0], [1.65, 43.5], [3.3, 100]],
            "chapters": [
                {"t": 0, "title": "flight"},
                {"t": 1.65, "title": "settle"},
                {"t": 2.36, "title": "hold"}
            ]
        }"#;
        let chapters = kinds_of(CHAPTERS_ONLY);
        assert_eq!(chapters, [CHAPTER_CUE, CHAPTER_CUE]);
        assert_eq!(shown(&chapters, DEFAULT_WIDTH), 2);
        assert_eq!(shown(&chapters, NARROW_WIDTH), 0);
        assert_eq!(
            roving_key(Thumb, shown(&chapters, DEFAULT_WIDTH), "ArrowDown"),
            Some(To(Cue(0)))
        );
        assert_eq!(
            roving_key(Thumb, shown(&chapters, NARROW_WIDTH), "ArrowDown"),
            None
        );
        // the whole markup is what the roving used to count, and counting
        // it below the drop is the defect: the answer must not be the same
        // as the answer over what is drawn there
        assert_eq!(
            roving_key(Thumb, chapters.len(), "ArrowDown"),
            Some(To(Cue(0)))
        );

        // one place answers which cues can be reached, and every reader of
        // the set goes through it: the roving's keys, the lookup that says
        // where the focus stands, the record a re-layout puts back and the
        // write itself
        let source = include_str!("chart.rs");
        let body_of = |signature: &str| {
            source
                .split_once(signature)
                .unwrap_or_else(|| panic!("{signature}"))
                .1
                .split_once("\n    }")
                .expect("it ends")
                .0
        };
        let read = body_of("fn cues(&self) -> Vec<Cue> {");
        assert!(read.contains("cue_is_drawn("), "{read}");
        assert!(read.contains("self.width()"), "{read}");
        for reader in [
            "fn holds_focus(&self) -> Option<Held> {",
            "fn move_stop(&self, to: &Element) {",
            "fn roved(&self, key: &str) -> bool {",
            "fn capture(&self, svg: Option<Element>, refocus: Option<Held>) {",
        ] {
            let body = body_of(reader);
            assert!(body.contains(".cues()"), "{reader}: {body}");
            assert!(!body.contains("query_selector_all"), "{reader}: {body}");
        }
        // and the index lookup is handed that same set rather than reading
        // one of its own, so the positions the arrows move over are the
        // positions the write knows
        let at = body_of("fn roving_at(&self, cues: &[Cue], order: &[usize]) -> Option<Roving> {");
        assert!(!at.contains(".cues()"), "{at}");
        let roved = body_of("fn roved(&self, key: &str) -> bool {");
        assert_eq!(roved.matches(".cues()").count(), 1, "{roved}");
        // the question is asked in the one place and nowhere else, so no
        // caller can hold a second opinion about what is drawn
        let live = source.split_once("mod tests {").expect("the tests").0;
        assert_eq!(live.matches("cue_is_drawn(").count(), 2, "{live}");

        // and the write follows the decision instead of running ahead of
        // it: the `0` used to go on before a focus that could silently do
        // nothing
        let write = body_of("fn move_stop(&self, to: &Element) {");
        let decided = write.find("stop_lands(").expect("the stop is decided");
        let written = write
            .find("set_attribute(\"tabindex\"")
            .expect("the stop is written");
        let focused = write.find("give_focus(").expect("the focus follows");
        assert!(decided < written, "the stop is written first: {write}");
        assert!(written < focused, "{write}");
        // and the fallback really is the stop: the one member of the set
        // that is always drawn, and the node the focus then goes to
        assert!(write.contains("Roving::Thumb => stop.as_ref()"), "{write}");
        assert!(write.contains("give_focus(onto)"), "{write}");
    }

    /// The roving is a tabindex that moves, not a fixed stop with arrow
    /// keys beside it. The emitter draws the set at rest and the element
    /// moves the attribute; both halves are here, because either alone is
    /// the thing the criterion was read as met by and was not.
    #[test]
    fn the_tab_stop_is_an_attribute_that_moves_over_the_whole_set() {
        let markup = one_chart();
        // at rest: the stop is on the thumb and every cue is at -1
        let stops: Vec<&str> = heads(&markup, "g")
            .into_iter()
            .chain(heads(&markup, "svg"))
            .filter(|head| attr(head, "tabindex") == "0")
            .collect();
        assert_eq!(stops.len(), 1, "{stops:?}");
        assert_eq!(attr(stops[0], "class"), "playhead");
        let cues: Vec<&str> = heads(&markup, "g")
            .into_iter()
            .filter(|head| attr(head, "role") == "button")
            .collect();
        assert!(!cues.is_empty(), "{markup}");
        for cue in &cues {
            assert_eq!(attr(cue, "tabindex"), "-1", "{cue}");
        }

        // and the element moves it: the one place that hands the focus to
        // a node writes the attribute on every member of the set as it
        // goes, so exactly one of them is ever in the tab order
        let source = include_str!("chart.rs");
        let body = source
            .split_once("fn move_stop(&self, to: &Element) {")
            .expect("the stop mover")
            .1
            .split_once("\n    }")
            .expect("it ends")
            .0;
        assert!(body.contains("set_attribute(\"tabindex\""), "{body}");
        assert!(body.contains("self.cues()"), "{body}");
        assert!(body.contains("self.focus_target()"), "{body}");
        assert!(body.contains("give_focus(onto)"), "{body}");
        // and it is the only writer of the attribute, so no path can put a
        // second stop in the set. The needle is assembled, so these lines
        // are not among the occurrences it counts
        let writes = ["set_attribute(\"tab", "index\""].concat();
        assert_eq!(source.matches(&writes).count(), 1, "{source}");
        // every mover goes through it, so none of them can move the focus
        // and leave the tab order behind
        for mover in [
            "fn capture(&self, svg: Option<Element>, refocus: Option<Held>) {",
            "fn on_down(&self, event: &Event) {",
            "fn roved(&self, key: &str) -> bool {",
            "fn on_arrival(&self) {",
        ] {
            let body = source
                .split_once(mover)
                .unwrap_or_else(|| panic!("{mover}"))
                .1
                .split_once("\n    }")
                .expect("it ends")
                .0;
            assert!(body.contains("self.move_stop("), "{mover}: {body}");
            assert!(
                !body.contains("give_focus("),
                "{mover} moves the focus alone: {body}"
            );
        }
    }

    /// Decision 20's ring, as the two numbers 2.4.13 asks for rather than
    /// as a colour name. The ring is painted with the focus token over the
    /// surface, and the state it changes from is derived from the
    /// stylesheet rather than assumed: every rule that can reach the ring
    /// is read, and one that painted it while unfocused would be the other
    /// colour of the second pair. There is none, so the ring's own pixels
    /// show the surface until the focus arrives.
    #[test]
    fn the_focus_ring_clears_three_to_one_against_the_surface_and_against_itself() {
        let css = shadow_css();
        let ring: Vec<(String, String, String)> = rules_of(&css)
            .into_iter()
            .filter(|(_, selector, _)| could_reach(selector, "head-ring"))
            .collect();
        // the ring is painted once, on `:focus-visible` alone: a press must
        // not ring the chart it is scrubbing
        assert_eq!(ring.len(), 1, "{ring:?}");
        let (at, selector, body) = &ring[0];
        assert!(at.is_empty(), "the ring is painted inside {at}");
        assert!(selector.contains(":focus-visible"), "{selector}");
        assert!(body.contains("stroke: currentColor"), "{body}");
        assert!(body.contains("color: var(--op-focus)"), "{body}");
        assert!(body.contains("stroke-width: 2"), "{body}");
        // what those pixels show when the ring is not shown: whatever a
        // rule outside `:focus-visible` strokes it with, and the surface
        // where there is none, since a rect with no stroke paints nothing
        // and the ring's interior is `fill-opacity` 0. The token is read
        // out of the stylesheet, so a rule that started painting the ring
        // unfocused would be measured against instead of ignored
        let painted: Vec<String> = ring
            .iter()
            .filter(|(_, selector, _)| !selector.contains(":focus-visible"))
            .flat_map(|(_, _, body)| body.split(';').map(str::to_owned).collect::<Vec<String>>())
            .filter(|d| d.trim_start().starts_with("stroke:"))
            .filter_map(|d| {
                let (_, value) = d.split_once(':')?;
                let token = value.trim().strip_prefix("var(")?.strip_suffix(')')?;
                Some(token.trim().to_owned())
            })
            .collect();
        assert!(painted.len() <= 1, "{painted:?}");
        let unfocused = painted
            .first()
            .cloned()
            .unwrap_or_else(|| "--op-surface".to_owned());

        for (theme, tokens) in [
            ("dark", crate::palette::dark()),
            ("light", crate::palette::light()),
        ] {
            let colour = |name: &str| {
                op_colour::Srgb::from_hex(&tokens[name])
                    .unwrap_or_else(|| panic!("{name} is not #RRGGBB"))
            };
            let focus = colour("--op-focus");
            let surface = colour("--op-surface");
            // 2.4.13's first floor: the indicator against what is behind it
            let against_surface = op_colour::wcag_contrast(focus, surface);
            assert!(
                against_surface >= 3.0,
                "{theme}: the ring is {against_surface:.2}:1 against the surface"
            );
            // and its second: the same pixels, focused against unfocused,
            // where unfocused is whatever the stylesheet says the ring is
            // painted with before the focus arrives
            let against_itself = op_colour::wcag_contrast(focus, colour(&unfocused));
            assert!(
                against_itself >= 3.0,
                "{theme}: the ring changes by {against_itself:.2}:1 from {unfocused} when it is shown"
            );
        }
    }

    /// Whether a rule with this selector could reach the element whose
    /// opening tag is `head`, by a class of its own or by an attribute
    /// selector the element answers.
    ///
    /// [`could_reach`] reads the class attribute and nothing else, which
    /// is how the site's own ring came to be drawn around the cue buttons
    /// unseen: a `<g>` carrying no class is named by no class, so every
    /// rule that could reach it was skipped before the scan ran.
    fn could_reach_head(selector: &str, head: &str) -> bool {
        if could_reach(selector, &attr(head, "class")) {
            return true;
        }
        selector.split('[').skip(1).any(|rest| {
            let Some((name, value)) = rest.split_once('=') else {
                return false;
            };
            let value = value
                .split(']')
                .next()
                .unwrap_or_default()
                .trim_matches(['"', '\'']);
            let name = name.trim_end_matches(['^', '$', '*', '~', '|']);
            !value.is_empty() && attr(head, name) == value
        })
    }

    /// Whether a rule reaches every element in the tree, the drawing's
    /// nodes included: its selector names no element of its own at all,
    /// only the state it answers. The site's shared `:focus-visible` ring
    /// is the one of these that matters, and it is exactly the rule a
    /// class-only reader cannot see.
    fn reaches_everything(selector: &str) -> bool {
        selector.split(',').any(|part| {
            let named = part.split(':').next().unwrap_or_default().trim();
            named.is_empty() || named == "*"
        })
    }

    /// Every declaration in a block that sets an outline, as its property
    /// and its value.
    fn outlines(body: &str) -> Vec<(&str, &str)> {
        body.split(';')
            .filter_map(|declaration| declaration.split_once(':'))
            .map(|(property, value)| (property.trim(), value.trim()))
            .filter(|(property, _)| *property == "outline" || property.starts_with("outline-"))
            .collect()
    }

    /// Decision 20's rule against outlines on SVG. An outline on an SVG
    /// node is laid out in user space: it scales with the viewBox and the
    /// viewport clips it, so the 2 px a focus indicator is asked for
    /// becomes whatever the box happens to be drawn at, and at the edges
    /// it is cut off.
    ///
    /// The guard is read two ways, because one of them was the hole. A
    /// rule that names a class of the drawing is read as before, and may
    /// set no outline but `none`. A rule that names nothing at all reaches
    /// every element in the tree, the drawing's nodes included, and cannot
    /// be forbidden: it is the site's own `:focus-visible` ring, which the
    /// chart's `<summary>` and every other component's controls want. So
    /// every node of the drawing that can take focus has to turn it off by
    /// name, and that is what is checked here, over the nodes the emitter
    /// writes rather than a list kept beside them.
    ///
    /// The root svg is not one of those nodes. It is a replaced element in
    /// CSS layout, so an outline on it is a box outline in CSS px and is
    /// the site's ring doing its job; that is the indicator an embedded
    /// chart, whose svg is its only tab stop, is meant to have.
    #[test]
    fn no_rule_can_put_an_outline_on_a_node_of_the_drawing() {
        // every node inside the drawing a focus can land on, read out of
        // the markup both ways round: the thumb where this chart owns its
        // slider, and the cue buttons either way
        let markup = [chart_owning(true), chart_owning(false)];
        let mut focusable: Vec<String> = Vec::new();
        for one in &markup {
            let svg = one.split_once("<svg class=\"chart\"").expect("a chart").1;
            for tag in ["g", "rect", "line", "path", "text", "circle"] {
                for head in heads(svg, tag) {
                    if !attr(head, "tabindex").is_empty() {
                        focusable.push(head.to_owned());
                    }
                }
            }
        }
        // the reader found them, and both kinds are here
        assert!(
            focusable
                .iter()
                .any(|head| attr(head, "class") == "playhead"),
            "{focusable:?}"
        );
        assert!(
            focusable.iter().any(|head| attr(head, "role") == "button"),
            "{focusable:?}"
        );

        let mut read = 0usize;
        let mut blanket: Vec<String> = Vec::new();
        let mut suppressions: Vec<String> = Vec::new();
        for css in sheets() {
            for (at, selector, body) in rules_of(&css) {
                let names_drawing = emitted_classes()
                    .iter()
                    .any(|class| could_reach(&selector, class));
                let everything = reaches_everything(&selector);
                if !names_drawing && !everything {
                    continue;
                }
                read += 1;
                for (property, value) in outlines(&body) {
                    if value == "none" {
                        assert_eq!(property, "outline", "{at} {selector}");
                        suppressions.push(selector.clone());
                        continue;
                    }
                    // a rule that names a node of the drawing may not draw
                    // one at all, whatever it is answering
                    assert!(
                        !names_drawing,
                        "{at} {selector} draws an outline on a node of the drawing"
                    );
                    blanket.push(selector.clone());
                }
            }
        }
        assert!(read > 20, "only {read} rules of the drawing read");
        // the site's ring really is in this tree, so the suppressions
        // below are load-bearing and not a formality
        assert!(
            blanket
                .iter()
                .any(|selector| selector.contains(":focus-visible")),
            "no rule reaching the whole tree sets an outline: {blanket:?}"
        );
        // and this is the reading that missed it: that rule names no class
        // at all, so a scan that asked only about classes skipped it
        // before it read a single declaration
        for selector in &blanket {
            assert!(
                !emitted_classes()
                    .iter()
                    .any(|class| could_reach(selector, class)),
                "`{selector}` is reached by a class after all"
            );
            assert!(reaches_everything(selector), "{selector}");
        }

        // and every focusable node of the drawing turns it off by name.
        // The element's own stylesheet is what is read here: the film
        // holds this markup in a shadow root of its own and answers for
        // what reaches it there
        let own: Vec<String> = rules_of(&shadow_css())
            .into_iter()
            .filter(|(_, selector, body)| {
                suppressions.contains(selector) && !outlines(body).is_empty()
            })
            .map(|(_, selector, _)| selector)
            .collect();
        for head in &focusable {
            assert!(
                own.iter().any(|selector| could_reach_head(selector, head)),
                "nothing turns the site's outline off on <{head}>; \
                 the rules that turn one off are {own:?}"
            );
        }
    }

    /// Decision 17's one tab stop. The element attaches its own root with
    /// `delegatesFocus`, so a click on the host or a `focus()` on it is a
    /// focus meant for the control inside and not for the host; the
    /// build's template says the same thing in markup, which is the only
    /// way a declarative root can carry it, since `attachShadow` on an
    /// element that already has one empties the root it found and ignores
    /// the init it was handed.
    ///
    /// Where such a focus lands is the engine's choice and not this
    /// attribute's, and the engine chooses the svg: what this pins is that
    /// the markup leaves one tab stop for it to have been meant for, and
    /// that the tab stop is the thumb. Moving it there is the element's
    /// own work, in the test below.
    #[test]
    fn one_tab_stop_lands_on_the_thumb_and_both_paths_delegate_the_focus() {
        let source = include_str!("chart.rs");
        let body = source
            .split_once("fn delegating_shadow_root(host: &HtmlElement)")
            .expect("the attach")
            .1;
        let body = body.split_once("\n}").expect("it ends").0;
        assert!(body.contains("delegatesFocus"), "{body}");
        assert!(body.contains("from_bool(true)"), "{body}");
        // an existing root is kept, never re-attached: attaching over a
        // declarative root would throw the pre-render away
        assert!(body.contains("host.shadow_root()"), "{body}");
        // and it is the only root this element attaches, so no path skips
        // the delegation. The needle is assembled, so these lines are not
        // among the occurrences it counts
        let attach = ["delegating_shadow_root(&self.", "host)"].concat();
        let any = ["shadow_root(&self.", "host)"].concat();
        assert_eq!(source.matches(&attach).count(), 2, "{source}");
        assert_eq!(source.matches(&any).count(), 2, "{source}");

        // the hydration path: the build wraps the same markup in a template
        // that carries the attribute
        let page = format!(
            "<opt:body xmlns:opt=\"https://www.openpower.tools/ns/opt\">\
             <opt:chart for=\"f\"><script type=\"application/json\">{BLOCK}</script>\
             </opt:chart></opt:body>"
        );
        let html = op_pages::lower(&page).expect("the page lowers");
        assert!(
            html.contains("<template shadowrootmode=\"open\" shadowrootdelegatesfocus>"),
            "{html}"
        );
        // and the one node such a focus is meant for is the thumb, because
        // the svg is no longer a stop and the cue buttons are `-1`
        let markup = one_chart();
        let stops: Vec<&str> = heads(&markup, "g")
            .into_iter()
            .chain(heads(&markup, "svg"))
            .filter(|head| attr(head, "tabindex") == "0")
            .collect();
        assert_eq!(stops.len(), 1, "{stops:?}");
        assert_eq!(attr(stops[0], "class"), "playhead");
        assert_eq!(attr(stops[0], "role"), "slider");
        // the cues start at -1, which is the roving's rest state: the
        // element moves the attribute when the reader steps into them
        // (`the_tab_stop_is_an_attribute_that_moves_over_the_whole_set`)
        let cues: Vec<&str> = heads(&markup, "g")
            .into_iter()
            .filter(|head| attr(head, "role") == "button")
            .collect();
        assert!(!cues.is_empty(), "{markup}");
        for cue in &cues {
            assert_eq!(attr(cue, "tabindex"), "-1", "{cue}");
        }
    }

    /// What the element does with a focus that arrives on a node that is
    /// not its stop.
    ///
    /// `delegatesFocus` is a request an engine answers its own way. Asked
    /// to focus a host whose root holds this drawing, it puts the root's
    /// active element on the outermost svg, which carries no `tabindex`
    /// and answers no keys, and not on the `role="slider"` thumb inside
    /// it, which is both. So the widget decides, and it decides for the
    /// two nodes an engine can leave a focus on and for no others: a cue
    /// the roving moved to and the data table's disclosure are where a
    /// reader went on purpose.
    #[test]
    fn a_focus_the_engine_placed_is_moved_to_the_stop_and_one_the_reader_placed_is_not() {
        let at = |inside, stop, svg| takes_the_stop(Arrival { inside, stop, svg });
        // the svg, which is what the engine delegates to here
        assert!(at(true, false, true));
        // the host itself, which is where a focus stays when the engine
        // finds nothing inside to delegate to: the root has no active
        // element to name
        assert!(at(false, false, false));
        // a cue button during the roving, the data table's disclosure, or
        // anything else the root holds
        assert!(!at(true, false, false));
        // and the stop keeps what it has, whichever node the stop is: a
        // chart inside a film has the svg for a stop (decision 15) and
        // must not be moved off itself for ever
        assert!(!at(true, true, false));
        assert!(!at(true, true, true));
        // the three a root cannot report, since one with no active element
        // names no node at all. Pinned so the rule is total, and so a
        // change to it has to say what it means by them: the stop is asked
        // about first, and a focus said to be on the stop is left alone
        assert!(at(false, false, true));
        assert!(!at(false, true, false));
        assert!(!at(false, true, true));
    }

    /// The element hears a focus arrive on the host as well as inside its
    /// root, and moves the ones the engine placed onto its stop.
    ///
    /// No native test can reach a shadow root's active element, dispatch a
    /// focus, or ask an engine what it delegates to, so the wiring is read
    /// off the source, as the other guards in this file are. What the rule
    /// itself says is the test above; only a browser can show that the
    /// rule is reached.
    #[test]
    fn the_element_hears_a_focus_arrive_on_the_host_and_moves_it_off_the_svg() {
        let source = include_str!("chart.rs");
        let body_of = |signature: &str| {
            source
                .split_once(signature)
                .unwrap_or_else(|| panic!("{signature}"))
                .1
                .split_once("\n    }")
                .expect("it ends")
                .0
        };
        // the listener rides on the host, which is where a focus that
        // never reached the root is heard at all, and comes off again with
        // the rest of the wiring. rustfmt decides where a call of this
        // length breaks and whether the last argument keeps a comma, so
        // both are read with the whitespace taken out of the file and both
        // needles stop at the callback
        let tight: String = source.chars().filter(|c| !c.is_whitespace()).collect();
        for call in [
            "self.host.add_event_listener_with_callback(\"focusin\",on_focus_in.as_ref().unchecked_ref()",
            "self.host.remove_event_listener_with_callback(\"focusin\",wiring.on_focus_in.as_ref().unchecked_ref()",
        ] {
            assert!(tight.contains(call), "{call}");
        }

        // what it answers with: the rule, the one place that names the
        // stop, and the one mover, which takes the tab stop along with it
        let arrival = body_of("fn on_arrival(&self) {");
        assert!(
            arrival.contains("takes_the_stop(self.arrival())"),
            "{arrival}"
        );
        assert!(arrival.contains("self.focus_target()"), "{arrival}");
        assert!(arrival.contains("self.move_stop("), "{arrival}");
        // and it cannot loop: the move raises the very event it answers,
        // and raises it synchronously, so a move in flight is the end of
        // the arrival it raised rather than the start of another
        assert!(arrival.contains("self.moving_focus.get()"), "{arrival}");
        assert!(arrival.contains("self.moving_focus.set(true)"), "{arrival}");
        assert!(
            arrival.contains("self.moving_focus.set(false)"),
            "{arrival}"
        );

        // the facts it judges come from the tree and from the handles the
        // element captured, never from a query of the root
        let asked = body_of("fn arrival(&self) -> Arrival {");
        assert!(asked.contains("self.root.active_element()"), "{asked}");
        assert!(asked.contains("self.focus_target()"), "{asked}");
        assert!(asked.contains("self.dom.borrow().svg"), "{asked}");
        assert!(!asked.contains("query_selector"), "{asked}");

        // which is what keeps the discarded half of a pre-render out of
        // it. The build ships a wide drawing and a narrow one, with a
        // thumb in each, and one of them is removed only when the element
        // keeps the drawing the container query chose; every handle is
        // read out of that one svg
        assert_eq!(built().shadow.matches(THUMB_OPEN).count(), 2);
        let capture = body_of("fn capture(&self, svg: Option<Element>, refocus: Option<Held>) {");
        assert!(capture.contains("svg.as_ref()"), "{capture}");
        assert!(capture.contains("find(\"g.playhead\")"), "{capture}");
        // and the thumb is named in that one svg-scoped query and nowhere
        // else. The needle is assembled, so this line is not the
        // occurrence it counts
        let names = ["g.play", "head\""].concat();
        assert_eq!(source.matches(&names).count(), 1, "{source}");
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
        // they join the three the element already writes, and all six
        // differ
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
