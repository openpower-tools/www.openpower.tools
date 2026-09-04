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
//! speed, Escape.
//!
//! Internal state is exposed as custom states - `playing`, `pending`,
//! `peeking` - so a page can style or test against them; every render
//! dispatches a composed `opt-film-time` event whose detail is
//! `{ time, duration, playing }` (an `opt-machine for="..."` follows it,
//! reading `time`); the chart is a
//! `role=slider` with value text naming the frame and chapter, and a
//! polite live region announces seeks.
//!
//! The traffic runs the other way as well: an `opt-chart for="<film id>"`
//! sends `opt-chart-seek`, `opt-chart-peek` and `opt-chart-toggle`, and
//! the film applies them to its own clock. Those events are composed and
//! bubbling, so one listener on the document hears every chart on the
//! page and `intent_for` decides which of them address this film.

use std::cell::RefCell;
use std::rc::Rc;

use op_webc::{CustomElement, ElementDefinition, set_state};
use wasm_bindgen::prelude::*;
use web_sys::{Element, Event, HtmlElement, KeyboardEvent, MouseEvent, PointerEvent};

use super::chart::{PEEK_EVENT, SEEK_EVENT, TIME_FIELD, TOGGLE_EVENT};
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

fn fmt(t: f64) -> String {
    format!("{t:.2}s")
}

fn show_stage(dom: &Dom, d: &Data, k: usize, tag: &str) {
    let _ = dom.stage.style().set_property(
        "background-position",
        &format!("{}px 0", -(k as f64) * dom.stage_w),
    );
    dom.stage_label.set_text_content(Some(&format!(
        "{} · frame {} of {}{}{}",
        fmt(d.times[k]),
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
    let _ = dom.slider.set_attribute("value", &k.to_string());
    let _ = js_sys::Reflect::set(
        &dom.slider,
        &JsValue::from_str("value"),
        &JsValue::from_f64(k as f64),
    );
    dom.time_label.set_text_content(Some(&fmt(st.tc)));
    // the clock has moved, so a chapter step may have run out of chapters
    for (dir, button) in &dom.chapters {
        let _ = button
            .toggle_attribute_with_force("disabled", chapter_target(d, st.tc, *dir).is_none());
    }
    sync_video(dom, st);
    if let Some(chart) = &dom.chart {
        let x = dom.layout.x_of(st.tc);
        // one transform carries the line, the dot and the readout together
        if let Some(playhead) = dom.q(".playhead") {
            let _ = playhead.set_attribute("transform", &format!("translate({x:.1} 0)"));
        }
        if let Some(label) = dom.q(".head-t") {
            label.set_text_content(Some(&fmt(st.tc)));
        }
        if let Some(played) = dom.q(".bar-played") {
            let _ =
                played.set_attribute("width", &format!("{:.1}", (x - dom.layout.left).max(0.0)));
        }
        let _ = chart.set_attribute("aria-valuenow", &format!("{:.2}", st.tc));
        let _ = chart.set_attribute(
            "aria-valuetext",
            &format!(
                "{:.2} seconds, frame {} of {}, {}",
                st.tc,
                k + 1,
                n,
                d.chapter_at(st.tc).1
            ),
        );
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
    ptime.set_text_content(Some(&format!("{} · {}", fmt(d.times[k]), c.1)));
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
}

impl Player {
    fn seek_to(&self, t: f64) {
        {
            let mut st = self.state.borrow_mut();
            st.tc = t.clamp(0.0, self.data.end());
        }
        self.render();
        let st = self.state.borrow();
        let k = self.data.frame_at(st.tc);
        self.dom.live.set_text_content(Some(&format!(
            "{}, frame {} of {}",
            fmt(st.tc),
            k + 1,
            self.data.times.len()
        )));
    }

    fn render(&self) {
        render(&self.dom, &self.data, &self.state.borrow());
    }

    fn pause(&self) {
        let mut st = self.state.borrow_mut();
        st.playing = false;
        st.last = None;
        if let (Some(id), Some(window)) = (st.raf.take(), web_sys::window()) {
            let _ = window.cancel_animation_frame(id);
        }
        self.dom.play.set_text_content(Some("Play"));
        set_state(&self.dom.host, "playing", false);
        drop(st);
        sync_video(&self.dom, &self.state.borrow());
    }

    fn play(&self) {
        {
            let mut st = self.state.borrow_mut();
            if st.tc >= self.data.end() {
                st.tc = 0.0;
            }
            st.playing = true;
            st.last = None;
        }
        self.dom.play.set_text_content(Some("Pause"));
        set_state(&self.dom.host, "playing", true);
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
            "pending — release to seek, Esc to cancel",
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
            self.dom.live.set_text_content(Some("seek cancelled"));
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
pub(crate) fn chart_css() -> String {
    let rules = chart_rules();
    format!(
        ".chart {{ max-width: 100%; height: auto; cursor: ew-resize; display: block; touch-action: none; font-family: var(--op-font-sans); font-size: 11px; }}
{CHART_SHAPE_CSS}
.chart .band, .chart .peek-band {{ fill: var(--op-accent); fill-opacity: 0.08; }}
.chart .bar-bg {{ fill: var(--op-border); }} .chart .bar-played {{ fill: var(--op-accent); }}
{CHART_CUE_CSS}
.chart .head {{ stroke: var(--op-accent); stroke-width: 1.5; }} .chart .head-dot {{ fill: var(--op-accent); }}
.chart .head-t {{ fill: var(--op-accent); font-weight: 700; paint-order: stroke; stroke: var(--op-surface); stroke-width: 4; }}
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
                    fmt(*t)
                } else if delta > 0.001 {
                    format!("{} · {:.0}%", fmt(*t), delta * 100.0)
                } else {
                    format!("{} · same", fmt(*t))
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
        let chart = if data.series.is_empty() {
            String::new()
        } else {
            op_chart::render(&spec_of(&data), layout).svg
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
        {
            let p = player.clone();
            listen(
                &dom.slider,
                "input",
                Closure::new(move |e: Event| {
                    if let Some(target) = e.target()
                        && let Ok(v) = js_sys::Reflect::get(&target, &JsValue::from_str("value"))
                        && let Some(k) = v.as_string().and_then(|s| s.parse::<usize>().ok())
                    {
                        p.pause();
                        p.seek_to(p.data.times[k.min(p.data.times.len() - 1)]);
                    }
                }),
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
                    if m.buttons() & 1 == 1 {
                        let t = p.t_at_pointer(&m);
                        p.seek_to(t);
                        hide_peek(&p.dom);
                    } else if let Some(chart) = &p.dom.chart {
                        let r = chart.get_bounding_client_rect();
                        let t = p.t_at_pointer(&m);
                        show_peek(&p.dom, &p.data, t, Some(f64::from(m.client_x()) - r.left()));
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
                    let t = p.t_at_pointer(&pe);
                    p.seek_to(t);
                }),
            );
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
                    let handled = if let Some(dir) = chapter_key(&key, chapter_alias) {
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
                                p.dom.live.set_text_content(Some(&format!("speed {r}x")));
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
    use super::super::chart_style::{FORCED_COLOURS_CSS, PRINT_CSS, SERIES_CSS};
    use super::*;

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
}
