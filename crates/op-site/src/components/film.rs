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
//! frames, J/L ten, 0-9 tenths, Home/End, `<` `>` speed, Escape.
//!
//! Internal state is exposed as custom states - `playing`, `pending`,
//! `peeking` - so a page can style or test against them; every render
//! dispatches a composed `opt-film-time` event carrying the clock (an
//! `opt-machine for="..."` follows it); the chart is a
//! `role=slider` with value text naming the frame and chapter, and a
//! polite live region announces seeks.

use std::cell::RefCell;
use std::rc::Rc;

use op_webc::{CustomElement, ElementDefinition, set_state};
use wasm_bindgen::prelude::*;
use web_sys::{Element, Event, HtmlElement, KeyboardEvent, MouseEvent, PointerEvent};

use super::{BASE_CSS, shadow_root};
use crate::html::escape;

pub const DEFINITION: ElementDefinition = ElementDefinition {
    source: op_webc::here!(),
    tag: "opt-film",
    observed_attributes: &[],
    create: |host| Box::new(Film { host, wiring: None }),
};

/// One sampled series drawn on the chart.
struct Series {
    label: String,
    color: String,
    t: Vec<f64>,
    y: Vec<f64>,
    dash: bool,
    lw: f64,
    at: f64,
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
            .map(|s| Series {
                label: text(&s, "label"),
                color: text(&s, "color"),
                t: nums(&s, "t"),
                y: nums(&s, "y"),
                dash: get(&s, "dash").as_bool().unwrap_or(false),
                lw: num(&s, "lw", 1.8),
                at: num(&s, "at", 0.85),
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
}

// ---- the chart, an SVG with a playhead and a chapter bar --------------
const CW: f64 = 900.0;
const CH: f64 = 268.0;
const ML: f64 = 46.0;
const MR: f64 = 14.0;
const MT: f64 = 16.0;
const MB: f64 = 48.0;
const YMIN: f64 = -4.0;
const YMAX: f64 = 106.0;

fn x_of(d: &Data, t: f64) -> f64 {
    ML + (t / d.t_max).clamp(0.0, 1.0) * (CW - ML - MR)
}

fn y_of(v: f64) -> f64 {
    MT + (YMAX - v.clamp(YMIN, YMAX)) / (YMAX - YMIN) * (CH - MT - MB)
}

fn chart_svg(d: &Data) -> String {
    let mut out = format!(
        "<svg class=\"chart\" part=\"chart\" viewBox=\"0 0 {CW} {CH}\" tabindex=\"0\" role=\"slider\" aria-label=\"playhead\" aria-valuemin=\"0\" aria-valuemax=\"{:.2}\" aria-valuenow=\"0\" aria-valuetext=\"0.00 seconds\">",
        d.end()
    );
    for v in [0.0, 25.0, 50.0, 75.0, 100.0] {
        let y = y_of(v);
        let w = if v == 0.0 || v == 100.0 { 1.0 } else { 0.5 };
        out.push_str(&format!("<line class=\"grid\" x1=\"{ML}\" x2=\"{:.1}\" y1=\"{y:.1}\" y2=\"{y:.1}\" stroke-width=\"{w}\"/>", CW - MR));
        out.push_str(&format!(
            "<text class=\"axis\" x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"end\">{v:.0}</text>",
            ML - 6.0,
            y + 4.0
        ));
    }
    let step = if d.t_max <= 5.0 { 0.5 } else { 1.0 };
    let mut t = 0.0;
    while t <= d.t_max + 1e-9 {
        let x = x_of(d, t);
        out.push_str(&format!(
            "<line class=\"tick\" x1=\"{x:.1}\" x2=\"{x:.1}\" y1=\"{:.1}\" y2=\"{:.1}\"/>",
            CH - MB,
            CH - MB + 4.0
        ));
        out.push_str(&format!(
            "<text class=\"axis\" x=\"{x:.1}\" y=\"{:.1}\" text-anchor=\"middle\">{t}s</text>",
            CH - MB + 16.0
        ));
        t += step;
    }
    let mid_y = (CH - MB + MT) / 2.0;
    out.push_str(&format!(
        "<text class=\"axis\" x=\"14\" y=\"{mid_y:.1}\" transform=\"rotate(-90 14 {mid_y:.1})\" text-anchor=\"middle\">{}</text>",
        escape(&d.ylabel)
    ));
    for (tm, label) in d.chapters.iter().skip(1) {
        let x = x_of(d, *tm);
        out.push_str(&format!(
            "<line class=\"mark\" x1=\"{x:.1}\" x2=\"{x:.1}\" y1=\"{MT}\" y2=\"{:.1}\"/>",
            CH - MB
        ));
        out.push_str(&format!(
            "<text class=\"marklabel\" x=\"{:.1}\" y=\"{:.1}\">{}</text>",
            x + 4.0,
            MT + 10.0,
            escape(label)
        ));
    }
    for s in &d.series {
        let pts: Vec<String> =
            s.t.iter()
                .zip(&s.y)
                .map(|(t, v)| format!("{:.1},{:.1}", x_of(d, *t), y_of(*v)))
                .collect();
        let dash = if s.dash {
            " stroke-dasharray=\"5 4\""
        } else {
            ""
        };
        out.push_str(&format!(
            "<polyline points=\"{}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{}\"{dash} stroke-linejoin=\"round\"/>",
            pts.join(" "),
            escape(&s.color),
            s.lw
        ));
        if !s.label.is_empty() && !s.t.is_empty() {
            let i = ((s.t.len() as f64 * s.at) as usize).min(s.t.len() - 1);
            out.push_str(&format!(
                "<text class=\"serieslabel\" x=\"{:.1}\" y=\"{:.1}\" fill=\"{}\">{}</text>",
                x_of(d, s.t[i]) + 4.0,
                y_of(s.y[i]) - 5.0,
                escape(&s.color),
                escape(&s.label)
            ));
        }
    }
    let by = CH - 10.0;
    out.push_str(&format!(
        "<rect class=\"band\" x=\"{ML}\" y=\"{MT}\" width=\"0\" height=\"{:.1}\"/>",
        CH - MB - MT
    ));
    out.push_str(&format!(
        "<rect class=\"bar-bg\" x=\"{ML}\" y=\"{by}\" width=\"{:.1}\" height=\"4\" rx=\"2\"/>",
        CW - ML - MR
    ));
    out.push_str(&format!(
        "<rect class=\"bar-played\" x=\"{ML}\" y=\"{by}\" width=\"0\" height=\"4\" rx=\"2\"/>"
    ));
    for (tm, _) in d.chapters.iter().skip(1) {
        let x = x_of(d, *tm);
        out.push_str(&format!(
            "<rect class=\"chapter\" x=\"{:.1}\" y=\"{:.1}\" width=\"2\" height=\"10\"/>",
            x - 1.0,
            by - 3.0
        ));
    }
    out.push_str(&format!("<line class=\"peek-line\" x1=\"{ML}\" x2=\"{ML}\" y1=\"{MT}\" y2=\"{:.1}\" visibility=\"hidden\"/>", CH - MB));
    out.push_str(&format!(
        "<line class=\"head\" x1=\"{ML}\" x2=\"{ML}\" y1=\"{MT}\" y2=\"{:.1}\"/>",
        by + 4.0
    ));
    out.push_str(&format!(
        "<circle class=\"head-dot\" cx=\"{ML}\" cy=\"{:.1}\" r=\"5\"/>",
        by + 2.0
    ));
    out.push_str(&format!(
        "<text class=\"head-t\" x=\"{:.1}\" y=\"{:.1}\">0.00s</text>",
        ML + 4.0,
        CH - MB - 6.0
    ));
    out.push_str("</svg>");
    out
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
    reelbox: HtmlElement,
    reel: HtmlElement,
    gate: HtmlElement,
    frames: Vec<HtmlElement>,
    slider: Element,
    time_label: Element,
    play: Element,
    rate: Element,
    chart: Option<Element>,
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
    // machine diagram's playhead) follow this composed, bubbling event.
    let init = web_sys::CustomEventInit::new();
    init.set_bubbles(true);
    init.set_composed(true);
    init.set_detail(&JsValue::from_f64(st.tc));
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
    if let Some(chart) = &dom.chart {
        let x = x_of(d, st.tc);
        if let Some(head) = dom.q(".head") {
            let _ = head.set_attribute("x1", &format!("{x:.1}"));
            let _ = head.set_attribute("x2", &format!("{x:.1}"));
        }
        if let Some(dot) = dom.q(".head-dot") {
            let _ = dot.set_attribute("cx", &format!("{x:.1}"));
        }
        if let Some(label) = dom.q(".head-t") {
            let _ = label.set_attribute("x", &format!("{:.1}", x + 4.0));
            label.set_text_content(Some(&fmt(st.tc)));
        }
        if let Some(played) = dom.q(".bar-played") {
            let _ = played.set_attribute("width", &format!("{:.1}", (x - ML).max(0.0)));
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

fn show_peek(dom: &Dom, d: &Data, t: f64, anchor_x: Option<f64>) {
    let (Some(chart), Some(peek), Some(pframe), Some(ptime)) =
        (&dom.chart, &dom.peek, &dom.pframe, &dom.ptime)
    else {
        return;
    };
    let k = d.frame_at(t);
    let x = x_of(d, t);
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
    if let Some(band) = dom.q(".band") {
        let _ = band.set_attribute("x", &format!("{:.1}", x_of(d, c.0)));
        let _ = band.set_attribute(
            "width",
            &format!("{:.1}", (x_of(d, next_c) - x_of(d, c.0)).max(0.0)),
        );
    }
    let _ = pframe.style().set_property(
        "background-position",
        &format!("{}px 0", -(k as f64) * dom.cell_w),
    );
    ptime.set_text_content(Some(&format!("{} · {}", fmt(d.times[k]), c.1)));
    peek.set_hidden(false);
    let rect = chart.get_bounding_client_rect();
    let px = anchor_x.unwrap_or_else(|| x * (rect.width() / CW));
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
    if let Some(band) = dom.q(".band") {
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
        let px = (f64::from(e.client_x()) - r.left()) * (CW / r.width());
        ((px - ML) / (CW - ML - MR) * self.data.t_max).clamp(0.0, self.data.end())
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

struct Wiring {
    _player: Rc<Player>,
    _closures: Vec<Closure<dyn FnMut(Event)>>,
}

struct Film {
    host: HtmlElement,
    wiring: Option<Wiring>,
}

const SPEEDS: [f64; 3] = [0.25, 0.5, 1.0];

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
        let chart = if data.series.is_empty() {
            String::new()
        } else {
            chart_svg(&data)
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
        shadow.set_inner_html(&format!(
            "<style>{BASE_CSS}
:host {{ display: block; border: 1px solid var(--op-border); background: var(--op-surface); color: var(--op-text); padding: 0.5rem; max-width: 930px; user-select: none; -webkit-user-select: none; outline: 2px solid transparent; outline-offset: 2px; font-size: 0.9rem; }}
:host(:focus-visible) {{ outline-color: var(--op-accent); }}
.stagebox {{ display: flex; flex-direction: column; align-items: center; padding: 4px 0 8px; }}
.stage {{ background-repeat: no-repeat; border: 1px solid var(--op-border); }}
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
.bar button:hover {{ color: var(--op-link-hover); border-color: var(--op-link-hover); }}
.bar button:focus-visible, .bar select:focus-visible, .bar input:focus-visible, .chart:focus-visible {{ outline: 2px solid var(--op-accent); outline-offset: 2px; }}
.bar input[type=range] {{ flex: 1; min-width: 220px; accent-color: var(--op-accent); }}
.t {{ font-variant-numeric: tabular-nums; min-width: 4.5rem; }} .n {{ color: var(--op-muted); }}
.keys {{ font-size: 0.85em; color: var(--op-muted); margin: 0.3rem 0 0; }} .keys summary {{ cursor: pointer; }}
.keys dl {{ display: grid; grid-template-columns: max-content 1fr; gap: 0.15rem 0.8rem; margin: 0.4rem 0; }} .keys dt {{ font-family: var(--op-font-mono); }} .keys dd {{ margin: 0; }}
.chartbox {{ margin-top: 0.6rem; position: relative; }}
.chart {{ max-width: 100%; height: auto; cursor: ew-resize; display: block; touch-action: none; font-family: var(--op-font-sans); font-size: 11px; }}
.chart .grid {{ stroke: var(--op-border); }} .chart .tick {{ stroke: var(--op-border-strong); }} .chart .axis {{ fill: var(--op-muted); }}
.chart .mark {{ stroke: var(--op-accent); stroke-dasharray: 3 3; }} .chart .marklabel {{ fill: var(--op-accent); }}
.chart .serieslabel {{ font-weight: 700; paint-order: stroke; stroke: var(--op-surface); stroke-width: 4; }}
.chart .band {{ fill: var(--op-accent); opacity: 0.08; }} .chart .bar-bg {{ fill: var(--op-border); }} .chart .bar-played {{ fill: var(--op-accent); }}
.chart .chapter {{ fill: var(--op-surface); stroke: var(--op-border-strong); stroke-width: 0.6; }}
.chart .peek-line {{ stroke: var(--op-muted); stroke-dasharray: 3 3; }}
.chart .head {{ stroke: var(--op-accent); stroke-width: 1.5; }} .chart .head-dot {{ fill: var(--op-accent); }}
.chart .head-t {{ fill: var(--op-accent); font-weight: 700; paint-order: stroke; stroke: var(--op-surface); stroke-width: 4; }}
.peek {{ position: absolute; bottom: 56px; transform: translateX(-50%); pointer-events: none; background: var(--op-raised); border: 1px solid var(--op-border-strong); border-radius: 3px; padding: 3px; z-index: 3; }}
.peek .pframe {{ background-repeat: no-repeat; }} .peek .ptime {{ font-size: 0.8em; text-align: center; color: var(--op-text); font-variant-numeric: tabular-nums; white-space: nowrap; }}
.sr {{ position: absolute; width: 1px; height: 1px; overflow: hidden; clip: rect(0 0 0 0); white-space: nowrap; }}
</style>
<div class=\"stagebox\" part=\"stage\"><div class=\"stage\" style=\"width:{stage_w}px;height:{stage_h}px;background-image:url('{sheet}');background-size:{}px {stage_h}px\"></div><div class=\"stagelabel\"></div></div>
<div class=\"reelbox\" part=\"reel\"><div class=\"gate\"></div><div class=\"reel\">{cells}</div></div>
<div class=\"bar\"><button type=\"button\" class=\"play\">Play</button><select aria-label=\"speed\"><option value=\"1\">1x</option><option value=\"0.5\">0.5x</option><option value=\"0.25\">0.25x</option></select><input type=\"range\" min=\"0\" max=\"{}\" value=\"0\" aria-label=\"frame\"><span class=\"t\"></span><span class=\"n\">{n} frames; captions give the share of pixels changed since the previous frame</span></div>
<details class=\"keys\"><summary>Keys</summary><dl><dt>Space, K</dt><dd>play / pause</dd><dt>, .</dt><dd>previous / next frame</dd><dt>← →</dt><dd>five frames back / forward</dd><dt>J L</dt><dd>ten frames back / forward</dd><dt>0-9</dt><dd>seek to 0-90 %</dd><dt>Home End</dt><dd>first / last frame</dd><dt>&lt; &gt;</dt><dd>slower / faster</dd><dt>Esc</dt><dd>cancel a pending seek on the strip</dd></dl><p>Hover the chart or the strip to peek without moving the playhead; press and drag across the strip to choose a frame and release to seek.</p></details>
<div class=\"chartbox\">{chart}{peek}</div><span class=\"sr\" aria-live=\"polite\"></span>",
            stage_w * n as f64,
            n - 1,
            sheet = escape(&data.sheet),
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
            reelbox,
            reel,
            gate,
            frames,
            slider,
            time_label,
            play,
            rate,
            chart: q(".chart"),
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
                    let handled = match key.as_str() {
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
                            let r = SPEEDS[(i + dir).clamp(0, SPEEDS.len() as i64 - 1) as usize];
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
                    };
                    if handled {
                        ke.prevent_default();
                    }
                }),
            );
        }
        player.render();
        self.wiring = Some(Wiring {
            _player: player,
            _closures: closures,
        });
    }
}
