//! The chart rules two elements share: the film's own chart and
//! `<opt-chart>` draw the same classed SVG, so the palette table, the
//! forced-colours mapping and the print mapping live here once and each
//! element's stylesheet includes them whole.

/// How many `--op-series-N` tokens the palette defines.
pub(crate) const SERIES_TOKENS: usize = 6;

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
/// to a system colour here: series, axes and text on CanvasText with the
/// dashes and markers carrying identity, the played region and playhead on
/// Highlight, the band as an outline.
pub(crate) const FORCED_COLOURS_CSS: &str = "
@media (forced-colors: active) {
  .chart { forced-color-adjust: auto; background: Canvas; }
  .chart path[class^=series], .chart .swatch, .chart .tick, .chart .mark, .chart .chapter, .chart .peek-line { stroke: CanvasText; }
  .chart .marker { display: inline; stroke: CanvasText; fill: Canvas; }
  .chart .axis, .chart .marklabel, .chart .endlabel, .chart .head-t { fill: CanvasText; stroke: Canvas; }
  .chart .grid { stroke: GrayText; }
  .chart .band { fill: none; stroke: CanvasText; stroke-dasharray: 4 3; opacity: 1; }
  .chart .bar-bg { fill: GrayText; } .chart .bar-played { fill: Highlight; }
  .chart .head { stroke: Highlight; } .chart .head-dot { fill: Highlight; }
}";

/// Print keeps no palette: everything goes to print blacks and greys on
/// white, the dashes and markers carry identity, and the band is an
/// outline. Backgrounds are forced to print so the halos still work.
pub(crate) const PRINT_CSS: &str = "
@media print {
  .chart { print-color-adjust: exact; -webkit-print-color-adjust: exact; background: white; }
  .chart path[class^=series], .chart .swatch, .chart .tick, .chart .mark, .chart .chapter { stroke: black; }
  .chart .marker { display: inline; stroke: black; fill: white; }
  .chart .axis, .chart .marklabel, .chart .endlabel, .chart .head-t { fill: black; stroke: white; }
  .chart .grid { stroke: #bbbbbb; }
  .chart .band { fill: none; stroke: black; stroke-dasharray: 4 3; opacity: 1; }
  .chart .bar-bg { fill: #dddddd; } .chart .bar-played { fill: #555555; }
  .chart .head { stroke: black; } .chart .head-dot { fill: black; }
  .chart .peek-line { display: none; }
}";

/// The chart's static rules, in the order the stylesheet carries them.
pub(crate) fn chart_rules() -> String {
    format!("{SERIES_CSS}\n{FORCED_COLOURS_CSS}\n{PRINT_CSS}")
}
