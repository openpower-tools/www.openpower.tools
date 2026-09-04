//! `<opt-film sheet="..." title="...">`: a frame-by-frame player for the
//! interaction reports, built from the site's own vocabulary so the
//! reports can render and exercise themselves. The data - frame times,
//! per-frame change, chapters (the machine's marks) and the sampled
//! series - comes from a `<script type="application/json">` child; the
//! frames come from one sprite sheet.
//!
//! The model follows the YouTube playback research: a centred stage
//! shows the current frame; a still strip of every frame pages forward
//! under a travelling marker (no scrolling motion), each frame captioned
//! with its time and the share of pixels changed; a chart of the sampled
//! series carries the playhead and a chapter bar. Hovering the chart or
//! the strip PEEKS (thumbnail, time and chapter follow the pointer, the
//! playhead does not move); pressing the chart scrubs; pressing a strip
//! frame and dragging chooses a pending frame, release seeks, Escape
//! cancels. Keys while focused: Space/K, `,` `.` frame step, arrows five
//! frames, J/L ten, Shift+arrow one second, PageUp/PageDown by chapter
//! (Ctrl or Alt with an arrow as an alias), 0-9 tenths, Home/End, `<` `>`
//! speed, Escape. Enter or Space on a cue button in the chart, which is
//! where a screen reader can put the focus, seeks to that cue instead.
//!
//! Internal state is exposed as custom states - `playing`, `pending`,
//! `peeking` - so a page can style or test against them; every render
//! dispatches a composed `opt-film-time` event whose detail is
//! `{ time, duration, playing }` (an `opt-machine for="..."` follows it,
//! reading `time`); the control bar's range input is this widget's one
//! slider and carries the value text naming the time, the frame and the
//! chapter, written at once for a seek, a play, a pause and the focus
//! arriving, and on a clock tick only while that input has the focus and
//! no oftener than the debounce (decision 18); the chart beside it is a
//! titled `graphics-document` whose series are named groups and whose
//! playhead is decoration (decision 15); a polite live region says what
//! happened in the chart's own words, a seek, a play and a pause. A
//! scrub is said once, when it settles, on the control and in the region
//! alike, and the time both of them name is spelt the chart's way.
//!
//! The traffic runs the other way as well: an `opt-chart for="<film id>"`
//! sends `opt-chart-seek`, `opt-chart-peek` and `opt-chart-toggle`, and
//! the film applies them to its own clock. Those events are composed and
//! bubbling, so one listener on the document hears every chart on the
//! page and `intent_for` decides which of them address this film.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use op_webc::{CustomElement, ElementDefinition, set_state};
use wasm_bindgen::prelude::*;
use web_sys::{Element, Event, HtmlElement, KeyboardEvent, MouseEvent, PointerEvent};

use super::chart::{
    CUE_BUTTONS, PEEK_EVENT, Roved, Roving, SEEK_EVENT, Said, TIME_FIELD, TOGGLE_EVENT,
    cue_indicator_css, in_words, message, now, readout, roving_key, valuetext_due,
};
use super::chart_style::{CHART_CUE_CSS, CHART_SHAPE_CSS, SERIES_TOKENS, chart_rules};
use super::{BASE_CSS, shadow_root};
use crate::html::escape;

pub const DEFINITION: ElementDefinition = ElementDefinition {
    source: op_webc::here!(),
    tag: "opt-film",
    observed_attributes: &[],
    properties: &[],
    create: |host| Box::new(Film { host, wiring: None }),
};

/// One sampled series drawn on the chart.
struct Series {
    label: String,
    /// Palette series 1 to 6 (`--op-series-N`); by position when absent.
    index: usize,
    t: Vec<f64>,
    y: Vec<f64>,
    lw: f64,
}

/// Everything the element renders from.
struct Data {
    sheet: String,
    w: f64,
    h: f64,
    times: Vec<f64>,
    deltas: Vec<f64>,
    chapters: Vec<(f64, String)>,
    series: Vec<Series>,
    ylabel: String,
    t_max: f64,
}

fn get(v: &JsValue, key: &str) -> JsValue {
    js_sys::Reflect::get(v, &JsValue::from_str(key)).unwrap_or(JsValue::UNDEFINED)
}

fn num(v: &JsValue, key: &str, default: f64) -> f64 {
    get(v, key).as_f64().unwrap_or(default)
}

fn text(v: &JsValue, key: &str) -> String {
    get(v, key).as_string().unwrap_or_default()
}

fn nums(v: &JsValue, key: &str) -> Vec<f64> {
    js_sys::Array::from(&get(v, key))
        .iter()
        .filter_map(|x| x.as_f64())
        .collect()
}

impl Data {
    fn parse(json: &str, sheet: String) -> Option<Self> {
        let v = js_sys::JSON::parse(json).ok()?;
        let times = nums(&v, "times");
        if times.is_empty() {
            return None;
        }
        let deltas = nums(&v, "deltas");
        let chapters = js_sys::Array::from(&get(&v, "chapters"))
            .iter()
            .filter_map(|pair| {
                let pair = js_sys::Array::from(&pair);
                Some((pair.get(0).as_f64()?, pair.get(1).as_string()?))
            })
            .collect::<Vec<_>>();
        let series = js_sys::Array::from(&get(&v, "series"))
            .iter()
            .enumerate()
            .map(|(i, s)| Series {
                label: text(&s, "label"),
                index: (num(&s, "series", i as f64 + 1.0).round() as usize).clamp(1, SERIES_TOKENS),
                t: nums(&s, "t"),
                y: nums(&s, "y"),
                lw: num(&s, "lw", 2.0),
            })
            .collect::<Vec<_>>();
        let last = times.last().copied().unwrap_or(0.0);
        let t_max = series
            .iter()
            .flat_map(|s| s.t.iter().copied())
            .fold(last, f64::max);
        Some(Self {
            sheet,
            w: num(&v, "w", 1.0),
            h: num(&v, "h", 1.0),
            times,
            deltas,
            chapters: if chapters.is_empty() {
                vec![(0.0, "start".to_owned())]
            } else {
                chapters
            },
            series,
            ylabel: {
                let l = text(&v, "ylabel");
                if l.is_empty() {
                    "progress %".to_owned()
                } else {
                    l
                }
            },
            t_max: if t_max > 0.0 { t_max } else { 1.0 },
        })
    }

    fn frame_at(&self, t: f64) -> usize {
        let mut k = 0;
        for (j, tj) in self.times.iter().enumerate() {
            if *tj <= t + 1e-6 {
                k = j;
            }
        }
        k
    }

    fn chapter_at(&self, t: f64) -> &(f64, String) {
        let mut c = &self.chapters[0];
        for ch in &self.chapters {
            if ch.0 <= t + 1e-6 {
                c = ch;
            }
        }
        c
    }

    fn end(&self) -> f64 {
        self.t_max.max(self.times.last().copied().unwrap_or(0.0))
    }

    /// Start of the chapter after `t`, if there is one.
    fn next_chapter_start(&self, t: f64) -> Option<f64> {
        self.chapters.iter().map(|c| c.0).find(|&c| c > t + 1e-6)
    }

    /// Where a previous-chapter key lands: the current chapter's start when
    /// the playhead has moved past the chapter's first frame, otherwise the
    /// previous chapter's start (or the beginning).
    fn prev_chapter_start(&self, t: f64) -> f64 {
        let current = self.chapter_at(t).0;
        if self.frame_at(t) > self.frame_at(current) {
            return current;
        }
        self.chapters
            .iter()
            .map(|c| c.0)
            .filter(|&c| c < current - 1e-6)
            .fold(0.0, f64::max)
    }
}

// ---- chapter steps: the Page keys and the two bar buttons -------------
/// A step by chapter, in either direction.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Chapter {
    Prev,
    Next,
}

/// The two chapter buttons, in bar order: the class the markup carries,
/// the button's text (which is its accessible name), and the step it
/// takes. `Player::chapter` takes that step for the buttons and for the
/// keys alike.
const CHAPTER_CONTROLS: [(&str, &str, Chapter); 2] = [
    ("chapter-prev", "Previous chapter", Chapter::Prev),
    ("chapter-next", "Next chapter", Chapter::Next),
];

/// Where a chapter step from `t` lands, or `None` when there is no
/// chapter to step to that way, which is when the button for it is
/// disabled. Both directions read the same helpers the keys landed on
/// before there were buttons.
fn chapter_target(d: &Data, t: f64, dir: Chapter) -> Option<f64> {
    match dir {
        Chapter::Next => d.next_chapter_start(t),
        Chapter::Prev => Some(d.prev_chapter_start(t)).filter(|&c| c < t - 1e-6),
    }
}

/// The chapter step a key asks for: Page Up and Page Down, with Ctrl or
/// Alt and an arrow as the alias, as YouTube has it.
fn chapter_key(key: &str, alias: bool) -> Option<Chapter> {
    match key {
        "PageDown" => Some(Chapter::Next),
        "PageUp" => Some(Chapter::Prev),
        "ArrowRight" if alias => Some(Chapter::Next),
        "ArrowLeft" if alias => Some(Chapter::Prev),
        _ => None,
    }
}

/// The control bar: play, the two chapter steps, speed, the frame slider
/// and the readout. Pure, so the markup is testable without a browser.
fn control_bar(frames: usize) -> String {
    let chapters: String = CHAPTER_CONTROLS
        .iter()
        .map(|(class, name, _)| {
            format!("<button type=\"button\" class=\"{class}\">{name}</button>")
        })
        .collect();
    format!(
        "<div class=\"bar\"><button type=\"button\" class=\"play\">Play</button>{chapters}<select aria-label=\"speed\"><option value=\"1\">1x</option><option value=\"0.5\">0.5x</option><option value=\"0.25\">0.25x</option></select><input type=\"range\" min=\"0\" max=\"{}\" value=\"0\" aria-label=\"frame\"><span class=\"t\"></span><span class=\"n\">{frames} frames; captions give the share of pixels changed since the previous frame</span></div>",
        frames.saturating_sub(1)
    )
}

// ---- the chart: drawn by op-chart, moved here -------------------------
/// The film's data as a chart spec: one axis for every series, chapters as
/// marks, and the colours exactly as the data passes them.
fn spec_of(d: &Data) -> op_chart::Spec {
    op_chart::Spec {
        end: d.t_max,
        duration: d.end(),
        // every film series is a percentage, so the chart stays on the
        // percent scale whatever a data-driven chart puts itself on
        y: op_chart::layout::PERCENT,
        ylabel: d.ylabel.clone(),
        chapters: d
            .chapters
            .iter()
            .map(|(t, label)| op_chart::Chapter {
                t: *t,
                label: label.clone(),
            })
            .collect(),
        // the film annotates nothing of its own: its cues are all chapters,
        // and the marks and band belong to a chart drawn from a data block
        marks: Vec::new(),
        band: None,
        series: d
            .series
            .iter()
            .map(|s| op_chart::Series {
                label: s.label.clone(),
                index: s.index,
                points: s
                    .t
                    .iter()
                    .copied()
                    .zip(s.y.iter().copied())
                    .map(Some)
                    .collect(),
                width: s.lw,
            })
            .collect(),
    }
}

/// The id of the visible chart title in the film's shadow tree, which the
/// chart's svg is named by. The film has no heading of its own - the host's
/// `title` names the whole player, not the plot - so decision 15's visible
/// title is written here and pointed at from the markup the emitter draws.
const CHART_TITLE_ID: &str = "charttitle";

/// The chart's visible title: what is plotted, against what. The value
/// label the block carries is the chart's own name for its y axis, so the
/// title reads from the data rather than from a string invented here.
fn chart_title(d: &Data) -> String {
    format!(
        "<div class=\"charttitle\" id=\"{CHART_TITLE_ID}\">{} over time</div>",
        escape(&d.ylabel)
    )
}

/// What the emitter cannot know about the film's chart (decision 15): the
/// id of the title above it, the unit its series are measured in, and that
/// its thumb is not this widget's slider. The film's control bar holds a
/// native range input, and one widget has one slider, so the chart's
/// playhead announces nothing and takes no tab stop of its own.
///
/// Every film series is a percentage, which is why [`spec_of`] keeps the
/// chart on the percent scale, so each of them is announced in per cent.
fn chart_aria(d: &Data) -> op_chart::Aria {
    op_chart::Aria {
        title: CHART_TITLE_ID.to_owned(),
        units: vec!["%".to_owned(); d.series.len()],
        slider: false,
    }
}

/// The thumb's opening tag, as the emitter writes it for a chart that owns
/// no slider. Whether that thumb is decoration is the consumer's fact and
/// not the drawing's: `Aria::slider` says only that the role and the values
/// are not to be written, and here the film's own readout and its slider
/// say the time already, so the group is hidden as well. The tag is named
/// so a test can hold it to what the emitter writes, since a string that
/// stopped matching would hide nothing and say nothing about it.
const THUMB_TAG: &str = "<g class=\"playhead\" part=\"playhead\"";

/// The film's chart: the emitter's markup, named by the title above it and
/// with its thumb marked as the decoration it is here.
///
/// The drawing carries the emitter's cue buttons, one to a chapter. In the
/// film they are reached by a pointer, by a screen reader walking the
/// drawing, and by nothing else: this element has no entry gesture of its
/// own and is deliberately not given one. `<opt-chart>` roves into its
/// cues with Down from its thumb, and the film's chart has no thumb to
/// leave, its playhead being decoration ([`chart_aria`]) with no role and
/// no tab stop; the film's one slider is the bar's native range input,
/// where Down is that control's own key and steps the frame back. So the
/// element's entry gesture cannot be borrowed here without taking a key
/// off a native control, and what it would lead to is a second set of
/// chapter controls beside the two named buttons the bar already carries
/// ([`CHAPTER_CONTROLS`]), which are in the tab order and are the keyboard
/// route to a chapter, along with Page Up and Page Down and the Ctrl or
/// Alt arrow alias. The buttons stay what they are, then: named targets,
/// which answer Enter and Space the element's own way when something does
/// put the focus on one ([`cue_press`]).
fn chart_svg(d: &Data, l: op_chart::Layout) -> String {
    let svg = op_chart::render_with(&spec_of(d), l, &chart_aria(d)).svg;
    svg.replace(THUMB_TAG, &format!("{THUMB_TAG} aria-hidden=\"true\""))
}

/// What a key does while a cue button in the film's chart holds the focus:
/// the instant to seek to, and [`None`] for every key that is the film's
/// own. `stamp` is the `data-t` the focused cue's hit rect carries and
/// [`None`] where the focus is anywhere else, so a key pressed on the
/// strip, on the bar or on the host is answered by nothing here and the
/// film's key table behaves exactly as it did.
///
/// The keys are not a table of the film's own: they are the element's
/// ([`roving_key`]), asked as a reader standing on a cue, and only the
/// press is taken from it. A `role="button"` answers Enter and Space or it
/// is not a button, and Space on a cue reached the film's key table before
/// this and played the film, which is the one thing a press on a chapter
/// must not do. The count is one because the film asks only about the cue
/// that has the focus: the moves the element's roving answers are left to
/// the film's own table, there being no roving here to move (see
/// [`chart_svg`]), so Escape on a cue still drops a pending seek.
///
/// A cue whose rect says nothing reads as [`f64::INFINITY`], as it does in
/// the element, and [`Player::seek_to`] clamps it to the end of the film:
/// a press that lands somewhere, rather than a key falling through to the
/// arm that plays.
fn cue_press(stamp: Option<&str>, key: &str) -> Option<f64> {
    let stamp = stamp?;
    matches!(roving_key(Roving::Cue(0), 1, key), Some(Roved::Press))
        .then(|| stamp.parse::<f64>().unwrap_or(f64::INFINITY))
}

/// What the film's slider announces beside its own value, which is a frame
/// index: the clock in seconds, the frame and the chapter it is in. It sits
/// on the range input because that is this widget's slider, and a value
/// written on the chart, which is a document, is a value nothing announces.
///
/// The time is the chart's own spelling of one ([`in_words`]) and not a
/// second rounding beside it: the region says "1.6 seconds" for the
/// instant the control used to call "1.55 seconds", and a reader who
/// heard both heard one position given two times. The frame index keeps
/// the film's own wording, because it is what this slider's value, its
/// minimum and its maximum are counted in; the chart's thumb counts in
/// seconds and names its total, which on a control whose scale is frames
/// would be a total of nothing it carries.
fn value_text(d: &Data, t: f64) -> String {
    format!(
        "{}, frame {} of {}, {}",
        in_words(t),
        d.frame_at(t) + 1,
        d.times.len(),
        d.chapter_at(t).1
    )
}

/// Why the film is about to say that value (decision 18).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Say {
    /// The reader asked for the clock to move and the asking is over: a
    /// seek that is no part of a gesture, a play, a pause, the release
    /// that ends a scrub. The first render says it this way as well, and
    /// so does the focus arriving, so the words are on the input before
    /// anything can tab to it and are the clock's own when something
    /// does.
    AtOnce,
    /// The clock moved under a pointer gesture still in flight, which
    /// here is a scrub across the chart or a drag of the film's own
    /// slider. Both seek on every step the pointer takes, so the words
    /// wait for the end of the gesture, which says them once
    /// ([`says_now`]); the number does not wait, the thumb being drawn
    /// from it.
    InFlight,
    /// The clock ran on by itself, once an animation frame.
    OnTheClock,
}

/// Whether that write happens now. A gesture that is over is said at
/// once; a clock tick waits for the focus and for the wait, which is the
/// chart's rule and not a second copy of it ([`valuetext_due`]). A film
/// plays at the frame rate, and a reader who has tabbed to the slider
/// would otherwise be interrupted about sixty times a second by a phrase
/// none of which is ever finished.
///
/// A gesture still in flight says nothing at all, rather than waiting.
/// The debounce would let a phrase through three times a second for as
/// long as a drag lasts, and a drag is one gesture with one thing to say:
/// the release says it, which is the rule the region already keeps
/// ([`says_now`]).
fn value_due(say: Say, last: f64, now: f64, focused: bool) -> bool {
    match say {
        Say::AtOnce => true,
        Say::InFlight => false,
        Say::OnTheClock => valuetext_due(last, now, focused),
    }
}

/// Whether the frame index goes onto the slider now.
///
/// The number and the words are on different clocks. The number is what
/// the thumb is drawn from, so a tick writes it whenever nothing has
/// tabbed to the control: a value nobody is listening to is announced to
/// nobody, and the thumb has to keep up with the film. While the control
/// does have the focus a native slider's value is spoken as surely as its
/// words are, so there the number waits with them, on the same rule
/// ([`valuetext_due`]) and the same clock. A gesture writes at once
/// either way, one still in flight included: the thumb has to stay under
/// the pointer dragging it, and a thumb that stops while the pointer
/// moves is the drag not working. Only the clock waits.
fn index_due(say: Say, last: f64, now: f64, focused: bool) -> bool {
    say != Say::OnTheClock || !focused || valuetext_due(last, now, focused)
}

/// The three moments the live region's rule is asked about.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Moment {
    /// A gesture opening: a press on the chart, or an `input` from the
    /// native slider, which is a thumb somebody has hold of.
    Press,
    /// A seek, from wherever it came.
    Seek,
    /// The end of that gesture: a release, a cancel, or the slider's own
    /// `change`.
    Settle,
}

/// What that moment leaves behind: whether a scrub is in flight after it,
/// and whether the film says its piece now, in that order.
///
/// A scrub seeks on every `pointermove`, and a sentence per move is the
/// same flood on the region that the debounce keeps off the value
/// (decision 18). So a gesture says nothing while it moves and says the
/// one thing it had to say when it settles, naming where it landed. A
/// seek that is no part of a gesture, a key, a chapter button or a bound
/// chart, is not held up by any of this and speaks at once.
///
/// Both voices are on this one answer ([`Player::moment`]): the sentence
/// in the region, and the words on the control. The control used to write
/// its words on every seek there was, so the drag the region sat quiet
/// through rewrote the control about as often as the pointer moved.
fn says_now(moment: Moment, scrubbing: bool) -> (bool, bool) {
    match moment {
        Moment::Press => (true, false),
        Moment::Seek => (scrubbing, !scrubbing),
        Moment::Settle => (false, scrubbing),
    }
}

/// Which of the film's own slider's events is which moment. A native
/// range input fires `input` for every pixel of a thumb dragged with the
/// pointer and one `change` when the reader lets go of it, so a drag of
/// that thumb is the gesture a scrub across the chart is, and takes the
/// same rule: the drag seeks and stays quiet, the end of it says the one
/// thing the gesture had to say. There is no other seam to tell the two
/// apart, an `input` saying nothing about the device that sent it. A key
/// on the same control fires both in the one press, which is how a key
/// still speaks at once.
const SLIDER_MOMENTS: [(&str, Moment); 2] = [("input", Moment::Press), ("change", Moment::Settle)];

/// What a pointer move over the chart is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Moved {
    /// A move inside a scrub this chart opened, which drags the clock.
    Scrub,
    /// A move with nothing held down, which shows the peek.
    Hover,
    /// A move with the primary button down that this chart never saw
    /// pressed: a drag begun somewhere else, passing through.
    Stray,
}

/// Which of the three a move is, from the buttons it carries and whether
/// a press here opened a scrub.
///
/// A button being down is not by itself a scrub. A drag begun anywhere
/// else on the page and dragged over the chart arrives with the same
/// buttons and no press of this chart's behind it, and the film answers
/// it with neither a seek nor a peek: nothing opened the gesture, so
/// nothing is coming to settle it, and every move of it would speak on
/// its own account ([`says_now`]). A scrub of the film's own is captured
/// to the chart at the press, so the moves belonging to one arrive here
/// whether the pointer is still over the chart or not.
fn moved(buttons: u16, scrubbing: bool) -> Moved {
    match (buttons & 1 == 1, scrubbing) {
        (true, true) => Moved::Scrub,
        (true, false) => Moved::Stray,
        (false, _) => Moved::Hover,
    }
}

/// What the region says when a pending seek on the strip is let go: the
/// film's own sentence, the chart's [`message`] having no word for a
/// gesture the element does not have. It opens with a capital because
/// every sentence in that region does, and it goes through the film's one
/// voice like the rest of them.
const CANCELLED: &str = "Seek cancelled";

/// What a change of the play state has to say, and nothing at all where it
/// did not change: a frame step, a chapter and a digit all pause the film
/// on their way, and a film already paused has not been paused again. The
/// chart announces the two on the same terms, when they change and never
/// once a frame (decision 18).
fn toggled(was: bool, now: bool) -> Option<Said> {
    match (was, now) {
        (false, true) => Some(Said::Playing),
        (true, false) => Some(Said::Paused),
        _ => None,
    }
}

// ---- state and DOM ---------------------------------------------------
struct State {
    tc: f64,
    playing: bool,
    pending: Option<usize>,
    last: Option<f64>,
    rate: f64,
    raf: Option<i32>,
}

/// The rendered parts the effects act on.
struct Dom {
    host: HtmlElement,
    stage: HtmlElement,
    stage_label: Element,
    /// The recorded video over the stage, when the film has one.
    video: Option<web_sys::HtmlVideoElement>,
    reelbox: HtmlElement,
    reel: HtmlElement,
    gate: HtmlElement,
    frames: Vec<HtmlElement>,
    slider: Element,
    time_label: Element,
    play: Element,
    rate: Element,
    /// The chapter buttons with the step each takes, in bar order.
    chapters: Vec<(Chapter, Element)>,
    chart: Option<Element>,
    /// The geometry the chart was drawn with, for hit-testing and the playhead.
    layout: op_chart::Layout,
    peek: Option<HtmlElement>,
    pframe: Option<HtmlElement>,
    ptime: Option<Element>,
    live: Element,
    cell_w: f64,
    stage_w: f64,
}

impl Dom {
    fn q(&self, sel: &str) -> Option<Element> {
        self.chart
            .as_ref()
            .and_then(|c| c.query_selector(sel).ok().flatten())
    }
}

fn show_stage(dom: &Dom, d: &Data, k: usize, tag: &str) {
    let _ = dom.stage.style().set_property(
        "background-position",
        &format!("{}px 0", -(k as f64) * dom.stage_w),
    );
    dom.stage_label.set_text_content(Some(&format!(
        "{} · frame {} of {}{}{}",
        readout(d.times[k]),
        k + 1,
        d.times.len(),
        if tag.is_empty() { "" } else { " · " },
        tag
    )));
}

fn render(dom: &Dom, d: &Data, st: &State) {
    let n = d.times.len();
    let k = d.frame_at(st.tc);
    let next = (k + 1).min(n - 1);
    let span = d.times[next] - d.times[k];
    let frac = if span > 0.0 {
        ((st.tc - d.times[k]) / span).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let mid = |j: usize| {
        f64::from(dom.frames[j].offset_left()) + f64::from(dom.frames[j].offset_width()) / 2.0
    };
    let pitch = if n > 1 {
        f64::from(dom.frames[1].offset_left() - dom.frames[0].offset_left())
    } else {
        f64::from(dom.frames[0].offset_width())
    };
    let per_page =
        ((f64::from(dom.reelbox.client_width()) / pitch.max(1.0)).floor() as usize).max(1);
    let page = k / per_page;
    let page_left = f64::from(dom.frames[page * per_page].offset_left());
    let _ = dom
        .reel
        .style()
        .set_property("transform", &format!("translateX({}px)", -page_left));
    let same_page = next / per_page == page;
    let pos = mid(k)
        + if same_page {
            frac * (mid(next) - mid(k))
        } else {
            0.0
        };
    let _ = dom
        .gate
        .style()
        .set_property("left", &format!("{}px", pos - page_left));
    for (j, fr) in dom.frames.iter().enumerate() {
        let _ = fr.class_list().toggle_with_force("current", j == k);
        let _ = fr
            .class_list()
            .toggle_with_force("pending", Some(j) == st.pending);
    }
    if st.pending.is_none() {
        show_stage(dom, d, k, &d.chapter_at(st.tc).1);
        let _ = dom.stage.class_list().remove_1("pending");
    }
    // Broadcast the clock: other projections of the same timeline (the
    // machine diagram's playhead, the chart) follow this composed, bubbling
    // event. The detail is an object rather than a bare number so a follower
    // can draw the played bar and the end without reading them off the film.
    let init = web_sys::CustomEventInit::new();
    init.set_bubbles(true);
    init.set_composed(true);
    let detail = js_sys::Object::new();
    let set = |key: &str, value: &JsValue| {
        let _ = js_sys::Reflect::set(&detail, &JsValue::from_str(key), value);
    };
    set("time", &JsValue::from_f64(st.tc));
    set("duration", &JsValue::from_f64(d.end()));
    set("playing", &JsValue::from_bool(st.playing));
    init.set_detail(&detail);
    if let Ok(event) = web_sys::CustomEvent::new_with_event_init_dict("opt-film-time", &init) {
        let _ = dom.host.dispatch_event(&event);
    }
    // the slider's own number and words are announcements and not a
    // paint: both are written where the gesture is known
    // ([`Player::say_value`]), on the clocks decision 18 gives them
    dom.time_label.set_text_content(Some(&readout(st.tc)));
    // the clock has moved, so a chapter step may have run out of chapters
    for (dir, button) in &dom.chapters {
        let _ = button
            .toggle_attribute_with_force("disabled", chapter_target(d, st.tc, *dir).is_none());
    }
    sync_video(dom, st);
    if dom.chart.is_some() {
        let x = dom.layout.x_of(st.tc);
        // one transform carries the line, the dot and the readout together
        if let Some(playhead) = dom.q(".playhead") {
            let _ = playhead.set_attribute("transform", &format!("translate({x:.1} 0)"));
        }
        if let Some(label) = dom.q(".head-t") {
            label.set_text_content(Some(&readout(st.tc)));
        }
        if let Some(played) = dom.q(".bar-played") {
            let _ =
                played.set_attribute("width", &format!("{:.1}", (x - dom.layout.left).max(0.0)));
        }
    }
}

/// Keep the recorded video on the film's clock: play and pause with it, run
/// at its rate, and re-seek whenever the two drift past a threshold (a
/// small one while paused, since a paused frame should be exact).
fn sync_video(dom: &Dom, st: &State) {
    let Some(video) = &dom.video else {
        return;
    };
    if video.ready_state() >= 2 && !video.class_list().contains("ready") {
        let _ = video.class_list().add_1("ready");
    }
    video.set_playback_rate(st.rate);
    if st.playing {
        if video.paused() {
            let _ = video.play();
        }
    } else if !video.paused() {
        let _ = video.pause();
    }
    let threshold = if st.playing { 0.12 } else { 0.02 };
    if (video.current_time() - st.tc).abs() > threshold {
        video.set_current_time(st.tc);
    }
}

fn show_peek(dom: &Dom, d: &Data, t: f64, anchor_x: Option<f64>) {
    let (Some(chart), Some(peek), Some(pframe), Some(ptime)) =
        (&dom.chart, &dom.peek, &dom.pframe, &dom.ptime)
    else {
        return;
    };
    let k = d.frame_at(t);
    let x = dom.layout.x_of(t);
    if let Some(line) = dom.q(".peek-line") {
        let _ = line.set_attribute("x1", &format!("{x:.1}"));
        let _ = line.set_attribute("x2", &format!("{x:.1}"));
        let _ = line.set_attribute("visibility", "visible");
    }
    let c = d.chapter_at(t);
    let next_c = d
        .chapters
        .iter()
        .find(|ch| ch.0 > c.0)
        .map(|ch| ch.0)
        .unwrap_or(d.end());
    if let Some(band) = dom.q(PEEK_BAND) {
        let _ = band.set_attribute("x", &format!("{:.1}", dom.layout.x_of(c.0)));
        let _ = band.set_attribute(
            "width",
            &format!(
                "{:.1}",
                (dom.layout.x_of(next_c) - dom.layout.x_of(c.0)).max(0.0)
            ),
        );
    }
    let _ = pframe.style().set_property(
        "background-position",
        &format!("{}px 0", -(k as f64) * dom.cell_w),
    );
    ptime.set_text_content(Some(&format!("{} · {}", readout(d.times[k]), c.1)));
    peek.set_hidden(false);
    let rect = chart.get_bounding_client_rect();
    let px = anchor_x.unwrap_or_else(|| x * (rect.width() / dom.layout.width));
    let _ = peek.style().set_property("left", &format!("{px}px"));
    set_state(&dom.host, "peeking", true);
}

fn hide_peek(dom: &Dom) {
    if let Some(peek) = &dom.peek {
        peek.set_hidden(true);
    }
    if let Some(line) = dom.q(".peek-line") {
        let _ = line.set_attribute("visibility", "hidden");
    }
    if let Some(band) = dom.q(PEEK_BAND) {
        let _ = band.set_attribute("width", "0");
    }
    set_state(&dom.host, "peeking", false);
}

/// The player: data, state, DOM handles, and the closures that must live
/// as long as the element.
/// The animation-frame callback, re-armed by itself while playing.
type Tick = Rc<RefCell<Option<Closure<dyn FnMut(f64)>>>>;

struct Player {
    dom: Rc<Dom>,
    data: Rc<Data>,
    state: Rc<RefCell<State>>,
    tick: Tick,
    /// When the slider's value was last said, on the page's own clock:
    /// what a clock tick waits behind (decision 18).
    value_written: Cell<f64>,
    /// Whether a pointer gesture is in flight: a scrub across the chart,
    /// or a drag of the slider's own thumb. It is the whole of what
    /// [`says_now`] needs to keep one from saying a sentence per move,
    /// and what tells a move over the chart from somebody else's drag
    /// passing across it ([`moved`]).
    scrub: Cell<bool>,
}

impl Player {
    fn seek_to(&self, t: f64) {
        {
            let mut st = self.state.borrow_mut();
            st.tc = t.clamp(0.0, self.data.end());
        }
        self.render();
        // a seek is a gesture, and a seek inside a scrub is a gesture
        // still in flight: what this one has to say is the moment's to
        // say, once, when the gesture settles
        self.moment(Moment::Seek);
    }

    fn render(&self) {
        render(&self.dom, &self.data, &self.state.borrow());
    }

    /// Whether the film's slider is the focused thing, asked of the tree
    /// rather than remembered: nothing here ever moves the focus, so the
    /// shadow root's own answer is the whole of it. The document's active
    /// element is only ever the host, the slider being inside that root.
    fn slider_focused(&self) -> bool {
        self.dom
            .host
            .shadow_root()
            .and_then(|root| root.active_element())
            .is_some_and(|active| active == self.dom.slider)
    }

    /// The `data-t` the focused cue button in the film's chart carries, and
    /// [`None`] where the focus is anywhere else: the strip, the bar, the
    /// host, or nothing at all. Asked of the tree, as [`slider_focused`]
    /// is, because nothing in the film moves the focus and the shadow
    /// root's own answer is the whole of it.
    ///
    /// The buttons are named as the element names them ([`CUE_BUTTONS`])
    /// and the instant is read off the hit rect inside the one that has
    /// the focus, which is where the emitter writes it, so both elements
    /// read one markup one way.
    fn focused_cue(&self) -> Option<String> {
        let active = self.dom.host.shadow_root()?.active_element()?;
        let ours = self
            .dom
            .chart
            .as_ref()
            .is_some_and(|chart| chart.contains(Some(active.as_ref())));
        if !ours || !active.matches(CUE_BUTTONS).unwrap_or(false) {
            return None;
        }
        active
            .query_selector("[data-t]")
            .ok()
            .flatten()?
            .get_attribute("data-t")
    }

    /// Put the clock on the film's one slider: the frame index it carries
    /// as a value, and the words that say what that index means. Each on
    /// the clock decision 18 gives it ([`index_due`], [`value_due`]).
    ///
    /// Both sit here and not in [`render`] because they are announcements
    /// and not a paint: the paint runs on every animation frame and has no
    /// business knowing why it was called, while every gesture the film
    /// answers is a method on this player and the clock's own tick is the
    /// one caller that is not one. That is the whole of the difference
    /// between a phrase a reader asked for and a phrase per frame.
    ///
    /// Only a write of the words moves the wait on. A number written
    /// while nothing has the focus is spoken to nobody, so it costs a
    /// listener arriving later none of their first phrase.
    fn say_value(&self, say: Say) {
        let now = now();
        let focused = self.slider_focused();
        let last = self.value_written.get();
        if index_due(say, last, now, focused) {
            let k = self.data.frame_at(self.state.borrow().tc);
            let _ = self.dom.slider.set_attribute("value", &k.to_string());
            let _ = js_sys::Reflect::set(
                &self.dom.slider,
                &JsValue::from_str("value"),
                &JsValue::from_f64(k as f64),
            );
        }
        if !value_due(say, last, now, focused) {
            return;
        }
        let text = value_text(&self.data, self.state.borrow().tc);
        let _ = self.dom.slider.set_attribute("aria-valuetext", &text);
        self.value_written.set(now);
    }

    /// What the region says: one sentence naming what happened, in the
    /// chart's own words ([`message`]), so the two halves of a page say
    /// the same thing the same way. Where the clock now stands is the
    /// slider's own value and is not repeated here, which is decision
    /// 18's rule against saying one thing twice.
    fn announce(&self, said: Said) {
        let t = self.state.borrow().tc;
        let chapter = self.data.chapter_at(t).1.clone();
        self.say(&message(said, t, &chapter));
    }

    /// Put one short message in the region, which is the film's one voice
    /// and the only thing that writes there. The element keeps a voice of
    /// its own for the same reason: a message written straight onto the
    /// node is a message no rule about the region reaches, and the two
    /// the film says in its own words, a cancelled seek and a speed, went
    /// in that way and opened in lower case while every sentence beside
    /// them opened with a capital.
    fn say(&self, text: &str) {
        self.dom.live.set_text_content(Some(text));
    }

    /// What the film says at a moment in a pointer gesture, and the
    /// scrub the rule turns on ([`says_now`]). The press, the release and
    /// the cancel hand it their moment; every seek asks before it speaks.
    ///
    /// Both voices are answered here, off the one answer, because a scrub
    /// floods them both alike: the region takes one sentence naming what
    /// happened, in the chart's own words, and the control takes the
    /// words for where the clock now stands. While the gesture is still
    /// in flight neither speaks and the frame index goes on by itself,
    /// so the thumb keeps up with the pointer with no phrase per move
    /// behind it ([`index_due`]).
    fn moment(&self, moment: Moment) {
        let (scrubbing, says) = says_now(moment, self.scrub.get());
        self.scrub.set(scrubbing);
        if says {
            self.say_value(Say::AtOnce);
            self.announce(Said::Seeked);
        } else {
            // the number goes on by itself, so the thumb keeps up with a
            // pointer that is still dragging it
            self.say_value(Say::InFlight);
        }
    }

    fn pause(&self) {
        let mut st = self.state.borrow_mut();
        let was_playing = st.playing;
        st.playing = false;
        st.last = None;
        if let (Some(id), Some(window)) = (st.raf.take(), web_sys::window()) {
            let _ = window.cancel_animation_frame(id);
        }
        self.dom.play.set_text_content(Some("Play"));
        set_state(&self.dom.host, "playing", false);
        drop(st);
        // the clock stopping is news: without a broadcast a follower keeps
        // the last tick's playing flag, so a chart cannot announce the pause
        // it just answered
        self.render();
        self.say_value(Say::AtOnce);
        // the stop is news for a listener as well, and only when it
        // happened ([`toggled`])
        if let Some(said) = toggled(was_playing, false) {
            self.announce(said);
        }
    }

    fn play(&self) {
        let was_playing = {
            let mut st = self.state.borrow_mut();
            if st.tc >= self.data.end() {
                st.tc = 0.0;
            }
            let was_playing = st.playing;
            st.playing = true;
            st.last = None;
            was_playing
        };
        self.dom.play.set_text_content(Some("Pause"));
        set_state(&self.dom.host, "playing", true);
        // the last word before the ticks take over and begin to wait
        self.say_value(Say::AtOnce);
        if let Some(said) = toggled(was_playing, true) {
            self.announce(said);
        }
        self.request_frame();
    }

    fn request_frame(&self) {
        if let (Some(window), Some(cb)) = (web_sys::window(), self.tick.borrow().as_ref())
            && let Ok(id) = window.request_animation_frame(cb.as_ref().unchecked_ref())
        {
            self.state.borrow_mut().raf = Some(id);
        }
    }

    fn step(&self, dk: i64) {
        self.pause();
        let k = self.data.frame_at(self.state.borrow().tc) as i64;
        let j = (k + dk).clamp(0, self.data.times.len() as i64 - 1) as usize;
        self.seek_to(self.data.times[j]);
    }

    /// Shift plus an arrow: one second of film time either way.
    fn jump_seconds(&self, dt: f64) {
        self.pause();
        let t = self.state.borrow().tc + dt;
        self.seek_to(t);
    }

    /// One chapter step: forward to the next chapter's start, back to this
    /// chapter's start and then to the previous one's. The Page keys and
    /// the two chapter buttons both take it. With no chapter left forward,
    /// the key still runs to the end (the button for it is disabled);
    /// with none left back, the film stays where it is.
    fn chapter(&self, dir: Chapter) {
        self.pause();
        let t = self.state.borrow().tc;
        match chapter_target(&self.data, t, dir) {
            Some(target) => self.seek_to(target),
            None if dir == Chapter::Next => self.seek_to(self.data.end()),
            None => {}
        }
    }

    fn set_pending(&self, k: usize) {
        self.state.borrow_mut().pending = Some(k);
        self.render();
        show_stage(
            &self.dom,
            &self.data,
            k,
            "pending, release to seek, Esc to cancel",
        );
        let _ = self.dom.stage.class_list().add_1("pending");
        show_peek(&self.dom, &self.data, self.data.times[k], None);
        set_state(&self.dom.host, "pending", true);
    }

    fn commit(&self) {
        let pending = self.state.borrow_mut().pending.take();
        if let Some(k) = pending {
            hide_peek(&self.dom);
            set_state(&self.dom.host, "pending", false);
            self.seek_to(self.data.times[k]);
        }
    }

    fn cancel(&self) {
        if self.state.borrow_mut().pending.take().is_some() {
            hide_peek(&self.dom);
            set_state(&self.dom.host, "pending", false);
            self.render();
            self.say(CANCELLED);
        }
    }

    fn t_at_pointer(&self, e: &MouseEvent) -> f64 {
        let Some(chart) = &self.dom.chart else {
            return 0.0;
        };
        let r = chart.get_bounding_client_rect();
        let layout = self.dom.layout;
        let px = (f64::from(e.client_x()) - r.left()) * (layout.width / r.width());
        layout.t_at(px).clamp(0.0, self.data.end())
    }

    fn frame_under(&self, e: &MouseEvent) -> Option<usize> {
        let document = web_sys::window()?.document()?;
        let el = document.element_from_point(e.client_x() as f32, e.client_y() as f32)?;
        // hit-test inside our own shadow tree
        let el = self
            .dom
            .host
            .shadow_root()?
            .element_from_point(e.client_x() as f32, e.client_y() as f32)
            .unwrap_or(el);
        let fr = el.closest(".fr").ok().flatten()?;
        fr.get_attribute("data-k")?.parse().ok()
    }
}

// ---- what a bound chart asks of the film ------------------------------
/// What a chart's event asks for. A seek and a peek carry the time in the
/// detail's [`TIME_FIELD`] field (a peek with no time hides the peek); a
/// toggle carries no detail at all.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Intent {
    Seek,
    Peek,
    Toggle,
}

/// Which event a chart sends for which intent. The names are the chart's
/// own, taken from the element that dispatches them, so a listener here
/// can never bind to a name the sender has stopped using.
const CHART_INTENTS: [(&str, Intent); 3] = [
    (SEEK_EVENT, Intent::Seek),
    (PEEK_EVENT, Intent::Peek),
    (TOGGLE_EVENT, Intent::Toggle),
];

/// What an event of `kind` asks of this film, if anything. The events
/// bubble and are composed, so the one listener on the document hears
/// every chart on the page and this decides whose they are: a chart names
/// the film it drives with `for`, and a chart with no `for` drives the
/// film it sits inside. A chart naming another film is not ours wherever
/// it sits, and a name we do not know asks for nothing.
///
/// Both answers are about the chart the event came from, which is the head
/// of the event's composed path: `for_attr` is read off it and `descendant`
/// says whether this film's host is on that same path. Nothing here may be
/// read off `target`, which retargeting has moved to a wrapper by the time
/// the document sees it.
fn intent_for(
    kind: &str,
    for_attr: Option<&str>,
    my_id: Option<&str>,
    descendant: bool,
) -> Option<Intent> {
    let (_, intent) = CHART_INTENTS.iter().find(|(name, _)| *name == kind)?;
    let ours = match for_attr.filter(|f| !f.is_empty()) {
        Some(film) => my_id.is_some_and(|id| id == film),
        None => descendant,
    };
    ours.then_some(*intent)
}

struct Wiring {
    _player: Rc<Player>,
    _closures: Vec<Closure<dyn FnMut(Event)>>,
}

struct Film {
    host: HtmlElement,
    wiring: Option<Wiring>,
}

const SPEEDS: [f64; 3] = [0.25, 0.5, 1.0];

/// The rect the film widens over the chapter under the pointer, by the
/// class the emitter writes on it. Named here because a query is a string
/// the compiler cannot check against the markup: the test below holds the
/// two together. It is not the block's annotation band, which is a rect of
/// the same shape and another thing entirely.
pub(crate) const PEEK_BAND: &str = ".peek-band";

/// The film's copy of the chart rules: the blocks it shares with
/// `<opt-chart>` whole, and the film's own colours for the parts it paints
/// from another token than the element does. A function rather than a
/// string inside the markup, so a test can read what the film ships and
/// hold it to the same rules as the element's stylesheet.
///
/// The peek band, which the film widens over the chapter under the
/// pointer, takes its wash here; the edge that belongs to the annotation
/// band alone is in [`CHART_SHAPE_CSS`]. [`chart_rules`] comes last, for
/// the reason given there.
///
/// The chart's text is 12 px and so is the title above it, decision 24's
/// floor: no text in a chart is drawn smaller, which is also the size the
/// emitter estimates its label widths at, so the labels are placed for the
/// size they are drawn at.
///
/// The cue buttons take decision 20's indicator and not the site's ring,
/// in the element's own words ([`cue_indicator_css`]). The playhead needs
/// none of this: the film's thumb is decoration ([`chart_aria`]), with no
/// role, no tab stop and nothing to focus.
pub(crate) fn chart_css() -> String {
    let rules = chart_rules();
    let cues = cue_indicator_css();
    format!(
        ".chart {{ max-width: 100%; height: auto; cursor: ew-resize; display: block; touch-action: none; font-family: var(--op-font-sans); font-size: 12px; }}
.charttitle {{ font-size: 12px; color: var(--op-text); margin-bottom: 0.25rem; }}
{CHART_SHAPE_CSS}
.chart .band, .chart .peek-band {{ fill: var(--op-accent); fill-opacity: 0.08; }}
.chart .bar-bg {{ fill: var(--op-border); }} .chart .bar-played {{ fill: var(--op-accent); }}
{CHART_CUE_CSS}
.chart .head {{ stroke: var(--op-accent); stroke-width: 1.5; }} .chart .head-dot {{ fill: var(--op-accent); }}
.chart .head-t {{ fill: var(--op-accent); font-weight: 700; paint-order: stroke; stroke: var(--op-surface); stroke-width: 4; }}
{cues}
{rules}"
    )
}

impl CustomElement for Film {
    fn connected(&mut self) {
        if self.wiring.is_some() {
            return;
        }
        let sheet = self.host.get_attribute("sheet").unwrap_or_default();
        let json = self
            .host
            .query_selector("script[type=\"application/json\"]")
            .ok()
            .flatten()
            .and_then(|s| s.text_content())
            .unwrap_or_default();
        let Some(data) = Data::parse(&json, sheet) else {
            shadow_root(&self.host).set_inner_html(&format!(
                "<style>{BASE_CSS}</style><p>opt-film: no frame data.</p>"
            ));
            return;
        };
        let n = data.times.len();
        let scale = if data.w < 220.0 {
            1.5
        } else {
            (320.0 / data.w).min(1.0)
        };
        let cell_w = (data.w * scale).round();
        let cell_h = (data.h * scale).round();
        let ss = (900.0 / data.w).min(if data.w < 220.0 { 3.0 } else { 1.25 });
        let stage_w = (data.w * ss).round();
        // A recorded video, when the element names one, plays over the
        // sheet's frame in the stage and follows the same clock; the sheet
        // stays underneath as the poster and the fallback.
        let video_markup = self
            .host
            .get_attribute("video")
            .filter(|v| !v.is_empty())
            .map(|v| {
                format!(
                    "<video class=\"stagevideo\" muted playsinline preload=\"metadata\" aria-hidden=\"true\" src=\"{}\"></video>",
                    escape(&v)
                )
            })
            .unwrap_or_default();
        let stage_h = (data.h * ss).round();
        let cells: String = data
            .times
            .iter()
            .enumerate()
            .map(|(k, t)| {
                let delta = data.deltas.get(k).copied().unwrap_or(0.0);
                let caption = if k == 0 {
                    readout(*t)
                } else if delta > 0.001 {
                    format!("{} · {:.0}%", readout(*t), delta * 100.0)
                } else {
                    format!("{} · same", readout(*t))
                };
                format!(
                    "<figure class=\"fr\" data-k=\"{k}\"><div class=\"cell\" style=\"width:{cell_w}px;height:{cell_h}px;background-image:url('{}');background-size:{}px {cell_h}px;background-position:{}px 0\"></div><figcaption>{caption}</figcaption></figure>",
                    escape(&data.sheet),
                    cell_w * n as f64,
                    -(k as f64) * cell_w
                )
            })
            .collect();
        let chart_css = chart_css();
        let layout = op_chart::Layout::film(data.t_max);
        // the chart, under the visible title its svg is named by (decision
        // 15); a film with nothing sampled draws neither, and then it has
        // no peek to show either
        let chart = if data.series.is_empty() {
            String::new()
        } else {
            format!("{}{}", chart_title(&data), chart_svg(&data, layout))
        };
        let peek = if chart.is_empty() {
            String::new()
        } else {
            format!(
                "<div class=\"peek\" hidden><div class=\"pframe\" style=\"width:{cell_w}px;height:{cell_h}px;background-image:url('{}');background-size:{}px {cell_h}px\"></div><div class=\"ptime\"></div></div>",
                escape(&data.sheet),
                cell_w * n as f64
            )
        };
        let shadow = shadow_root(&self.host);
        // The gridlines are axis-aligned hairlines and take crispEdges, so
        // they land on a device pixel instead of being blurred across two.
        // The swatch keeps it as well, though it is a legend key rather
        // than a rule: the interaction report samples its colour a pixel at
        // a time, and an anti-aliased end would give the probe a blend of
        // the swatch and the surface behind it.
        shadow.set_inner_html(&format!(
            "<style>{BASE_CSS}
:host {{ display: block; border: 1px solid var(--op-border); background: var(--op-surface); color: var(--op-text); padding: 0.5rem; max-width: 930px; user-select: none; -webkit-user-select: none; outline: 2px solid transparent; outline-offset: 2px; font-size: 0.9rem; }}
:host(:focus-visible) {{ outline-color: var(--op-accent); }}
.stagebox {{ display: flex; flex-direction: column; align-items: center; padding: 4px 0 8px; }}
.stagewrap {{ position: relative; display: inline-block; }}
.stage {{ background-repeat: no-repeat; border: 1px solid var(--op-border); }}
.stagevideo {{ position: absolute; inset: 1px; width: calc(100% - 2px); height: calc(100% - 2px); display: none; object-fit: fill; }}
.stagevideo.ready {{ display: block; }}
.stage.pending {{ outline: 2px dashed var(--op-accent); outline-offset: 2px; }}
.stagelabel {{ font-size: 0.8em; color: var(--op-muted); min-height: 1.2em; margin-top: 0.3rem; font-variant-numeric: tabular-nums; }}
.reelbox {{ position: relative; overflow: hidden; width: 100%; padding: 6px 0; border-top: 1px solid var(--op-border); touch-action: none; }}
.gate {{ position: absolute; left: 0; top: 0; bottom: 0; width: 0; margin-left: -1px; border-left: 2px solid var(--op-accent); opacity: 0.8; pointer-events: none; z-index: 2; }}
.reel {{ display: flex; gap: 8px; align-items: flex-start; will-change: transform; }}
.fr {{ margin: 0; flex: none; cursor: pointer; text-align: center; }}
.fr .cell {{ background-repeat: no-repeat; border: 1px solid var(--op-border); box-sizing: content-box; }}
.fr:hover .cell {{ border-color: var(--op-border-strong); }}
.fr.current .cell {{ outline: 2px solid var(--op-accent); outline-offset: 1px; }}
.fr.pending .cell {{ outline: 2px dashed var(--op-accent); outline-offset: 1px; }}
.fr figcaption {{ font-size: 0.75em; color: var(--op-muted); font-variant-numeric: tabular-nums; white-space: nowrap; }}
.bar {{ display: flex; gap: 0.6rem; align-items: center; margin-top: 0.4rem; flex-wrap: wrap; }}
.bar button, .bar select {{ font: inherit; color: var(--op-text); background: var(--op-raised); border: 1px solid var(--op-border-strong); border-radius: 0.25rem; padding: 0.15rem 0.6rem; cursor: pointer; }}
.bar button:hover:enabled {{ color: var(--op-link-hover); border-color: var(--op-link-hover); }}
.bar button:disabled {{ color: var(--op-muted); border-color: var(--op-border); cursor: default; }}
.bar button:focus-visible, .bar select:focus-visible, .bar input:focus-visible, .chart:focus-visible {{ outline: 2px solid var(--op-accent); outline-offset: 2px; }}
.bar input[type=range] {{ flex: 1; min-width: 220px; accent-color: var(--op-accent); }}
.t {{ font-variant-numeric: tabular-nums; min-width: 4.5rem; }} .n {{ color: var(--op-muted); }}
.keys {{ font-size: 0.85em; color: var(--op-muted); margin: 0.3rem 0 0; }} .keys summary {{ cursor: pointer; }}
.keys dl {{ display: grid; grid-template-columns: max-content 1fr; gap: 0.15rem 0.8rem; margin: 0.4rem 0; }} .keys dt {{ font-family: var(--op-font-mono); }} .keys dd {{ margin: 0; }}
.chartbox {{ margin-top: 0.6rem; position: relative; }}
{chart_css}
.peek {{ position: absolute; bottom: 56px; transform: translateX(-50%); pointer-events: none; background: var(--op-raised); border: 1px solid var(--op-border-strong); border-radius: 3px; padding: 3px; z-index: 3; }}
.peek .pframe {{ background-repeat: no-repeat; }} .peek .ptime {{ font-size: 0.8em; text-align: center; color: var(--op-text); font-variant-numeric: tabular-nums; white-space: nowrap; }}
.sr {{ position: absolute; width: 1px; height: 1px; overflow: hidden; clip: rect(0 0 0 0); white-space: nowrap; }}
</style>
<div class=\"stagebox\" part=\"stage\"><div class=\"stagewrap\"><div class=\"stage\" style=\"width:{stage_w}px;height:{stage_h}px;background-image:url('{sheet}');background-size:{}px {stage_h}px\"></div>{video_markup}</div><div class=\"stagelabel\"></div></div>
<div class=\"reelbox\" part=\"reel\"><div class=\"gate\"></div><div class=\"reel\">{cells}</div></div>
{bar}
<details class=\"keys\"><summary>Keys</summary><dl><dt>Space, K</dt><dd>play / pause</dd><dt>, .</dt><dd>previous / next frame</dd><dt>← →</dt><dd>five frames back / forward</dd><dt>J L</dt><dd>ten frames back / forward</dd><dt>Shift ← →</dt><dd>one second back / forward</dd><dt>PgUp PgDn</dt><dd>previous / next chapter (also Ctrl or Alt with an arrow)</dd><dt>0-9</dt><dd>seek to 0-90 %</dd><dt>Home End</dt><dd>first / last frame</dd><dt>&lt; &gt;</dt><dd>slower / faster</dd><dt>Esc</dt><dd>cancel a pending seek on the strip</dd></dl><p>Hover the chart or the strip to peek without moving the playhead; press and drag across the strip to choose a frame and release to seek.</p></details>
<div class=\"chartbox\">{chart}{peek}</div><span class=\"sr\" aria-live=\"polite\"></span>",
            stage_w * n as f64,
            sheet = escape(&data.sheet),
            bar = control_bar(n),
        ));
        if self.host.get_attribute("tabindex").is_none() {
            let _ = self.host.set_attribute("tabindex", "0");
        }
        if self.host.get_attribute("role").is_none() {
            let _ = self.host.set_attribute("role", "group");
        }
        let q = |sel: &str| shadow.query_selector(sel).ok().flatten();
        let qh = |sel: &str| q(sel).and_then(|e| e.dyn_into::<HtmlElement>().ok());
        let frames: Vec<HtmlElement> = (0..n)
            .filter_map(|k| qh(&format!(".fr[data-k=\"{k}\"]")))
            .collect();
        let (
            Some(stage),
            Some(stage_label),
            Some(reelbox),
            Some(reel),
            Some(gate),
            Some(slider),
            Some(time_label),
            Some(play),
            Some(rate),
            Some(live),
        ) = (
            qh(".stage"),
            q(".stagelabel"),
            qh(".reelbox"),
            qh(".reel"),
            qh(".gate"),
            q("input[type=range]"),
            q(".t"),
            q("button.play"),
            q("select"),
            q(".sr"),
        )
        else {
            return;
        };
        let dom = Rc::new(Dom {
            host: self.host.clone(),
            stage,
            stage_label,
            video: qh(".stagevideo").and_then(|v| v.dyn_into::<web_sys::HtmlVideoElement>().ok()),
            reelbox,
            reel,
            gate,
            frames,
            slider,
            time_label,
            play,
            rate,
            chapters: CHAPTER_CONTROLS
                .iter()
                .filter_map(|(class, _, dir)| Some((*dir, q(&format!("button.{class}"))?)))
                .collect(),
            chart: q(".chart"),
            layout,
            peek: qh(".peek"),
            pframe: qh(".pframe"),
            ptime: q(".ptime"),
            live,
            cell_w,
            stage_w,
        });
        let data = Rc::new(data);
        let state = Rc::new(RefCell::new(State {
            tc: 0.0,
            playing: false,
            pending: None,
            last: None,
            rate: 1.0,
            raf: None,
        }));
        let player = Rc::new(Player {
            dom: dom.clone(),
            data: data.clone(),
            state: state.clone(),
            tick: Rc::new(RefCell::new(None)),
            value_written: Cell::new(f64::NEG_INFINITY),
            scrub: Cell::new(false),
        });
        // the animation-frame loop: advances the clock on real time
        {
            let p = player.clone();
            *player.tick.borrow_mut() = Some(Closure::new(move |now: f64| {
                let playing = {
                    let mut st = p.state.borrow_mut();
                    if !st.playing {
                        false
                    } else {
                        if let Some(last) = st.last {
                            st.tc += (now - last) / 1000.0 * st.rate;
                            if st.tc > p.data.end() + 0.6 {
                                st.tc = 0.0;
                            }
                        }
                        st.last = Some(now);
                        true
                    }
                };
                if playing {
                    p.render();
                    // the clock's own write: onto a slider that has the
                    // focus, and no oftener than the wait (decision 18)
                    p.say_value(Say::OnTheClock);
                    p.request_frame();
                }
            }));
        }
        let mut closures: Vec<Closure<dyn FnMut(Event)>> = Vec::new();
        let mut listen = |target: &Element, name: &str, closure: Closure<dyn FnMut(Event)>| {
            let _ = target.add_event_listener_with_callback(name, closure.as_ref().unchecked_ref());
            closures.push(closure);
        };
        {
            let p = player.clone();
            listen(
                &dom.play,
                "click",
                Closure::new(move |_| {
                    if p.state.borrow().playing {
                        p.pause()
                    } else {
                        p.play()
                    }
                }),
            );
        }
        for (dir, button) in &dom.chapters {
            let p = player.clone();
            let dir = *dir;
            listen(button, "click", Closure::new(move |_| p.chapter(dir)));
        }
        // The slider's own gesture, in the two events it tells it with
        // ([`SLIDER_MOMENTS`]): the drag moves the clock and says nothing,
        // and the change that ends it says where the drag landed.
        for (name, moment) in SLIDER_MOMENTS {
            let p = player.clone();
            listen(
                &dom.slider,
                name,
                Closure::new(move |e: Event| match moment {
                    Moment::Press => {
                        if let Some(target) = e.target()
                            && let Ok(v) =
                                js_sys::Reflect::get(&target, &JsValue::from_str("value"))
                            && let Some(k) = v.as_string().and_then(|s| s.parse::<usize>().ok())
                        {
                            // the film stops on the first step of the drag
                            // and is not stopped again on the rest of them:
                            // a pause speaks at once, so pausing per pixel
                            // is the flood by another door
                            if p.state.borrow().playing {
                                p.pause();
                            }
                            p.moment(Moment::Press);
                            p.seek_to(p.data.times[k.min(p.data.times.len() - 1)]);
                        }
                    }
                    _ => p.moment(moment),
                }),
            );
        }
        {
            let p = player.clone();
            listen(
                &dom.slider,
                "focus",
                // what a reader hears on arrival is written on arrival.
                // The clock's own writes stop while nothing has the focus
                // (decision 18), so the words on the control are as old
                // as the last gesture, and a film played through with the
                // focus elsewhere left them at the time it started from.
                // The element keeps its thumb current from the other end,
                // writing as the focus leaves; either way what is spoken
                // on arrival is where the clock now stands.
                Closure::new(move |_| p.say_value(Say::AtOnce)),
            );
        }
        {
            let p = player.clone();
            listen(
                &dom.rate,
                "change",
                Closure::new(move |e: Event| {
                    if let Some(target) = e.target()
                        && let Ok(v) = js_sys::Reflect::get(&target, &JsValue::from_str("value"))
                        && let Some(r) = v.as_string().and_then(|s| s.parse::<f64>().ok())
                    {
                        p.state.borrow_mut().rate = r;
                    }
                }),
            );
        }
        if let Some(chart) = dom.chart.clone() {
            let p = player.clone();
            listen(
                &chart,
                "pointermove",
                Closure::new(move |e: Event| {
                    let Ok(m) = e.dyn_into::<MouseEvent>() else {
                        return;
                    };
                    match moved(m.buttons(), p.scrub.get()) {
                        Moved::Scrub => {
                            let t = p.t_at_pointer(&m);
                            p.seek_to(t);
                            hide_peek(&p.dom);
                        }
                        Moved::Hover => {
                            if let Some(chart) = &p.dom.chart {
                                let r = chart.get_bounding_client_rect();
                                let t = p.t_at_pointer(&m);
                                show_peek(
                                    &p.dom,
                                    &p.data,
                                    t,
                                    Some(f64::from(m.client_x()) - r.left()),
                                );
                            }
                        }
                        // somebody else's drag, passing over the chart:
                        // the film has nothing to answer it with
                        Moved::Stray => {}
                    }
                }),
            );
            let p = player.clone();
            listen(
                &chart,
                "pointerleave",
                Closure::new(move |_| hide_peek(&p.dom)),
            );
            let p = player.clone();
            listen(
                &chart,
                "pointerdown",
                Closure::new(move |e: Event| {
                    e.prevent_default();
                    let Ok(pe) = e.dyn_into::<PointerEvent>() else {
                        return;
                    };
                    p.pause();
                    hide_peek(&p.dom);
                    if let Some(chart) = &p.dom.chart {
                        let _ = chart.set_pointer_capture(pe.pointer_id());
                    }
                    // the press begins a scrub: what it and every move
                    // after it have to say is said once, at the release
                    p.moment(Moment::Press);
                    let t = p.t_at_pointer(&pe);
                    p.seek_to(t);
                }),
            );
            for name in ["pointerup", "pointercancel"] {
                let p = player.clone();
                listen(
                    &chart,
                    name,
                    Closure::new(move |_| p.moment(Moment::Settle)),
                );
            }
        }
        for (k, fr) in dom.frames.iter().enumerate() {
            let p = player.clone();
            listen(
                fr,
                "pointerenter",
                Closure::new(move |_| {
                    if p.state.borrow().pending.is_none() {
                        show_peek(&p.dom, &p.data, p.data.times[k], None);
                    }
                }),
            );
            let p = player.clone();
            listen(
                fr,
                "pointerleave",
                Closure::new(move |_| {
                    if p.state.borrow().pending.is_none() {
                        hide_peek(&p.dom);
                    }
                }),
            );
        }
        {
            let p = player.clone();
            listen(
                &dom.reelbox,
                "pointerdown",
                Closure::new(move |e: Event| {
                    let Ok(pe) = e.dyn_into::<PointerEvent>() else {
                        return;
                    };
                    let Some(k) = p.frame_under(&pe) else { return };
                    pe.prevent_default();
                    p.pause();
                    let _ = p.dom.reelbox.set_pointer_capture(pe.pointer_id());
                    p.set_pending(k);
                }),
            );
            let p = player.clone();
            listen(
                &dom.reelbox,
                "pointermove",
                Closure::new(move |e: Event| {
                    if p.state.borrow().pending.is_none() {
                        return;
                    }
                    let Ok(m) = e.dyn_into::<MouseEvent>() else {
                        return;
                    };
                    // read the pending frame in its own statement: a borrow
                    // inside an `if let` condition lives through the body and
                    // would collide with set_pending's borrow_mut
                    let current = p.state.borrow().pending;
                    if let Some(k) = p.frame_under(&m)
                        && Some(k) != current
                    {
                        p.set_pending(k);
                    }
                }),
            );
            let p = player.clone();
            listen(&dom.reelbox, "pointerup", Closure::new(move |_| p.commit()));
            let p = player.clone();
            listen(
                &dom.reelbox,
                "pointercancel",
                Closure::new(move |_| p.cancel()),
            );
        }
        {
            let p = player.clone();
            listen(
                &self.host,
                "keydown",
                Closure::new(move |e: Event| {
                    let Ok(ke) = e.dyn_into::<KeyboardEvent>() else {
                        return;
                    };
                    let key = ke.key();
                    let n = p.data.times.len() as i64;
                    // Ctrl or Alt with an arrow aliases the chapter keys, as
                    // YouTube does; Shift with an arrow is the larger jump.
                    let chapter_alias = ke.ctrl_key() || ke.alt_key();
                    // What holds the focus is read before the table, so a
                    // press on one of the chart's cue buttons is answered
                    // as the element answers it and the table never sees
                    // the key: Space on a chapter seeks to the chapter,
                    // where before it played the film ([`cue_press`]).
                    let handled = if let Some(t) = cue_press(p.focused_cue().as_deref(), &key) {
                        p.pause();
                        p.seek_to(t);
                        true
                    } else if let Some(dir) = chapter_key(&key, chapter_alias) {
                        p.chapter(dir);
                        true
                    } else {
                        match key.as_str() {
                            "ArrowRight" if ke.shift_key() => {
                                p.jump_seconds(1.0);
                                true
                            }
                            "ArrowLeft" if ke.shift_key() => {
                                p.jump_seconds(-1.0);
                                true
                            }
                            " " | "k" | "K" => {
                                if p.state.borrow().playing {
                                    p.pause()
                                } else {
                                    p.play()
                                }
                                true
                            }
                            "." => {
                                p.step(1);
                                true
                            }
                            "," => {
                                p.step(-1);
                                true
                            }
                            "ArrowRight" => {
                                p.step(5);
                                true
                            }
                            "ArrowLeft" => {
                                p.step(-5);
                                true
                            }
                            "l" | "L" => {
                                p.step(10);
                                true
                            }
                            "j" | "J" => {
                                p.step(-10);
                                true
                            }
                            "Home" => {
                                p.step(-n);
                                true
                            }
                            "End" => {
                                p.step(n);
                                true
                            }
                            "Escape" => {
                                p.cancel();
                                true
                            }
                            ">" | "<" => {
                                let dir: i64 = if key == ">" { 1 } else { -1 };
                                let rate = p.state.borrow().rate;
                                let i = SPEEDS
                                    .iter()
                                    .position(|r| (*r - rate).abs() < 1e-9)
                                    .unwrap_or(2) as i64;
                                let r =
                                    SPEEDS[(i + dir).clamp(0, SPEEDS.len() as i64 - 1) as usize];
                                p.state.borrow_mut().rate = r;
                                let _ = js_sys::Reflect::set(
                                    &p.dom.rate,
                                    &JsValue::from_str("value"),
                                    &JsValue::from_str(&format!("{r}")),
                                );
                                p.say(&format!("Speed {r}x"));
                                true
                            }
                            k if k.len() == 1 && k.as_bytes()[0].is_ascii_digit() => {
                                let tenth = f64::from(k.as_bytes()[0] - b'0') / 10.0;
                                p.pause();
                                let t = p.data.end() * tenth;
                                p.seek_to(t);
                                true
                            }
                            _ => false,
                        }
                    };
                    if handled {
                        ke.prevent_default();
                    }
                }),
            );
        }
        // the recording shows itself as soon as it has a frame, without
        // waiting for the next render
        if let Some(video) = dom.video.clone() {
            let v = video.clone();
            listen(
                &video,
                "loadeddata",
                Closure::new(move |_| {
                    let _ = v.class_list().add_1("ready");
                }),
            );
        }
        // A bound chart's intents. One listener on the document, taking all
        // three names, is enough: the events are composed and bubbling, so
        // they reach the document from any chart on the page, and the
        // source test says which of them are addressing this film. Reading
        // the source off the event also means a chart that upgrades later,
        // or moves, needs no rebinding here.
        {
            let p = player.clone();
            let my_id = self.host.get_attribute("id");
            let closure = Closure::<dyn FnMut(Event)>::new(move |e: Event| {
                // the path is the whole route the event took, innermost
                // first, so its head is the chart `intent_for` needs and
                // `target` is not. An event not being dispatched has an
                // empty path, and there the retargeted target is all
                // there is.
                let path = e.composed_path();
                let Some(source) = path
                    .get(0)
                    .dyn_into::<Element>()
                    .ok()
                    .or_else(|| e.target().and_then(|t| t.dyn_into::<Element>().ok()))
                else {
                    return;
                };
                // the path also carries every host it crossed on the way
                // out, this film's own among them, which is what makes a
                // chart inside this film's shadow root ours as surely as
                // one slotted into its light DOM. `Node.contains` said the
                // same only by counting a node as containing itself, after
                // retargeting had made the chart into this host.
                let host: &JsValue = p.dom.host.as_ref();
                let descendant = path.iter().any(|node| node == *host);
                let Some(intent) = intent_for(
                    &e.type_(),
                    source.get_attribute("for").as_deref(),
                    my_id.as_deref(),
                    descendant,
                ) else {
                    return;
                };
                // Reflect::get throws on a detail that is not an object,
                // which reads here as no time at all
                let time = e
                    .dyn_ref::<web_sys::CustomEvent>()
                    .and_then(|c| {
                        js_sys::Reflect::get(&c.detail(), &JsValue::from_str(TIME_FIELD)).ok()
                    })
                    .and_then(|v| v.as_f64());
                match intent {
                    Intent::Seek => {
                        if let Some(t) = time {
                            p.pause();
                            p.seek_to(t);
                        }
                    }
                    Intent::Peek => match time {
                        Some(t) => show_peek(&p.dom, &p.data, t, None),
                        None => hide_peek(&p.dom),
                    },
                    Intent::Toggle => {
                        if p.state.borrow().playing {
                            p.pause()
                        } else {
                            p.play()
                        }
                    }
                }
            });
            if let Some(document) = web_sys::window().and_then(|w| w.document()) {
                for (name, _) in CHART_INTENTS {
                    let _ = document
                        .add_event_listener_with_callback(name, closure.as_ref().unchecked_ref());
                }
            }
            closures.push(closure);
        }
        player.render();
        // the words go on the slider before anything can tab to it
        player.say_value(Say::AtOnce);
        self.wiring = Some(Wiring {
            _player: player,
            _closures: closures,
        });
    }
}

#[cfg(test)]
mod chart_reference {
    //! Fixtures shaped like the films the report tool emits, and the
    //! film-side rules over them.
    use super::super::chart::{CUE_RULE, VALUETEXT_WAIT};
    use super::super::chart_style::{FORCED_COLOURS_CSS, PRINT_CSS, SERIES_CSS};
    use super::*;

    /// The opening tag of every `<tag>` in `svg`: what stands between the
    /// name and the `>` that ends it.
    fn heads<'a>(svg: &'a str, tag: &str) -> Vec<&'a str> {
        svg.split(&format!("<{tag}"))
            .skip(1)
            .filter_map(|rest| rest.split_once('>').map(|(head, _)| head))
            .collect()
    }

    /// Every `<text>` in `svg` as its opening tag and the words it draws.
    fn texts(svg: &str) -> Vec<(&str, &str)> {
        svg.split("<text")
            .skip(1)
            .filter_map(|rest| {
                let (head, rest) = rest.split_once('>')?;
                Some((head, rest.split_once("</text>")?.0))
            })
            .collect()
    }

    /// The attribute a slider role is written as, assembled so that a test
    /// asserting the film writes it nowhere is never one of its own
    /// matches when it reads this file's source.
    fn slider_role() -> String {
        ["role=", "\"slider\""].concat()
    }

    /// One attribute of an opening tag, empty where the tag has none. The
    /// space before the name is part of the needle, so `x` cannot be found
    /// inside another attribute's name or value.
    fn attr(head: &str, name: &str) -> String {
        head.split_once(&format!(" {name}=\""))
            .and_then(|(_, rest)| rest.split_once('"'))
            .map_or(String::new(), |(value, _)| value.to_owned())
    }

    fn series(label: &str, index: usize, t: &[f64], y: &[f64], lw: f64) -> Series {
        Series {
            label: label.to_owned(),
            index,
            t: t.to_vec(),
            y: y.to_vec(),
            lw,
        }
    }

    /// The demo film on /component/film/: eight frames, one series.
    pub(crate) fn demo() -> Data {
        let times = [0.0, 0.2, 0.45, 0.8, 1.2, 1.7, 2.3, 3.0];
        Data {
            sheet: "/opt-film-demo.png".to_owned(),
            w: 220.0,
            h: 140.0,
            times: times.to_vec(),
            deltas: vec![0.0, 0.12, 0.31, 0.5, 0.42, 0.2, 0.08, 0.0],
            chapters: vec![(0.0, "start".to_owned()), (1.2, "settle".to_owned())],
            series: vec![series(
                "thumb travel",
                2,
                &times,
                &[0.0, 8.0, 30.0, 61.0, 84.0, 95.0, 99.0, 100.0],
                2.4,
            )],
            ylabel: "progress %".to_owned(),
            t_max: 3.0,
        }
    }

    /// A toggle flight: two series, a dashed one, a label with an angle
    /// bracket to escape, and a long time axis that switches the tick step.
    pub(crate) fn flight() -> Data {
        let t: Vec<f64> = (0..=37).map(|i| f64::from(i) * 0.1).collect();
        let ghost: Vec<f64> = t
            .iter()
            .map(|x| (100.0 * (1.0 - (-x * 1.4).exp())).min(100.0))
            .collect();
        let palette: Vec<f64> = t
            .iter()
            .map(|x| (100.0 * (x / 3.0).clamp(0.0, 1.0)).min(100.0))
            .collect();
        Data {
            sheet: "flight-film.png".to_owned(),
            w: 320.0,
            h: 180.0,
            times: t.clone(),
            deltas: t.iter().map(|x| (x * 3.0).sin().abs() * 0.4).collect(),
            chapters: vec![
                (0.0, "flight".to_owned()),
                (1.5, "abort <early>".to_owned()),
                (3.03, "settled".to_owned()),
            ],
            series: vec![
                series("ghost left %", 3, &t, &ghost, 2.4),
                series("palette blend %", 1, &t, &palette, 1.8),
            ],
            ylabel: "% (opacity, left)".to_owned(),
            t_max: 3.7,
        }
    }

    #[test]
    fn every_series_carries_its_palette_class_and_the_stylesheet_maps_it() {
        let d = flight();
        let svg = op_chart::render(&spec_of(&d), op_chart::Layout::film(d.t_max)).svg;
        assert!(svg.contains("class=\"series-3\"") && svg.contains("class=\"series-1\""));
        let d = demo();
        let svg = op_chart::render(&spec_of(&d), op_chart::Layout::film(d.t_max)).svg;
        assert!(
            svg.contains("<path class=\"series-2\"") && svg.contains("class=\"swatch series-2\"")
        );
        assert!(!svg.contains("stroke=\"#") && !svg.contains("fill=\"#"));
        for n in 1..=SERIES_TOKENS {
            assert!(SERIES_CSS.contains(&format!(
                ".chart .series-{n} {{ stroke: var(--op-series-{n}); }}"
            )));
        }
        // one dash pattern per series after the first, all distinct
        let dashes: Vec<&str> = SERIES_CSS
            .split("stroke-dasharray: ")
            .skip(1)
            .map(|d| d.split(';').next().unwrap())
            .collect();
        assert_eq!(dashes.len(), SERIES_TOKENS - 1);
        let distinct: std::collections::BTreeSet<&str> = dashes.iter().copied().collect();
        assert_eq!(distinct.len(), dashes.len());
        assert!(!SERIES_CSS.contains(".series-1 { stroke-dasharray"));
        // forced colours: every paint the chart uses is mapped to a system colour
        for class in [
            "path[class^=series]",
            ".marker",
            ".endlabel",
            ".grid",
            ".band",
            ".bar-played",
            ".head",
            ".peek-line",
        ] {
            assert!(
                FORCED_COLOURS_CSS.contains(class),
                "{class} has no forced-colours rule"
            );
        }
        for system in ["CanvasText", "Canvas", "GrayText", "Highlight"] {
            assert!(FORCED_COLOURS_CSS.contains(system));
        }
        assert!(!FORCED_COLOURS_CSS.contains("var(--op-series"));
        // print: the same paints mapped to print blacks and greys on white, identity by dash and marker
        for class in [
            "path[class^=series]",
            ".swatch",
            ".tick",
            ".chart .mark,",
            ".axis",
            ".marker",
            ".endlabel",
            ".head-t",
            ".head ",
            ".grid",
            ".band",
            ".bar-played",
            "print-color-adjust: exact",
        ] {
            assert!(PRINT_CSS.contains(class), "{class} has no print rule");
        }
        assert!(PRINT_CSS.starts_with("\n@media print {"));
        assert!(!PRINT_CSS.contains("var(--op-series"));
        // the print rule never touches the series dash table: the declaration block that
        // addresses the series paths carries no dasharray
        let series_block = PRINT_CSS
            .split("path[class^=series]")
            .nth(1)
            .and_then(|rest| rest.split('{').nth(1))
            .and_then(|block| block.split('}').next())
            .expect("a print rule for the series");
        assert!(!series_block.contains("stroke-dasharray"), "{series_block}");
        // and print addresses no single series at all
        assert!(!PRINT_CSS.contains(".series-"));
        assert!(PRINT_CSS.contains(".chart .marker { display: inline;"));
        // and the assembled rules the stylesheet carries include all three blocks
        let rules = chart_rules();
        assert!(
            rules.contains("@media print {")
                && rules.contains("@media (forced-colors: active)")
                && rules.contains(".chart .series-6")
        );
    }

    #[test]
    fn chapter_keys_land_on_chapter_starts() {
        let d = flight(); // chapters at 0, 1.5 and 3.03; frames every 0.1 s
        assert_eq!(d.next_chapter_start(0.0), Some(1.5));
        assert_eq!(d.next_chapter_start(1.5), Some(3.03));
        assert_eq!(d.next_chapter_start(3.5), None);
        // exactly on a chapter start: the previous chapter
        assert_eq!(d.prev_chapter_start(1.5), 0.0);
        assert_eq!(d.prev_chapter_start(3.03), 1.5);
        // past the chapter's first frame: back to the chapter start
        assert_eq!(d.prev_chapter_start(1.7), 1.5);
        assert_eq!(d.prev_chapter_start(0.3), 0.0);
        assert_eq!(d.prev_chapter_start(3.6), 3.03);
    }

    #[test]
    fn chart_intents_name_their_events_and_only_this_films_charts_apply() {
        // which name means which intent: the table is read in order, so a
        // pair swapped here would have a seek hide the preview instead.
        // The names themselves are the chart's, pinned where they are set
        assert_eq!(
            CHART_INTENTS.map(|(name, _)| name),
            ["opt-chart-seek", "opt-chart-peek", "opt-chart-toggle"]
        );
        let mine = Some("film-toggle");
        // a chart naming this film: every intent arrives
        for (name, intent) in CHART_INTENTS {
            assert_eq!(
                intent_for(name, Some("film-toggle"), mine, false),
                Some(intent),
                "{name} from a chart naming this film"
            );
        }
        // a chart naming another film, wherever it sits
        assert_eq!(
            intent_for("opt-chart-seek", Some("film-other"), mine, false),
            None
        );
        assert_eq!(
            intent_for("opt-chart-seek", Some("film-other"), mine, true),
            None
        );
        // no name: the film the chart sits inside, and no other
        assert_eq!(
            intent_for("opt-chart-seek", None, mine, true),
            Some(Intent::Seek)
        );
        assert_eq!(intent_for("opt-chart-seek", None, mine, false), None);
        // an empty `for` is no name at all
        assert_eq!(
            intent_for("opt-chart-peek", Some(""), mine, true),
            Some(Intent::Peek)
        );
        assert_eq!(intent_for("opt-chart-peek", Some(""), mine, false), None);
        // a film with no id is addressed only by what it contains
        assert_eq!(
            intent_for("opt-chart-toggle", Some("film-toggle"), None, false),
            None
        );
        assert_eq!(
            intent_for("opt-chart-toggle", None, None, true),
            Some(Intent::Toggle)
        );
        // a name that is not one of the three asks for nothing
        assert_eq!(
            intent_for("opt-film-time", Some("film-toggle"), mine, true),
            None
        );
        assert_eq!(intent_for("opt-chart-scrub", None, mine, true), None);
        // a chart inside a third element's shadow root, naming another
        // film, with that wrapper sitting in this film's light DOM. The
        // wrapper has no `for` and this film does contain it, so reading
        // the retargeted target would have this film apply a seek
        // addressed to another. Read off the chart the composed path
        // names, the `for` decides and the answer is no
        assert_eq!(
            intent_for("opt-chart-seek", Some("film-other"), mine, true),
            None
        );
        // and the same chart, naming this film from inside that wrapper,
        // is ours: the path carries the chart whatever tree it sits in
        assert_eq!(
            intent_for("opt-chart-seek", Some("film-toggle"), mine, false),
            Some(Intent::Seek)
        );
        // which is only true because the caller reads the composed path.
        // `target` is retargeted to the outermost host outside this
        // listener's tree, and that host is the wrapper. Two needles, for
        // two ways of losing that: reading no path at all, and reading one
        // but asking membership of something else, as `Node.contains` did
        // by counting a node as containing itself. Both are assembled, so
        // these lines are not among their own matches
        let source = include_str!("film.rs");
        let path = ["e.composed", "_path()"].concat();
        assert!(
            source.contains(&path),
            "the listener must read the event's composed path"
        );
        let membership = ["path.iter().any(", "|node| node == *host)"].concat();
        assert!(
            source.contains(&membership),
            "this film's host must be looked for on that same path"
        );
    }

    #[test]
    fn the_chapter_buttons_sit_in_the_bar_and_take_the_step_the_keys_take() {
        let bar = control_bar(8);
        for (class, name, _) in CHAPTER_CONTROLS {
            assert!(
                bar.contains(&format!(
                    "<button type=\"button\" class=\"{class}\">{name}</button>"
                )),
                "{class} is not in the bar: {bar}"
            );
        }
        assert_eq!(
            CHAPTER_CONTROLS.map(|(_, name, _)| name),
            ["Previous chapter", "Next chapter"]
        );
        // they sit with the transport: play, back, forward, then speed
        let at = |needle: &str| bar.find(needle).expect(needle);
        assert!(at("class=\"play\"") < at("class=\"chapter-prev\""));
        assert!(at("class=\"chapter-prev\"") < at("class=\"chapter-next\""));
        assert!(at("class=\"chapter-next\"") < at("<select"));
        // a button and a key ask for the same step, of the same type ...
        assert_eq!(chapter_key("PageDown", false), Some(Chapter::Next));
        assert_eq!(chapter_key("PageUp", false), Some(Chapter::Prev));
        assert_eq!(chapter_key("ArrowRight", true), Some(Chapter::Next));
        assert_eq!(chapter_key("ArrowLeft", true), Some(Chapter::Prev));
        assert_eq!(chapter_key("ArrowRight", false), None);
        assert_eq!(chapter_key("Home", true), None);
        // ... and one helper takes it: the two call sites are the key
        // handler and the buttons' click, and there are no others
        let source = include_str!("film.rs");
        // the needle is assembled, so this line is not one of its matches
        let call = ["p.chapter", "(dir)"].concat();
        assert_eq!(
            source.matches(&call).count(),
            2,
            "the keys and the buttons must step through the same helper"
        );
        // where the step lands is the helper the keys landed on before
        let d = flight(); // chapters at 0, 1.5 and 3.03; the film ends at 3.7
        for t in [0.0, 0.4, 1.5, 1.7, 3.03, 3.6] {
            assert_eq!(
                chapter_target(&d, t, Chapter::Next),
                d.next_chapter_start(t)
            );
            match chapter_target(&d, t, Chapter::Prev) {
                Some(target) => {
                    assert_eq!(target, d.prev_chapter_start(t));
                    assert!(target < t);
                }
                None => assert!(
                    (d.prev_chapter_start(t) - t).abs() <= 1e-6,
                    "no step back from {t}, so the step must not move"
                ),
            }
        }
        // and a button is disabled exactly where there is no chapter to
        // step to: at the start going back, in the last chapter going on
        assert_eq!(chapter_target(&d, 0.0, Chapter::Prev), None);
        assert_eq!(chapter_target(&d, 3.6, Chapter::Next), None);
        assert_eq!(chapter_target(&d, 0.4, Chapter::Prev), Some(0.0));
        assert_eq!(chapter_target(&d, 0.0, Chapter::Next), Some(1.5));
    }

    #[test]
    fn the_layout_the_film_keeps_matches_the_chart_it_drew() {
        let d = flight();
        let layout = op_chart::Layout::film(d.t_max);
        let r = op_chart::render(&spec_of(&d), layout);
        assert_eq!(r.layout, layout);
        assert_eq!(r.layout.t_at(r.layout.x_of(1.5)), 1.5);
    }

    /// The rect the peek widens is the one the emitter draws for it, and
    /// no other: the two rects of that shape differ only by their class,
    /// and a query for the block's band would take whichever came first.
    #[test]
    fn the_peek_moves_the_rect_the_emitter_draws_for_it_and_no_other() {
        let d = flight();
        let svg = op_chart::render(&spec_of(&d), op_chart::Layout::film(d.t_max)).svg;
        let class = format!("class=\"{}\"", PEEK_BAND.trim_start_matches('.'));
        assert_eq!(svg.matches(&class).count(), 1, "{class} in {svg}");
        // and the film draws no annotation band, so nothing else is there
        // to be taken for it
        assert!(!svg.contains("class=\"band\""), "{svg}");
        assert!(d.chapters.len() > 1, "a film with no chapter to peek at");
    }

    /// Decision 15 in the embedded case: the film's chart is a document
    /// with a name and its thumb is decoration, because the film's own
    /// control bar holds the slider. The svg was a focusable
    /// `role="slider"` whose value the film wrote and nothing announced,
    /// which is a control with no words in a widget that already has one.
    #[test]
    fn the_films_chart_is_a_named_document_and_its_thumb_is_decoration() {
        let d = flight();
        let l = op_chart::Layout::film(d.t_max);
        let svg = chart_svg(&d, l);
        assert!(
            svg.starts_with(&format!(
                "<svg class=\"chart\" part=\"chart\" viewBox=\"0 0 900 268\" tabindex=\"0\" role=\"graphics-document\" aria-labelledby=\"{CHART_TITLE_ID}\">"
            )),
            "{}",
            svg.split_once('>').map_or("", |(head, _)| head)
        );
        // the name it points at is visible markup in the same shadow tree,
        // and it says what the chart draws
        let title = chart_title(&d);
        assert!(
            title.contains(&format!("id=\"{CHART_TITLE_ID}\"")) && title.contains(&d.ylabel),
            "{title}"
        );
        assert!(!title.contains("hidden"), "{title}");
        assert!(
            chart_css().contains(".charttitle {"),
            "the title is unpainted"
        );
        // the chart claims no control and announces no value: the film's
        // own slider does both
        for never in [slider_role().as_str(), "aria-valu"] {
            assert!(!svg.contains(never), "{never} in the film's chart");
        }
        // the thumb is hidden, and there is nothing focusable inside it to
        // be hidden away from a keyboard
        let after = svg.split_once(THUMB_TAG).expect("the thumb").1;
        let head = after.split_once('>').expect("the tag ends").0;
        assert!(
            head.starts_with(" aria-hidden=\"true\""),
            "the thumb is not hidden: {head}"
        );
        let group = after.split_once("</g>").expect("the group ends").0;
        for never in ["tabindex", "role="] {
            assert!(!group.contains(never), "{never} inside the thumb: {group}");
        }
        // and the emitter writes the tag this hangs on, so the day it
        // changes the film fails here rather than quietly exposing the
        // readout and the ring to a reader again
        let bare = op_chart::render_with(&spec_of(&d), l, &chart_aria(&d)).svg;
        assert_eq!(bare.matches(THUMB_TAG).count(), 1, "{bare}");
        assert!(
            !bare.contains(&format!("{THUMB_TAG} aria-hidden")),
            "{bare}"
        );
        // one tab stop in the chart, and it is the svg the film listens on
        assert_eq!(svg.matches("tabindex=\"0\"").count(), 1, "{svg}");
    }

    /// One widget, one slider (decision 15): the film's is the native range
    /// input in its control bar, which keeps its own name and its own
    /// value, and nothing else in the film's markup takes the role.
    #[test]
    fn the_film_has_one_slider_and_it_is_the_native_range_input() {
        let d = flight();
        let n = d.times.len();
        let markup = format!(
            "{}{}{}",
            control_bar(n),
            chart_title(&d),
            chart_svg(&d, op_chart::Layout::film(d.t_max))
        );
        assert!(
            markup.contains(&format!(
                "<input type=\"range\" min=\"0\" max=\"{}\" value=\"0\" aria-label=\"frame\">",
                n - 1
            )),
            "{markup}"
        );
        // the role is assembled, so neither this test nor the source scan
        // below is one of its own matches
        assert!(
            !markup.contains(&slider_role()),
            "a second slider in {markup}"
        );
        let source = include_str!("film.rs");
        let in_source = ["role=", "\\\"", "slider"].concat();
        assert!(!source.contains(&in_source), "the film writes that role");
        // every aria value the film writes goes to that slider: one written
        // on the chart is a value nothing announces, which is what the
        // chart carried while its role was a graphics one
        let value = ["aria-", "value"].concat();
        let mut written = 0;
        for (at, _) in source.match_indices(&value) {
            let before = &source[at.saturating_sub(80)..at];
            assert!(
                before.contains("slider"),
                "a value off the slider: {before}"
            );
            written += 1;
        }
        assert_eq!(written, 1, "the film's slider must announce its value");
        // and what it announces is the clock, the frame and the chapter,
        // which the frame index it carries cannot say
        assert_eq!(value_text(&d, 0.0), "0 seconds, frame 1 of 38, flight");
        // the frame is the one showing at that instant, counted from one,
        // and the chapter is the one it is in
        assert_eq!(d.frame_at(1.55), 15, "the film's frames are 0.1 s apart");
        assert_eq!(
            value_text(&d, 1.55),
            "1.6 seconds, frame 16 of 38, abort <early>"
        );
    }

    /// The structure the emitter draws, named from the film's own data
    /// (decision 15): one object per series with its samples, its range in
    /// per cent and its span, and every chapter a button on the rect a
    /// pointer already hits.
    #[test]
    fn the_charts_series_and_chapters_are_named_from_the_films_own_data() {
        let d = flight();
        let svg = chart_svg(&d, op_chart::Layout::film(d.t_max));
        let named = |role: &str| -> Vec<String> {
            heads(&svg, "g")
                .iter()
                .filter(|head| attr(head, "role") == role)
                .map(|head| attr(head, "aria-label"))
                .collect()
        };
        let want: Vec<String> = d
            .series
            .iter()
            .map(|s| {
                let lo = s.y.iter().copied().fold(f64::INFINITY, f64::min);
                let hi = s.y.iter().copied().fold(f64::NEG_INFINITY, f64::max);
                // the range is spoken in the emitter's own rounding
                // ([`op_chart::announced`]), which is exported for exactly
                // this: a recomputation that restated it would pass while
                // the two drifted apart
                format!(
                    "{}, {} samples, {} to {} %, from {:.2} s to {:.2} s",
                    s.label,
                    s.t.len().min(s.y.len()),
                    op_chart::announced(lo),
                    op_chart::announced(hi),
                    s.t[0],
                    s.t[s.t.len() - 1]
                )
            })
            .collect();
        assert_eq!(
            named("graphics-object"),
            [want, vec!["Chapters".to_owned()]].concat()
        );
        // the unit is the film's own reading of its data: every series it
        // draws is a percentage, which is why the chart stays on that scale
        assert_eq!(chart_aria(&d).units, vec!["%".to_owned(); d.series.len()]);
        assert!(!chart_aria(&d).slider, "the film's chart owns no slider");
        // one button per chapter after the first, named and timed, on the
        // hit rect that was there for the pointer already
        assert_eq!(
            named("button"),
            d.chapters
                .iter()
                .skip(1)
                .map(|(t, label)| op_chart::escape(&format!("{label}, {t:.2} s")))
                .collect::<Vec<_>>()
        );
        assert_eq!(svg.matches("role=\"button\"").count(), d.chapters.len() - 1);
    }

    /// Decision 24's floor: no text in a chart is under 12 px. The film drew
    /// 11 px into a layout whose label widths the emitter estimates for a
    /// 12 px face, so the two agree now, as the film and the element do.
    /// At that size the labels still fit the film's fixed 900 by 268 box.
    #[test]
    fn the_chart_text_is_twelve_px_and_the_labels_still_fit_the_film_box() {
        /// The room a character takes at 12 px, the emitter's own estimate.
        const ADVANCE: f64 = 6.5;
        let sizes = |css: &str| -> Vec<f64> {
            css.split("font-size: ")
                .skip(1)
                .filter_map(|rest| rest.split("px").next()?.parse::<f64>().ok())
                .collect()
        };
        let ours = sizes(&chart_css());
        assert!(
            ours.contains(&12.0) && ours.iter().all(|px| *px >= 12.0),
            "{ours:?}"
        );
        // and the element's chart is drawn at that same size, so a reader
        // who meets both meets one size of text
        let theirs = sizes(include_str!("chart.rs"));
        assert!(
            theirs.contains(&12.0) && theirs.iter().all(|px| *px >= 12.0),
            "{theirs:?}"
        );
        for d in [demo(), flight()] {
            let l = op_chart::Layout::film(d.t_max);
            let svg = chart_svg(&d, l);
            // (baseline, left, right, the words) of every label drawn flat;
            // the value axis' own name runs down the margin turned on its
            // side, where a width along x means nothing
            let mut rows: Vec<(f64, f64, f64, String)> = Vec::new();
            for (head, body) in texts(&svg) {
                if !attr(head, "transform").is_empty() {
                    continue;
                }
                let x: f64 = attr(head, "x").parse().expect("a number");
                // the readout travels with the playhead, whose group
                // carries the offset it is written at
                let x = x + if attr(head, "class") == "head-t" {
                    l.x_of(0.0)
                } else {
                    0.0
                };
                let w = body.chars().count() as f64 * ADVANCE;
                let (left, right) = match attr(head, "text-anchor").as_str() {
                    "end" => (x - w, x),
                    "middle" => (x - w / 2.0, x + w / 2.0),
                    _ => (x, x + w),
                };
                assert!(
                    left >= 0.0 && right <= l.width,
                    "`{body}` runs from {left} to {right}, outside the {} px box",
                    l.width
                );
                rows.push((
                    attr(head, "y").parse().expect("a number"),
                    left,
                    right,
                    body.to_owned(),
                ));
            }
            assert!(rows.len() > 8, "only {} labels read", rows.len());
            rows.sort_by(|a, b| a.partial_cmp(b).expect("no label is at NaN"));
            for pair in rows.windows(2) {
                let ((y, _, right, first), (next_y, left, _, second)) = (&pair[0], &pair[1]);
                assert!(
                    (y - next_y).abs() > 0.5 || left >= right,
                    "`{first}` and `{second}` overlap on the row at {y}"
                );
            }
        }
    }

    /// The names the interaction report reads out of the film's shadow
    /// tree. It queries them as strings from another language, so a rename
    /// here is a broken report there and neither build would say so.
    #[test]
    fn the_names_the_interaction_report_reads_are_still_in_the_markup() {
        let d = demo();
        let svg = chart_svg(&d, op_chart::Layout::film(d.t_max));
        for class in [
            "chart",
            "playhead",
            "head",
            "head-dot",
            "head-t",
            "bar-bg",
            "bar-played",
            "peek-line",
        ] {
            assert!(
                svg.contains(&format!("class=\"{class}\"")),
                "the report reads .{class} and the chart draws it no longer"
            );
        }
        let bar = control_bar(d.times.len());
        for piece in [
            "class=\"play\"",
            "class=\"chapter-prev\"",
            "class=\"chapter-next\"",
            "class=\"t\"",
            "<input type=\"range\"",
        ] {
            assert!(bar.contains(piece), "the report reads {piece}: {bar}");
        }
        // and the pieces the film writes straight into its shadow markup,
        // where no helper returns them to a test
        let source = include_str!("film.rs");
        for class in ["fr", "chartbox", "peek", "sr"] {
            assert!(
                source.contains(&["class=", "\\\"", class, "\\\""].concat()),
                "the report reads .{class} and the film writes it no longer"
            );
        }
    }

    /// The animation frame's timestamps over `span` seconds of play, as a
    /// sixty hertz loop delivers them.
    fn ticks(span: f64) -> Vec<f64> {
        let frames = (span * 60.0) as usize;
        (0..frames).map(|k| k as f64 / 60.0).collect()
    }

    /// What a run of clock ticks put on the slider, and when.
    struct Heard {
        /// The times the frame index went on.
        numbers: Vec<f64>,
        /// The times the words for it did.
        words: Vec<f64>,
    }

    /// The times a run of clock ticks writes the slider's number and its
    /// words at, behind a write at `last`. The loop is
    /// [`Player::say_value`]'s own, stamp and all: only a write of the
    /// words moves the wait on, and a tick that says nothing leaves it
    /// where it was.
    fn written_at(last: f64, times: &[f64], focused: bool) -> Heard {
        let mut last = last;
        let mut heard = Heard {
            numbers: Vec::new(),
            words: Vec::new(),
        };
        for now in times {
            if index_due(Say::OnTheClock, last, *now, focused) {
                heard.numbers.push(*now);
            }
            if value_due(Say::OnTheClock, last, *now, focused) {
                heard.words.push(*now);
                last = *now;
            }
        }
        heard
    }

    /// Decision 18's clock rule is the chart's, reused rather than copied:
    /// for every clock and either focus the film's answer for a tick is
    /// exactly [`valuetext_due`]'s, and the film asks it rather than
    /// restating it, so the two cannot drift apart.
    #[test]
    fn the_films_clock_rule_is_the_charts_own_and_not_a_second_copy() {
        for before in [f64::NEG_INFINITY, 0.0, 1.0, 12.5] {
            for step in [0.0, 0.05, VALUETEXT_WAIT - 0.001, VALUETEXT_WAIT, 5.0] {
                let at = if before.is_finite() {
                    before + step
                } else {
                    step
                };
                for focus in [true, false] {
                    assert_eq!(
                        value_due(Say::OnTheClock, before, at, focus),
                        valuetext_due(before, at, focus),
                        "a tick at {at} behind {before}, focused {focus}"
                    );
                }
                // the number's rule is that same one once the control has
                // the focus, and no rule at all before that
                assert_eq!(
                    index_due(Say::OnTheClock, before, at, true),
                    valuetext_due(before, at, true),
                    "a number at {at} behind {before}"
                );
                assert!(index_due(Say::OnTheClock, before, at, false));
            }
        }
        // and both rules ask that one rather than restating it, the
        // words' and the number's. The needle is assembled, so this test
        // is never one of its own matches.
        let source = include_str!("film.rs");
        let asks = ["valuetext_due(", "last, now, focused)"].concat();
        for rule in [
            "fn value_due(say: Say, last: f64, now: f64, focused: bool) -> bool {",
            "fn index_due(say: Say, last: f64, now: f64, focused: bool) -> bool {",
        ] {
            let body = source
                .split_once(rule)
                .unwrap_or_else(|| panic!("{rule}"))
                .1
                .split_once("\n}")
                .expect("it ends")
                .0;
            assert!(body.contains(&asks), "{rule} answers for itself: {body}");
        }
        assert_eq!(
            source.matches(&asks).count(),
            2,
            "the chart's rule is asked twice and answered nowhere else"
        );
    }

    /// A clock tick onto a slider nothing has tabbed to says nothing at
    /// all: an unfocused value is not being spoken, so a playing film
    /// writes no words onto it (decision 18).
    #[test]
    fn a_clock_tick_with_the_slider_unfocused_writes_nothing() {
        // a minute of play at the frame rate, and not one write
        assert!(
            written_at(f64::NEG_INFINITY, &ticks(60.0), false)
                .words
                .is_empty()
        );
        // however long ago the last one was
        assert!(!value_due(Say::OnTheClock, 0.0, 1.0e6, false));
        // the same ticks onto a slider that does have the focus do speak,
        // so it is the focus that decides here and not the loop
        assert!(
            !written_at(f64::NEG_INFINITY, &ticks(60.0), true)
                .words
                .is_empty()
        );
    }

    /// A seek says the new time at once: neither the wait nor the focus
    /// holds a gesture up, because a reader who has just moved the clock
    /// is owed the answer to the thing they asked (decision 18).
    #[test]
    fn a_seek_writes_at_once() {
        // the write before was a millisecond ago and nothing has the
        // focus, so both of the clock's conditions fail
        assert!(!value_due(Say::OnTheClock, 1.0, 1.001, false));
        // and the gesture speaks anyway, focus or no focus
        assert!(value_due(Say::AtOnce, 1.0, 1.001, false));
        assert!(value_due(Say::AtOnce, 1.0, 1.001, true));
        // a seek, a play and a pause are the three that ask for it, the
        // seek through the moment that knows whether its gesture is over
        // ([`says_now`]), and the animation frame is the one caller that
        // waits. No native test can reach a shadow root, so the wiring is
        // read off the source, as the other guards in this file are.
        let source = include_str!("film.rs");
        let body_of = |signature: &str| {
            source
                .split_once(signature)
                .unwrap_or_else(|| panic!("{signature}"))
                .1
                .split_once("\n    }")
                .expect("it ends")
                .0
        };
        let at_once = ["say_value(Say::", "AtOnce)"].concat();
        for gesture in [
            "fn moment(&self, moment: Moment) {",
            "fn play(&self) {",
            "fn pause(&self) {",
        ] {
            let body = body_of(gesture);
            assert!(body.contains(&at_once), "{gesture} says nothing: {body}");
        }
        let seek = body_of("fn seek_to(&self, t: f64) {");
        assert!(
            seek.contains(&["self.moment(Moment::", "Seek)"].concat()),
            "the seek hands its moment to nothing: {seek}"
        );
        let on_the_clock = ["say_value(Say::", "OnTheClock)"].concat();
        assert_eq!(
            source.matches(&on_the_clock).count(),
            1,
            "one caller waits, and it is the animation frame"
        );
        // and the paint says nothing on its own account, whoever called it
        let render = source
            .split_once("fn render(dom: &Dom, d: &Data, st: &State) {")
            .expect("the render")
            .1
            .split_once("\n}")
            .expect("it ends")
            .0;
        let number = ["set_attribute(\"", "value\""].concat();
        for said in [
            at_once.as_str(),
            on_the_clock.as_str(),
            "value_text(",
            number.as_str(),
        ] {
            assert!(!render.contains(said), "the paint speaks: {render}");
        }
    }

    /// Two ticks inside the wait are one phrase. This is what stands
    /// between a reader who has tabbed to the slider and sixty
    /// interruptions a second (decision 18).
    #[test]
    fn two_ticks_inside_the_window_write_once() {
        // two animation frames, 16 ms apart, on a focused slider with
        // no write behind them
        let pair = written_at(f64::NEG_INFINITY, &[0.0, 1.0 / 60.0], true).words;
        assert_eq!(pair.len(), 1, "{pair:?}");
        // and with a seek's write just behind them, neither says a word
        assert!(
            written_at(0.0, &[1.0 / 60.0, 2.0 / 60.0], true)
                .words
                .is_empty()
        );
        // a second of play is sixty ticks, and the wait paces what is
        // heard: never two phrases inside one wait
        let said = written_at(f64::NEG_INFINITY, &ticks(1.0), true).words;
        assert!(
            said.len() <= 1 + (1.0 / VALUETEXT_WAIT) as usize,
            "{} phrases in a second of play: {said:?}",
            said.len()
        );
        assert!(said.len() >= 2, "the clock stopped speaking: {said:?}");
        for pair in said.windows(2) {
            assert!(pair[1] - pair[0] >= VALUETEXT_WAIT, "{pair:?}");
        }
    }

    /// The number waits only for a listener. It is what the thumb is
    /// drawn from, so an unfocused slider takes it on every tick and
    /// keeps up with the film; a focused one is spoken, so there it waits
    /// with the words (decision 18).
    #[test]
    fn the_frame_index_follows_every_tick_until_the_slider_is_focused() {
        let second = ticks(1.0);
        // unfocused: sixty ticks, sixty numbers, and no words at all
        let heard = written_at(f64::NEG_INFINITY, &second, false);
        assert_eq!(heard.numbers.len(), second.len());
        assert!(heard.words.is_empty());
        // focused: the number keeps the words' pace exactly, so a reader
        // on the control is not read a value sixty times a second
        let heard = written_at(f64::NEG_INFINITY, &second, true);
        assert_eq!(heard.numbers, heard.words);
        assert!(
            heard.numbers.len() <= 1 + (1.0 / VALUETEXT_WAIT) as usize,
            "{} numbers in a second of play",
            heard.numbers.len()
        );
        // a gesture writes it at once, focus or no focus
        assert!(index_due(Say::AtOnce, 1.0, 1.001, true));
        assert!(index_due(Say::AtOnce, 1.0, 1.001, false));
        // and whenever it is written it is the frame the clock is on, so
        // a debounced write is late and never stale
        let source = include_str!("film.rs");
        let say = source
            .split_once("fn say_value(&self, say: Say) {")
            .expect("the write")
            .1
            .split_once("\n    }")
            .expect("it ends")
            .0;
        assert!(
            say.contains("self.data.frame_at(self.state.borrow().tc)"),
            "{say}"
        );
    }

    /// A scrub says one sentence, and says it when it settles. A press on
    /// the chart seeks on every move under it, and a sentence per move is
    /// the flood the debounce keeps off the value, on the region instead
    /// (decision 18).
    #[test]
    fn a_scrub_says_one_sentence_and_says_it_when_it_settles() {
        // where a run of moments speaks, run through the element's own
        // rule and its own flag
        let says = |moments: &[Moment]| -> Vec<usize> {
            let mut scrubbing = false;
            let mut spoke = Vec::new();
            for (i, moment) in moments.iter().enumerate() {
                let (in_flight, says) = says_now(*moment, scrubbing);
                scrubbing = in_flight;
                if says {
                    spoke.push(i);
                }
            }
            spoke
        };
        // a press, its own seek, two moves and the release: one sentence,
        // and it is the release that says it
        assert_eq!(
            says(&[
                Moment::Press,
                Moment::Seek,
                Moment::Seek,
                Moment::Seek,
                Moment::Settle,
            ]),
            vec![4]
        );
        // a seek that is no part of a gesture speaks at once, every time
        assert_eq!(says(&[Moment::Seek, Moment::Seek]), vec![0, 1]);
        // a release with no press behind it says nothing
        assert_eq!(says(&[Moment::Settle]), Vec::<usize>::new());
        // and the gesture is over when it settles: the next key seek
        // speaks again
        assert_eq!(
            says(&[Moment::Press, Moment::Seek, Moment::Settle, Moment::Seek]),
            vec![2, 3]
        );
        // the wiring: the chart's own listeners hand it the press and the
        // settle, and the seek asks from the one place a seek happens
        let source = include_str!("film.rs");
        let chart = source
            .split_once("if let Some(chart) = dom.chart.clone() {")
            .expect("the chart's listeners")
            .1
            .split_once("\n        }")
            .expect("they end")
            .0;
        for (event, moment) in [("pointerdown", "Press"), ("pointerup", "Settle")] {
            assert!(chart.contains(event), "no {event} listener: {chart}");
            assert!(
                chart.contains(&["moment(Moment::", moment, ")"].concat()),
                "{event} asks for nothing: {chart}"
            );
        }
        assert!(chart.contains("pointercancel"), "{chart}");
        let seek = source
            .split_once("fn seek_to(&self, t: f64) {")
            .expect("the seek")
            .1
            .split_once("\n    }")
            .expect("it ends")
            .0;
        assert!(
            seek.contains(&["self.moment(Moment::", "Seek)"].concat()),
            "{seek}"
        );
    }

    /// The region says what happened and the slider says what it reads
    /// (decision 18). The sentence is the chart's own [`message`], so a
    /// page whose chart and film both speak says one thing one way, and
    /// the film keeps no second wording of its own.
    #[test]
    fn the_region_says_what_happened_in_the_charts_own_words() {
        // a sentence naming the event and the time; the frame index is
        // the slider's own value and is not said twice
        let said = message(Said::Seeked, 1.55, "abort <early>");
        assert_eq!(said, "Seeked to 1.6 seconds, chapter abort <early>");
        assert!(!said.contains("frame"), "{said}");
        assert_eq!(message(Said::Playing, 1.55, "flight"), "Playing");
        assert_eq!(message(Said::Paused, 1.55, "flight"), "Paused");
        let source = include_str!("film.rs");
        let body_of = |signature: &str| {
            source
                .split_once(signature)
                .unwrap_or_else(|| panic!("{signature}"))
                .1
                .split_once("\n    }")
                .expect("it ends")
                .0
        };
        // the seek says its sentence through the region's own rule
        let seek = body_of("fn seek_to(&self, t: f64) {");
        assert!(
            seek.contains(&["self.moment(Moment::", "Seek)"].concat()),
            "{seek}"
        );
        // play and pause say what changed, and nothing where nothing did:
        // a frame step pauses a film already paused, and that is no event
        assert_eq!(toggled(false, true), Some(Said::Playing));
        assert_eq!(toggled(true, false), Some(Said::Paused));
        assert_eq!(toggled(true, true), None);
        assert_eq!(toggled(false, false), None);
        for (toggle, call) in [
            ("fn play(&self) {", "toggled(was_playing, true)"),
            ("fn pause(&self) {", "toggled(was_playing, false)"),
        ] {
            let body = body_of(toggle);
            assert!(body.contains(call), "{toggle} asks nothing: {body}");
            assert!(body.contains("self.announce(said)"), "{toggle}: {body}");
        }
        // the region has one voice, and it is the chart's
        let announce = body_of("fn announce(&self, said: Said) {");
        assert!(
            announce.contains("message(said, t, &chapter)"),
            "{announce}"
        );
        // and the seek writes no wording of its own beside it
        let seek = body_of("fn seek_to(&self, t: f64) {");
        assert!(
            !seek.contains("live"),
            "a second wording in the seek: {seek}"
        );
    }

    /// A scrub writes the control's words once, when the gesture
    /// settles, and its number all the way along the drag. A press on the
    /// chart seeks on every move under it, so a phrase per move is the
    /// flood the debounce keeps off a playing film arriving by the other
    /// door (decision 18); the number is not held back with them, because
    /// the slider's thumb is drawn from it and has to stay under the
    /// pointer that is dragging it.
    #[test]
    fn a_scrub_writes_the_controls_words_once_and_its_number_all_along() {
        // a drag through the element's own rules and its own flag: which
        // moments put the number on the control, and which put the words
        // on it. The moves are a frame apart, which is as often as a
        // scrub seeks.
        let heard = |moments: &[Moment], focused: bool| -> Heard {
            let mut scrubbing = false;
            let mut last = f64::NEG_INFINITY;
            let mut heard = Heard {
                numbers: Vec::new(),
                words: Vec::new(),
            };
            for (i, moment) in moments.iter().enumerate() {
                let (in_flight, says) = says_now(*moment, scrubbing);
                scrubbing = in_flight;
                let say = if says { Say::AtOnce } else { Say::InFlight };
                let now = i as f64 / 60.0;
                if index_due(say, last, now, focused) {
                    heard.numbers.push(now);
                }
                if value_due(say, last, now, focused) {
                    heard.words.push(now);
                    last = now;
                }
            }
            heard
        };
        // the press, its own seek, two moves and the release
        let drag = [
            Moment::Press,
            Moment::Seek,
            Moment::Seek,
            Moment::Seek,
            Moment::Settle,
        ];
        for focused in [false, true] {
            let heard = heard(&drag, focused);
            assert_eq!(
                heard.words,
                vec![4.0 / 60.0],
                "focused {focused}: {:?}",
                heard.words
            );
            assert_eq!(
                heard.numbers.len(),
                drag.len(),
                "the thumb stopped under the pointer, focused {focused}: {:?}",
                heard.numbers
            );
        }
        // a seek that is no part of a gesture says its words at once,
        // every time: the drag is what waits, not the seeking
        let heard = heard(&[Moment::Seek, Moment::Seek, Moment::Seek], false);
        assert_eq!(heard.words.len(), 3, "{:?}", heard.words);
        // a gesture in flight says nothing, however long it has been
        // going and whoever is listening
        for focused in [false, true] {
            assert!(!value_due(Say::InFlight, f64::NEG_INFINITY, 1.0e6, focused));
            // while the number it leaves out goes on every time
            assert!(index_due(Say::InFlight, 1.0, 1.001, focused));
        }
        // the wiring: one moment answers for both voices, and the seek
        // hands it every seek there is
        let source = include_str!("film.rs");
        let body_of = |signature: &str| {
            source
                .split_once(signature)
                .unwrap_or_else(|| panic!("{signature}"))
                .1
                .split_once("\n    }")
                .expect("it ends")
                .0
        };
        let moment = body_of("fn moment(&self, moment: Moment) {");
        assert!(
            moment.contains("says_now(moment, self.scrub.get())"),
            "{moment}"
        );
        assert!(
            moment.contains("say_value("),
            "the control is left out of it: {moment}"
        );
        assert!(moment.contains("announce(Said::Seeked)"), "{moment}");
        // and the seek says nothing on its own account beside it, which
        // is what wrote the control once a pointer move
        let seek = body_of("fn seek_to(&self, t: f64) {");
        assert!(
            seek.contains(&["self.moment(Moment::", "Seek)"].concat()),
            "{seek}"
        );
        assert!(
            !seek.contains("say_value("),
            "a second voice in the seek: {seek}"
        );
    }

    /// The film's own slider is a native range input, and a thumb dragged
    /// across it fires `input` for every pixel it crosses and one
    /// `change` when the reader lets go of it. That is the gesture a
    /// scrub across the chart is, and it takes the same rule: the drag
    /// says nothing and the end of it says the one thing the gesture had
    /// to say (decision 18). The input used to seek on its own account,
    /// each one a settled seek, so a thumb dragged the width of the bar
    /// said a sentence a pixel. A key on the same control fires both
    /// events in the one press, so a key still speaks at once.
    #[test]
    fn a_drag_of_the_slider_speaks_once_and_a_key_on_it_speaks_at_once() {
        let moment_of = |kind: &str| -> Moment {
            SLIDER_MOMENTS
                .iter()
                .find(|(name, _)| *name == kind)
                .unwrap_or_else(|| panic!("the slider answers no {kind}"))
                .1
        };
        // where a run of the slider's own events speaks, through the
        // element's own rule and its own flag
        let says = |events: &[&str]| -> Vec<usize> {
            let mut scrubbing = false;
            let mut spoke = Vec::new();
            for (i, kind) in events.iter().enumerate() {
                let opening = moment_of(kind);
                let mut moments = vec![opening];
                // an input moves the clock as well, and the seek it makes
                // asks this same rule ([`Player::seek_to`])
                if opening == Moment::Press {
                    moments.push(Moment::Seek);
                }
                for moment in moments {
                    let (in_flight, said) = says_now(moment, scrubbing);
                    scrubbing = in_flight;
                    if said {
                        spoke.push(i);
                    }
                }
            }
            spoke
        };
        // a thumb dragged across twelve pixels and let go: one sentence,
        // and it is the letting go that says it
        let mut drag = vec!["input"; 12];
        drag.push("change");
        assert_eq!(says(&drag), vec![12], "a sentence a pixel");
        // a key press on the slider is one input and its own change, both
        // in the one press: the film speaks, and waits for nothing
        assert_eq!(says(&["input", "change"]), vec![1]);
        // and the gesture is over when it ends: the next one speaks for
        // itself rather than being swallowed by the last
        assert_eq!(says(&["input", "change", "input", "change"]), vec![1, 3]);
        // the wiring: both of the slider's events are listened for, from
        // that one table, and the drag opens the gesture rather than
        // seeking on its own account
        let source = include_str!("film.rs");
        let slider = source
            .split_once("for (name, moment) in SLIDER_MOMENTS {")
            .expect("the slider's listeners")
            .1
            .split_once("\n        }")
            .expect("they end")
            .0;
        assert!(
            slider.contains("&dom.slider,"),
            "not the film's slider: {slider}"
        );
        assert!(
            slider.contains(&["p.moment(Moment::", "Press)"].concat()),
            "the drag opens no gesture: {slider}"
        );
        // and it stops a film that is running once, on its first step: a
        // pause speaks at once, so pausing on every pixel of a drag is
        // the same flood arriving by another door
        assert!(
            slider.contains("if p.state.borrow().playing {"),
            "the drag pauses on every pixel of itself: {slider}"
        );
    }

    /// A drag begun somewhere else and dragged over the chart is not the
    /// film's gesture to answer. It arrives with the primary button down
    /// and no press of this chart's behind it, so nothing opened a scrub
    /// and nothing is coming to settle one, and the film seeks for none
    /// of it: a seek outside a gesture is a settled seek and says so,
    /// which was a sentence for every move somebody else's drag made
    /// across the chart ([`says_now`]). A hover is still a hover, and the
    /// film's own scrub still drags the clock.
    #[test]
    fn a_drag_the_chart_never_saw_open_neither_seeks_nor_peeks() {
        // the film's own scrub: the press set the flag, and the moves
        // under it carry the clock with them
        assert_eq!(moved(1, true), Moved::Scrub);
        // the same button with no press of this chart's behind it
        assert_eq!(moved(1, false), Moved::Stray);
        // nothing held down is a hover, whatever the flag says
        for scrubbing in [false, true] {
            assert_eq!(moved(0, scrubbing), Moved::Hover, "scrubbing {scrubbing}");
        }
        // the primary button is the one that scrubs: the others carry a
        // menu or a page's own gesture, and drag nothing here
        for buttons in [2, 4, 16] {
            assert_eq!(moved(buttons, true), Moved::Hover, "buttons {buttons}");
            // held with the primary one, the primary one still scrubs
            assert_eq!(moved(buttons | 1, true), Moved::Scrub, "buttons {buttons}");
        }
        // what a stray move is kept away from: the seek it would have
        // made speaks on its own account, once for every move
        assert_eq!(says_now(Moment::Seek, false), (false, true));
        // the wiring: the chart's move asks that rule and keeps no second
        // copy of the button test. The needle is assembled, so this test
        // is never one of its own matches.
        let source = include_str!("film.rs");
        let chart = source
            .split_once("if let Some(chart) = dom.chart.clone() {")
            .expect("the chart's listeners")
            .1
            .split_once("\n        }")
            .expect("they end")
            .0;
        assert!(
            chart.contains("moved(m.buttons(), p.scrub.get())"),
            "the move answers for itself: {chart}"
        );
        let button_test = ["m.buttons()", " & 1"].concat();
        assert!(
            !chart.contains(&button_test),
            "a second copy of the rule: {chart}"
        );
    }

    /// The control and the region name one instant one way. The region
    /// says the time in the chart's own words and the control gave the
    /// same position two decimals of its own, so a reader who heard both
    /// heard "1.6 seconds" and "1.55 seconds" for one place on the track.
    #[test]
    fn the_control_and_the_region_say_one_time_for_one_instant() {
        let d = flight();
        let said = in_words(1.55);
        assert_eq!(said, "1.6 seconds");
        assert_eq!(
            value_text(&d, 1.55),
            "1.6 seconds, frame 16 of 38, abort <early>"
        );
        assert_eq!(
            message(Said::Seeked, 1.55, &d.chapter_at(1.55).1),
            "Seeked to 1.6 seconds, chapter abort <early>"
        );
        // and everywhere else on the track, not only there: whatever the
        // instant, the words on the control open with the time the
        // sentence in the region names
        for k in 0..=200 {
            let t = d.end() * f64::from(k) / 200.0;
            let said = in_words(t);
            let words = value_text(&d, t);
            let sentence = message(Said::Seeked, t, &d.chapter_at(t).1);
            assert!(words.starts_with(&said), "the control says {words} at {t}");
            assert!(
                sentence.contains(&said),
                "the region says {sentence} at {t}"
            );
        }
    }

    /// What the control says is written when the focus arrives, so a
    /// reader who tabs to it in the middle of a film hears where the film
    /// is and not where it was. The clock's own writes stop while nothing
    /// has the focus (decision 18), which is what leaves the words stale;
    /// the element keeps its thumb current from the other end, writing
    /// the long form as the focus leaves.
    #[test]
    fn the_control_says_the_clock_it_stands_on_when_the_focus_arrives() {
        // a minute of play with the focus elsewhere writes no words at
        // all, so nothing but the arrival can make them current
        assert!(
            written_at(f64::NEG_INFINITY, &ticks(60.0), false)
                .words
                .is_empty()
        );
        // and the arrival does not wait behind the debounce: it is a
        // gesture like the rest
        assert!(value_due(Say::AtOnce, 0.0, 0.0, true));
        // the wiring: the control's own listeners, one of them the focus
        // arriving and saying at once. The needle is assembled, so this
        // test is never one of its own matches.
        let source = include_str!("film.rs");
        let at_once = ["say_value(Say::", "AtOnce)"].concat();
        let control = ["&dom.", "slider,"].concat();
        let blocks: Vec<&str> = source
            .split(&control)
            .skip(1)
            .filter_map(|rest| rest.split_once("\n        }").map(|(block, _)| block))
            .collect();
        assert!(
            blocks
                .iter()
                .any(|block| block.contains("\"focus\"") && block.contains(&at_once)),
            "nothing writes the control's words when the focus arrives: {blocks:?}"
        );
    }

    /// The region has one voice, and every sentence in it opens with a
    /// capital. Two are the film's own, a cancelled seek and a speed, the
    /// chart's [`message`] having no word for either; they went straight
    /// onto the node in lower case while every sentence beside them went
    /// through the helper and opened with a capital.
    #[test]
    fn every_sentence_in_the_region_opens_with_a_capital_in_the_one_voice() {
        for said in [Said::Seeked, Said::Playing, Said::Paused, Said::Chapter] {
            let text = message(said, 1.55, "flight");
            assert!(text.starts_with(char::is_uppercase), "{text}");
        }
        assert!(CANCELLED.starts_with(char::is_uppercase), "{CANCELLED}");
        // one place writes the region, and everything that speaks goes
        // through it. The needles are assembled, so this test is never
        // one of its own matches.
        let source = include_str!("film.rs");
        let node = ["live.set_text_", "content("].concat();
        assert_eq!(
            source.matches(&node).count(),
            1,
            "a second voice writes the region"
        );
        let voice = [".", "say("].concat();
        let mut own: Vec<&str> = Vec::new();
        for call in source.split(&voice).skip(1) {
            let statement = call.split(");").next().expect("the call ends");
            // a sentence the chart words, or one behind a name, carries no
            // literal here and is held to its capital where it is written
            let Some((_, rest)) = statement.split_once('"') else {
                continue;
            };
            own.push(rest.split('"').next().expect("a literal"));
        }
        for text in &own {
            assert!(
                text.starts_with(char::is_uppercase),
                "the region says `{text}`"
            );
        }
        assert!(
            own.iter().any(|text| text.starts_with("Speed ")),
            "the speed is said in some other voice: {own:?}"
        );
        // and the cancel goes through the voice rather than around it
        let cancel = source
            .split_once("fn cancel(&self) {")
            .expect("the cancel")
            .1
            .split_once("\n    }")
            .expect("it ends")
            .0;
        assert!(
            cancel.contains(&[voice.as_str(), "CANCELLED)"].concat()),
            "{cancel}"
        );
    }

    /// The project writes no em dashes, and the film has two voices that
    /// could carry one: the stage's label and the region.
    #[test]
    fn the_film_writes_no_em_dash_anywhere() {
        let source = include_str!("film.rs");
        for dash in ['\u{2014}', '\u{2013}'] {
            assert!(
                !source.contains(dash),
                "{dash:?} at byte {:?}",
                source.find(dash)
            );
        }
    }

    /// Decision 20 inside the film: a cue a reader can focus takes the
    /// element's own in-SVG indicator and not the site's ring. The
    /// emitter draws the same cue buttons here as it does for
    /// `<opt-chart>`, [`BASE_CSS`] is inlined above them, and its
    /// `:focus-visible` would put an outline around a `<g>` in user
    /// space: a hairline that scales with the viewBox and is clipped at
    /// either end of the track. The rules are read out of the element's
    /// own stylesheet, so the day it paints a cue differently this fails
    /// rather than letting the two looks drift apart.
    #[test]
    fn a_focused_cue_in_the_films_chart_takes_the_elements_own_indicator() {
        // the buttons the rules address are really drawn here, and the
        // site's ring really does reach them
        let d = flight();
        let svg = chart_svg(&d, op_chart::Layout::film(d.t_max));
        assert_eq!(
            svg.matches("<g role=\"button\"").count(),
            d.chapters.len() - 1,
            "{svg}"
        );
        assert!(
            BASE_CSS.contains(":focus-visible { outline: 2px solid"),
            "{BASE_CSS}"
        );
        // the element's own cue rules, included whole
        let css = chart_css();
        let theirs = cue_indicator_css();
        assert!(css.contains(&theirs), "the film is missing `{theirs}`");
        // the opt-out is one of them, and it is what keeps the ring off
        assert!(
            theirs.contains(&format!("{CUE_RULE}:focus {{ outline: none; }}")),
            "{theirs}"
        );
        // and no rule of the film's own puts an outline on a node of the
        // drawing: every outline written here turns one off
        assert_eq!(
            css.matches("outline").count(),
            css.matches("outline: none;").count(),
            "an outline on a node of the drawing: {css}"
        );
    }

    /// A press on a cue button in the film's chart seeks to that cue, and
    /// never reaches the film's own key table, where Space plays the film.
    ///
    /// The emitter draws the same buttons here as it draws for
    /// `<opt-chart>` and the test above asserts they are drawn, so the
    /// defect the element was fixed for stood in this file too: a reader
    /// whose screen reader put the focus on a chapter and pressed Space
    /// played the film rather than going to the chapter, and Enter did
    /// nothing at all. One role, one behaviour, in both elements.
    #[test]
    fn a_press_on_a_focused_cue_in_the_film_seeks_to_it_rather_than_playing() {
        // the press is the element's own answer to the same role, so a
        // reader who learnt it on a chart keeps it in a film
        assert_eq!(cue_press(Some("1.500"), " "), Some(1.5));
        assert_eq!(cue_press(Some("1.500"), "Enter"), Some(1.5));
        // and the press is the only thing taken from a focused cue: every
        // other key is still the film's own, the arrows and Escape among
        // them, there being no roving here for them to move
        for key in [
            "k",
            "K",
            "j",
            "J",
            "l",
            "L",
            ",",
            ".",
            "0",
            "9",
            "Home",
            "End",
            "PageUp",
            "PageDown",
            "ArrowRight",
            "ArrowLeft",
            "ArrowUp",
            "ArrowDown",
            "Escape",
            "<",
            ">",
            "Tab",
        ] {
            assert_eq!(cue_press(Some("1.500"), key), None, "{key} on a cue");
        }
        // with the focus anywhere else the film keeps every key it has,
        // Space at the head of them
        for key in ["Enter", " ", "k", "Escape", "PageDown"] {
            assert_eq!(cue_press(None, key), None, "{key} off a cue");
        }
        // a cue whose rect says nothing still lands somewhere, as the
        // element's press does, rather than falling through to the arm
        // that plays: the seek clamps it to the end of the film
        assert_eq!(cue_press(Some(""), "Enter"), Some(f64::INFINITY));

        // No native test can reach a shadow root, so the wiring is read
        // off the source, as the other guards in this file are. What holds
        // the focus is asked about before the key table, and the answer
        // seeks and marks the key handled, so the arm that plays the film
        // never sees a press on a cue.
        let source = include_str!("film.rs");
        let keys = source
            .split_once("let handled = if let")
            .expect("the film's key handler")
            .1
            .split_once("if handled {")
            .expect("it ends")
            .0;
        let press = keys
            .find("cue_press(p.focused_cue()")
            .unwrap_or_else(|| panic!("the key table never asks what holds the focus: {keys}"));
        let plays = keys
            .find(&["\" \" | \"k\"", " | \"K\" =>"].concat())
            .unwrap_or_else(|| panic!("the film's play arm is spelt some other way: {keys}"));
        assert!(
            press < plays,
            "the arm that plays the film sees Space first: {keys}"
        );
        let arm = keys
            .split_once("cue_press(")
            .expect("the press")
            .1
            .split_once("} else if")
            .expect("the table after it")
            .0;
        // it pauses, it seeks to the cue's own time, and the `true` at the
        // end of it is the claim that keeps the key off the table
        for call in ["p.pause()", "p.seek_to(t)", "true"] {
            assert!(
                arm.contains(call),
                "the answer to a press is missing `{call}`: {arm}"
            );
        }
        // and what it asks about is what the tree says has the focus, not
        // something the film remembered, read off the same markup the
        // element reads
        let focused = source
            .split_once("fn focused_cue(&self) -> Option<String> {")
            .expect("the focus read")
            .1
            .split_once("\n    }")
            .expect("it ends")
            .0;
        for read in [
            "shadow_root()",
            "active_element()",
            "CUE_BUTTONS",
            "[data-t]",
        ] {
            assert!(
                focused.contains(read),
                "the focus read is missing {read}: {focused}"
            );
        }
    }
}
