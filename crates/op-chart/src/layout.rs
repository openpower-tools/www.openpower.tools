//! The chart box and its scales, shared by the emitter and by the element
//! for hit-testing, so both agree on where a time is. The text scale is
//! one of them: what a label measures is what the drawing has to leave
//! room for, so the correction for a face the browser never got belongs
//! beside the geometry it changes.

use crate::labels::{Face, TEXT_PX};

/// The percent scale the film's chart is drawn on: 0 to 100 with a little
/// room below and rather more above, where the end labels sit. A box takes
/// it until a spec puts it on the data's own domain.
pub const PERCENT: (f64, f64) = (-4.0, 106.0);

/// Geometry in viewBox units (CSS px at the design width).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layout {
    pub width: f64,
    pub height: f64,
    /// Margins around the plot rectangle.
    pub left: f64,
    pub right: f64,
    pub top: f64,
    pub bottom: f64,
    /// End of the time axis (start is 0).
    pub end: f64,
    /// Value domain, drawn top to bottom.
    pub y_min: f64,
    pub y_max: f64,
    /// Whether strokes keep their width when the box is scaled to fit its
    /// container: true for a box measured in CSS px, which a stylesheet may
    /// then scale, false for the film's fixed chart.
    pub non_scaling: bool,
    /// What every measured text width is multiplied by before the drawing
    /// reserves room for it: 1 wherever the text is set in the face the
    /// advance tables were read from, which is every load but one where
    /// that face never arrives ([`Self::with_text_scale`]).
    pub text_scale: f64,
}

impl Layout {
    /// A chart in any box, the width and height in CSS px, with the film's
    /// margins: room for a value axis on the left and a time axis, readout
    /// and track along the bottom. The viewBox equals the box, so a user
    /// unit is a CSS pixel and the strokes are marked non-scaling in case
    /// the element is scaled after layout.
    pub fn sized(width: f64, height: f64, end: f64) -> Self {
        Self {
            width,
            height,
            left: 46.0,
            right: 14.0,
            top: 16.0,
            bottom: 48.0,
            end: if end > 0.0 { end } else { 1.0 },
            y_min: PERCENT.0,
            y_max: PERCENT.1,
            non_scaling: true,
            text_scale: 1.0,
        }
    }

    /// The same box over another value domain. A domain that is not an
    /// interval is no domain at all and leaves the box as it was, so the
    /// scale can never divide by zero.
    #[must_use]
    pub fn with_y(mut self, lo: f64, hi: f64) -> Self {
        if hi > lo {
            self.y_min = lo;
            self.y_max = hi;
        }
        self
    }

    /// The same box with every text measurement scaled by `scale`.
    ///
    /// The advance tables are a measurement of the face the site serves,
    /// so they are exact wherever a browser draws with it and this is 1.
    /// Where that face is blocked or fails to load the browser sets the
    /// chart in something else, and a consumer that can see what that
    /// something else measures passes the ratio it found. That ratio is an
    /// estimate for a substituted face and not a measurement of it: one
    /// number stands for every string and for both weights, and the face
    /// a canvas substitutes need not be the one the page falls back to.
    /// It is enough because it is only ever reached when the drawing would
    /// otherwise be laid out for a face nothing on screen is set in, and
    /// what it buys is room on the right order rather than labels written
    /// over each other.
    ///
    /// A scale that is not a positive finite number is no measurement and
    /// leaves the box as it was, so a browser that answers with nothing
    /// cannot collapse the drawing.
    #[must_use]
    pub fn with_text_scale(mut self, scale: f64) -> Self {
        if scale.is_finite() && scale > 0.0 {
            self.text_scale = scale;
        }
        self
    }

    /// The room `text` takes in this box: its advances at the size both
    /// consumers' stylesheets set (decision 14), corrected by
    /// [`Self::text_scale`]. Every width the emitter reserves and every
    /// `textLength` it pins comes through here, so a drawing laid out for
    /// a substituted face is corrected in one place.
    ///
    /// The estimate this replaced was 6.5 px to the character, which is
    /// wrong in both directions and by more than the clear space a row
    /// asks for: it claims 71.5 px for "first frame", which sets in 55.9,
    /// and 13 px for "WW", which sets in 21.4.
    pub fn label_width(&self, text: &str, face: Face) -> f64 {
        crate::labels::text_width(text, TEXT_PX, face) * self.text_scale
    }

    /// The film's chart: 900 by 268, scaled to the page by `max-width`, so
    /// its strokes scale with it as they always have.
    pub fn film(end: f64) -> Self {
        Self {
            non_scaling: false,
            ..Self::sized(900.0, 268.0, end)
        }
    }

    pub fn plot_width(&self) -> f64 {
        self.width - self.left - self.right
    }

    pub fn plot_height(&self) -> f64 {
        self.height - self.top - self.bottom
    }

    pub fn plot_bottom(&self) -> f64 {
        self.height - self.bottom
    }

    /// Baseline of the time-axis labels in the axis band under the plot.
    pub fn axis_label_y(&self) -> f64 {
        self.plot_bottom() + 16.0
    }

    /// Baseline of the playhead readout: in the axis band below the tick
    /// labels and above the track, so it never sits over the data and never
    /// overprints a tick label as it travels.
    pub fn readout_y(&self) -> f64 {
        self.plot_bottom() + 30.0
    }

    /// Centre line of the track bar under the axis.
    pub fn track_y(&self) -> f64 {
        self.height - 10.0
    }

    /// Horizontal position of a time, clamped to the axis.
    pub fn x_of(&self, t: f64) -> f64 {
        self.left + (t / self.end).clamp(0.0, 1.0) * self.plot_width()
    }

    /// The time under a horizontal position, clamped to the axis.
    pub fn t_at(&self, x: f64) -> f64 {
        ((x - self.left) / self.plot_width() * self.end).clamp(0.0, self.end)
    }

    /// The sample nearest a horizontal position, as its index and its own
    /// time, when it lies within `radius` CSS px of that position, and
    /// nothing at all when the nearest one is further away or there are no
    /// samples to hit. Only x is compared: a pointer anywhere over the
    /// plot takes the sample in its column, whatever height it is at, so
    /// the same answer serves a hover high above a line and a press on it.
    /// `times` is the series' own order, non-decreasing, and may be empty.
    /// A tie goes to the earlier sample, so a pointer midway between two
    /// takes the one that happened first.
    pub fn nearest(&self, x: f64, times: &[f64], radius: f64) -> Option<(usize, f64)> {
        let mut best: Option<(usize, f64)> = None;
        for (i, t) in times.iter().enumerate() {
            let away = (self.x_of(*t) - x).abs();
            // strictly nearer, so a tie leaves the earlier sample standing
            if best.is_none_or(|(_, near)| away < near) {
                best = Some((i, away));
            }
        }
        best.filter(|(_, away)| *away <= radius)
            .map(|(i, _)| (i, times[i]))
    }

    /// Vertical position of a value, clamped to the value domain.
    pub fn y_of(&self, v: f64) -> f64 {
        self.top
            + (self.y_max - v.clamp(self.y_min, self.y_max)) / (self.y_max - self.y_min)
                * self.plot_height()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn the_axis_spans_the_plot_and_the_value_domain_spans_its_height() {
        let l = Layout::film(3.7);
        assert_eq!(l.x_of(0.0), 46.0);
        assert_eq!(l.x_of(3.7), 886.0);
        assert_eq!(l.x_of(-1.0), 46.0);
        assert_eq!(l.x_of(99.0), 886.0);
        assert_eq!(l.y_of(106.0), 16.0);
        assert_eq!(l.y_of(-4.0), 220.0);
        assert!(l.y_of(0.0) > l.y_of(100.0));
        assert_eq!(l.axis_label_y(), 236.0);
        assert_eq!(l.readout_y(), 250.0);
        assert!(
            l.readout_y() < l.track_y() - 3.0 - 4.0,
            "the readout clears the chapter ticks"
        );
        assert_eq!(l.track_y(), 258.0);
    }

    #[test]
    fn a_box_takes_the_value_domain_it_is_given_and_refuses_a_degenerate_one() {
        let l = Layout::sized(640.0, 240.0, 3.0);
        assert_eq!((l.y_min, l.y_max), PERCENT);
        let l = l.with_y(0.0, 1000.0);
        assert_eq!((l.y_min, l.y_max), (0.0, 1000.0));
        // the domain spans the plot, top to bottom, and nothing in it clamps
        assert_eq!(l.y_of(1000.0), l.top);
        assert_eq!(l.y_of(0.0), l.plot_bottom());
        assert!((l.y_of(500.0) - (l.top + l.plot_height() / 2.0)).abs() < 1e-9);
        // an empty or inverted domain is not a domain: the box keeps its own
        let kept = l.with_y(5.0, 5.0).with_y(9.0, 2.0);
        assert_eq!((kept.y_min, kept.y_max), (0.0, 1000.0));
    }

    /// The text scale multiplies what a label measures and nothing else
    /// about the box, and only a positive finite number is a measurement.
    #[test]
    fn a_box_scales_its_text_by_a_measurement_and_refuses_anything_else() {
        let l = Layout::sized(640.0, 240.0, 3.0);
        assert_eq!(l.text_scale, 1.0);
        for face in [Face::Regular, Face::Bold] {
            // an unscaled box measures the tables, and a scaled one
            // measures them times the scale
            let table = crate::labels::text_width("first frame", TEXT_PX, face);
            assert!(table > 0.0);
            assert!((l.label_width("first frame", face) - table).abs() < 1e-9);
            let wide = l.with_text_scale(1.25).label_width("first frame", face);
            assert!((wide - table * 1.25).abs() < 1e-9, "{wide} for {table}");
        }
        // nothing that is not a measurement is taken for one
        for bad in [0.0, -1.5, f64::NAN, f64::INFINITY] {
            assert_eq!(l.with_text_scale(bad).text_scale, 1.0, "{bad}");
        }
        // the box is otherwise the box it was: a correction to the text is
        // not a change to the geometry
        let scaled = l.with_text_scale(1.25);
        assert_eq!(
            Layout {
                text_scale: 1.0,
                ..scaled
            },
            l
        );
        // and it survives a re-domained box, since that is one drawing
        assert_eq!(scaled.with_y(0.0, 10.0).text_scale, 1.25);
    }

    #[test]
    fn a_degenerate_end_becomes_one_second() {
        assert_eq!(Layout::film(0.0).end, 1.0);
        assert_eq!(Layout::film(-2.0).end, 1.0);
        assert_eq!(Layout::sized(640.0, 240.0, 0.0).end, 1.0);
    }

    #[test]
    fn any_box_keeps_the_films_margins_and_marks_its_strokes_non_scaling() {
        let l = Layout::sized(640.0, 240.0, 4.0);
        assert_eq!((l.width, l.height), (640.0, 240.0));
        assert_eq!((l.left, l.right, l.top, l.bottom), (46.0, 14.0, 16.0, 48.0));
        assert_eq!((l.plot_width(), l.plot_height()), (580.0, 176.0));
        assert_eq!(l.x_of(0.0), 46.0);
        assert_eq!(l.x_of(4.0), 626.0);
        assert!(l.non_scaling);
        // the film's box is 900 by 268 with the same margins and scales
        let f = Layout::film(4.0);
        assert_eq!((f.width, f.height), (900.0, 268.0));
        assert_eq!(
            (f.left, f.right, f.top, f.bottom),
            (l.left, l.right, l.top, l.bottom)
        );
        assert_eq!((f.y_min, f.y_max, f.end), (l.y_min, l.y_max, l.end));
        // and its strokes scale with the page, as they always have
        assert!(!f.non_scaling);
    }

    #[test]
    fn the_nearest_sample_is_the_one_in_the_pointers_column_inside_the_radius() {
        let l = Layout::sized(640.0, 240.0, 2.0);
        // the plot runs 46 to 626, so a second of this axis is 290 px
        assert_eq!(
            (l.x_of(0.0), l.x_of(1.0), l.x_of(2.0)),
            (46.0, 336.0, 626.0)
        );
        // nothing sampled is nothing to hit, however wide the radius
        assert_eq!(l.nearest(300.0, &[], 1e9), None);
        // one sample: hit up to the radius, missed past it, the boundary
        // counting as a hit
        assert_eq!(l.nearest(336.0, &[1.0], 24.0), Some((0, 1.0)));
        assert_eq!(l.nearest(360.0, &[1.0], 24.0), Some((0, 1.0)));
        assert_eq!(l.nearest(312.0, &[1.0], 24.0), Some((0, 1.0)));
        assert_eq!(l.nearest(360.1, &[1.0], 24.0), None);
        assert_eq!(l.nearest(311.9, &[1.0], 24.0), None);
        // a pointer left of the plot takes the first sample and one right
        // of it the last, when the radius reaches that far and not before
        let times = [0.0, 1.0, 2.0];
        assert_eq!(l.nearest(0.0, &times, 46.0), Some((0, 0.0)));
        assert_eq!(l.nearest(0.0, &times, 45.9), None);
        assert_eq!(l.nearest(700.0, &times, 74.0), Some((2, 2.0)));
        assert_eq!(l.nearest(700.0, &times, 73.9), None);
        // and inside the plot it is the column that decides
        assert_eq!(l.nearest(200.0, &times, 24.0), None);
        assert_eq!(l.nearest(200.0, &times, 200.0), Some((1, 1.0)));
    }

    #[test]
    fn a_tie_between_two_samples_goes_to_the_earlier_one() {
        let l = Layout::sized(640.0, 240.0, 2.0);
        // 336 is 290 px from each of the samples at 46 and at 626
        assert_eq!(l.nearest(336.0, &[0.0, 2.0], 300.0), Some((0, 0.0)));
        // two samples at one instant are one place, and the earlier wins
        assert_eq!(l.nearest(500.0, &[1.0, 1.0], 400.0), Some((0, 1.0)));
    }

    proptest! {
        #[test]
        fn t_at_inverts_x_of_inside_the_axis(end in 0.1f64..1000.0, t in 0.0f64..1.0) {
            let l = Layout::film(end);
            let t = t * end;
            let back = l.t_at(l.x_of(t));
            prop_assert!((back - t).abs() <= end * 1e-9, "{t} -> {back}");
        }

        /// The same round trip in any box a container query may hand us,
        /// and the value scale ordered: a larger value never sits lower.
        #[test]
        fn any_sized_box_round_trips_a_time_and_orders_its_values(
            w in 200.0f64..2000.0, h in 100.0f64..1000.0, end in 1.0f64..10000.0,
            f in 0.0f64..1.0, a in -10.0f64..110.0, b in -10.0f64..110.0
        ) {
            let l = Layout::sized(w, h, end);
            let t = f * end;
            let back = l.t_at(l.x_of(t));
            prop_assert!((back - t).abs() <= end * 1e-9, "{t} -> {back}");
            let (lo, hi) = if a < b { (a, b) } else { (b, a) };
            prop_assert!(l.y_of(lo) >= l.y_of(hi), "{lo} at {} is above {hi} at {}", l.y_of(lo), l.y_of(hi));
            prop_assert!(l.y_of(hi) >= l.top && l.y_of(lo) <= l.plot_bottom());
        }

        /// In any box, over any non-decreasing times and from anywhere on
        /// or off the plot: what comes back is a sample at the least
        /// distance, the first of them where several tie, and nothing at
        /// all exactly when that least distance is past the radius.
        #[test]
        fn the_nearest_sample_minimises_the_distance_and_answers_inside_the_radius(
            w in 200.0f64..2000.0, h in 100.0f64..1000.0, end in 1.0f64..1000.0,
            fractions in prop::collection::vec(0.0f64..1.0, 0..24),
            fx in -0.3f64..1.3, radius in 0.0f64..300.0
        ) {
            let l = Layout::sized(w, h, end);
            let mut times: Vec<f64> = fractions.iter().map(|f| f * end).collect();
            times.sort_by(f64::total_cmp);
            let x = l.left + fx * l.plot_width();
            let away = |t: f64| (l.x_of(t) - x).abs();
            // the same distances the box itself measures, so the least of
            // them is one of them exactly
            let least = times.iter().copied().map(away).fold(f64::INFINITY, f64::min);
            match l.nearest(x, &times, radius) {
                Some((i, t)) => {
                    prop_assert!(i < times.len());
                    prop_assert_eq!(t, times[i]);
                    prop_assert_eq!(away(t), least, "sample {} of {:?}", i, times);
                    prop_assert!(least <= radius);
                    let first = times.iter().position(|t| away(*t) == least);
                    prop_assert_eq!(Some(i), first, "a tie goes to the earlier sample");
                }
                None => prop_assert!(times.is_empty() || least > radius, "{least} within {radius}"),
            }
        }
    }
}
