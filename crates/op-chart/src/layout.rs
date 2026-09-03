//! The chart box and its scales, shared by the emitter and by the element
//! for hit-testing, so both agree on where a time is.

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
}

impl Layout {
    /// The film's chart: 900 by 268 with room for a value axis on the left
    /// and a time axis, readout and track along the bottom.
    pub fn film(end: f64) -> Self {
        Self {
            width: 900.0,
            height: 268.0,
            left: 46.0,
            right: 14.0,
            top: 16.0,
            bottom: 48.0,
            end: if end > 0.0 { end } else { 1.0 },
            y_min: -4.0,
            y_max: 106.0,
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

    /// Baseline of the text in the axis band under the plot: the time-axis
    /// labels and the playhead readout, which sits there rather than over
    /// the data.
    pub fn axis_label_y(&self) -> f64 {
        self.plot_bottom() + 16.0
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
        assert_eq!(l.track_y(), 258.0);
    }

    #[test]
    fn a_degenerate_end_becomes_one_second() {
        assert_eq!(Layout::film(0.0).end, 1.0);
        assert_eq!(Layout::film(-2.0).end, 1.0);
    }

    proptest! {
        #[test]
        fn t_at_inverts_x_of_inside_the_axis(end in 0.1f64..1000.0, t in 0.0f64..1.0) {
            let l = Layout::film(end);
            let t = t * end;
            let back = l.t_at(l.x_of(t));
            prop_assert!((back - t).abs() <= end * 1e-9, "{t} -> {back}");
        }
    }
}
