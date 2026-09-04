//! The chart box and its scales, shared by the emitter and by the element
//! for hit-testing, so both agree on where a time is.

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
    }
}
