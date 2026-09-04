//! `<opt-machine for="film-id">`: an interaction machine's transition
//! diagram, drawn from a `<script type="application/json">` child of
//! the form `{"nodes": [..], "edges": [[from, input, to], ..],
//! "highlight": [from, input, to] | null, "trace": [[t, from, input,
//! to], ..]}`. The interaction reports use it as their organising
//! principle: each behaviour highlights the edge it exercises. Nodes are
//! flight states; loops are inputs that leave the flight alone.
//!
//! The machine has a playhead of its own: with a recorded `trace` (the
//! transitions the behaviour actually went through, with their times)
//! and a `for` naming an `opt-film`, a token follows the film's clock -
//! resting on the settled node, travelling along the edge for a short
//! while after each transition - so the diagram is one more projection
//! of the same timeline.

use op_webc::{CustomElement, ElementDefinition};
use wasm_bindgen::JsCast;
use web_sys::HtmlElement;

use super::{BASE_CSS, shadow_root};
use crate::html::escape;

pub const DEFINITION: ElementDefinition = ElementDefinition {
    source: op_webc::here!(),
    tag: "opt-machine",
    observed_attributes: &[],
    properties: &[],
    create: |host| {
        Box::new(MachineDiagram {
            host,
            _follow: None,
        })
    },
};

struct Graph {
    nodes: Vec<String>,
    edges: Vec<(String, String, String)>,
    highlight: Option<(String, String, String)>,
    /// `(time, from, input, to)`, in time order.
    trace: Vec<(f64, String, String, String)>,
}

fn triple(v: &wasm_bindgen::JsValue) -> Option<(String, String, String)> {
    // `Array.from(null)` throws; a missing or null highlight is simply none
    if !js_sys::Array::is_array(v) {
        return None;
    }
    let a = js_sys::Array::from(v);
    Some((
        a.get(0).as_string()?,
        a.get(1).as_string()?,
        a.get(2).as_string()?,
    ))
}

fn parse(json: &str) -> Option<Graph> {
    let v = js_sys::JSON::parse(json).ok()?;
    // absent keys read as `undefined`, which is not a value to build from
    let get = |k: &str| {
        js_sys::Reflect::get(&v, &wasm_bindgen::JsValue::from_str(k))
            .ok()
            .filter(|x| !x.is_undefined() && !x.is_null())
    };
    let nodes: Vec<String> = js_sys::Array::from(&get("nodes")?)
        .iter()
        .filter_map(|n| n.as_string())
        .collect();
    let edges: Vec<(String, String, String)> = js_sys::Array::from(&get("edges")?)
        .iter()
        .filter_map(|e| triple(&e))
        .collect();
    let highlight = get("highlight").and_then(|h| triple(&h));
    let trace = get("trace")
        .filter(js_sys::Array::is_array)
        .map(|t| {
            js_sys::Array::from(&t)
                .iter()
                .filter_map(|row| {
                    let a = js_sys::Array::from(&row);
                    Some((
                        a.get(0).as_f64()?,
                        a.get(1).as_string()?,
                        a.get(2).as_string()?,
                        a.get(3).as_string()?,
                    ))
                })
                .collect()
        })
        .unwrap_or_default();
    (!nodes.is_empty()).then_some(Graph {
        nodes,
        edges,
        highlight,
        trace,
    })
}

/// Vertical spacing of the diagram: node centres, node radius, and the
/// lanes below the nodes that carry backward and long edges.
const NODE_Y: f64 = 120.0;
const NODE_R: f64 = 36.0;
const LANE_FIRST: f64 = 60.0;
const LANE_STEP: f64 = 34.0;
const CORNER: f64 = 12.0;

fn node_pos(nodes: &[String], name: &str) -> (f64, f64) {
    if nodes.len() == 1 {
        (180.0, NODE_Y)
    } else {
        let i = nodes.iter().position(|x| x == name).unwrap_or(0);
        (150.0 + i as f64 * 240.0, NODE_Y)
    }
}

fn index_of(nodes: &[String], name: &str) -> usize {
    nodes.iter().position(|x| x == name).unwrap_or(0)
}

/// Whether an edge is drawn as a straight line between neighbours (true)
/// or routed through a lane below the nodes (false). Loops are neither.
fn straight(nodes: &[String], from: &str, to: &str) -> bool {
    let (i, j) = (index_of(nodes, from), index_of(nodes, to));
    j == i + 1
}

/// The lane of every routed edge: the first lane free of any edge whose
/// span strictly overlaps, shorter spans first, so two edges never share a
/// run. Edges that only touch at a node do share a lane; their drops sit on
/// opposite sides of that node.
pub fn lanes(
    nodes: &[String],
    edges: &[(String, String, String)],
) -> Vec<((String, String), usize)> {
    let mut routed: Vec<(usize, usize, String, String)> = edges
        .iter()
        .filter(|(f, _, t)| f != t && !straight(nodes, f, t))
        .map(|(f, _, t)| {
            let (i, j) = (index_of(nodes, f), index_of(nodes, t));
            (i.min(j), i.max(j), f.clone(), t.clone())
        })
        .collect();
    routed.sort_by_key(|(lo, hi, f, t)| (hi - lo, *lo, f.clone(), t.clone()));
    routed.dedup();
    let mut assigned: Vec<(usize, usize, usize)> = Vec::new(); // (lo, hi, lane)
    let mut out = Vec::new();
    for (lo, hi, f, t) in routed {
        let mut lane = 0;
        while assigned
            .iter()
            .any(|(alo, ahi, l)| *l == lane && lo < *ahi && *alo < hi)
        {
            lane += 1;
        }
        assigned.push((lo, hi, lane));
        out.push(((f, t), lane));
    }
    out
}

fn lane_of(lanes: &[((String, String), usize)], from: &str, to: &str) -> usize {
    lanes
        .iter()
        .find(|((f, t), _)| f == from && t == to)
        .map_or(0, |(_, l)| *l)
}

/// The corner points of a routed edge: down from the source, along the
/// lane, up into the target. Drops move outward with the lane so several
/// edges leaving one node never share a vertical.
fn route(nodes: &[String], from: &str, to: &str, lane: usize) -> [(f64, f64); 4] {
    let (x1, y1) = node_pos(nodes, from);
    let (x2, _) = node_pos(nodes, to);
    let dx = 10.0 + lane as f64 * 6.0;
    let (sx, ex) = if x2 > x1 {
        (x1 + dx, x2 - dx)
    } else {
        (x1 - dx, x2 + dx)
    };
    let ly = y1 + NODE_R + LANE_FIRST + lane as f64 * LANE_STEP;
    [(sx, y1 + NODE_R), (sx, ly), (ex, ly), (ex, y1 + NODE_R)]
}

/// A point along an edge at parameter `s` in 0..=1, by the same geometry
/// `svg` draws: a straight line between neighbours, a lane run for routed
/// edges (by arc length), a cubic loop for self edges.
pub fn point_on_edge(
    nodes: &[String],
    lanes: &[((String, String), usize)],
    from: &str,
    to: &str,
    s: f64,
) -> (f64, f64) {
    let r = NODE_R;
    let (x1, y1) = node_pos(nodes, from);
    let (x2, _) = node_pos(nodes, to);
    let s = s.clamp(0.0, 1.0);
    if from == to {
        // cubic loop above the node, as drawn
        let (p0, p1, p2, p3) = (
            (x1 - 14.0, y1 - r + 2.0),
            (x1 - 40.0, y1 - 90.0),
            (x1 + 40.0, y1 - 90.0),
            (x1 + 14.0, y1 - r + 2.0),
        );
        let u = 1.0 - s;
        return (
            u * u * u * p0.0 + 3.0 * u * u * s * p1.0 + 3.0 * u * s * s * p2.0 + s * s * s * p3.0,
            u * u * u * p0.1 + 3.0 * u * u * s * p1.1 + 3.0 * u * s * s * p2.1 + s * s * s * p3.1,
        );
    }
    if straight(nodes, from, to) {
        let (ax, bx) = (x1 + r, x2 - r);
        return (ax + (bx - ax) * s, y1);
    }
    let pts = route(nodes, from, to, lane_of(lanes, from, to));
    let seg = |a: (f64, f64), b: (f64, f64)| ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
    let lens = [
        seg(pts[0], pts[1]),
        seg(pts[1], pts[2]),
        seg(pts[2], pts[3]),
    ];
    let total: f64 = lens.iter().sum();
    let mut d = s * total;
    for k in 0..3 {
        if d <= lens[k] || k == 2 {
            let f = if lens[k] > 0.0 {
                (d / lens[k]).clamp(0.0, 1.0)
            } else {
                1.0
            };
            let (a, b) = (pts[k], pts[k + 1]);
            return (a.0 + (b.0 - a.0) * f, a.1 + (b.1 - a.1) * f);
        }
        d -= lens[k];
    }
    pts[3]
}

/// Where the token sits at time `t` given a trace: on the destination
/// node once a transition's traversal window has passed, on the edge
/// while it is being traversed, on the origin node before any.
pub const TRAVERSE_SECONDS: f64 = 0.35;

pub fn token_at(
    nodes: &[String],
    lanes: &[((String, String), usize)],
    trace: &[(f64, String, String, String)],
    t: f64,
) -> (f64, f64) {
    let mut current: Option<&(f64, String, String, String)> = None;
    for row in trace {
        if row.0 <= t + 1e-6 {
            current = Some(row);
        }
    }
    match current {
        None => {
            let start = trace
                .first()
                .map(|r| r.1.as_str())
                .unwrap_or_else(|| nodes.first().map(String::as_str).unwrap_or(""));
            point_on_edge(nodes, lanes, start, start, 0.0)
        }
        Some((t0, from, _input, to)) => {
            let s = ((t - t0) / TRAVERSE_SECONDS).clamp(0.0, 1.0);
            if from == to && s >= 1.0 {
                return point_on_edge(nodes, lanes, to, to, 0.0);
            }
            point_on_edge(nodes, lanes, from, to, s)
        }
    }
}

/// The diagram as SVG markup (pure, so it is testable natively).
pub fn svg(
    nodes: &[String],
    edges: &[(String, String, String)],
    highlight: Option<&(String, String, String)>,
) -> String {
    let n = nodes.len();
    let width = if n > 1 {
        240.0 * n as f64 + 60.0
    } else {
        360.0
    };
    let pos = |name: &str| -> (f64, f64) {
        if n == 1 {
            (180.0, 120.0)
        } else {
            let i = nodes.iter().position(|x| x == name).unwrap_or(0);
            (150.0 + i as f64 * 240.0, 120.0)
        }
    };
    let r = NODE_R;
    let lane_table = lanes(nodes, edges);
    let deepest = lane_table.iter().map(|(_, l)| *l).max();
    let height = deepest.map_or(250.0, |l| {
        (NODE_Y + r + LANE_FIRST + l as f64 * LANE_STEP + 18.0).max(250.0)
    });
    let is_hl =
        |f: &str, i: &str, t: &str| highlight.is_some_and(|h| h.0 == f && h.1 == i && h.2 == t);
    let mut out = format!(
        "<svg viewBox=\"0 0 {width} {height}\" width=\"{width}\" height=\"{height}\" role=\"img\" aria-label=\"interaction machine\"><defs><marker id=\"a\" viewBox=\"0 0 10 10\" refX=\"9\" refY=\"5\" markerWidth=\"8\" markerHeight=\"8\" orient=\"auto-start-reverse\"><path d=\"M0 0L10 5L0 10z\" class=\"arrow\"/></marker><marker id=\"ah\" viewBox=\"0 0 10 10\" refX=\"9\" refY=\"5\" markerWidth=\"8\" markerHeight=\"8\" orient=\"auto-start-reverse\"><path d=\"M0 0L10 5L0 10z\" class=\"arrow hl\"/></marker></defs>"
    );
    let mut loops: Vec<(String, Vec<String>)> = Vec::new();
    for (f, i, t) in edges {
        if f == t {
            match loops.iter_mut().find(|(name, _)| name == f) {
                Some((_, inputs)) => inputs.push(i.clone()),
                None => loops.push((f.clone(), vec![i.clone()])),
            }
        }
    }
    for (f, i, t) in edges {
        if f == t {
            continue;
        }
        let hl = is_hl(f, i, t);
        let (x1, y1) = pos(f);
        let (x2, y2) = pos(t);
        let forward = x2 > x1;
        let adjacent = straight(nodes, f, t);
        let class = if hl { "edge hl" } else { "edge" };
        let marker = if hl { "ah" } else { "a" };
        let (path, lx, ly) = if forward && adjacent {
            (
                format!("M{} {y1} L{} {y2}", x1 + r, x2 - r),
                (x1 + x2) / 2.0,
                y1 - 10.0,
            )
        } else {
            // down from the source, along its lane, up into the target, with
            // rounded corners; the label sits above the run, off the stroke
            let lane = lane_of(&lane_table, f, t);
            let [p0, p1, p2, p3] = route(nodes, f, t, lane);
            let dir = if p2.0 > p1.0 { 1.0 } else { -1.0 };
            let path = format!(
                "M{} {} L{} {} Q{} {} {} {} L{} {} Q{} {} {} {} L{} {}",
                p0.0,
                p0.1,
                p1.0,
                p1.1 - CORNER,
                p1.0,
                p1.1,
                p1.0 + dir * CORNER,
                p1.1,
                p2.0 - dir * CORNER,
                p2.1,
                p2.0,
                p2.1,
                p2.0,
                p2.1 - CORNER,
                p3.0,
                p3.1
            );
            (path, (p1.0 + p2.0) / 2.0, p1.1 - 7.0)
        };
        out.push_str(&format!(
            "<path d=\"{path}\" class=\"{class}\" marker-end=\"url(#{marker})\"/>"
        ));
        out.push_str(&format!(
            "<text x=\"{lx}\" y=\"{ly}\" text-anchor=\"middle\" class=\"label{}\">{}</text>",
            if hl { " hl" } else { "" },
            escape(i)
        ));
    }
    for name in nodes {
        let (x, y) = pos(name);
        let involved = highlight.is_some_and(|h| &h.0 == name || &h.2 == name);
        out.push_str(&format!(
            "<circle cx=\"{x}\" cy=\"{y}\" r=\"{r}\" class=\"node{}\"/>",
            if involved { " hl" } else { "" }
        ));
        out.push_str(&format!(
            "<text x=\"{x}\" y=\"{}\" text-anchor=\"middle\" class=\"name\">{}</text>",
            y + 5.0,
            escape(name)
        ));
        if let Some((_, inputs)) = loops.iter().find(|(l, _)| l == name) {
            let hl_loop = highlight.is_some_and(|h| &h.0 == name && &h.2 == name);
            let hl_input = highlight.filter(|_| hl_loop).map(|h| h.1.as_str());
            out.push_str(&format!(
                "<path d=\"M{} {} C{} {} {} {} {} {}\" class=\"edge{}\" marker-end=\"url(#{})\"/>",
                x - 14.0,
                y - r + 2.0,
                x - 40.0,
                y - 90.0,
                x + 40.0,
                y - 90.0,
                x + 14.0,
                y - r + 2.0,
                if hl_loop { " hl" } else { "" },
                if hl_loop { "ah" } else { "a" }
            ));
            let labels: Vec<String> = inputs
                .iter()
                .map(|i| {
                    if Some(i.as_str()) == hl_input {
                        format!("<tspan class=\"hl\">{}</tspan>", escape(i))
                    } else {
                        escape(i)
                    }
                })
                .collect();
            out.push_str(&format!(
                "<text x=\"{x}\" y=\"{}\" text-anchor=\"middle\" class=\"label\">{}</text>",
                y - 82.0,
                labels.join(" / ")
            ));
        }
    }
    out.push_str("<circle class=\"token\" r=\"9\" cx=\"-100\" cy=\"-100\"/>");
    out.push_str("</svg>");
    out
}

/// The clock an `opt-film-time` detail carries: the `time` field when the
/// detail is the object `{ time, duration, playing }` the film dispatches, and
/// otherwise the bare number the event used to carry, so a page that still
/// sends one keeps its playhead. An object without a `time` is not a clock.
pub(crate) fn film_time_of(number: Option<f64>, object_time: Option<f64>) -> Option<f64> {
    object_time.or(number)
}

struct MachineDiagram {
    host: HtmlElement,
    _follow: Option<wasm_bindgen::closure::Closure<dyn FnMut(web_sys::Event)>>,
}

impl CustomElement for MachineDiagram {
    fn connected(&mut self) {
        let json = self
            .host
            .query_selector("script[type=\"application/json\"]")
            .ok()
            .flatten()
            .and_then(|s| s.text_content())
            .unwrap_or_default();
        let graph = parse(&json);
        let body = match &graph {
            Some(g) => svg(&g.nodes, &g.edges, g.highlight.as_ref()),
            None => "<p>opt-machine: no machine data.</p>".to_owned(),
        };
        shadow_root(&self.host).set_inner_html(&format!(
            "<style>{BASE_CSS}
:host {{ display: block; margin: 0.4rem 0 1rem; font-family: var(--op-font-sans); font-size: 13px; }}
svg {{ max-width: 100%; height: auto; }}
.edge {{ fill: none; stroke: var(--op-muted); stroke-width: 1.3; }} .edge.hl {{ stroke: var(--op-accent); stroke-width: 3; }}
.arrow {{ fill: var(--op-muted); }} .arrow.hl {{ fill: var(--op-accent); }}
.label {{ fill: var(--op-muted); paint-order: stroke; stroke: var(--op-bg); stroke-width: 5; }} .label.hl, .label .hl {{ fill: var(--op-accent); font-weight: 700; }}
.node {{ fill: var(--op-surface); stroke: var(--op-border-strong); stroke-width: 1.3; }} .node.hl {{ stroke: var(--op-accent); stroke-width: 2; fill: color-mix(in srgb, var(--op-accent) 14%, var(--op-surface)); }}
.name {{ fill: var(--op-text); font-weight: 600; }}
.token {{ fill: var(--op-accent); stroke: var(--op-bg); stroke-width: 2; }}
</style>{body}"
        ));
        // the playhead: rest the token at the trace's start, then follow the film's clock
        let Some(graph) = graph else {
            return;
        };
        let shadow = shadow_root(&self.host);
        let place = {
            let token = shadow.query_selector(".token").ok().flatten();
            let nodes = graph.nodes.clone();
            let trace = graph.trace.clone();
            let lane_table = lanes(&nodes, &graph.edges);
            move |t: f64| {
                if let Some(token) = &token {
                    let (x, y) = token_at(&nodes, &lane_table, &trace, t);
                    let _ = token.set_attribute("cx", &format!("{x:.1}"));
                    let _ = token.set_attribute("cy", &format!("{y:.1}"));
                }
            }
        };
        place(0.0);
        let Some(film_id) = self.host.get_attribute("for") else {
            return;
        };
        let Some(film) = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.get_element_by_id(&film_id))
        else {
            return;
        };
        let closure = wasm_bindgen::closure::Closure::<dyn FnMut(web_sys::Event)>::new(
            move |e: web_sys::Event| {
                if let Ok(custom) = e.dyn_into::<web_sys::CustomEvent>() {
                    let detail = custom.detail();
                    // `Reflect::get` throws on a number, a null or an
                    // undefined detail, which reads here as no field
                    let object_time =
                        js_sys::Reflect::get(&detail, &wasm_bindgen::JsValue::from_str("time"))
                            .ok()
                            .and_then(|v| v.as_f64());
                    if let Some(t) = film_time_of(detail.as_f64(), object_time) {
                        place(t);
                    }
                }
            },
        );
        let _ = film
            .add_event_listener_with_callback("opt-film-time", closure.as_ref().unchecked_ref());
        self._follow = Some(closure);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(x: &str) -> String {
        x.to_owned()
    }

    /// The toggle's machine: three flight states, every input.
    fn toggle() -> (Vec<String>, Vec<(String, String, String)>) {
        let nodes = vec![s("Idle"), s("Toward"), s("Back")];
        let edges = vec![
            (s("Idle"), s("Attend"), s("Idle")),
            (s("Idle"), s("Activate"), s("Toward")),
            (s("Toward"), s("Activate"), s("Back")),
            (s("Toward"), s("Finished"), s("Idle")),
            (s("Back"), s("Activate"), s("Toward")),
            (s("Back"), s("Finished"), s("Idle")),
        ];
        (nodes, edges)
    }

    /// Routed edges never share a run, labels sit above their run, and the
    /// diagram grows to hold the deepest lane.
    #[test]
    fn routed_edges_take_distinct_lanes_and_labels_sit_above_their_runs() {
        let (nodes, edges) = toggle();
        let table = lanes(&nodes, &edges);
        let lane = |f: &str, t: &str| lane_of(&table, f, t);
        // the two short backward edges only touch at Toward and share lane 0;
        // the long one spans both and takes the next lane
        assert_eq!(lane("Toward", "Idle"), 0);
        assert_eq!(lane("Back", "Toward"), 0);
        assert_eq!(lane("Back", "Idle"), 1);
        let out = svg(&nodes, &edges, None);
        // every routed label is 7 px above its lane's run and no two lanes coincide
        let runs: Vec<f64> = (0..=1)
            .map(|l| NODE_Y + NODE_R + LANE_FIRST + l as f64 * LANE_STEP)
            .collect();
        for run in &runs {
            assert!(
                out.contains(&format!(
                    "y=\"{}\" text-anchor=\"middle\" class=\"label\"",
                    run - 7.0
                )),
                "label above run {run}"
            );
        }
        assert!(out.contains(&format!(
            "viewBox=\"0 0 780 {}\"",
            NODE_Y + NODE_R + LANE_FIRST + LANE_STEP + 18.0
        )));
        // the drops of the two edges meeting at Toward sit on opposite sides of it
        let [p0, ..] = route(&nodes, "Toward", "Idle", 0);
        let [.., p3] = route(&nodes, "Back", "Toward", 0);
        let (tx, _) = node_pos(&nodes, "Toward");
        assert!(p0.0 < tx && p3.0 > tx);
        // a lane run stays clear of the run above it by the lane step
        assert!(runs[1] - runs[0] >= 30.0);
    }

    #[test]
    fn highlights_exactly_the_requested_edge_and_its_nodes() {
        let nodes = vec![s("Idle"), s("Toward"), s("Back")];
        let edges = vec![
            (s("Idle"), s("Activate"), s("Toward")),
            (s("Toward"), s("Finished"), s("Idle")),
            (s("Idle"), s("Attend"), s("Idle")),
        ];
        let out = svg(
            &nodes,
            &edges,
            Some(&(s("Idle"), s("Activate"), s("Toward"))),
        );
        assert_eq!(out.matches("class=\"edge hl\"").count(), 1);
        assert_eq!(out.matches("class=\"node hl\"").count(), 2);
        assert!(out.contains("class=\"label hl\">Activate<"));
        let loop_hl = svg(&nodes, &edges, Some(&(s("Idle"), s("Attend"), s("Idle"))));
        assert!(loop_hl.contains("<tspan class=\"hl\">Attend</tspan>"));
        assert_eq!(
            loop_hl.matches("class=\"edge hl\"").count(),
            1,
            "the loop edge is the highlighted one"
        );
    }

    #[test]
    fn the_token_rests_on_nodes_and_travels_edges_on_the_trace_clock() {
        let (nodes, edges) = toggle();
        let lanes = lanes(&nodes, &edges);
        let trace = vec![
            (1.0, s("Idle"), s("Activate"), s("Toward")),
            (4.0, s("Toward"), s("Finished"), s("Idle")),
        ];
        let idle = point_on_edge(&nodes, &lanes, "Idle", "Idle", 0.0);
        let toward = point_on_edge(&nodes, &lanes, "Toward", "Toward", 0.0);
        assert_eq!(
            token_at(&nodes, &lanes, &trace, 0.0),
            idle,
            "before any transition: on the start node"
        );
        let mid = token_at(&nodes, &lanes, &trace, 1.0 + TRAVERSE_SECONDS / 2.0);
        assert!(
            idle.0 < mid.0 && mid.0 < toward.0,
            "halfway through the traversal window: between the nodes"
        );
        assert_eq!(
            token_at(&nodes, &lanes, &trace, 2.5),
            point_on_edge(&nodes, &lanes, "Idle", "Toward", 1.0),
            "settled at the destination"
        );
        let back = token_at(&nodes, &lanes, &trace, 4.0 + TRAVERSE_SECONDS / 2.0);
        assert!(
            back.1 > 120.0,
            "the return edge is drawn below the nodes, so the token dips"
        );
        assert_eq!(
            token_at(&nodes, &lanes, &trace, 9.0),
            point_on_edge(&nodes, &lanes, "Toward", "Idle", 1.0)
        );
    }

    #[test]
    fn a_single_node_machine_still_draws_its_loops() {
        let nodes = vec![s("Idle")];
        let edges = vec![
            (s("Idle"), s("Attend"), s("Idle")),
            (s("Idle"), s("Activate"), s("Idle")),
        ];
        let out = svg(&nodes, &edges, None);
        assert!(out.contains("Attend / Activate"));
        assert!(!out.contains("class=\"edge hl\""));
    }

    #[test]
    fn the_playhead_reads_the_time_field_and_still_accepts_a_bare_number() {
        // `{ time: 1.5, duration: 3.0, playing: true }`: the detail the film
        // dispatches, where the number extraction finds nothing
        assert_eq!(film_time_of(None, Some(1.5)), Some(1.5));
        // a bare number, the shape the event used to carry
        assert_eq!(film_time_of(Some(2.25), None), Some(2.25));
        // an object without a time, and a detail that is neither a number nor
        // an object, arrive here alike: no clock, so the token stays put
        assert_eq!(film_time_of(None, None), None);
        // and a detail that is somehow both is read as the object it is
        assert_eq!(film_time_of(Some(2.25), Some(1.5)), Some(1.5));
    }
}
