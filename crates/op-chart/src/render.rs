//! Classed SVG for a [`Spec`], laid out by [`Layout`]. Elements sit in
//! z-ordered groups: axes, bands, marks, series, track, cursor, playhead,
//! targets. The playhead group is the only thing that moves per tick: one
//! `transform` carries its line, dot and readout. The targets are last,
//! invisible and hittable, so a pointer meets a cue's target before
//! anything drawn under it.
//!
//! The same emission carries decision 15's accessible structure, because
//! the structure is the markup: the svg is a `graphics-document`, the
//! drawing that only decorates it sits inside `aria-hidden` groups, each
//! series is a `graphics-object` named from its own samples, each cue's
//! hit rect sits inside a button that names the cue, and the playhead is
//! the thumb a slider role and the aria values belong on. What the
//! geometry cannot know - the id of the consumer's own visible title, the
//! unit a series is measured in, and whether this chart's thumb is the
//! widget's slider or the decoration a film's control bar leaves it -
//! arrives in [`Aria`].

use crate::{Layout, Mark, Series, Spec};

/// The emitted markup and the geometry it was drawn with.
#[derive(Clone, Debug, PartialEq)]
pub struct Rendered {
    pub svg: String,
    pub layout: Layout,
}

/// What decision 15's structure has to be told, because the drawing does
/// not know it.
///
/// The default is a chart that names nothing and owns no control, which is
/// what a chart embedded in another element's shadow tree wants: the film's
/// own chart, whose range input is the one slider, and the palette
/// specimen's figures, which are images of a chart rather than charts.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Aria {
    /// The id of the visible title in the shadow tree this markup lands in,
    /// which the svg and the thumb are named by; empty where the consumer
    /// has none, and then neither is named here.
    pub title: String,
    /// The unit each series is measured in, in the spec's order. A series
    /// with none, or past the end of the list, has its range announced
    /// as a bare pair of numbers.
    pub units: Vec<String>,
    /// Whether this chart's thumb is the widget's slider: the role, the
    /// tab stop and the aria values go on it only then. One slider per
    /// widget is the rule (decision 15), so a chart inside a film says no
    /// and leaves the announcing to the film's own control.
    pub slider: bool,
}

impl Aria {
    /// ` aria-labelledby="..."` naming the consumer's title, and nothing at
    /// all where there is no title to name.
    fn labelled_by(&self) -> String {
        if self.title.is_empty() {
            String::new()
        } else {
            format!(" aria-labelledby=\"{}\"", escape(&self.title))
        }
    }

    /// The unit of the series at `i`.
    fn unit(&self, i: usize) -> &str {
        self.units.get(i).map_or("", String::as_str)
    }
}

/// A value as a name carries it: rounded to a hundredth and written at the
/// shortest spelling that survives the round trip, so a range reads
/// "0 to 100" and a block whose arithmetic left `0.30000000000000004`
/// behind is announced as "0.3" rather than read out in full. Every other
/// number the emitter writes is fixed-precision, and a name is the one
/// place a raw `f64` would be spoken a digit at a time.
pub fn announced(v: f64) -> String {
    let rounded = (v * 100.0).round() / 100.0;
    // a domain reaching exactly -0.0 is a zero and is said as one: the
    // comparison is true for both zeros, which is what picks this branch
    if rounded == 0.0 {
        "0".to_owned()
    } else {
        rounded.to_string()
    }
}

/// What a series announces: decision 15's "Name, N samples, min to max
/// unit, from t0 to t1". Every number is read off the samples the block
/// carried, not off the path, which is thinned per pixel column and would
/// otherwise make the count a fact about the box the chart was drawn in. A
/// series with no name of its own is announced by its numbers alone rather
/// than by an invented one.
fn series_label(s: &Series, unit: &str) -> String {
    let mut parts: Vec<String> = Vec::new();
    if !s.label.is_empty() {
        parts.push(s.label.clone());
    }
    let present: Vec<(f64, f64)> = s.points.iter().flatten().copied().collect();
    parts.push(match present.len() {
        0 => "no samples".to_owned(),
        1 => "1 sample".to_owned(),
        n => format!("{n} samples"),
    });
    if let (Some((t0, _)), Some((t1, _))) = (present.first(), present.last()) {
        let mut lo = f64::INFINITY;
        let mut hi = f64::NEG_INFINITY;
        for (_, v) in &present {
            lo = lo.min(*v);
            hi = hi.max(*v);
        }
        let (lo, hi) = (announced(lo), announced(hi));
        parts.push(if unit.is_empty() {
            format!("{lo} to {hi}")
        } else {
            format!("{lo} to {hi} {unit}")
        });
        parts.push(format!("from {t0:.2} s to {t1:.2} s"));
    }
    parts.join(", ")
}

/// What one cue's button announces: its name and the instant it stands
/// for. A cue the block left nameless is announced by its time alone.
fn cue_label(label: &str, t: f64) -> String {
    if label.is_empty() {
        format!("{t:.2} s")
    } else {
        format!("{label}, {t:.2} s")
    }
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
/// The side of a pointer target in CSS px: the 24 by 24 minimum SC 2.5.8
/// asks of every target a pointer must hit.
const TARGET: f64 = 24.0;
/// How far outside the thumb's target its focus ring is drawn. Outward,
/// because a ring inset into the thing it rings fails 2.4.13 (decision 20).
const RING_OFFSET: f64 = 2.0;
/// What the thumb announces as its name.
///
/// The document above it is named by the consumer's visible title, which
/// is the list of series the chart draws; a slider named by that list
/// reads its whole legend out before its own value on every step along the
/// track. What this control moves is the clock, so that is what it is
/// called, and x is time in every chart this renderer draws. A deviation
/// from decision 15's letter, which names the thumb from the title too.
const THUMB_NAME: &str = "Time";
/// The ring's own stroke, which the consumer's stylesheet paints at this
/// width. The geometry leaves room for half of it beyond the ring's path,
/// since a stroke is drawn either side of the line it follows and what
/// hangs past the viewport is clipped: a ring that lost its lower half
/// would be the focus indicator drawn at 1 px.
const RING_WIDTH: f64 = 2.0;
/// Room a label takes per character. A static emitter has no font
/// metrics, so the width is estimated: 6.5 px is generous for the 12 px
/// face the stylesheet sets, which is the safe direction when the estimate
/// is what decides whether two labels collide.
const LABEL_ADVANCE: f64 = 6.5;
/// Clear space between two labels that share a row: the mark labels along
/// the bottom edge, and the cue labels along the top.
const ROW_LABEL_GAP: f64 = 8.0;
/// How far back from a series' last point its end label is anchored. The
/// swatch runs from 16 to 4 px behind that point, and the label ends 4 px
/// before the swatch.
const END_LABEL_X: f64 = 20.0;
/// How far right of its own rule a chapter's label starts.
const CHAPTER_LABEL_X: f64 = 4.0;
/// The room a label takes, at [`LABEL_ADVANCE`] to the character.
fn label_width(text: &str) -> f64 {
    text.chars().count() as f64 * LABEL_ADVANCE
}

/// Markers per series at most, spread over the samples.
const MAX_MARKERS: usize = 8;

/// Where each mark's label sits along the bottom edge, and `None` for a
/// mark that keeps its rule and loses its label. Every label wants the
/// middle of its own rule; the spreader shifts them apart by the widest
/// label's slot and pulls the run inside the plot, as it spreads the end
/// labels down the right-hand side. A label the spreader had to push clear
/// of its own rule points at nothing, so it is dropped rather than drawn
/// where it misleads: decision 24's greedy removal. Which of two colliding
/// marks keeps its label is the spreader's answer, not a rule of this
/// function's own, and it is not always the earlier mark. The first sweep
/// pushes later labels away from earlier ones, so where the run fits, the
/// earlier mark keeps its place and the later one is moved and dropped;
/// but a run that overflows the right-hand end is pinned there by the
/// second sweep and pulled left, and then it is an earlier label that is
/// carried off its own mark and dropped. Two marks at one instant are
/// moved alike, and the spreader's stable order leaves the label with
/// whichever the block lists first. One pass, with no reshuffle into the
/// room a dropped label leaves.
fn mark_labels(marks: &[Mark], l: &Layout) -> Vec<Option<f64>> {
    let widths: Vec<f64> = marks.iter().map(|m| label_width(&m.label)).collect();
    let widest = widths.iter().copied().fold(0.0, f64::max);
    let wanted: Vec<f64> = marks.iter().map(|m| l.x_of(m.t)).collect();
    let placed = crate::labels::spread(
        &wanted,
        widest + ROW_LABEL_GAP,
        l.left + widest / 2.0,
        l.width - l.right - widest / 2.0,
    );
    marks
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let moved = (placed[i] - wanted[i]).abs();
            (!m.label.is_empty() && moved <= widths[i] / 2.0).then_some(placed[i])
        })
        .collect()
}

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

/// The left edge of the column the direct end labels take in the top right
/// corner, and the plot's own right edge where no series carries one. The
/// labels along the top row stop clear of it: an end label is what tells
/// one series from another where colour cannot, so it is never moved and
/// never covered (decision 24).
fn end_label_edge(spec: &Spec, drawn: &[Drawn], l: &Layout) -> f64 {
    spec.series
        .iter()
        .zip(drawn)
        .filter(|(s, dr)| !s.label.is_empty() && !dr.present.is_empty())
        .map(|(s, dr)| {
            let (t, _) = dr.present[dr.present.len() - 1];
            l.x_of(t) - END_LABEL_X - label_width(&s.label)
        })
        .fold(l.width - l.right, f64::min)
}

/// Which of a row's labels are drawn. `boxes` is the horizontal extent each
/// label wants, `None` for a cue with nothing to say, and `edge` is the
/// first x the row may not reach into.
///
/// One greedy pass along the row with [`ROW_LABEL_GAP`] of clear space
/// between neighbours, which is decision 24's greedy placement and removal:
/// a label that cannot take the place it wants is dropped rather than drawn
/// over its neighbour, because a label shifted off its own cue points at
/// nothing. Two contests are settled here. Against another cue label the
/// earlier one wins, always: the pass runs left to right, nothing is ever
/// moved, and so the label nearer the playhead at the axis origin is the
/// one that keeps its place. Against the end labels the end labels win,
/// always: `edge` is reserved before the pass begins, so a cue label near
/// the end of the timeline is the one that goes.
fn place_row(boxes: &[Option<(f64, f64)>], edge: f64) -> Vec<bool> {
    // a label with no box sorts last and is never placed, so it can never
    // stand in the way of one with something to draw
    let lefts: Vec<f64> = boxes
        .iter()
        .map(|b| b.map_or(f64::INFINITY, |(a, _)| a))
        .collect();
    let mut drawn = vec![false; boxes.len()];
    let mut taken = f64::NEG_INFINITY;
    for i in crate::labels::order_by(&lefts) {
        let Some((a, b)) = boxes[i] else { continue };
        if a < taken + ROW_LABEL_GAP || b + ROW_LABEL_GAP > edge {
            continue;
        }
        drawn[i] = true;
        taken = taken.max(b);
    }
    drawn
}

/// Draw `spec` in the box and scales `l` describes, naming nothing beyond
/// the data: the structure of [`render_with`] with [`Aria::default`].
pub fn render(spec: &Spec, l: Layout) -> Rendered {
    render_with(spec, l, &Aria::default())
}

/// Draw `spec` in the box and scales `l` describes. The caller chooses the
/// layout (the film uses [`Layout::film`]) so the element and the page
/// build can size a chart without the renderer knowing about either, and
/// `aria` says what the accessible structure is to name.
pub fn render_with(spec: &Spec, l: Layout, aria: &Aria) -> Rendered {
    // the box says how big the chart is, the spec says what it is a chart
    // of: the value domain travels with the data, not with the box
    let l = l.with_y(spec.y.0, spec.y.1);
    // The svg is a graphics-document and never a slider or an image
    // (decision 15): a slider role would make every element under it
    // presentational, and `role="img"` collapses the subtree, and the
    // chart's series are worth exposing. The value belongs on the thumb
    // below, which is the operable thing.
    //
    // One tab stop into a chart, and where the thumb is the slider it is
    // the thumb (decision 17). The svg gives up its own stop there: it is
    // the first focusable area inside the shadow tree, so a host with
    // `delegatesFocus` would land on the document rather than on the
    // control, and Tab would pass through two stops where the decision
    // asks for one. Where the widget's slider is somewhere else - the film
    // draws this markup into its own tree, and its range input is the
    // control - the thumb is decoration with no role and no value, and the
    // svg keeps the stop as the only thing a keyboard can reach here.
    let stop = if aria.slider { "" } else { " tabindex=\"0\"" };
    let mut out = format!(
        "<svg class=\"chart\" part=\"chart\" viewBox=\"0 0 {} {}\"{stop} role=\"graphics-document\"{}>",
        l.width,
        l.height,
        aria.labelled_by()
    );
    // in a box measured in CSS px a stylesheet may still scale the svg, so
    // every stroked shape holds its width; the film's fixed box does not
    let ns = if l.non_scaling {
        " vector-effect=\"non-scaling-stroke\""
    } else {
        ""
    };

    // The grid, the axes, the ticks, the band and the marks are the
    // chart's decoration: they say again, in a shape, what the summary and
    // the table say in words, so one group hides the lot from a reader who
    // is being told rather than shown (decision 15). It carries no class:
    // it is there for the accessibility tree alone, and a class would
    // invite a stylesheet to hang paint on a wrapper the z-order does not
    // know about.
    out.push_str("<g aria-hidden=\"true\">");
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

    // what each series actually draws: the samples it was given, thinned
    // per pixel column when it carries more of them than the plot is wide,
    // as a path and as the present points the cues sit on. Drawn far below
    // this, but measured here: where a series ends is where its end label
    // sits, and the labels along the top row are placed clear of those.
    let drawn: Vec<Drawn> = spec
        .series
        .iter()
        .map(|s| drawn_of(&decimate(&s.points, &l), &l))
        .collect();
    // the top row: the band's label over the middle of its span, then one
    // per chapter just right of its own rule, all placed in one pass so no
    // two of them overlap and none reaches into the end labels' column
    let row_y = l.top + 10.0;
    let mut row_boxes: Vec<Option<(f64, f64)>> = vec![spec.band.as_ref().and_then(|band| {
        let half = label_width(&band.label) / 2.0;
        let mid = (l.x_of(band.t0) + l.x_of(band.t1)) / 2.0;
        (!band.label.is_empty()).then_some((mid - half, mid + half))
    })];
    row_boxes.extend(spec.chapters.iter().skip(1).map(|ch| {
        let left = l.x_of(ch.t) + CHAPTER_LABEL_X;
        (!ch.label.is_empty()).then_some((left, left + label_width(&ch.label)))
    }));
    let top_row = place_row(&row_boxes, end_label_edge(spec, &drawn, &l));

    // the band is a wash under everything else the chart draws, so it goes
    // in a group of its own before the marks and the series. It carries no
    // paint: the class maps to the band token, and `stroke-width` is the
    // width of an edge the stylesheet paints in the surface colour, which
    // is decision 24's gap where the band's own edge meets a series line or
    // the playhead. It is a narrower reading than the decision's wording: a
    // series line that crosses over the band still meets it with no gap,
    // since only the boundary is drawn. A span given backwards draws the
    // same span: a negative width is an error in SVG, so the edges are
    // ordered before they are written.
    if let Some(band) = &spec.band {
        let (a, b) = (l.x_of(band.t0), l.x_of(band.t1));
        let (a, b) = if a <= b { (a, b) } else { (b, a) };
        out.push_str("<g class=\"bands\">");
        out.push_str(&format!(
            "<rect class=\"band\" x=\"{a:.1}\" y=\"{}\" width=\"{:.1}\" height=\"{:.1}\" stroke-width=\"1\"{ns}/>",
            l.top,
            b - a,
            l.plot_height()
        ));
        // a label with no room of its own on the top row is dropped; the
        // wash still shows the span it named
        if top_row[0] {
            out.push_str(&format!(
                "<text class=\"band-label\" x=\"{:.1}\" y=\"{row_y:.1}\" text-anchor=\"middle\">{}</text>",
                (a + b) / 2.0,
                escape(&band.label)
            ));
        }
        out.push_str("</g>");
    }

    // the marks the block annotates the plot with, then the chapter rules
    // and their labels in a `chapters` group of their own, as the ticks on
    // the track below are, so one rule in the consumer's stylesheet hides
    // every chapter cue in a narrow box. Mark labels run along the bottom
    // edge, where they cross no gridline label and no chapter label.
    let mark_row = l.plot_bottom() - 4.0;
    out.push_str("<g class=\"marks\">");
    for (m, at) in spec.marks.iter().zip(mark_labels(&spec.marks, &l)) {
        let x = l.x_of(m.t);
        // the rule carries its own time, as its target does, so the
        // element can name the mark a pointer or a key has reached
        // without measuring the markup back into times
        out.push_str(&format!(
            "<line class=\"mark\" data-t=\"{:.3}\" x1=\"{x:.1}\" x2=\"{x:.1}\" y1=\"{}\" y2=\"{:.1}\" stroke-width=\"1\"{ns}/>",
            m.t,
            l.top,
            l.plot_bottom()
        ));
        // a label with no room of its own is dropped; the rule stays
        if let Some(x) = at {
            out.push_str(&format!(
                "<text class=\"mark-label\" x=\"{x:.1}\" y=\"{mark_row:.1}\" text-anchor=\"middle\">{}</text>",
                escape(&m.label)
            ));
        }
    }
    out.push_str("<g class=\"chapters\">");
    for (ch, at) in spec.chapters.iter().skip(1).zip(&top_row[1..]) {
        let x = l.x_of(ch.t);
        out.push_str(&format!(
            "<line class=\"mark\" x1=\"{x:.1}\" x2=\"{x:.1}\" y1=\"{}\" y2=\"{:.1}\"{ns}/>",
            l.top,
            l.plot_bottom()
        ));
        // as with a mark, a label with no room is dropped and the rule stays
        if *at {
            out.push_str(&format!(
                "<text class=\"marklabel\" x=\"{:.1}\" y=\"{row_y:.1}\">{}</text>",
                x + CHAPTER_LABEL_X,
                escape(&ch.label)
            ));
        }
    }
    // the chapters, the marks, and with them the decoration the axes and
    // the band opened
    out.push_str("</g></g></g>");

    out.push_str("<g class=\"series\">");
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
    for (i, (s, dr)) in spec.series.iter().zip(&drawn).enumerate() {
        // one graphics-object per series, named from the samples the block
        // carried, so a reader is told what each line holds without being
        // read the line itself (decision 15)
        out.push_str(&format!(
            "<g role=\"graphics-object\" aria-label=\"{}\">",
            escape(&series_label(s, aria.unit(i)))
        ));
        // the class names the palette slot and the part exports it, so a
        // page can restyle one series through the element's boundary. The
        // path is hidden: the group above already says what it draws.
        out.push_str(&format!(
            "<path class=\"series-{}\" part=\"series-{}\" d=\"{}\" fill=\"none\" stroke-width=\"{}\" stroke-linejoin=\"round\" aria-hidden=\"true\"{ns}/>",
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
            // the end label draws the name the group already carries, so
            // it is hidden rather than announced a second time
            out.push_str(&format!(
                "<text class=\"endlabel\" x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"end\" aria-hidden=\"true\">{}</text>",
                x - END_LABEL_X,
                y + 4.0,
                escape(&s.label)
            ));
        }
        out.push_str("</g>");
    }
    out.push_str("</g>");

    let by = l.track_y();
    // the track and the peek rule are decoration too: the bar says where
    // the playhead has been, which the thumb's own value says in words,
    // and the chapter ticks on it are named by the buttons below
    out.push_str("<g class=\"track\" aria-hidden=\"true\">");
    // the film's peek band: a wash over the chapter under the pointer,
    // widened at runtime and empty until then. It carries a class of its
    // own rather than the block's `band`, so that a query for the one can
    // never take the other and a rule for one can never paint the other:
    // both are rects of the same shape and only the class tells them apart.
    out.push_str(&format!(
        "<rect class=\"peek-band\" x=\"{}\" y=\"{}\" width=\"0\" height=\"{:.1}\"{ns}/>",
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
        "<g class=\"cursor\" aria-hidden=\"true\"><line class=\"peek-line\" x1=\"{}\" x2=\"{}\" y1=\"{}\" y2=\"{:.1}\" visibility=\"hidden\"{ns}/></g>",
        l.left,
        l.left,
        l.top,
        l.plot_bottom()
    ));
    // The playhead is the thumb, and where this chart owns its widget it is
    // the slider: the role, the tab stop and the aria values sit here and
    // never on the svg, which is a document (decision 15). It holds the
    // readout as well, so the time is read as the slider's own value and
    // not a second time as loose text beside it.
    //
    // The 24 by 24 px rect states the thumb's target (decision 20); it does
    // not take the pointer. A press is read on the svg wherever it lands,
    // the track counting as one target under 2.5.8's spatial-selection
    // rule, and a rect that took the pointer here would put the focus on
    // the thumb instead of the chart on every press that met the playhead.
    // The ring around it is drawn 2 px outside that rect, shown by the
    // stylesheet on `:focus-visible` alone. It is a rect in the drawing
    // and never a CSS outline, because an outline on an SVG node is laid
    // out in user space: it scales with the viewBox and is clipped by the
    // viewport, so the 2 px the criterion asks for is 1 px in a box drawn
    // at half its viewBox and gone altogether at the edges (decision 20).
    // It states both ways of not painting its interior: `fill="none"`
    // keeps it out of hit-testing, which `visiblePainted` decides from the
    // fill alone, and `fill-opacity="0"` keeps it unpainted under a
    // stylesheet or a forced palette that puts a colour back into `fill`.
    let room = TARGET / 2.0 + RING_OFFSET + RING_WIDTH / 2.0;
    let thumb_row = (by + 2.0).min(l.height - room).max(room);
    let thumb = if aria.slider {
        format!(
            " role=\"slider\" tabindex=\"0\" aria-label=\"{THUMB_NAME}\" aria-valuemin=\"0\" aria-valuemax=\"{:.2}\" aria-valuenow=\"0\" aria-valuetext=\"0.00 seconds\"",
            spec.duration
        )
    } else {
        String::new()
    };
    out.push_str(&format!(
        "<g class=\"playhead\" part=\"playhead\"{thumb} transform=\"translate({:.1} 0)\"><line class=\"head\" x1=\"0\" x2=\"0\" y1=\"{}\" y2=\"{:.1}\"{ns}/><circle class=\"head-dot\" cx=\"0\" cy=\"{:.1}\" r=\"5\"{ns}/><rect class=\"target\" x=\"{:.1}\" y=\"{:.1}\" width=\"{TARGET}\" height=\"{TARGET}\" fill=\"none\"{ns}/><text class=\"head-t\" x=\"4\" y=\"{:.1}\" aria-hidden=\"true\">0.00s</text><rect class=\"head-ring\" x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" fill=\"none\" fill-opacity=\"0\"{ns}/></g>",
        l.left,
        l.top,
        by + 4.0,
        by + 2.0,
        -TARGET / 2.0,
        thumb_row - TARGET / 2.0,
        l.readout_y(),
        -TARGET / 2.0 - RING_OFFSET,
        thumb_row - TARGET / 2.0 - RING_OFFSET,
        TARGET + 2.0 * RING_OFFSET,
        TARGET + 2.0 * RING_OFFSET,
    ));

    // The pointer targets come last, so paint order gives them the pointer
    // ahead of the cues they stand for: one rect for every mark and one for
    // every chapter, so the count follows the data rather than the rows a
    // cue happens to be drawn on. Each is 24 by 24 CSS px, SC 2.5.8's
    // minimum, centred on its cue's x and on its cue's own row: the bottom
    // edge where a mark's label runs, the bar where a chapter's tick sits.
    // `fill="none"` with `pointer-events="all"` is hit without being
    // painted at all, where an opacity of zero or a transparent fill would
    // still be a painted rectangle for forced colours and print to undo.
    //
    // The one class is `target`, which no rule in either consumer's
    // stylesheet paints, and the cue it stands for is named in `data-cue`
    // beside its time. Naming the cue in the class instead would hand the
    // rect the cue's own paint: `mark` and `chapter` are the classes the
    // drawn rules and ticks carry, so a stroked hit rect would appear
    // wherever a cue does.
    //
    // Two cues closer than 24 px keep their own x, so their rects touch or
    // overlap and the later one takes the pointer in the strip they share.
    // What that overlap leans on is the criterion's spacing exception: a
    // circle of 24 px diameter centred on each target's bounding box must
    // meet no other target, which centres 24 px apart satisfy and centres
    // 10 px apart do not (their circles overlap by 14 px, as the rects do).
    // Below 24 px the chart is a dense visualisation under the criterion's
    // own exception and the alternatives carry it, so the emitter draws the
    // overlap rather than nudging a cue off its time.
    //
    // Each rect is also the operable thing its cue has, so it sits inside a
    // `role="button"` that names the cue and its time, and the two kinds
    // are gathered in a named graphics-object apiece (decision 15). The
    // buttons hold the rects that were here already rather than a second
    // set of their own: what a pointer hits and what a key reaches are one
    // element, so they can never come to stand for different instants. They
    // take `tabindex="-1"`, which is decision 17's roving tabindex at rest:
    // the thumb is the one tab stop until the reader steps into the cues,
    // and the consumer moves the attribute when they do.
    //
    // The button repeats its rect's `data-cue`, and carries no class: a
    // class is what the z-order groups are read by, and `mark` and
    // `chapter` are the classes the drawn rules and ticks are painted by,
    // so a cue that named its kind in a class would take that paint. A
    // consumer's stylesheet names the button by its role and its kind
    // instead, which is how decision 20's hover and focus styling reaches
    // it and how it turns off the site-wide `:focus-visible` outline that
    // would otherwise be drawn around a `<g>` in user space.
    out.push_str("<g class=\"targets\" part=\"targets\">");
    let cue_rect = |cue: &str, t: f64, row: f64, out: &mut String| {
        // the row is brought inside the box before it is written. The root
        // svg clips at its viewport and clipped geometry is not hit at all,
        // so a target centred on the chapter row, 8 px from the bottom
        // edge, would keep 24 px of markup and offer 20 px of target, under
        // the minimum this rect exists to meet. x is left as it is: both
        // margins are wider than half a target, and a rect moved along x
        // would no longer stand for its own time.
        let row = row.min(l.height - TARGET / 2.0).max(TARGET / 2.0);
        out.push_str(&format!(
            "<rect class=\"target\" part=\"target\" data-cue=\"{cue}\" data-t=\"{t:.3}\" x=\"{:.1}\" y=\"{:.1}\" width=\"{TARGET}\" height=\"{TARGET}\" fill=\"none\" pointer-events=\"all\"{ns}/>",
            l.x_of(t) - TARGET / 2.0,
            row - TARGET / 2.0
        ));
    };
    if !spec.marks.is_empty() {
        out.push_str("<g role=\"graphics-object\" aria-label=\"Marks\">");
        for m in &spec.marks {
            out.push_str(&format!(
                "<g role=\"button\" tabindex=\"-1\" data-cue=\"mark\" aria-label=\"{}\">",
                escape(&cue_label(&m.label, m.t))
            ));
            cue_rect("mark", m.t, mark_row, &mut out);
            out.push_str("</g>");
        }
        out.push_str("</g>");
    }
    if spec.chapters.len() > 1 {
        out.push_str("<g role=\"graphics-object\" aria-label=\"Chapters\">");
        for ch in spec.chapters.iter().skip(1) {
            out.push_str(&format!(
                "<g role=\"button\" tabindex=\"-1\" data-cue=\"chapter\" aria-label=\"{}\">",
                escape(&cue_label(&ch.label, ch.t))
            ));
            cue_rect("chapter", ch.t, by + 2.0, &mut out);
            out.push_str("</g>");
        }
        out.push_str("</g>");
    }
    out.push_str("</g>");
    out.push_str("</svg>");
    Rendered {
        svg: out,
        layout: l,
    }
}

#[cfg(test)]
pub(crate) mod fixtures {
    use crate::{Band, Chapter, Mark, Series, Spec};

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

    fn mark(t: f64, label: &str) -> Mark {
        Mark {
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
            marks: Vec::new(),
            band: None,
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
            marks: Vec::new(),
            band: None,
            series: vec![
                series("ghost left %", 3, &t, &ghost, 2.4),
                series("palette blend %", 1, &t, &palette, 1.8),
            ],
        }
    }

    /// The demo annotated: two marks with room for both labels, and a
    /// band over a stretch that starts clear of the chapter rule and runs
    /// under the second mark, so the layering shows.
    pub fn annotated() -> Spec {
        Spec {
            marks: vec![mark(0.6, "first frame"), mark(2.1, "steady")],
            band: Some(Band {
                t0: 1.4,
                t1: 2.4,
                label: "warm up".to_owned(),
            }),
            ..demo()
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
            marks: Vec::new(),
            band: None,
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
    use super::fixtures::{annotated, demo, empty, flight, gaps, many, one_point, ramp};
    use super::*;
    use crate::{Band, Chapter, Mark, Series};
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
    fn sized_annotated_snapshot() {
        insta::assert_snapshot!(sized(&annotated()).svg);
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
        assert!(r.svg.contains(
            "<text class=\"head-t\" x=\"4\" y=\"250.0\" aria-hidden=\"true\">0.00s</text>"
        ));
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
            let known = ["chart", "playhead", "targets", "target"].contains(&name)
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

    /// A numeric attribute of the first element opening with `head`.
    fn number(svg: &str, head: &str, attr: &str) -> f64 {
        attribute(svg, head, attr)
            .parse()
            .expect("a number the emitter wrote")
    }

    /// One attribute of one emitted rect.
    fn rect_of(rect: &str, key: &str) -> String {
        attribute(rect, "<rect", key)
    }

    /// One numeric attribute of one emitted rect.
    fn rect_num(rect: &str, key: &str) -> f64 {
        number(rect, "<rect", key)
    }

    /// The marks group's own children: the rules and labels of the block's
    /// own marks, up to the nested group the chapter rules live in.
    fn own_marks(svg: &str) -> &str {
        let inside = svg
            .split_once("<g class=\"marks\">")
            .expect("the marks group")
            .1;
        inside
            .split_once("<g class=\"chapters\">")
            .expect("the chapters group inside it")
            .0
    }

    /// The rects inside the targets group, in the order they are written,
    /// checking on the way that nothing at all follows the group: the
    /// targets are the last thing in the markup, which is what hands them
    /// the pointer. Each sits inside a button of its own now, so the piece
    /// a rect is read out of carries that button's opening tag with it.
    fn targets(svg: &str) -> Vec<&str> {
        let tail = svg
            .split_once("<g class=\"targets\" part=\"targets\">")
            .expect("the targets group")
            .1;
        let inside = tail
            .strip_suffix("</g></svg>")
            .expect("the targets are written last");
        inside
            .split_inclusive("/>")
            .filter(|piece| piece.contains("<rect"))
            .collect()
    }

    /// Every cue carries an invisible 24 by 24 px rect on its own row, and
    /// they come last so a pointer meets them before what they stand for.
    #[test]
    fn every_cue_carries_a_pointer_target_that_is_hittable_without_being_seen() {
        let spec = annotated();
        let l = Layout::film(spec.end);
        let svg = film(&spec).svg;
        // last of all the groups, and nothing after it but the closing tag
        assert_eq!(groups(&svg).last().map(String::as_str), Some("targets"));
        let rects = targets(&svg);
        // the rows the cues are drawn on, read back from the markup: the
        // bottom edge where the mark labels run and the middle of a tick on
        // the bar, so the targets are checked against what is drawn rather
        // than against numbers repeated from the emitter
        let mark_row = number(&svg, "<text class=\"mark-label\"", "y");
        let tick_row = number(&svg, "<rect class=\"chapter\"", "y")
            + number(&svg, "<rect class=\"chapter\"", "height") / 2.0;
        assert_eq!(
            (mark_row, tick_row),
            (l.plot_bottom() - 4.0, l.track_y() + 2.0)
        );
        // one rect per mark and one per chapter past the start: the count
        // follows the data, and a chapter no longer brings two
        let cues: Vec<(&str, f64, f64)> = spec
            .marks
            .iter()
            .map(|m| ("mark", m.t, mark_row))
            .chain(
                spec.chapters
                    .iter()
                    .skip(1)
                    .map(|ch| ("chapter", ch.t, tick_row)),
            )
            .collect();
        assert_eq!(rects.len(), cues.len());
        assert_eq!(rects.len(), 3, "{rects:?}");
        for (rect, (cue, t, row)) in rects.iter().zip(cues) {
            // one class, which nothing paints, and the cue it stands for in
            // an attribute: a hit rect classed `mark` or `chapter` would
            // take the dashes those classes are drawn with
            assert_eq!(rect_of(rect, "class"), "target");
            assert_eq!(rect_of(rect, "part"), "target");
            assert_eq!(rect_of(rect, "data-cue"), cue);
            assert_eq!(rect_of(rect, "data-t"), format!("{t:.3}"));
            // 24 by 24 CSS px, centred on the cue's x and on its own row
            assert_eq!(
                (rect_num(rect, "width"), rect_num(rect, "height")),
                (24.0, 24.0)
            );
            assert!(
                (rect_num(rect, "x") + 12.0 - l.x_of(t)).abs() <= 0.05,
                "{rect} is not centred on {}",
                l.x_of(t)
            );
            // on its cue's row, or as near it as the box allows: a rect
            // hanging past the viewBox is clipped, and what is clipped is
            // not hit, so a target there would be smaller than it says
            let centre = rect_num(rect, "y") + TARGET / 2.0;
            let flush = (rect_num(rect, "y") + TARGET - l.height).abs() <= 0.05;
            assert!(centre <= row + 0.05, "{rect} is below the row at {row}");
            assert!(
                (centre - row).abs() <= 0.05 || flush,
                "{rect} is neither on the row at {row} nor flush with the bottom edge"
            );
            // and it lies inside the box on both axes, every one of them
            for (lo, hi, side) in [
                (rect_num(rect, "x"), l.width, "x"),
                (rect_num(rect, "y"), l.height, "y"),
            ] {
                assert!(lo >= 0.0, "{rect} starts before the box on {side}");
                assert!(lo + TARGET <= hi, "{rect} runs past the box on {side}");
            }
            // invisible and still hittable: no paint at all, and no
            // opacity trick standing in for one
            assert_eq!(rect_of(rect, "fill"), "none");
            assert_eq!(rect_of(rect, "pointer-events"), "all");
            assert!(!rect.contains("opacity"), "{rect}");
            assert!(!rect.contains("stroke=\""), "{rect}");
        }
        // the chapter's own row is 8 px from the bottom edge, closer than
        // half a target, so its rect is the one the box brings back inside
        // and it ends flush with that edge rather than 4 px past it
        assert_eq!(l.height - tick_row, TARGET / 3.0);
        let chapter = rects.last().expect("the chapter's target");
        assert_eq!(rect_num(chapter, "y") + TARGET, l.height);
        // the marks' row is down at the plot's bottom edge and the tick's
        // is below the axis: the two rows are a band apart, not one on the
        // other, and the times are the cues' own at three decimals
        let rows: Vec<f64> = rects.iter().map(|r| rect_num(r, "y")).collect();
        assert!(rows[2] - rows[0] > 24.0, "{rows:?}");
        let ts: Vec<String> = rects.iter().map(|r| rect_of(r, "data-t")).collect();
        assert_eq!(ts, ["0.600", "2.100", "1.200"]);
        // a chart whose only cue is its start still writes the group, so
        // the element can hang one listener on it whatever it draws
        assert!(targets(&sized(&empty()).svg).is_empty());
    }

    /// Cues closer than a target keep their own x: the rects touch and
    /// overlap, the later one taking the pointer where they meet. The
    /// accessibility phase reads this, so the numbers are pinned here.
    #[test]
    fn cues_closer_than_a_target_overlap_rather_than_move_off_their_time() {
        // 5.8 s over the 580 px plot puts a tenth of a second at 10 px
        let spec = Spec {
            end: 5.8,
            duration: 5.8,
            marks: vec![
                Mark {
                    t: 1.0,
                    label: "first".to_owned(),
                },
                Mark {
                    t: 1.1,
                    label: "second".to_owned(),
                },
            ],
            ..empty()
        };
        let l = Layout::sized(640.0, 240.0, spec.end);
        assert_eq!((l.x_of(1.0), l.x_of(1.1)), (146.0, 156.0));
        let svg = render(&spec, l).svg;
        let rects = targets(&svg);
        // both marks are there, neither merged away nor moved
        assert_eq!(rects.len(), 2, "{rects:?}");
        // the emission order, which is the paint order: the later mark's
        // rect is written after the earlier one's, so where the two overlap
        // the later is on top and takes the pointer in that strip
        let written: Vec<String> = rects
            .iter()
            .map(|r| format!("{} {}", rect_of(r, "data-cue"), rect_of(r, "data-t")))
            .collect();
        assert_eq!(written, ["mark 1.000", "mark 1.100"]);
        // each rect keeps the x of the mark it stands for: nothing is
        // merged away and nothing is nudged off its own time
        let centres: Vec<f64> = rects.iter().map(|r| rect_num(r, "x") + 12.0).collect();
        assert_eq!(centres, [146.0, 156.0]);
        // 10 px apart and 24 px wide, so the two overlap by 14 px, and a
        // pointer in that strip takes the later mark
        assert_eq!(centres[1] - centres[0], 10.0);
        assert_eq!((centres[0] + 12.0) - (centres[1] - 12.0), 14.0);
        // and both rules are drawn, whatever their labels could do
        assert_eq!(svg.matches("<line class=\"mark\"").count(), 2);
    }

    /// The markup inside the group whose opening tag begins with `open`,
    /// up to that group's own closing tag: the groups opened inside it are
    /// counted, so a group holding groups is read whole.
    fn inside<'a>(svg: &'a str, open: &str) -> &'a str {
        let start = svg
            .find(open)
            .unwrap_or_else(|| panic!("no `{open}` in the markup"));
        let body = svg[start..].split_once('>').expect("the tag ends").1;
        let mut depth = 1usize;
        for (i, _) in body.match_indices('<') {
            let rest = &body[i + 1..];
            if rest.starts_with("g ") || rest.starts_with("g>") {
                depth += 1;
            } else if rest.starts_with("/g>") {
                depth -= 1;
                if depth == 0 {
                    return &body[..i];
                }
            }
        }
        panic!("the group opened by `{open}` never closes");
    }

    /// The `aria-label` of every element carrying `role`, in the order they
    /// are written. A role written without a name fails here rather than
    /// coming back as an empty string: an unnamed object is the defect.
    fn labelled(svg: &str, role: &str) -> Vec<String> {
        svg.split(&format!("role=\"{role}\""))
            .skip(1)
            .map(|rest| {
                let head = rest.split_once('>').expect("the tag ends").0;
                const KEY: &str = "aria-label=\"";
                let i = head
                    .find(KEY)
                    .unwrap_or_else(|| panic!("a {role} with no name: `{head}`"));
                let after = &head[i + KEY.len()..];
                after[..after.find('"').expect("a closing quote")].to_owned()
            })
            .collect()
    }

    /// Decision 15's structure, so far as the drawing carries it: the svg
    /// is a document, everything that only decorates it is inside one
    /// hidden group or hidden where it stands, and each series is an
    /// object named from its own data.
    #[test]
    fn the_decoration_is_hidden_and_the_series_are_named_objects() {
        let spec = annotated();
        let svg = sized(&spec).svg;
        assert!(
            svg.starts_with(
                "<svg class=\"chart\" part=\"chart\" viewBox=\"0 0 640 240\" tabindex=\"0\" role=\"graphics-document\">"
            ),
            "{svg}"
        );
        assert!(!svg.contains("role=\"img\""), "{svg}");
        // one hidden group holds the grid, the axes, the ticks, the band
        // and the marks, drawn things a reader is told about in words
        let decoration = inside(&svg, "<g aria-hidden=\"true\">");
        for group in [
            "<g class=\"axes\">",
            "<g class=\"bands\">",
            "<g class=\"marks\">",
            "<g class=\"chapters\">",
        ] {
            assert!(decoration.contains(group), "{group} is not hidden");
        }
        for drawn in [
            "class=\"grid\"",
            "class=\"axis\"",
            "class=\"tick\"",
            "class=\"band\"",
            "class=\"mark\"",
            "class=\"mark-label\"",
        ] {
            assert!(decoration.contains(drawn), "{drawn} is not hidden");
        }
        // and nothing announced is inside it
        for exposed in ["class=\"series-", "class=\"playhead\"", "class=\"targets\""] {
            assert!(!decoration.contains(exposed), "{exposed} is hidden");
        }
        // the track and the peek rule are decoration where they stand: the
        // bar says what the thumb's own value says, and the ticks on it are
        // named by the chapter buttons
        assert!(svg.contains("<g class=\"track\" aria-hidden=\"true\">"));
        assert!(svg.contains("<g class=\"cursor\" aria-hidden=\"true\">"));

        // one object per series, named "Name, N samples, min to max unit,
        // from t0 to t1", computed here from the fixture
        let units = vec!["%".to_owned()];
        let svg = render_with(
            &spec,
            Layout::sized(640.0, 240.0, spec.end),
            &Aria {
                units: units.clone(),
                ..Aria::default()
            },
        )
        .svg;
        let s = &spec.series[0];
        let present: Vec<(f64, f64)> = s.points.iter().flatten().copied().collect();
        let lo = present
            .iter()
            .map(|(_, v)| *v)
            .fold(f64::INFINITY, f64::min);
        let hi = present
            .iter()
            .map(|(_, v)| *v)
            .fold(f64::NEG_INFINITY, f64::max);
        let want = format!(
            "{}, {} samples, {lo} to {hi} {}, from {:.2} s to {:.2} s",
            s.label,
            present.len(),
            units[0],
            present[0].0,
            present[present.len() - 1].0
        );
        assert_eq!(
            labelled(&svg, "graphics-object"),
            [want, "Marks".to_owned(), "Chapters".to_owned()]
        );
        // the line and the end label are hidden inside it, so the name is
        // announced once and not again as loose text
        let object = inside(&svg, "<g role=\"graphics-object\"");
        assert!(
            object.contains("<path class=\"series-2\" part=\"series-2\"")
                && object.contains("class=\"endlabel\"")
                && object.contains("class=\"swatch series-2\"")
        );
        for hidden in ["<path class=\"series-2\"", "<text class=\"endlabel\""] {
            let head = object
                .split_once(hidden)
                .expect("the element")
                .1
                .split_once('>')
                .expect("the tag ends")
                .0;
            assert!(head.contains("aria-hidden=\"true\""), "{hidden}: {head}");
        }
    }

    /// The numbers a series announces are the block's, not the drawing's:
    /// a chart thinned to two points per pixel column still says how many
    /// samples it holds.
    #[test]
    fn a_series_is_named_by_its_samples_and_not_by_the_thinned_path() {
        let spec = many();
        let svg = render_with(
            &spec,
            Layout::sized(640.0, 240.0, spec.end),
            &Aria {
                units: vec!["%".to_owned()],
                ..Aria::default()
            },
        )
        .svg;
        let name = labelled(&svg, "graphics-object")
            .into_iter()
            .next()
            .expect("the series");
        assert!(
            name.starts_with("every sample, 1800 samples, 0 to 100 %, from 0.00 s to 3.00 s"),
            "{name}"
        );
        // the path really was thinned, so the count could not have come
        // from the points that were drawn
        let drawn = attribute(&svg, "<path class=\"series-4\"", "d")
            .matches(['M', 'L'])
            .count();
        assert!(drawn < spec.series[0].points.len(), "{drawn} points drawn");
        // a series with gaps counts the samples it has and not the holes,
        // and one with no name of its own is announced by its numbers
        let svg = sized(&gaps()).svg;
        assert_eq!(
            labelled(&svg, "graphics-object"),
            ["6 samples, 10 to 90, from 0.00 s to 3.00 s"]
        );
        // and a series with nothing in it says so rather than a range
        let svg = sized(&empty()).svg;
        assert_eq!(
            labelled(&svg, "graphics-object"),
            ["nothing yet, no samples"]
        );
    }

    /// A series announces numbers a listener can hear. Binary floating
    /// point does not divide by ten, so a block whose values were summed
    /// or scaled arrives carrying `0.30000000000000004`, and a name built
    /// with `f64`'s own Display reads all seventeen digits out. Every
    /// other number the emitter writes is fixed-precision; these are the
    /// two that were not.
    #[test]
    fn the_numbers_a_series_announces_are_not_raw_floating_point() {
        // the decision on its own: a hundredth, at the shortest spelling
        // that survives it, and no trailing zeros to be read aloud
        assert_eq!(announced(0.1 + 0.2), "0.3");
        assert_eq!(announced(100.0), "100");
        assert_eq!(announced(0.0), "0");
        assert_eq!(announced(-0.0), "0");
        assert_eq!(announced(-12.5), "-12.5");
        assert_eq!(announced(99.437_199_358_559_6), "99.44");
        assert_eq!(announced(2.0 / 3.0), "0.67");
        // and in the name the block's own arithmetic reaches
        let noisy: Vec<f64> = (0..4).map(|i| f64::from(i) * 0.1 + 0.2).collect();
        let spec = Spec {
            y: (0.0, 1.0),
            series: vec![Series {
                label: "drift".to_owned(),
                index: 1,
                points: [0.0, 1.0, 2.0, 3.0]
                    .into_iter()
                    .zip(noisy.iter().copied())
                    .map(Some)
                    .collect(),
                width: 2.0,
            }],
            ..demo()
        };
        // the raw values really are the long ones, so the name below is
        // shortened by the emitter and not by the arithmetic
        assert_eq!(noisy[1].to_string(), "0.30000000000000004");
        let name = labelled(&sized(&spec).svg, "graphics-object")
            .into_iter()
            .next()
            .expect("the series");
        assert_eq!(name, "drift, 4 samples, 0.2 to 0.5, from 0.00 s to 3.00 s");
    }

    /// Every cue is the operable thing decision 15 asks for: a button that
    /// names it, holding the one rect a pointer already hit.
    #[test]
    fn every_cue_is_a_button_that_names_it_and_holds_its_own_target() {
        let spec = annotated();
        let svg = sized(&spec).svg;
        assert_eq!(
            labelled(&svg, "button"),
            ["first frame, 0.60 s", "steady, 2.10 s", "settle, 1.20 s"]
        );
        // the two kinds are gathered under a name apiece, each holding its
        // own cues and no others
        let marks = inside(&svg, "<g role=\"graphics-object\" aria-label=\"Marks\">");
        let chapters = inside(&svg, "<g role=\"graphics-object\" aria-label=\"Chapters\">");
        assert_eq!(
            marks.matches("part=\"target\" data-cue=\"mark\"").count(),
            spec.marks.len()
        );
        assert_eq!(
            marks
                .matches("part=\"target\" data-cue=\"chapter\"")
                .count(),
            0
        );
        assert_eq!(
            chapters
                .matches("part=\"target\" data-cue=\"chapter\"")
                .count(),
            spec.chapters.len() - 1
        );
        assert_eq!(
            chapters
                .matches("part=\"target\" data-cue=\"mark\"")
                .count(),
            0
        );
        // one rect to a button, and no second set of them: what a pointer
        // hits and what a key reaches are the same element
        assert_eq!(
            svg.matches("<rect class=\"target\" part=\"target\"")
                .count(),
            spec.marks.len() + spec.chapters.len() - 1
        );
        let group = inside(&svg, "<g class=\"targets\" part=\"targets\">");
        let buttons: Vec<&str> = group.split("<g role=\"button\"").skip(1).collect();
        assert_eq!(buttons.len(), 3);
        for button in buttons {
            let (head, body) = button.split_once('>').expect("the tag ends");
            let body = body.split_once("</g>").expect("the button closes").0;
            assert!(head.contains("tabindex=\"-1\""), "{head}");
            assert_eq!(body.matches("<rect").count(), 1, "{body}");
            assert!(body.contains("pointer-events=\"all\""), "{body}");
            // the button names the instant its own rect stands for
            let t: f64 = rect_of(body, "data-t").parse().expect("a number");
            let label = attribute(&format!("<g {head}>"), "<g", "aria-label");
            assert!(label.ends_with(&format!("{t:.2} s")), "{label} is not {t}");
            // and the kind of cue it is, which is the only handle a
            // stylesheet has on it: the button carries no class, so a rule
            // that hides the chapter cues with their ticks, or draws the
            // focus indicator a `<g>` may not take as an outline, has this
            // and the role and nothing else. It agrees with its own rect
            let kind = attribute(&format!("<g {head}>"), "<g", "data-cue");
            assert!(kind == "mark" || kind == "chapter", "{head}");
            assert_eq!(rect_of(body, "data-cue"), kind, "{button}");
        }
        // a chart with no cue at all writes neither group and no button
        let bare = sized(&empty()).svg;
        assert!(!bare.contains("role=\"button\""), "{bare}");
        assert!(!bare.contains("aria-label=\"Chapters\""), "{bare}");
    }

    /// The thumb is the slider where the chart owns one: the role, the tab
    /// stop, the name and the value sit on it, and the readout inside it,
    /// so the time is announced once, as that slider's own value.
    #[test]
    fn the_thumb_carries_the_slider_the_value_and_the_readout() {
        let spec = demo();
        let l = Layout::sized(640.0, 240.0, spec.end);
        let aria = Aria {
            title: "chart-title".to_owned(),
            units: vec!["%".to_owned()],
            slider: true,
        };
        let svg = render_with(&spec, l, &aria).svg;
        let head = svg
            .split_once("<g class=\"playhead\"")
            .expect("the thumb")
            .1
            .split_once('>')
            .expect("the tag ends")
            .0;
        for want in [
            "role=\"slider\"",
            "tabindex=\"0\"",
            // the control is named for what it moves and not for what the
            // chart draws: the document above it carries the consumer's
            // title, and a slider named by that list reads the whole
            // legend out before its own value on every step
            "aria-label=\"Time\"",
            "aria-valuemin=\"0\"",
            &format!("aria-valuemax=\"{:.2}\"", spec.duration),
            "aria-valuenow=\"0\"",
            "aria-valuetext=\"0.00 seconds\"",
        ] {
            assert!(head.contains(want), "{want} is not on the thumb: {head}");
        }
        // and nowhere else: the svg is a document, whatever the thumb is
        let root = svg.split_once('>').expect("the svg tag ends").0;
        for never in ["role=\"slider\"", "role=\"img\"", "aria-value"] {
            assert!(!root.contains(never), "{never} on the svg: {root}");
        }
        assert!(root.contains("role=\"graphics-document\""));
        assert!(root.contains("aria-labelledby=\"chart-title\""));
        // the consumer's title names the document and nothing else: it is
        // the list of series, which is what the figure is of and not what
        // the control does
        assert_eq!(svg.matches("aria-labelledby=\"chart-title\"").count(), 1);
        assert!(!head.contains("aria-labelledby"), "{head}");
        assert_eq!(svg.matches("role=\"slider\"").count(), 1);
        assert_eq!(svg.matches("aria-valuenow").count(), 1);
        // and the thumb is the tab stop the drawing ships with: the svg
        // gives its own up, so a `delegatesFocus` host lands on the
        // control and Tab passes through once (decision 17). The cue
        // buttons are `-1`, which is decision 17's roving tabindex at
        // rest; the consumer moves the attribute from here
        assert!(!root.contains("tabindex"), "a tab stop on the svg: {root}");
        assert_eq!(svg.matches("tabindex=\"0\"").count(), 1, "{svg}");
        // the line, the dot, the hit rect, the readout and the ring are all
        // inside it, and the readout is written once
        let thumb = inside(&svg, "<g class=\"playhead\"");
        for part in [
            "class=\"head\"",
            "class=\"head-dot\"",
            "class=\"target\"",
            "class=\"head-t\"",
            "class=\"head-ring\"",
        ] {
            assert!(thumb.contains(part), "{part} is not inside the thumb");
        }
        assert_eq!(svg.matches("class=\"head-t\"").count(), 1);
        // the hit rect states the thumb's 24 px target and leaves the
        // pointer to the svg, which reads a press wherever it lands
        let piece = |class: &str| {
            thumb
                .split_inclusive("/>")
                .find(|e| e.contains(class))
                .unwrap_or_else(|| panic!("no {class} in the thumb"))
        };
        let rect = piece("<rect class=\"target\"");
        assert_eq!(
            (rect_num(rect, "width"), rect_num(rect, "height")),
            (TARGET, TARGET)
        );
        assert_eq!(rect_num(rect, "x"), -TARGET / 2.0);
        assert!(!rect.contains("pointer-events"), "{rect}");
        assert_eq!(rect_of(rect, "fill"), "none");
        // the ring sits 2 px outside that rect, with room for its own
        // stroke inside the box at both edges. Outward, because a ring
        // inset into the thing it rings is drawn over the thumb's own
        // pixels and fails the contrast criterion (decision 20)
        let ring = piece("<rect class=\"head-ring\"");
        assert_eq!(rect_of(ring, "fill"), "none");
        // it never paints its interior, whatever `fill` becomes
        assert_eq!(rect_of(ring, "fill-opacity"), "0");
        assert_eq!(rect_num(ring, "x"), rect_num(rect, "x") - RING_OFFSET);
        assert_eq!(rect_num(ring, "y"), rect_num(rect, "y") - RING_OFFSET);
        assert_eq!(
            rect_num(ring, "width"),
            rect_num(rect, "width") + 2.0 * RING_OFFSET
        );
        // and the gap is the criterion's own number, written here as the
        // number and not as the constant that produced it: decision 20
        // asks for 2 px outward, and a constant halved would keep every
        // assertion above true
        assert_eq!(rect_num(rect, "x") - rect_num(ring, "x"), 2.0);
        assert_eq!(rect_num(rect, "y") - rect_num(ring, "y"), 2.0);
        assert_eq!(
            rect_num(ring, "x") + rect_num(ring, "width")
                - (rect_num(rect, "x") + rect_num(rect, "width")),
            2.0
        );
        assert_eq!(
            rect_num(ring, "y") + rect_num(ring, "height")
                - (rect_num(rect, "y") + rect_num(rect, "height")),
            2.0
        );
        assert!(rect_num(ring, "y") - RING_WIDTH / 2.0 >= 0.0, "{ring}");
        assert!(
            rect_num(ring, "y") + rect_num(ring, "height") + RING_WIDTH / 2.0 <= l.height,
            "{ring}"
        );
    }

    /// A chart embedded in another element's shadow tree owns no slider:
    /// the film's control bar is the one that announces the value, so the
    /// thumb here is decoration and the chart adds no second tab stop.
    #[test]
    fn a_chart_that_owns_no_slider_leaves_its_thumb_decoration() {
        let spec = flight();
        let svg = film(&spec).svg;
        for never in ["role=\"slider\"", "aria-valu", "aria-labelledby"] {
            assert!(!svg.contains(never), "{never} in a chart that owns none");
        }
        // the svg is the one tab stop; the cue buttons are reached by
        // script and by a roving tabindex, never by Tab
        assert_eq!(svg.matches("tabindex=\"0\"").count(), 1);
        assert!(svg.starts_with(
            "<svg class=\"chart\" part=\"chart\" viewBox=\"0 0 900 268\" tabindex=\"0\" role=\"graphics-document\">"
        ));
        // and the structure that is not a control is there all the same
        assert_eq!(
            labelled(&svg, "button").len(),
            spec.chapters.len() - 1 + spec.marks.len()
        );
        assert_eq!(
            labelled(&svg, "graphics-object").len(),
            spec.series.len() + 1
        );
        assert!(svg.contains("<g class=\"playhead\" part=\"playhead\" transform="));
    }

    /// The rules, the wash and the labels a block's own annotations draw.
    #[test]
    fn marks_and_a_band_draw_their_rules_wash_and_labels() {
        let spec = annotated();
        let l = Layout::sized(640.0, 240.0, spec.end);
        let svg = render(&spec, l).svg;
        // one rule per mark, from the top of the plot to its bottom, thin
        let rules: Vec<&str> = own_marks(&svg)
            .split_inclusive("/>")
            .filter(|e| e.contains("<line class=\"mark\""))
            .collect();
        assert_eq!(rules.len(), 2, "{rules:?}");
        for (rule, m) in rules.iter().zip(&spec.marks) {
            let x = attribute(rule, "<line", "x1")
                .parse::<f64>()
                .expect("a number");
            assert_eq!(
                attribute(rule, "<line", "x2"),
                attribute(rule, "<line", "x1")
            );
            assert!((x - l.x_of(m.t)).abs() <= 0.05, "{rule}");
            assert_eq!(attribute(rule, "<line", "y1"), format!("{}", l.top));
            assert_eq!(
                attribute(rule, "<line", "y2"),
                format!("{:.1}", l.plot_bottom())
            );
            assert_eq!(attribute(rule, "<line", "stroke-width"), "1");
            // the rule names its own time, at the three decimals its
            // target writes, so the two can be matched up
            assert_eq!(attribute(rule, "<line", "data-t"), format!("{:.3}", m.t));
        }
        // the chapter rules share the class and carry no time of their
        // own: a rule a `data-t` names is a mark
        let chapters = svg
            .split_once("<g class=\"chapters\">")
            .expect("the chapter rules")
            .1;
        assert!(!chapters.contains("<line class=\"mark\" data-t"), "{svg}");
        // one label per mark, along the bottom edge, on its own rule
        let labels: Vec<&str> = own_marks(&svg)
            .split_inclusive("</text>")
            .filter(|e| e.contains("<text class=\"mark-label\""))
            .collect();
        assert_eq!(labels.len(), 2, "{labels:?}");
        for (label, m) in labels.iter().zip(&spec.marks) {
            let x = attribute(label, "<text", "x")
                .parse::<f64>()
                .expect("a number");
            assert!((x - l.x_of(m.t)).abs() <= 0.05, "{label}");
            assert_eq!(
                attribute(label, "<text", "y"),
                format!("{:.1}", l.plot_bottom() - 4.0)
            );
            assert_eq!(attribute(label, "<text", "text-anchor"), "middle");
            assert!(label.ends_with(&format!(">{}</text>", m.label)), "{label}");
        }
        // the band is a rect in its own group, behind the series, with its
        // edges at the times the block gave and no paint of its own
        let band = spec.band.as_ref().expect("the fixture's band");
        assert!(svg.contains("<g class=\"bands\">"));
        let rect = svg
            .split_inclusive("/>")
            .find(|e| e.contains("<rect class=\"band\""))
            .expect("the band");
        // both edges to the tenth of a pixel the emitter writes
        assert!(
            (rect_num(rect, "x") - l.x_of(band.t0)).abs() <= 0.05,
            "{rect}"
        );
        assert!(
            (rect_num(rect, "x") + rect_num(rect, "width") - l.x_of(band.t1)).abs() <= 0.05,
            "{rect}"
        );
        assert_eq!(rect_num(rect, "y"), l.top);
        assert_eq!(rect_num(rect, "height"), l.plot_height());
        // the 1 px edge the stylesheet paints in the surface colour is the
        // gap where the band meets a series line or the playhead
        assert_eq!(rect_of(rect, "stroke-width"), "1");
        assert!(
            !rect.contains("fill=") && !rect.contains("stroke=\""),
            "{rect}"
        );
        // and its label sits over the middle of the span
        let label = svg
            .split_inclusive("</text>")
            .find(|e| e.contains("<text class=\"band-label\""))
            .expect("the band label");
        let mid = (l.x_of(band.t0) + l.x_of(band.t1)) / 2.0;
        assert!(
            (attribute(label, "<text", "x")
                .parse::<f64>()
                .expect("a number")
                - mid)
                .abs()
                <= 0.05
        );
        assert!(label.ends_with(">warm up</text>"), "{label}");
        // the wash is written before the series it sits behind
        assert!(svg.find("<g class=\"bands\">") < svg.find("<g class=\"series\">"));
        // a span given backwards draws the same span, never a negative width
        let back = render(
            &Spec {
                band: Some(Band {
                    t0: band.t1,
                    t1: band.t0,
                    label: band.label.clone(),
                }),
                ..spec.clone()
            },
            l,
        )
        .svg;
        let flipped = back
            .split_inclusive("/>")
            .find(|e| e.contains("<rect class=\"band\""))
            .expect("the band");
        assert_eq!(rect_of(flipped, "x"), rect_of(rect, "x"));
        assert_eq!(rect_of(flipped, "width"), rect_of(rect, "width"));
    }

    /// Decision 24's greedy removal: two marks too close for both labels
    /// keep both rules, and the one nearest the playhead keeps its label.
    #[test]
    fn a_mark_label_with_no_room_is_dropped_and_its_rule_stays() {
        let spec = Spec {
            marks: vec![
                Mark {
                    t: 1.0,
                    label: "first frame".to_owned(),
                },
                Mark {
                    t: 1.1,
                    label: "steady".to_owned(),
                },
            ],
            ..demo()
        };
        let l = Layout::sized(640.0, 240.0, spec.end);
        // 19 px apart, where the labels want about 72 and 39 px of room
        assert!((l.x_of(1.1) - l.x_of(1.0) - 19.3).abs() < 0.1);
        let svg = render(&spec, l).svg;
        // both rules are drawn, at their own times
        assert_eq!(own_marks(&svg).matches("<line class=\"mark\"").count(), 2);
        for m in &spec.marks {
            assert!(
                svg.contains(&format!(
                    "<line class=\"mark\" data-t=\"{:.3}\" x1=\"{:.1}\"",
                    m.t,
                    l.x_of(m.t)
                )),
                "no rule at {}",
                m.t
            );
        }
        // one label survives, the earlier mark's: the mark nearest the
        // playhead at the axis origin wins, and it stays on its own rule
        let labels: Vec<&str> = own_marks(&svg)
            .split_inclusive("</text>")
            .filter(|e| e.contains("<text class=\"mark-label\""))
            .collect();
        assert_eq!(labels.len(), 1, "{labels:?}");
        assert!(labels[0].ends_with(">first frame</text>"), "{}", labels[0]);
        assert_eq!(
            attribute(labels[0], "<text", "x"),
            format!("{:.1}", l.x_of(1.0))
        );
        // and the dropped label leaves the target behind: a mark is
        // hittable whether or not its name could be drawn
        let hit: Vec<String> = targets(&svg)
            .iter()
            .filter(|r| rect_of(r, "data-cue") == "mark")
            .map(|r| rect_of(r, "data-t"))
            .collect();
        assert_eq!(hit, ["1.000", "1.100"]);
        // room enough and both labels are drawn
        let apart = Spec {
            marks: vec![
                Mark {
                    t: 0.4,
                    label: "first frame".to_owned(),
                },
                Mark {
                    t: 2.6,
                    label: "steady".to_owned(),
                },
            ],
            ..demo()
        };
        let svg = render(&apart, l).svg;
        assert_eq!(svg.matches("<text class=\"mark-label\"").count(), 2);
    }

    /// The other half of the mark row's contest, which the spreader and
    /// not the emitter decides: a run of labels that overflows the
    /// right-hand end is pinned there and pulled left, so it is the later
    /// mark that stays on its rule and the earlier one that is carried off
    /// its own and dropped. The reverse of the case above, and the reason
    /// the emitter claims no rule of its own about which mark wins.
    #[test]
    fn a_run_of_mark_labels_against_the_right_end_drops_the_earlier_one() {
        let spec = Spec {
            marks: vec![
                Mark {
                    t: 2.7,
                    label: "first frame".to_owned(),
                },
                Mark {
                    t: 2.8,
                    label: "steady".to_owned(),
                },
            ],
            ..demo()
        };
        let l = Layout::sized(640.0, 240.0, spec.end);
        // the two want the same room, and the slot the earlier one takes
        // pushes the later past the last place a label may sit
        let (a, b) = (l.x_of(2.7), l.x_of(2.8));
        let widest = label_width("first frame");
        let hi = l.width - l.right - widest / 2.0;
        assert!(b - a < widest + ROW_LABEL_GAP, "{a} and {b} are clear");
        assert!(a + widest + ROW_LABEL_GAP > hi, "the run fits after all");
        let svg = render(&spec, l).svg;
        // both rules are drawn, and the label left is the later mark's
        assert_eq!(own_marks(&svg).matches("<line class=\"mark\"").count(), 2);
        assert_eq!(labels_of(&svg, "mark-label"), ["steady"]);
        // it is still over its own rule, within half its own width
        let x = number(&svg, "<text class=\"mark-label\"", "x");
        assert!(
            (x - b).abs() <= label_width("steady") / 2.0,
            "{x} is not on {b}"
        );
    }

    /// The text of every label written with `class`, in the order drawn.
    fn labels_of(svg: &str, class: &str) -> Vec<String> {
        let head = format!("<text class=\"{class}\"");
        svg.split(&head)
            .skip(1)
            .map(|rest| {
                let text = rest.split_once('>').expect("the text node").1;
                text.split_once('<').expect("a closing tag").0.to_owned()
            })
            .collect()
    }

    /// The chapter rules the chapters group holds, whatever their labels do.
    fn chapter_rules(svg: &str) -> usize {
        svg.split_once("<g class=\"marks\">")
            .expect("the marks group")
            .1
            .split_once("<g class=\"chapters\">")
            .expect("the chapters group")
            .1
            .matches("<line class=\"mark\"")
            .count()
    }

    /// Decision 24 along the top row: a chapter label and a band label that
    /// begin at one instant cannot both be drawn, so one is dropped rather
    /// than written over the other.
    #[test]
    fn a_chapter_label_and_a_band_label_at_one_instant_cannot_both_draw() {
        let spec = Spec {
            band: Some(Band {
                t0: 1.2,
                t1: 1.6,
                label: "settle".to_owned(),
            }),
            ..demo()
        };
        // the band opens where the demo's own chapter does
        assert_eq!(spec.chapters[1].t, spec.band.as_ref().expect("a band").t0);
        let l = Layout::sized(640.0, 240.0, spec.end);
        // the two boxes the emitter's own ruler asks for, overlapping by
        // far more than the clear space a shared row needs
        let chapter = l.x_of(1.2) + CHAPTER_LABEL_X;
        let ends = chapter + label_width("settle");
        let opens = (l.x_of(1.2) + l.x_of(1.6)) / 2.0 - label_width("settle") / 2.0;
        assert!(opens < ends, "{opens} is clear of {ends}");
        let svg = render(&spec, l).svg;
        // the chapter's draws and the band's is dropped: the pass runs left
        // to right, so the earlier label is placed first and keeps its place
        assert_eq!(labels_of(&svg, "marklabel"), ["settle"]);
        assert_eq!(labels_of(&svg, "band-label"), Vec::<String>::new(), "{svg}");
        assert_eq!(
            attribute(&svg, "<text class=\"marklabel\"", "x"),
            format!("{chapter:.1}")
        );
        // and neither cue loses more than the word: the chapter keeps its
        // rule and the band its wash, the one rect of the block's own class
        assert_eq!(chapter_rules(&svg), 1);
        assert_eq!(svg.matches("<rect class=\"band\"").count(), 1);
        // room enough and both are drawn: the same band, moved along
        let apart = Spec {
            band: Some(Band {
                t0: 1.9,
                t1: 2.6,
                label: "settle".to_owned(),
            }),
            ..demo()
        };
        let svg = render(&apart, l).svg;
        assert_eq!(labels_of(&svg, "marklabel"), ["settle"]);
        assert_eq!(labels_of(&svg, "band-label"), ["settle"]);
    }

    /// The end labels own the top right corner: they name the series, which
    /// is the one thing colour alone may not say, so a cue label that would
    /// reach into their column is the one that goes.
    #[test]
    fn a_cue_label_that_reaches_into_the_end_label_column_is_dropped() {
        let spec = Spec {
            chapters: vec![
                Chapter {
                    t: 0.0,
                    label: "start".to_owned(),
                },
                Chapter {
                    t: 2.3,
                    label: "settled".to_owned(),
                },
            ],
            ..demo()
        };
        let l = Layout::sized(640.0, 240.0, spec.end);
        let svg = render(&spec, l).svg;
        // the column the one end label takes: its own anchor back by the
        // room its text needs, read off the markup
        let column =
            number(&svg, "<text class=\"endlabel\"", "x") - label_width(&spec.series[0].label);
        // the chapter label wants a box that ends well inside the plot and
        // runs into that column, so the column is what drops it
        let right = l.x_of(2.3) + CHAPTER_LABEL_X + label_width("settled");
        assert!(
            right + ROW_LABEL_GAP < l.width - l.right,
            "{right} is off the plot, not into the column"
        );
        assert!(
            right + ROW_LABEL_GAP > column,
            "{right} clears the column at {column}"
        );
        assert_eq!(labels_of(&svg, "marklabel"), Vec::<String>::new(), "{svg}");
        // the cue itself is untouched: its rule, its tick and its target
        assert_eq!(chapter_rules(&svg), 1);
        assert_eq!(svg.matches("<rect class=\"chapter\"").count(), 1);
        assert_eq!(targets(&svg).len(), 1);
        // and the column is the only thing in its way: with no end label to
        // reserve it, the same chapter at the same x keeps its label
        let mut bare = spec.clone();
        bare.series[0].label.clear();
        let svg = render(&bare, l).svg;
        assert!(!svg.contains("endlabel"));
        assert_eq!(labels_of(&svg, "marklabel"), ["settled"]);
    }

    /// A cue with nothing to say writes no label and reserves no room.
    #[test]
    fn a_cue_with_no_label_neither_draws_nor_stands_in_the_way() {
        let mut spec = Spec {
            band: Some(Band {
                t0: 1.9,
                t1: 2.6,
                label: "settle".to_owned(),
            }),
            ..demo()
        };
        spec.chapters.push(Chapter {
            t: 2.1,
            label: String::new(),
        });
        let l = Layout::sized(640.0, 240.0, spec.end);
        // the nameless chapter is where a label of any width at all would
        // be placed before the band's and leave it no room
        let empty = l.x_of(2.1) + CHAPTER_LABEL_X;
        let opens = (l.x_of(1.9) + l.x_of(2.6)) / 2.0 - label_width("settle") / 2.0;
        assert!(
            empty < opens && opens - empty < ROW_LABEL_GAP,
            "{empty} does not crowd {opens}"
        );
        let svg = render(&spec, l).svg;
        // so the band keeps its label, and the row carries no empty text
        assert_eq!(labels_of(&svg, "band-label"), ["settle"]);
        assert_eq!(labels_of(&svg, "marklabel"), ["settle"]);
        // the chapter with nothing to say still draws its rule and its tick
        assert_eq!(chapter_rules(&svg), 2);
        assert_eq!(svg.matches("<rect class=\"chapter\"").count(), 2);
        // and a band with nothing to say draws its wash and no label
        let nameless = Spec {
            band: Some(Band {
                t0: 1.9,
                t1: 2.6,
                label: String::new(),
            }),
            ..demo()
        };
        let svg = render(&nameless, l).svg;
        assert!(!svg.contains("band-label"), "{svg}");
        assert_eq!(svg.matches("<rect class=\"band\"").count(), 1);
    }

    /// A chart that annotates nothing draws nothing for it.
    #[test]
    fn a_spec_without_marks_or_a_band_draws_neither() {
        for svg in [film(&demo()).svg, film(&flight()).svg, sized(&empty()).svg] {
            assert!(!svg.contains("bands"), "a band group with no band");
            // no rect carries the block's class at all: the wash the film
            // widens at runtime is the peek band, which is its own thing
            assert_eq!(svg.matches("<rect class=\"band\"").count(), 0);
            assert!(svg.contains("<rect class=\"peek-band\" x=\"46\" y=\"16\" width=\"0\""));
            assert!(!svg.contains("mark-label"), "a mark label with no mark");
            assert!(
                !svg.contains("data-cue=\"mark\""),
                "a mark target with no mark"
            );
            // the marks group stays: it is the chapters' home as well
            assert!(svg.contains("<g class=\"marks\"><g class=\"chapters\">"));
        }
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
        "bands",
        "band-label",
        "marks",
        "chapters",
        "mark",
        "marklabel",
        "mark-label",
        "series",
        "marker",
        "shown",
        "swatch",
        "endlabel",
        "track",
        "band",
        "peek-band",
        "bar-bg",
        "bar-played",
        "chapter",
        "cursor",
        "peek-line",
        "playhead",
        "head",
        "head-dot",
        "head-t",
        "head-ring",
        "targets",
        "target",
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
    /// chapter cue at once. `bands` opens the drawing: the band is a wash
    /// every other thing is drawn over. `targets` closes the list: the
    /// invisible hit rects are written after everything they stand for,
    /// because paint order is what hands them the pointer.
    const GROUP_ORDER: &[&str] = &[
        "axes", "bands", "marks", "chapters", "series", "track", "chapters", "cursor", "playhead",
        "targets",
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
            film(&annotated()).svg,
            sized(&annotated()).svg,
            sized(&empty()).svg,
            sized(&gaps()).svg,
            sized(&many()).svg,
        ];
        let mut seen: BTreeSet<String> = BTreeSet::new();
        for svg in &outputs {
            for c in classes(svg) {
                let known = EMITTED_CLASSES.contains(&c.as_str())
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
