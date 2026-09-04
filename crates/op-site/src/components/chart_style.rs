//! The chart rules two elements share: the film's own chart and
//! `<opt-chart>` draw the same classed SVG, so the palette table, the
//! forced-colours mapping and the print mapping live here once and each
//! element's stylesheet includes them whole.

/// How many `--op-series-N` tokens the palette defines.
pub(crate) const SERIES_TOKENS: usize = 6;

/// The parts of the chart both stylesheets paint identically: the axes,
/// the cue rules and their labels, the end labels, the swatch, the latent
/// markers, the annotation band's edge, and the high-contrast block.
///
/// Every label the emitter draws inside the plot carries decision 24's
/// halo, a 3 px stroke in the surface colour under the glyphs, so a label
/// over a series line is still a label over the surface. The band carries
/// the surface-coloured edge that is the same decision's gap where it
/// meets a series line; the peek band, which is the same shape and a
/// different thing, is a wash with no edge and is painted elsewhere.
pub(crate) const CHART_SHAPE_CSS: &str = ".chart .grid { stroke: var(--op-border); shape-rendering: crispEdges; } .chart .tick { stroke: var(--op-border-strong); } .chart .axis { fill: var(--op-muted); }
.chart .mark { stroke: var(--op-accent); stroke-dasharray: 3 3; }
.chart .marklabel { fill: var(--op-accent); paint-order: stroke; stroke: var(--op-surface); stroke-width: 3; }
.chart .mark-label, .chart .band-label { fill: var(--op-text); paint-order: stroke; stroke: var(--op-surface); stroke-width: 3; }
.chart .endlabel { fill: var(--op-text); font-size: 12px; font-weight: 700; paint-order: stroke; stroke: var(--op-surface); stroke-width: 3; }
.chart .swatch { stroke-width: 3; shape-rendering: crispEdges; }
.chart .marker { display: none; fill: var(--op-surface); stroke-width: 1.5; stroke-dasharray: none; } .chart .marker.shown { display: inline; }
.chart .band { stroke: var(--op-surface); }
@media (prefers-contrast: more) { .chart .grid { stroke: var(--op-border-strong); } .chart path[class^=series] { stroke-width: 3; } .chart .marker { display: inline; } }";

/// The chapter tick on the track and the peek rule under the pointer, the
/// two cues both stylesheets paint from the same tokens. A block of their
/// own rather than part of [`CHART_SHAPE_CSS`] because they are written
/// after the band and the track, which each element paints from a token of
/// its own.
pub(crate) const CHART_CUE_CSS: &str = ".chart .chapter { fill: var(--op-surface); stroke: var(--op-border-strong); stroke-width: 0.6; }
.chart .peek-line { stroke: var(--op-peek); stroke-dasharray: 3 3; }";

/// Series lines, swatches and markers take their colour from the palette
/// tokens and their dash from this table, so colour is never the only cue;
/// the markup carries only the class. Butt caps keep the pattern legible at
/// 2 px. A series line and a marker are both paths, so the dash rules keep
/// `[class^=series]`, which only the line's own class begins with.
pub(crate) const SERIES_CSS: &str = "
.chart path[class^=series] { stroke-linecap: butt; }
.chart .series-1 { stroke: var(--op-series-1); }
.chart .series-2 { stroke: var(--op-series-2); } .chart path[class^=series].series-2 { stroke-dasharray: 8 4; }
.chart .series-3 { stroke: var(--op-series-3); } .chart path[class^=series].series-3 { stroke-dasharray: 2 3; }
.chart .series-4 { stroke: var(--op-series-4); } .chart path[class^=series].series-4 { stroke-dasharray: 8 4 2 4; }
.chart .series-5 { stroke: var(--op-series-5); } .chart path[class^=series].series-5 { stroke-dasharray: 12 4; }
.chart .series-6 { stroke: var(--op-series-6); } .chart path[class^=series].series-6 { stroke-dasharray: 4 2 4 6; }";

/// Forced-colours mode keeps SVG author colours, so every paint is mapped
/// to a system colour here: series, axes and every label on CanvasText with
/// the dashes and markers carrying identity, the played region and playhead
/// on Highlight, both bands as an outline.
pub(crate) const FORCED_COLOURS_CSS: &str = "
@media (forced-colors: active) {
  .chart { forced-color-adjust: auto; background: Canvas; }
  .chart path[class^=series], .chart .swatch, .chart .tick, .chart .mark, .chart .chapter, .chart .peek-line { stroke: CanvasText; }
  .chart .marker { display: inline; stroke: CanvasText; fill: Canvas; }
  .chart .axis, .chart .marklabel, .chart .mark-label, .chart .band-label, .chart .endlabel, .chart .head-t { fill: CanvasText; stroke: Canvas; }
  .chart .grid { stroke: GrayText; }
  .chart .band, .chart .peek-band { fill: none; stroke: CanvasText; stroke-dasharray: 4 3; opacity: 1; fill-opacity: 1; }
  .chart .bar-bg { fill: GrayText; } .chart .bar-played { fill: Highlight; }
  .chart .head { stroke: Highlight; } .chart .head-dot { fill: Highlight; }
}";

/// Print keeps no palette: everything goes to print blacks and greys on
/// white, the dashes and markers carry identity, and both bands are an
/// outline. Backgrounds are forced to print so the halos still work.
pub(crate) const PRINT_CSS: &str = "
@media print {
  .chart { print-color-adjust: exact; -webkit-print-color-adjust: exact; background: white; }
  .chart path[class^=series], .chart .swatch, .chart .tick, .chart .mark, .chart .chapter { stroke: black; }
  .chart .marker { display: inline; stroke: black; fill: white; }
  .chart .axis, .chart .marklabel, .chart .mark-label, .chart .band-label, .chart .endlabel, .chart .head-t { fill: black; stroke: white; }
  .chart .grid { stroke: #bbbbbb; }
  .chart .band, .chart .peek-band { fill: none; stroke: black; stroke-dasharray: 4 3; opacity: 1; fill-opacity: 1; }
  .chart .bar-bg { fill: #dddddd; } .chart .bar-played { fill: #555555; }
  .chart .head { stroke: black; } .chart .head-dot { fill: black; }
  .chart .peek-line { display: none; }
}";

/// The chart's static rules, in the order the stylesheet carries them.
/// Both stylesheets that include this put it last, after every rule that
/// paints a token: at equal specificity the later rule wins, media query or
/// not, so a token rule written after the forced-colours block would put
/// the token straight back on a forced palette, and the same in print.
pub(crate) fn chart_rules() -> String {
    format!("{SERIES_CSS}\n{FORCED_COLOURS_CSS}\n{PRINT_CSS}")
}
