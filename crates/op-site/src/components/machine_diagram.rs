//! `<opt-machine>`: an interaction machine's transition diagram, drawn
//! from a `<script type="application/json">` child of the form
//! `{"nodes": [..], "edges": [[from, input, to], ..],
//! "highlight": [from, input, to] | null}`. The interaction reports use
//! it as their organising principle: each behaviour highlights the edge
//! it exercises. Nodes are flight states; loops are inputs that leave
//! the flight alone.

use op_webc::{CustomElement, ElementDefinition};
use web_sys::HtmlElement;

use super::{BASE_CSS, shadow_root};
use crate::html::escape;

pub const DEFINITION: ElementDefinition = ElementDefinition {
    source: op_webc::here!(),
    tag: "opt-machine",
    observed_attributes: &[],
    create: |host| Box::new(MachineDiagram { host }),
};

struct MachineDiagram {
    host: HtmlElement,
}

struct Graph {
    nodes: Vec<String>,
    edges: Vec<(String, String, String)>,
    highlight: Option<(String, String, String)>,
}

fn triple(v: &wasm_bindgen::JsValue) -> Option<(String, String, String)> {
    let a = js_sys::Array::from(v);
    Some((
        a.get(0).as_string()?,
        a.get(1).as_string()?,
        a.get(2).as_string()?,
    ))
}

fn parse(json: &str) -> Option<Graph> {
    let v = js_sys::JSON::parse(json).ok()?;
    let get = |k: &str| js_sys::Reflect::get(&v, &wasm_bindgen::JsValue::from_str(k)).ok();
    let nodes: Vec<String> = js_sys::Array::from(&get("nodes")?)
        .iter()
        .filter_map(|n| n.as_string())
        .collect();
    let edges: Vec<(String, String, String)> = js_sys::Array::from(&get("edges")?)
        .iter()
        .filter_map(|e| triple(&e))
        .collect();
    let highlight = get("highlight").and_then(|h| triple(&h));
    (!nodes.is_empty()).then_some(Graph {
        nodes,
        edges,
        highlight,
    })
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
    let r = 36.0;
    let is_hl =
        |f: &str, i: &str, t: &str| highlight.is_some_and(|h| h.0 == f && h.1 == i && h.2 == t);
    let mut out = format!(
        "<svg viewBox=\"0 0 {width} 250\" width=\"{width}\" height=\"250\" role=\"img\" aria-label=\"interaction machine\"><defs><marker id=\"a\" viewBox=\"0 0 10 10\" refX=\"9\" refY=\"5\" markerWidth=\"8\" markerHeight=\"8\" orient=\"auto-start-reverse\"><path d=\"M0 0L10 5L0 10z\" class=\"arrow\"/></marker><marker id=\"ah\" viewBox=\"0 0 10 10\" refX=\"9\" refY=\"5\" markerWidth=\"8\" markerHeight=\"8\" orient=\"auto-start-reverse\"><path d=\"M0 0L10 5L0 10z\" class=\"arrow hl\"/></marker></defs>"
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
        let adjacent = (nodes.iter().position(|x| x == f).unwrap_or(0) as i64
            - nodes.iter().position(|x| x == t).unwrap_or(0) as i64)
            .abs()
            == 1;
        let class = if hl { "edge hl" } else { "edge" };
        let marker = if hl { "ah" } else { "a" };
        let (path, lx, ly) = if forward && adjacent {
            (
                format!("M{} {y1} L{} {y2}", x1 + r, x2 - r),
                (x1 + x2) / 2.0,
                y1 - 10.0,
            )
        } else {
            let depth = if adjacent { 70.0 } else { 105.0 };
            let cy = y1 + depth;
            let (sx, ex) = if forward {
                (x1 + 8.0, x2 - 8.0)
            } else {
                (x1 - 8.0, x2 + 8.0)
            };
            let path = format!(
                "M{sx} {} Q{} {} {ex} {}",
                y1 + r,
                (x1 + x2) / 2.0,
                cy + 40.0,
                y2 + r
            );
            let lx = (sx + 2.0 * (x1 + x2) / 2.0 + ex) / 4.0;
            let ly = (y1 + r + 2.0 * (cy + 40.0) + y2 + r) / 4.0 + 4.0;
            (path, lx, ly)
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
    out.push_str("</svg>");
    out
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
        let body = match parse(&json) {
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
</style>{body}"
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(x: &str) -> String {
        x.to_owned()
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
}
