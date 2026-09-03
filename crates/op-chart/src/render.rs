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
/// Minimum vertical distance between end labels (12 px text plus a gap).
const LABEL_GAP: f64 = 14.0;
/// Markers per series at most, spread over the samples.
const MAX_MARKERS: usize = 8;

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
    // end labels first, so their vertical placement can be settled together
    let wanted: Vec<f64> = spec
        .series
        .iter()
        .filter(|s| !s.label.is_empty())
        .filter_map(|s| s.points.last().map(|(_, v)| l.y_of(*v) - 5.0))
        .collect();
    let mut placed =
        crate::labels::spread(&wanted, LABEL_GAP, l.top + 10.0, l.plot_bottom() - 3.0).into_iter();
    for s in &spec.series {
        let pts: Vec<String> = s
            .points
            .iter()
            .map(|(t, v)| format!("{:.1},{:.1}", l.x_of(*t), l.y_of(*v)))
            .collect();
        out.push_str(&format!(
            "<polyline class=\"series-{}\" points=\"{}\" fill=\"none\" stroke-width=\"{}\" stroke-linejoin=\"round\"/>",
            s.index,
            pts.join(" "),
            s.width
        ));
        let labelled = !s.label.is_empty() && !s.points.is_empty();
        // markers at sparse samples: always in the markup, shown by the
        // stylesheet for unlabelled series and in forced-colours mode
        let shown = if labelled { "" } else { " shown" };
        for i in crate::labels::marker_samples(s.points.len(), MAX_MARKERS) {
            let (t, v) = s.points[i];
            out.push_str(&format!(
                "<path class=\"marker series-{}{shown}\" transform=\"translate({:.1} {:.1})\" d=\"{}\"/>",
                s.index,
                l.x_of(t),
                l.y_of(v),
                crate::labels::marker_path(s.index)
            ));
        }
        if labelled {
            let (t, _) = s.points[s.points.len() - 1];
            let x = l.x_of(t);
            let y = placed.next().unwrap_or(l.top + 10.0);
            out.push_str(&format!(
                "<line class=\"swatch series-{}\" x1=\"{:.1}\" x2=\"{:.1}\" y1=\"{y:.1}\" y2=\"{y:.1}\"/>",
                s.index,
                x - 16.0,
                x - 4.0
            ));
            out.push_str(&format!(
                "<text class=\"endlabel\" x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"end\">{}</text>",
                x - 20.0,
                y + 4.0,
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

    fn series(label: &str, index: usize, t: &[f64], y: &[f64], width: f64) -> Series {
        Series {
            label: label.to_owned(),
            index,
            points: t.iter().copied().zip(y.iter().copied()).collect(),
            width,
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
                2,
                &times,
                &[0.0, 8.0, 30.0, 61.0, 84.0, 95.0, 99.0, 100.0],
                2.4,
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
                series("ghost left %", 3, &t, &ghost, 2.4),
                series("palette blend %", 1, &t, &palette, 1.8),
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
    fn the_emitter_writes_no_colour_at_all() {
        let svg = film(&flight()).svg;
        let found = paints(&svg);
        assert!(!found.is_empty());
        for p in &found {
            assert!(
                p == "none",
                "paint {p} in the markup: colours belong to the stylesheet"
            );
        }
        // every series is addressed by its class, line and label alike
        assert!(svg.contains("<polyline class=\"series-3\""));
        assert!(svg.contains("<polyline class=\"series-1\""));
        assert!(svg.contains("<line class=\"swatch series-3\""));
        assert!(svg.contains("<line class=\"swatch series-1\""));
        assert!(svg.contains("<text class=\"endlabel\""));
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

    #[test]
    fn end_labels_are_kept_apart_and_unlabelled_series_show_markers() {
        // two series ending at the same value would collide; the emitter spreads them
        let mut spec = demo();
        let mut twin = spec.series[0].clone();
        twin.index = 4;
        twin.label = "twin".to_owned();
        spec.series.push(twin);
        let svg = film(&spec).svg;
        let ys: Vec<f64> = svg
            .match_indices("<text class=\"endlabel\"")
            .map(|(i, _)| {
                let rest = &svg[i..];
                let y = rest
                    .split("y=\"")
                    .nth(1)
                    .unwrap()
                    .split('"')
                    .next()
                    .unwrap();
                y.parse::<f64>().unwrap()
            })
            .collect();
        assert_eq!(ys.len(), 2);
        assert!((ys[0] - ys[1]).abs() >= 14.0, "{ys:?}");
        // labelled series carry hidden markers; an unlabelled one shows them
        assert!(svg.contains("class=\"marker series-2\" "));
        let mut spec = demo();
        spec.series[0].label.clear();
        let svg = film(&spec).svg;
        assert!(svg.contains("class=\"marker series-2 shown\""));
        assert!(!svg.contains("endlabel"));
        assert_eq!(svg.matches("class=\"marker series-2 shown\"").count(), 8);
    }
}
