//! What a chart shows: series on one shared time axis, chapters, the
//! marks and band a block annotates them with, labels.

/// A named point on the time axis. The first chapter is the start and
/// draws no mark; every later one draws a vertical rule and a tick on the
/// track.
#[derive(Clone, Debug, PartialEq)]
pub struct Chapter {
    pub t: f64,
    pub label: String,
}

/// A labelled instant on the time axis: a thin rule across the plot with
/// its label along the bottom edge, dropped when the edge has no room for
/// it. Where a chapter starts a section, a mark points at one moment.
#[derive(Clone, Debug, PartialEq)]
pub struct Mark {
    pub t: f64,
    pub label: String,
}

/// A labelled span of the time axis, drawn as a wash behind the series.
#[derive(Clone, Debug, PartialEq)]
pub struct Band {
    pub t0: f64,
    pub t1: f64,
    pub label: String,
}

/// One sampled series.
#[derive(Clone, Debug, PartialEq)]
pub struct Series {
    /// Direct end label, placed at the series' last point and kept clear of
    /// its neighbours; empty for none, in which case the series carries
    /// markers instead.
    pub label: String,
    /// Which palette series this is, 1 to 6: the emitter writes the class
    /// `series-N` and the consumer's stylesheet maps it to `--op-series-N`.
    /// This crate never writes a colour.
    pub index: usize,
    /// `(t, value)` pairs in time order, `None` for a gap the line breaks
    /// at; values are clamped to the layout's value domain when drawn.
    pub points: Vec<Option<(f64, f64)>>,
    /// Stroke width in px; 2 is the palette's design width.
    pub width: f64,
}

/// Everything [`crate::render`] draws from.
#[derive(Clone, Debug, PartialEq)]
pub struct Spec {
    /// End of the time axis in seconds; the axis starts at 0.
    pub end: f64,
    /// The slider's `aria-valuemax`: at least `end`.
    pub duration: f64,
    /// The value domain the chart is drawn on, `(lo, hi)` with lo below hi;
    /// the layout is put on it before anything is drawn, so the gridlines,
    /// their labels and every point follow the data's own range.
    /// [`crate::layout::PERCENT`] is the percent scale the film uses.
    pub y: (f64, f64),
    pub ylabel: String,
    pub chapters: Vec<Chapter>,
    /// Labelled instants over the plot; empty for a chart that annotates
    /// nothing, which is what the film passes.
    pub marks: Vec<Mark>,
    /// The one labelled span drawn behind the series, when there is one.
    pub band: Option<Band>,
    pub series: Vec<Series>,
}
