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
    /// Direct label drawn near the point at `label_at`; empty for none.
    pub label: String,
    /// The stroke as the spec passes it. Empty means `currentColor`; this
    /// crate never invents a colour of its own.
    pub colour: String,
    /// `(t, value)` pairs in time order; values are clamped to the layout's
    /// value domain when drawn.
    pub points: Vec<(f64, f64)>,
    pub dash: bool,
    pub width: f64,
    /// Fraction along the series (0..1) at which the label sits.
    pub label_at: f64,
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
