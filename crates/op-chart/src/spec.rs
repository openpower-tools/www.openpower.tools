//! What a chart shows: series on one shared time axis, chapters, labels.

/// A named point on the time axis. The first chapter is the start and
/// draws no mark; every later one draws a vertical rule and a tick on the
/// track.
#[derive(Clone, Debug, PartialEq)]
pub struct Chapter {
    pub t: f64,
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
    /// `(t, value)` pairs in time order; values are clamped to the layout's
    /// value domain when drawn.
    pub points: Vec<(f64, f64)>,
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
    pub ylabel: String,
    pub chapters: Vec<Chapter>,
    pub series: Vec<Series>,
}
