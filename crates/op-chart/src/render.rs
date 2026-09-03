//! Classed SVG for a [`Spec`], laid out by [`Layout`]. Elements sit in
//! z-ordered groups: axes, marks, series, track, cursor, playhead. The
//! playhead group is the only thing that moves per tick: one `transform`
//! carries its line, dot and readout.

use crate::{Layout, Spec};

/// The emitted markup and the geometry it was drawn with.
#[derive(Clone, Debug, PartialEq)]
pub struct Rendered {
    pub svg: String,
    pub layout: Layout,
}

/// Escape text for an attribute or a text node.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// The value gridlines drawn and labelled on the left axis.
const VALUE_TICKS: [f64; 5] = [0.0, 25.0, 50.0, 75.0, 100.0];

/// Draw `spec` in the box and scales `l` describes. The caller chooses the
/// layout (the film uses [`Layout::film`]) so the element and the page
/// build can size a chart without the renderer knowing about either.
pub fn render(spec: &Spec, l: Layout) -> Rendered {
    let mut out = format!(
        "<svg class=\"chart\" part=\"chart\" viewBox=\"0 0 {} {}\" tabindex=\"0\" role=\"slider\" aria-label=\"playhead\" aria-valuemin=\"0\" aria-valuemax=\"{:.2}\" aria-valuenow=\"0\" aria-valuetext=\"0.00 seconds\">",
        l.width, l.height, spec.duration
    );

    out.push_str("<g class=\"axes\">");
    for v in VALUE_TICKS {
        let y = l.y_of(v);
        let w = if v == 0.0 || v == 100.0 { 1.0 } else { 0.5 };
        out.push_str(&format!(
            "<line class=\"grid\" x1=\"{}\" x2=\"{:.1}\" y1=\"{y:.1}\" y2=\"{y:.1}\" stroke-width=\"{w}\"/>",
            l.left,
            l.width - l.right
        ));
        out.push_str(&format!(
            "<text class=\"axis\" x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"end\">{v:.0}</text>",
            l.left - 6.0,
            y + 4.0
        ));
    }
    let step = if l.end <= 5.0 { 0.5 } else { 1.0 };
    let mut t = 0.0;
    while t <= l.end + 1e-9 {
        let x = l.x_of(t);
        out.push_str(&format!(
            "<line class=\"tick\" x1=\"{x:.1}\" x2=\"{x:.1}\" y1=\"{:.1}\" y2=\"{:.1}\"/>",
            l.plot_bottom(),
            l.plot_bottom() + 4.0
        ));
        out.push_str(&format!(
            "<text class=\"axis\" x=\"{x:.1}\" y=\"{:.1}\" text-anchor=\"middle\">{t}s</text>",
            l.axis_label_y()
        ));
        t += step;
    }
    let mid_y = (l.plot_bottom() + l.top) / 2.0;
    out.push_str(&format!(
        "<text class=\"axis\" x=\"14\" y=\"{mid_y:.1}\" transform=\"rotate(-90 14 {mid_y:.1})\" text-anchor=\"middle\">{}</text>",
        escape(&spec.ylabel)
    ));
    out.push_str("</g>");

    out.push_str("<g class=\"marks\">");
    for ch in spec.chapters.iter().skip(1) {
        let x = l.x_of(ch.t);
        out.push_str(&format!(
            "<line class=\"mark\" x1=\"{x:.1}\" x2=\"{x:.1}\" y1=\"{}\" y2=\"{:.1}\"/>",
            l.top,
            l.plot_bottom()
        ));
        out.push_str(&format!(
            "<text class=\"marklabel\" x=\"{:.1}\" y=\"{:.1}\">{}</text>",
            x + 4.0,
            l.top + 10.0,
            escape(&ch.label)
        ));
    }
    out.push_str("</g>");

    out.push_str("<g class=\"series\">");
    for s in &spec.series {
        let pts: Vec<String> = s
            .points
            .iter()
            .map(|(t, v)| format!("{:.1},{:.1}", l.x_of(*t), l.y_of(*v)))
            .collect();
        let dash = if s.dash {
            " stroke-dasharray=\"5 4\""
        } else {
            ""
        };
        let stroke = if s.colour.is_empty() {
            "currentColor"
        } else {
            s.colour.as_str()
        };
        out.push_str(&format!(
            "<polyline points=\"{}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{}\"{dash} stroke-linejoin=\"round\"/>",
            pts.join(" "),
            escape(stroke),
            s.width
        ));
        if !s.label.is_empty() && !s.points.is_empty() {
            let i = ((s.points.len() as f64 * s.label_at) as usize).min(s.points.len() - 1);
            let (t, v) = s.points[i];
            out.push_str(&format!(
                "<text class=\"serieslabel\" x=\"{:.1}\" y=\"{:.1}\" fill=\"{}\">{}</text>",
                l.x_of(t) + 4.0,
                l.y_of(v) - 5.0,
                escape(stroke),
                escape(&s.label)
            ));
        }
    }
    out.push_str("</g>");

    let by = l.track_y();
    out.push_str("<g class=\"track\">");
    out.push_str(&format!(
        "<rect class=\"band\" x=\"{}\" y=\"{}\" width=\"0\" height=\"{:.1}\"/>",
        l.left,
        l.top,
        l.plot_height()
    ));
    out.push_str(&format!(
        "<rect class=\"bar-bg\" x=\"{}\" y=\"{by}\" width=\"{:.1}\" height=\"4\" rx=\"2\"/>",
        l.left,
        l.plot_width()
    ));
    out.push_str(&format!(
        "<rect class=\"bar-played\" x=\"{}\" y=\"{by}\" width=\"0\" height=\"4\" rx=\"2\"/>",
        l.left
    ));
    for ch in spec.chapters.iter().skip(1) {
        let x = l.x_of(ch.t);
        out.push_str(&format!(
            "<rect class=\"chapter\" x=\"{:.1}\" y=\"{:.1}\" width=\"2\" height=\"10\"/>",
            x - 1.0,
            by - 3.0
        ));
    }
    out.push_str("</g>");

    out.push_str(&format!(
        "<g class=\"cursor\"><line class=\"peek-line\" x1=\"{}\" x2=\"{}\" y1=\"{}\" y2=\"{:.1}\" visibility=\"hidden\"/></g>",
        l.left,
        l.left,
        l.top,
        l.plot_bottom()
    ));
    out.push_str(&format!(
        "<g class=\"playhead\" transform=\"translate({:.1} 0)\"><line class=\"head\" x1=\"0\" x2=\"0\" y1=\"{}\" y2=\"{:.1}\"/><circle class=\"head-dot\" cx=\"0\" cy=\"{:.1}\" r=\"5\"/><text class=\"head-t\" x=\"4\" y=\"{:.1}\">0.00s</text></g>",
        l.left,
        l.top,
        by + 4.0,
        by + 2.0,
        l.readout_y()
    ));
    out.push_str("</svg>");
    Rendered {
        svg: out,
        layout: l,
    }
}

#[cfg(test)]
pub(crate) mod fixtures {
    use crate::{Chapter, Series, Spec};

    fn series(
        label: &str,
        colour: &str,
        t: &[f64],
        y: &[f64],
        dash: bool,
        width: f64,
        label_at: f64,
    ) -> Series {
        Series {
            label: label.to_owned(),
            colour: colour.to_owned(),
            points: t.iter().copied().zip(y.iter().copied()).collect(),
            dash,
            width,
            label_at,
        }
    }

    fn chapter(t: f64, label: &str) -> Chapter {
        Chapter {
            t,
            label: label.to_owned(),
        }
    }

    /// The demo film on /component/film/: eight frames, one series.
    pub fn demo() -> Spec {
        let times = [0.0, 0.2, 0.45, 0.8, 1.2, 1.7, 2.3, 3.0];
        Spec {
            end: 3.0,
            duration: 3.0,
            ylabel: "progress %".to_owned(),
            chapters: vec![chapter(0.0, "start"), chapter(1.2, "settle")],
            series: vec![series(
                "thumb travel",
                "#009E73",
                &times,
                &[0.0, 8.0, 30.0, 61.0, 84.0, 95.0, 99.0, 100.0],
                false,
                2.4,
                0.85,
            )],
        }
    }

    /// A toggle flight: two series, a dashed one, a label with an angle
    /// bracket to escape, and a long axis that switches the tick step.
    pub fn flight() -> Spec {
        let t: Vec<f64> = (0..=37).map(|i| f64::from(i) * 0.1).collect();
        let ghost: Vec<f64> = t
            .iter()
            .map(|x| (100.0 * (1.0 - (-x * 1.4).exp())).min(100.0))
            .collect();
        let palette: Vec<f64> = t
            .iter()
            .map(|x| (100.0 * (x / 3.0).clamp(0.0, 1.0)).min(100.0))
            .collect();
        Spec {
            end: 3.7,
            duration: 3.7,
            ylabel: "% (opacity, left)".to_owned(),
            chapters: vec![
                chapter(0.0, "flight"),
                chapter(1.5, "abort <early>"),
                chapter(3.03, "settled"),
            ],
            series: vec![
                series("ghost left %", "#0072B2", &t, &ghost, false, 2.4, 0.5),
                series("palette blend %", "#E69F00", &t, &palette, true, 1.8, 0.85),
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::{demo, flight};
    use super::*;

    /// The film's preset box for a spec.
    fn film(spec: &Spec) -> Rendered {
        render(spec, Layout::film(spec.end))
    }

    #[test]
    fn demo_chart_snapshot() {
        insta::assert_snapshot!(film(&demo()).svg);
    }

    #[test]
    fn flight_chart_snapshot() {
        insta::assert_snapshot!(film(&flight()).svg);
    }

    /// Every `fill=` and `stroke=` value in the markup.
    fn paints(svg: &str) -> Vec<String> {
        let mut out = Vec::new();
        for key in ["fill=\"", "stroke=\""] {
            let mut rest = svg;
            while let Some(i) = rest.find(key) {
                let after = &rest[i + key.len()..];
                let end = after.find('"').unwrap();
                out.push(after[..end].to_owned());
                rest = &after[end..];
            }
        }
        out
    }

    #[test]
    fn the_emitter_invents_no_colour_of_its_own() {
        let mut spec = flight();
        for s in &mut spec.series {
            s.colour.clear();
        }
        let svg = film(&spec).svg;
        let found = paints(&svg);
        assert!(!found.is_empty());
        for p in &found {
            assert!(
                p == "none" || p == "currentColor" || p.starts_with("var("),
                "colour literal {p} in the markup"
            );
        }
        // and with colours given, the only literals are exactly those
        let svg = film(&flight()).svg;
        let literals: std::collections::BTreeSet<String> = paints(&svg)
            .into_iter()
            .filter(|p| p != "none" && p != "currentColor" && !p.starts_with("var("))
            .collect();
        let given: std::collections::BTreeSet<String> =
            flight().series.iter().map(|s| s.colour.clone()).collect();
        assert_eq!(literals, given);
    }

    #[test]
    fn the_playhead_is_one_group_at_the_axis_origin() {
        let r = film(&demo());
        assert!(r.svg.contains("<g class=\"playhead\" transform=\"translate(46.0 0)\"><line class=\"head\" x1=\"0\" x2=\"0\""));
        assert!(
            r.svg
                .contains("<text class=\"head-t\" x=\"4\" y=\"250.0\">0.00s</text>")
        );
        assert_eq!(r.layout, Layout::film(3.0));
    }

    #[test]
    fn text_is_escaped_and_ticks_follow_the_axis_length() {
        let svg = film(&flight()).svg;
        assert!(svg.contains("abort &lt;early&gt;"));
        assert!(svg.contains(">3.5s</text>"));
        assert!(!svg.contains(">4s</text>"));
        let mut long = demo();
        long.end = 8.0;
        let svg = film(&long).svg;
        assert!(svg.contains(">8s</text>") && !svg.contains(">0.5s</text>"));
    }

    #[test]
    fn the_readout_sits_between_the_tick_labels_and_the_track() {
        let l = Layout::film(3.0);
        let svg = film(&demo()).svg;
        assert!(svg.contains(&format!(
            "y=\"{:.1}\" text-anchor=\"middle\">0s</text>",
            l.axis_label_y()
        )));
        assert!(l.readout_y() > l.axis_label_y() + 10.0);
        assert!(l.readout_y() + 2.0 < l.track_y() - 3.0);
    }

    #[test]
    fn a_wider_box_scales_the_axis_without_touching_the_spec() {
        let mut wide = Layout::film(3.0);
        wide.width = 1200.0;
        let r = render(&demo(), wide);
        assert!(
            r.svg
                .starts_with("<svg class=\"chart\" part=\"chart\" viewBox=\"0 0 1200 268\"")
        );
        assert_eq!(r.layout.x_of(3.0), 1186.0);
    }
}
