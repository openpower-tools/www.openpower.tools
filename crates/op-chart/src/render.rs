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

/// The value gridlines the percent scale draws: its quarters, which no
/// nice-tick rule would choose (25 is not 1, 2 or 5 times a power of ten)
/// and which every chart on the site has been read at.
const PERCENT_TICKS: [f64; 5] = [0.0, 25.0, 50.0, 75.0, 100.0];

/// The value gridlines drawn and labelled on the left axis: the percent
/// scale's quarters, and d3's nice ticks over any other domain.
fn value_ticks(l: &Layout) -> Vec<f64> {
    if (l.y_min, l.y_max) == crate::layout::PERCENT {
        return PERCENT_TICKS.to_vec();
    }
    crate::ticks::ticks(l.y_min, l.y_max, 4.0)
}

/// A gridline's label, with as many decimals as its step needs: a domain of
/// fractions must not print the same digit at every line. The percent
/// scale's step of 25 needs none, which is what the film has always drawn.
fn tick_text(v: f64, step: f64) -> String {
    let places = if step >= 1.0 || step <= 0.0 {
        0
    } else {
        (-step.log10().floor()) as usize
    }
    .min(6);
    format!("{v:.places$}")
}

/// Minimum vertical distance between end labels (12 px text plus a gap).
const LABEL_GAP: f64 = 14.0;
/// Markers per series at most, spread over the samples.
const MAX_MARKERS: usize = 8;

/// Keep min and max per pixel column, and only when a series carries more
/// present points than the plot is wide: a chart of tens or hundreds of
/// samples is drawn exactly as it was given. Within a column the two kept
/// points stay in x order, and a gap always ends the column it falls in, so
/// the line still breaks where the data does.
fn decimate(points: &[Option<(f64, f64)>], l: &Layout) -> Vec<Option<(f64, f64)>> {
    let present = points.iter().flatten().count();
    if (present as f64) <= l.plot_width() {
        return points.to_vec();
    }
    /// One column's kept points, in the order they were sampled.
    fn flush(
        out: &mut Vec<Option<(f64, f64)>>,
        lo: &mut Option<(usize, (f64, f64))>,
        hi: &mut Option<(usize, (f64, f64))>,
    ) {
        let (Some(a), Some(b)) = (lo.take(), hi.take()) else {
            return;
        };
        let (first, second) = if a.0 <= b.0 { (a, b) } else { (b, a) };
        out.push(Some(first.1));
        if first.0 != second.0 {
            out.push(Some(second.1));
        }
    }

    let mut out = Vec::with_capacity(points.len());
    // the column being gathered: its pixel and the lowest and highest
    // points in it, each with the position that keeps them in x order
    let mut column: Option<f64> = None;
    let mut lo: Option<(usize, (f64, f64))> = None;
    let mut hi: Option<(usize, (f64, f64))> = None;
    for (i, p) in points.iter().enumerate() {
        let Some((t, v)) = *p else {
            flush(&mut out, &mut lo, &mut hi);
            column = None;
            out.push(None);
            continue;
        };
        let c = l.x_of(t).floor();
        if column != Some(c) {
            flush(&mut out, &mut lo, &mut hi);
            column = Some(c);
        }
        if lo.is_none_or(|(_, (_, w))| v < w) {
            lo = Some((i, (t, v)));
        }
        if hi.is_none_or(|(_, (_, w))| v > w) {
            hi = Some((i, (t, v)));
        }
    }
    flush(&mut out, &mut lo, &mut hi);
    out
}

/// What one series draws: its path, the points its cues sit on, and which
/// of those have no present neighbour on either side. A sample alone is a
/// zero-length segment, which butt caps draw as nothing, so it carries a
/// marker to be seen at all.
struct Drawn {
    d: String,
    present: Vec<(f64, f64)>,
    alone: Vec<usize>,
}

/// Read a series' drawn points into the path and the cues.
fn drawn_of(points: &[Option<(f64, f64)>], l: &Layout) -> Drawn {
    let mut present = Vec::new();
    let mut alone = Vec::new();
    for (i, p) in points.iter().enumerate() {
        let Some(point) = *p else { continue };
        let has = |k: Option<usize>| {
            k.and_then(|k| points.get(k))
                .is_some_and(|neighbour| neighbour.is_some())
        };
        if !has(i.checked_sub(1)) && !has(Some(i + 1)) {
            alone.push(present.len());
        }
        present.push(point);
    }
    Drawn {
        d: path_d(points, l),
        present,
        alone,
    }
}

/// The `d` of a series: one move-to per run of present points and a
/// line-to for every point after it. A run of one emits a zero-length
/// segment, so a sample alone between two gaps is still a drawn thing.
fn path_d(points: &[Option<(f64, f64)>], l: &Layout) -> String {
    let mut d = String::new();
    for run in points.split(|p| p.is_none()) {
        let mut xy = run.iter().flatten().map(|(t, v)| (l.x_of(*t), l.y_of(*v)));
        let Some((x, y)) = xy.next() else {
            continue;
        };
        if !d.is_empty() {
            d.push(' ');
        }
        d.push_str(&format!("M {x:.1} {y:.1}"));
        let mut alone = true;
        for (x, y) in xy {
            alone = false;
            d.push_str(&format!(" L {x:.1} {y:.1}"));
        }
        if alone {
            d.push_str(&format!(" L {x:.1} {y:.1}"));
        }
    }
    d
}

/// Draw `spec` in the box and scales `l` describes. The caller chooses the
/// layout (the film uses [`Layout::film`]) so the element and the page
/// build can size a chart without the renderer knowing about either.
pub fn render(spec: &Spec, l: Layout) -> Rendered {
    // the box says how big the chart is, the spec says what it is a chart
    // of: the value domain travels with the data, not with the box
    let l = l.with_y(spec.y.0, spec.y.1);
    let mut out = format!(
        "<svg class=\"chart\" part=\"chart\" viewBox=\"0 0 {} {}\" tabindex=\"0\" role=\"slider\" aria-label=\"playhead\" aria-valuemin=\"0\" aria-valuemax=\"{:.2}\" aria-valuenow=\"0\" aria-valuetext=\"0.00 seconds\">",
        l.width, l.height, spec.duration
    );
    // in a box measured in CSS px a stylesheet may still scale the svg, so
    // every stroked shape holds its width; the film's fixed box does not
    let ns = if l.non_scaling {
        " vector-effect=\"non-scaling-stroke\""
    } else {
        ""
    };

    out.push_str("<g class=\"axes\">");
    let values = value_ticks(&l);
    // the step the labels are written to; a lone gridline needs none
    let vstep = values
        .windows(2)
        .next()
        .map_or(1.0, |pair| pair[1] - pair[0]);
    for (i, v) in values.iter().enumerate() {
        let y = l.y_of(*v);
        // the domain's outer pair of gridlines carries the heavier stroke
        let w = if i == 0 || i + 1 == values.len() {
            1.0
        } else {
            0.5
        };
        out.push_str(&format!(
            "<line class=\"grid\" x1=\"{}\" x2=\"{:.1}\" y1=\"{y:.1}\" y2=\"{y:.1}\" stroke-width=\"{w}\"{ns}/>",
            l.left,
            l.width - l.right
        ));
        out.push_str(&format!(
            "<text class=\"axis\" x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"end\">{}</text>",
            l.left - 6.0,
            y + 4.0,
            tick_text(*v, vstep)
        ));
    }
    let step = if l.end <= 5.0 { 0.5 } else { 1.0 };
    let mut t = 0.0;
    // every second time label is `alt`, so a narrow box can drop half of
    // them (a container query in the consumer's stylesheet) and keep the
    // axis readable without the emitter knowing the rendered width
    let mut k = 0usize;
    while t <= l.end + 1e-9 {
        let x = l.x_of(t);
        out.push_str(&format!(
            "<line class=\"tick\" x1=\"{x:.1}\" x2=\"{x:.1}\" y1=\"{:.1}\" y2=\"{:.1}\"{ns}/>",
            l.plot_bottom(),
            l.plot_bottom() + 4.0
        ));
        let alt = if k % 2 == 1 { " alt" } else { "" };
        out.push_str(&format!(
            "<text class=\"axis tick-label{alt}\" x=\"{x:.1}\" y=\"{:.1}\" text-anchor=\"middle\">{t}s</text>",
            l.axis_label_y()
        ));
        t += step;
        k += 1;
    }
    let mid_y = (l.plot_bottom() + l.top) / 2.0;
    out.push_str(&format!(
        "<text class=\"axis\" x=\"14\" y=\"{mid_y:.1}\" transform=\"rotate(-90 14 {mid_y:.1})\" text-anchor=\"middle\">{}</text>",
        escape(&spec.ylabel)
    ));
    out.push_str("</g>");

    // the chapter rules and their labels sit in a `chapters` group of their
    // own, as the ticks on the track below do, so one rule in the
    // consumer's stylesheet hides every chapter cue in a narrow box
    out.push_str("<g class=\"marks\"><g class=\"chapters\">");
    for ch in spec.chapters.iter().skip(1) {
        let x = l.x_of(ch.t);
        out.push_str(&format!(
            "<line class=\"mark\" x1=\"{x:.1}\" x2=\"{x:.1}\" y1=\"{}\" y2=\"{:.1}\"{ns}/>",
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
    out.push_str("</g></g>");

    out.push_str("<g class=\"series\">");
    // what each series actually draws: the samples it was given, thinned
    // per pixel column when it carries more of them than the plot is wide,
    // as a path and as the present points the cues sit on
    let drawn: Vec<Drawn> = spec
        .series
        .iter()
        .map(|s| drawn_of(&decimate(&s.points, &l), &l))
        .collect();
    // end labels first, so their vertical placement can be settled together
    let wanted: Vec<f64> = spec
        .series
        .iter()
        .zip(&drawn)
        .filter(|(s, _)| !s.label.is_empty())
        .filter_map(|(_, dr)| dr.present.last().map(|(_, v)| l.y_of(*v) - 5.0))
        .collect();
    let mut placed =
        crate::labels::spread(&wanted, LABEL_GAP, l.top + 10.0, l.plot_bottom() - 3.0).into_iter();
    for (s, dr) in spec.series.iter().zip(&drawn) {
        // the class names the palette slot and the part exports it, so a
        // page can restyle one series through the element's boundary
        out.push_str(&format!(
            "<path class=\"series-{}\" part=\"series-{}\" d=\"{}\" fill=\"none\" stroke-width=\"{}\" stroke-linejoin=\"round\"{ns}/>",
            s.index, s.index, dr.d, s.width
        ));
        let labelled = !s.label.is_empty() && !dr.present.is_empty();
        // markers at sparse samples: always in the markup, shown by the
        // stylesheet for unlabelled series and in forced-colours mode. A
        // sample with no present neighbour is shown whatever the series
        // does: it has no line of its own to be seen by.
        let mut at = crate::labels::marker_samples(dr.present.len(), MAX_MARKERS);
        for k in &dr.alone {
            if !at.contains(k) {
                at.push(*k);
            }
        }
        // a few indices: an insertion sort keeps the generic sort out of the wasm
        for k in 1..at.len() {
            let mut j = k;
            while j > 0 && at[j - 1] > at[j] {
                at.swap(j - 1, j);
                j -= 1;
            }
        }
        for i in at {
            let (t, v) = dr.present[i];
            let shown = if labelled && !dr.alone.contains(&i) {
                ""
            } else {
                " shown"
            };
            out.push_str(&format!(
                "<path class=\"marker series-{}{shown}\" transform=\"translate({:.1} {:.1})\" d=\"{}\"{ns}/>",
                s.index,
                l.x_of(t),
                l.y_of(v),
                crate::labels::marker_path(s.index)
            ));
        }
        if labelled {
            let (t, _) = dr.present[dr.present.len() - 1];
            let x = l.x_of(t);
            let y = placed.next().unwrap_or(l.top + 10.0);
            out.push_str(&format!(
                "<line class=\"swatch series-{}\" x1=\"{:.1}\" x2=\"{:.1}\" y1=\"{y:.1}\" y2=\"{y:.1}\"{ns}/>",
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
        "<rect class=\"band\" x=\"{}\" y=\"{}\" width=\"0\" height=\"{:.1}\"{ns}/>",
        l.left,
        l.top,
        l.plot_height()
    ));
    out.push_str(&format!(
        "<rect class=\"bar-bg\" x=\"{}\" y=\"{by}\" width=\"{:.1}\" height=\"4\" rx=\"2\"{ns}/>",
        l.left,
        l.plot_width()
    ));
    out.push_str(&format!(
        "<rect class=\"bar-played\" x=\"{}\" y=\"{by}\" width=\"0\" height=\"4\" rx=\"2\"{ns}/>",
        l.left
    ));
    out.push_str("<g class=\"chapters\">");
    for ch in spec.chapters.iter().skip(1) {
        let x = l.x_of(ch.t);
        out.push_str(&format!(
            "<rect class=\"chapter\" x=\"{:.1}\" y=\"{:.1}\" width=\"2\" height=\"10\"{ns}/>",
            x - 1.0,
            by - 3.0
        ));
    }
    out.push_str("</g></g>");

    out.push_str(&format!(
        "<g class=\"cursor\"><line class=\"peek-line\" x1=\"{}\" x2=\"{}\" y1=\"{}\" y2=\"{:.1}\" visibility=\"hidden\"{ns}/></g>",
        l.left,
        l.left,
        l.top,
        l.plot_bottom()
    ));
    out.push_str(&format!(
        "<g class=\"playhead\" part=\"playhead\" transform=\"translate({:.1} 0)\"><line class=\"head\" x1=\"0\" x2=\"0\" y1=\"{}\" y2=\"{:.1}\"{ns}/><circle class=\"head-dot\" cx=\"0\" cy=\"{:.1}\" r=\"5\"{ns}/><text class=\"head-t\" x=\"4\" y=\"{:.1}\">0.00s</text></g>",
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
            points: t.iter().copied().zip(y.iter().copied()).map(Some).collect(),
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
            y: crate::layout::PERCENT,
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
            y: crate::layout::PERCENT,
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

    /// Nothing sampled: the axes, the track and the playhead still draw.
    pub fn empty() -> Spec {
        Spec {
            end: 3.0,
            duration: 3.0,
            y: crate::layout::PERCENT,
            ylabel: "progress %".to_owned(),
            chapters: vec![chapter(0.0, "start")],
            series: vec![Series {
                label: "nothing yet".to_owned(),
                index: 1,
                points: Vec::new(),
                width: 2.0,
            }],
        }
    }

    /// A single sample, which draws as a zero-length segment.
    pub fn one_point() -> Spec {
        Spec {
            series: vec![series("first frame", 1, &[0.6], &[42.0], 2.0)],
            ..empty()
        }
    }

    /// A series that stops and starts again, with one sample alone between
    /// two gaps.
    pub fn gaps() -> Spec {
        Spec {
            series: vec![Series {
                label: String::new(),
                index: 2,
                points: vec![
                    Some((0.0, 10.0)),
                    Some((0.4, 40.0)),
                    Some((0.8, 35.0)),
                    None,
                    Some((1.2, 70.0)),
                    None,
                    None,
                    Some((2.4, 55.0)),
                    Some((3.0, 90.0)),
                ],
                width: 2.0,
            }],
            ..empty()
        }
    }

    /// Values in a repeating ramp, exact as integers so a test can compute
    /// the same extremes with whole numbers.
    pub fn ramp(n: usize, end: f64) -> Vec<Option<(f64, f64)>> {
        (0..n)
            .map(|i| {
                let t = if n > 1 {
                    i as f64 * end / (n - 1) as f64
                } else {
                    0.0
                };
                Some((t, ((i * 37) % 101) as f64))
            })
            .collect()
    }

    /// Far more samples than the pre-render's plot is wide: about three to
    /// each of its 581 pixel columns, so the emitter really does thin them
    /// down to the two the column needs.
    pub fn many() -> Spec {
        Spec {
            series: vec![Series {
                label: "every sample".to_owned(),
                index: 4,
                points: ramp(1800, 3.0),
                width: 2.0,
            }],
            ..empty()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::{demo, empty, flight, gaps, many, one_point, ramp};
    use super::*;
    use crate::Series;
    use std::collections::{BTreeMap, BTreeSet};

    /// The film's preset box for a spec.
    fn film(spec: &Spec) -> Rendered {
        render(spec, Layout::film(spec.end))
    }

    /// The pre-render's box: 640 by 240 CSS px, the width TanStack uses.
    fn sized(spec: &Spec) -> Rendered {
        render(spec, Layout::sized(640.0, 240.0, spec.end))
    }

    #[test]
    fn demo_chart_snapshot() {
        insta::assert_snapshot!(film(&demo()).svg);
    }

    #[test]
    fn flight_chart_snapshot() {
        insta::assert_snapshot!(film(&flight()).svg);
    }

    #[test]
    fn sized_empty_snapshot() {
        insta::assert_snapshot!(sized(&empty()).svg);
    }

    #[test]
    fn sized_one_point_snapshot() {
        insta::assert_snapshot!(sized(&one_point()).svg);
    }

    #[test]
    fn sized_gaps_snapshot() {
        insta::assert_snapshot!(sized(&gaps()).svg);
    }

    #[test]
    fn sized_many_points_snapshot() {
        insta::assert_snapshot!(sized(&many()).svg);
    }

    #[test]
    fn sized_film_chart_snapshot() {
        insta::assert_snapshot!(sized(&flight()).svg);
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

    /// Colours a renderer might reach for and this one may not.
    const NAMED_COLOURS: &[&str] = &[
        "black",
        "white",
        "red",
        "green",
        "blue",
        "grey",
        "gray",
        "currentColor",
        "CanvasText",
        "Highlight",
    ];

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
            assert!(!p.contains('#') && !p.contains("rgb("), "paint {p}");
            assert!(!NAMED_COLOURS.contains(&p.as_str()), "paint {p}");
        }
        // every series is addressed by its class, line and label alike
        assert!(svg.contains("<path class=\"series-3\""));
        assert!(svg.contains("<path class=\"series-1\""));
        assert!(svg.contains("<line class=\"swatch series-3\""));
        assert!(svg.contains("<line class=\"swatch series-1\""));
        assert!(svg.contains("<text class=\"endlabel\""));
    }

    #[test]
    fn the_playhead_is_one_group_at_the_axis_origin() {
        let r = film(&demo());
        assert!(r.svg.contains("<g class=\"playhead\" part=\"playhead\" transform=\"translate(46.0 0)\"><line class=\"head\" x1=\"0\" x2=\"0\""));
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

    /// Every time label carries `tick-label`, and every second one `alt`,
    /// starting with the second: a narrow box drops the alternates and the
    /// axis still begins with a label at zero.
    #[test]
    fn time_labels_alternate_so_a_narrow_box_can_drop_half_of_them() {
        for spec in [demo(), flight()] {
            let svg = film(&spec).svg;
            let labels: Vec<&str> = svg
                .match_indices("<text class=\"axis tick-label")
                .map(|(i, _)| {
                    let rest = &svg[i + "<text class=\"axis ".len()..];
                    &rest[..rest.find('"').expect("a closing quote")]
                })
                .collect();
            // one per tick, and no time label escapes the class
            let ticks = svg.matches("<line class=\"tick\"").count();
            assert_eq!(labels.len(), ticks, "{labels:?}");
            assert!(ticks >= 7, "only {ticks} ticks");
            for (k, class) in labels.iter().enumerate() {
                let want = if k % 2 == 1 {
                    "tick-label alt"
                } else {
                    "tick-label"
                };
                assert_eq!(*class, want, "label {k} of {labels:?}");
            }
            // the value-axis labels are not time labels and keep their class
            assert!(svg.contains("<text class=\"axis\" x=\"40.0\""));
        }
        // and the chapter cues are grouped so one rule hides them all
        let svg = film(&flight()).svg;
        assert!(svg.contains("<g class=\"marks\"><g class=\"chapters\"><line class=\"mark\""));
        assert!(svg.contains("<g class=\"chapters\"><rect class=\"chapter\""));
        assert_eq!(svg.matches("<g class=\"chapters\">").count(), 2);
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

    /// A page styles what it can address: decision 6 exports every series
    /// and the playhead by the name their class already carries.
    #[test]
    fn every_series_line_and_the_playhead_are_exported_as_parts() {
        let svg = film(&flight()).svg;
        assert!(svg.starts_with("<svg class=\"chart\" part=\"chart\""));
        assert!(svg.contains("<path class=\"series-3\" part=\"series-3\""));
        assert!(svg.contains("<path class=\"series-1\" part=\"series-1\""));
        // one per series line: the markers share the class and take no part
        assert_eq!(svg.matches("part=\"series-").count(), flight().series.len());
        assert!(svg.contains("class=\"marker series-3"));
        assert!(svg.contains("<g class=\"playhead\" part=\"playhead\""));
        assert_eq!(svg.matches("part=\"playhead\"").count(), 1);
        // every part names a class the stylesheet can reach as well
        for (i, _) in svg.match_indices("part=\"") {
            let rest = &svg[i + "part=\"".len()..];
            let name = &rest[..rest.find('"').expect("a closing quote")];
            let known = ["chart", "playhead"].contains(&name)
                || name
                    .strip_prefix("series-")
                    .is_some_and(|n| n.parse::<usize>().is_ok());
            assert!(known, "the markup exports an unknown part {name}");
        }
    }

    /// The labels on the value axis, in the order they are drawn.
    fn value_labels(svg: &str, l: &Layout) -> Vec<String> {
        let head = format!("<text class=\"axis\" x=\"{:.1}\" ", l.left - 6.0);
        svg.split(&head)
            .skip(1)
            .map(|rest| {
                let text = rest.split_once('>').expect("the text node").1;
                text.split_once('<').expect("a closing tag").0.to_owned()
            })
            .collect()
    }

    /// The `y` of every point in a series path's `d`.
    fn path_ys(svg: &str, head: &str) -> Vec<f64> {
        attribute(svg, head, "d")
            .split(['M', 'L'])
            .filter_map(|point| {
                let (_, y) = point.trim().split_once(' ')?;
                y.trim().parse::<f64>().ok()
            })
            .collect()
    }

    /// A series of milliseconds: nothing about it fits the percent scale,
    /// and the block it came from would have said so in its `y`.
    fn tall() -> Spec {
        let t = [0.0, 0.75, 1.5, 2.25, 3.0];
        let v = [0.0, 250.0, 400.0, 900.0, 1000.0];
        Spec {
            y: (-40.0, 1040.0),
            series: vec![Series {
                label: String::new(),
                index: 1,
                points: t.iter().copied().zip(v).map(Some).collect(),
                width: 2.0,
            }],
            ..empty()
        }
    }

    #[test]
    fn a_series_outside_the_percent_scale_is_drawn_on_its_own_domain() {
        let l = Layout::sized(640.0, 240.0, 3.0);
        let spec = tall();
        let svg = render(&spec, l).svg;
        // every point sits where the spec's own domain puts it, and none of
        // them is at an edge of the plot
        let scale = l.with_y(spec.y.0, spec.y.1);
        let ys = path_ys(&svg, "<path class=\"series-1\"");
        let want: Vec<f64> = spec.series[0]
            .points
            .iter()
            .flatten()
            .map(|(_, v)| {
                format!("{:.1}", scale.y_of(*v))
                    .parse()
                    .expect("a number the emitter wrote")
            })
            .collect();
        assert_eq!(ys, want, "{svg}");
        let lo = ys.iter().copied().fold(f64::INFINITY, f64::min);
        let hi = ys.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        assert!(hi < l.plot_bottom() && lo > l.top, "{ys:?}");
        // and they use the plot rather than a sliver of it
        assert!(hi - lo > 0.9 * l.plot_height(), "{ys:?}");
        // the axis is labelled in the data's own numbers, not in percent
        assert_eq!(
            value_labels(&svg, &l),
            ["0", "200", "400", "600", "800", "1000"]
        );
        // the same series on the percent scale is the defect this fixes:
        // every point above 106 clamps to the top edge and the line is flat
        let flat = render(
            &Spec {
                y: crate::layout::PERCENT,
                ..tall()
            },
            l,
        )
        .svg;
        let ys = path_ys(&flat, "<path class=\"series-1\"");
        assert!(ys.iter().filter(|y| **y == l.top).count() >= 3, "{ys:?}");
        assert_eq!(value_labels(&flat, &l), ["0", "25", "50", "75", "100"]);
    }

    #[test]
    fn a_domain_of_fractions_labels_every_gridline_differently() {
        let l = Layout::sized(640.0, 240.0, 3.0);
        let svg = render(
            &Spec {
                y: (0.0, 1.0),
                ..tall()
            },
            l,
        )
        .svg;
        let labels = value_labels(&svg, &l);
        assert_eq!(labels, ["0.0", "0.2", "0.4", "0.6", "0.8", "1.0"]);
        // the step decides the decimals: the percent scale's 25 needs none
        assert_eq!(tick_text(25.0, 25.0), "25");
        assert_eq!(tick_text(0.25, 0.25), "0.2");
        assert_eq!(tick_text(1.234, 0.01), "1.23");
        // a lone gridline has no step to read and is written whole
        assert_eq!(tick_text(7.0, 0.0), "7");
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

    /// The value of `attr` on the first element opening with `head`.
    fn attribute(svg: &str, head: &str, attr: &str) -> String {
        let i = svg
            .find(head)
            .unwrap_or_else(|| panic!("no {head} in the markup"));
        let key = format!("{attr}=\"");
        let rest = &svg[i..];
        let j = rest.find(&key).expect("the attribute");
        let after = &rest[j + key.len()..];
        after[..after.find('"').expect("a closing quote")].to_owned()
    }

    #[test]
    fn a_path_breaks_at_every_gap_and_a_lone_sample_draws_a_zero_length_segment() {
        let svg = sized(&gaps()).svg;
        let d = attribute(&svg, "<path class=\"series-2\"", "d");
        // three runs of present samples, so three move-tos
        let runs: Vec<&str> = d.split("M ").skip(1).map(str::trim).collect();
        assert_eq!(runs.len(), 3, "{d}");
        let lone: Vec<&&str> = runs
            .iter()
            .filter(|r| {
                let ends: Vec<&str> = r.split(" L ").collect();
                ends.len() == 2 && ends[0] == ends[1]
            })
            .collect();
        assert_eq!(lone.len(), 1, "the sample between two gaps: {d}");
        // and it carries a marker, since butt caps draw a zero-length
        // segment as nothing at all
        let l = Layout::sized(640.0, 240.0, gaps().end);
        assert!(
            svg.contains(&format!(
                "<path class=\"marker series-2 shown\" transform=\"translate({:.1} {:.1})\"",
                l.x_of(1.2),
                l.y_of(70.0)
            )),
            "{svg}"
        );
    }

    #[test]
    fn a_single_sample_is_one_shown_marker_at_its_own_point() {
        let svg = sized(&one_point()).svg;
        assert_eq!(svg.matches("class=\"marker").count(), 1);
        assert_eq!(svg.matches("class=\"marker series-1 shown\"").count(), 1);
        let l = Layout::sized(640.0, 240.0, one_point().end);
        assert!(
            svg.contains(&format!(
                "<path class=\"marker series-1 shown\" transform=\"translate({:.1} {:.1})\"",
                l.x_of(0.6),
                l.y_of(42.0)
            )),
            "{svg}"
        );
        // the series is still a path, with the zero-length segment on it
        let d = attribute(&svg, "<path class=\"series-1\"", "d");
        let ends: Vec<&str> = d.split(" L ").collect();
        assert_eq!(ends.len(), 2, "{d}");
        assert_eq!(ends[0].trim_start_matches("M "), ends[1]);
        // an empty series is an empty path, and draws no cue at all
        let svg = sized(&empty()).svg;
        assert_eq!(attribute(&svg, "<path class=\"series-1\"", "d"), "");
        assert!(!svg.contains("marker") && !svg.contains("endlabel"));
    }

    #[test]
    fn only_a_sized_box_marks_its_strokes_non_scaling() {
        assert!(!film(&flight()).svg.contains("vector-effect"));
        let svg = sized(&flight()).svg;
        let mut shapes = 0;
        for element in svg.split('<') {
            let shape = ["line", "path", "rect", "circle"]
                .iter()
                .any(|tag| element.starts_with(tag));
            if shape {
                shapes += 1;
                assert!(
                    element.contains("vector-effect=\"non-scaling-stroke\""),
                    "{element}"
                );
            } else {
                // the text nodes take their halo from the stylesheet and
                // scale with their own glyphs
                assert!(!element.contains("vector-effect"), "{element}");
            }
        }
        assert!(shapes > 20, "only {shapes} shapes in the markup");
    }

    #[test]
    fn thinning_starts_one_sample_past_the_plot_width_and_keeps_each_columns_extremes() {
        let l = Layout::sized(640.0, 240.0, 1.0);
        let width = l.plot_width() as usize;
        assert_eq!(width, 580);
        // exactly as many samples as the plot is wide: nothing is thinned
        let exact = ramp(width, l.end);
        assert_eq!(decimate(&exact, &l), exact);
        // the reference, computed here in whole numbers: the ramp's values
        // are integers, so every comparison below is exact
        let column = |t: f64| l.x_of(t).floor() as i64;
        let extremes = |points: &[Option<(f64, f64)>]| {
            let mut want: BTreeMap<i64, (i64, i64)> = BTreeMap::new();
            for (t, v) in points.iter().flatten() {
                let seen = want.entry(column(*t)).or_insert((*v as i64, *v as i64));
                seen.0 = seen.0.min(*v as i64);
                seen.1 = seen.1.max(*v as i64);
            }
            want
        };
        let check = |points: &[Option<(f64, f64)>]| {
            let out = decimate(points, &l);
            let want = extremes(points);
            let mut got: BTreeMap<i64, Vec<i64>> = BTreeMap::new();
            for (t, v) in out.iter().flatten() {
                got.entry(column(*t)).or_default().push(*v as i64);
            }
            assert_eq!(got.len(), want.len());
            for (c, kept) in &got {
                assert!(kept.len() <= 2, "column {c} kept {kept:?}");
                let (lo, hi) = want[c];
                assert_eq!(*kept.iter().min().expect("a sample"), lo, "column {c}");
                assert_eq!(*kept.iter().max().expect("a sample"), hi, "column {c}");
            }
            // and the samples stay in x order
            let xs: Vec<f64> = out.iter().flatten().map(|(t, _)| l.x_of(*t)).collect();
            assert!(xs.windows(2).all(|w| w[0] <= w[1]));
            out
        };
        // one sample past the plot width the thinning is on. Evenly spaced,
        // each of these still lands in a pixel column of its own (the plot
        // spans 581 whole pixels, edge to edge), so every one survives
        let more = ramp(width + 1, l.end);
        let out = check(&more);
        assert_eq!(out.len(), more.len());
        // where columns really do hold several samples, the thinning drops
        // everything between the lowest and the highest
        let dense = ramp(width * 3, l.end);
        let out = check(&dense);
        assert!(out.len() < dense.len(), "{} of {}", out.len(), dense.len());
        assert!(out.len() <= 2 * (width + 1));
        // a gap still breaks the line
        let mut broken = dense.clone();
        broken[300] = None;
        assert_eq!(
            decimate(&broken, &l).iter().filter(|p| p.is_none()).count(),
            1
        );
    }

    /// Every class this emitter writes.
    const EMITTED_CLASSES: &[&str] = &[
        "chart",
        "axes",
        "grid",
        "axis",
        "tick",
        "tick-label",
        "alt",
        "marks",
        "chapters",
        "mark",
        "marklabel",
        "series",
        "marker",
        "shown",
        "swatch",
        "endlabel",
        "track",
        "band",
        "bar-bg",
        "bar-played",
        "chapter",
        "cursor",
        "peek-line",
        "playhead",
        "head",
        "head-dot",
        "head-t",
    ];

    /// Names the research note reserves for parts the element will draw
    /// later; allowed here, but nothing writes them yet.
    const RESERVED_CLASSES: &[&str] = &[
        "mark-label",
        "playhead-label",
        "peek",
        "plot",
        "background",
        "readout",
    ];

    /// The z-order the emitter writes its groups in. The note's list
    /// (grid, axes, series, marks, band, chapters, peek, playhead, labels)
    /// is carried by these: the gridlines and the axis text share `axes`,
    /// the band and the chapter ticks share `track`, `cursor` is the peek
    /// line, and each end label rides with its own series so the two are
    /// placed together. Marks are drawn before the series here, so that a
    /// chapter rule never covers a line. `chapters` appears twice because
    /// it is a nested group in both `marks` (the rules and their labels)
    /// and `track` (the ticks on the bar): one selector then hides every
    /// chapter cue at once.
    const GROUP_ORDER: &[&str] = &[
        "axes", "marks", "chapters", "series", "track", "chapters", "cursor", "playhead",
    ];

    /// Every class token in the markup.
    fn classes(svg: &str) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        let mut rest = svg;
        while let Some(i) = rest.find("class=\"") {
            let after = &rest[i + "class=\"".len()..];
            let end = after.find('"').expect("a closing quote");
            for token in after[..end].split_whitespace() {
                out.insert(token.to_owned());
            }
            rest = &after[end..];
        }
        out
    }

    /// The groups in the order they are written.
    fn groups(svg: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut rest = svg;
        while let Some(i) = rest.find("<g class=\"") {
            let after = &rest[i + "<g class=\"".len()..];
            let end = after.find('"').expect("a closing quote");
            out.push(after[..end].to_owned());
            rest = &after[end..];
        }
        out
    }

    #[test]
    fn the_markup_uses_only_known_classes_in_one_z_order() {
        let mut unlabelled = demo();
        unlabelled.series[0].label.clear();
        let outputs = [
            film(&demo()).svg,
            film(&flight()).svg,
            film(&unlabelled).svg,
            sized(&empty()).svg,
            sized(&gaps()).svg,
            sized(&many()).svg,
        ];
        let mut seen: BTreeSet<String> = BTreeSet::new();
        for svg in &outputs {
            for c in classes(svg) {
                let known = EMITTED_CLASSES.contains(&c.as_str())
                    || RESERVED_CLASSES.contains(&c.as_str())
                    || c.strip_prefix("series-")
                        .is_some_and(|n| n.parse::<usize>().is_ok());
                assert!(known, "the markup carries an unknown class {c}");
                seen.insert(c);
            }
            // the groups appear once each, in the z-order, absent ones skipped
            let written = groups(svg);
            let mut order = GROUP_ORDER.iter();
            for g in &written {
                assert!(
                    order.any(|k| k == g),
                    "group {g} out of order in {written:?}"
                );
            }
        }
        // and every class in the list is one the emitter really writes
        let emitted: BTreeSet<&str> = seen
            .iter()
            .map(String::as_str)
            .filter(|c| !c.starts_with("series-"))
            .collect();
        assert_eq!(emitted, EMITTED_CLASSES.iter().copied().collect());
        // the data block beside this markup can never end the script early
        assert!(
            !crate::escape_script("{\"label\": \"</SCRIPT>\"}")
                .to_ascii_lowercase()
                .contains("</script")
        );
    }
}
