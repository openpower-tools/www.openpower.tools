//! Does the browser draw the chart's labels where the emitter placed them?
//!
//! `op-chart` already checks that no two labels overlap, computing every
//! box from the advance tables. That is the emitter marking its own
//! homework: it proves the placement agrees with the ruler it was placed
//! by, and says nothing about the ruler, nor about what the page's own
//! stylesheet does to the text once it is on a screen. A rule that set
//! another size, or letter-spacing, or a family the tables were not read
//! from, would move every label and leave that check green.
//!
//! So this reads the same labels out of a real page in a real browser,
//! with the site's own css, and measures each one with
//! `getComputedTextLength`, which is the browser's answer and not ours.
//! The boxes are advance boxes, as the emitter's are, so the two checks
//! make the same claim about the same rectangles and can only disagree if
//! the browser disagrees with the tables. What the paint covers is
//! recorded beside each label, so the report can also say how much room
//! the ink really left.
//!
//! The second pass loads the page with the served faces blocked, which is
//! the case the advance tables are wrong for. There the labels that must
//! fit a fixed slot carry `textLength`, and the browser is asked whether
//! it honoured it.

use op_chart::data::json::{Value, parse};

/// A rectangle in the svg's own user units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Box2 {
    pub left: f64,
    pub right: f64,
    pub top: f64,
    pub bottom: f64,
}

impl Box2 {
    /// How far two rectangles reach into each other along each axis. Both
    /// positive is an overlap; either at or below zero is a miss, and the
    /// number is then how far apart they are.
    pub fn into_each_other(self, other: Self) -> (f64, f64) {
        (
            self.right.min(other.right) - self.left.max(other.left),
            self.bottom.min(other.bottom) - self.top.max(other.top),
        )
    }
}

/// One label as the browser drew it.
#[derive(Clone, Debug)]
pub struct Label {
    /// The class the emitter wrote, which names the row it belongs to.
    pub class: String,
    pub text: String,
    /// The advance box, from the browser's own measurement of the text and
    /// the anchor it is drawn with, carried through the element's
    /// transform so every label is in one coordinate system.
    pub left: f64,
    pub right: f64,
    /// The baseline the text sits on, in the same units.
    pub baseline: f64,
    /// What the paint covers, when the browser reported a box for it. A
    /// label with nothing to draw has none.
    pub ink: Option<Box2>,
    /// The `textLength` the markup pins this label to, if it carries one.
    pub pinned: Option<f64>,
}

impl Label {
    /// The box the emitter reserved: the measured advance by the em box
    /// the rows are spaced by, which is the rectangle `op-chart` places
    /// against.
    pub fn reserved(&self, text_px: f64) -> Box2 {
        Box2 {
            left: self.left,
            right: self.right,
            top: self.baseline - text_px * op_chart::ASCENT,
            bottom: self.baseline + text_px * op_chart::DESCENT,
        }
    }
}

/// One load of the page, at one width.
#[derive(Clone, Debug)]
pub struct View {
    /// The css width the chart was laid out at.
    pub width: f64,
    /// Whether the element reported `:state(hydrated)`. It only does when
    /// it adopted the pre-render it was served; when the pre-render is the
    /// wrong width it draws its own instead, and this stays false while
    /// [`View::rendered_by`] names the element.
    pub hydrated: bool,
    /// What drew the svg the labels were read from, as the markup says.
    pub rendered_by: String,
    /// Whether the served faces were blocked for this load.
    pub blocked: bool,
    pub labels: Vec<Label>,
}

/// Everything one capture run read.
#[derive(Clone, Debug)]
pub struct Capture {
    pub browser: String,
    pub binary: String,
    pub text_px: f64,
    pub views: Vec<View>,
}

/// Two labels that lie over each other, and by how much.
#[derive(Clone, Debug)]
pub struct Crossing {
    pub width: f64,
    pub a: Label,
    pub b: Label,
    pub across: f64,
    pub down: f64,
}

/// Boxes that meet edge to edge are not boxes over each other, and a
/// hundredth of a pixel is nothing a screen can draw. The same tolerance
/// the native check uses, so the two agree on what touching means.
pub const TOUCHING: f64 = 0.01;

/// The rows the chart draws, so a view that has quietly stopped drawing
/// one can be told from a view that has nothing to say. A chart with no
/// marks draws no mark labels, so this is what a view must have at least
/// one of before its silence counts as evidence.
pub const ROWS: [&str; 4] = ["axis", "axis tick-label", "endlabel", "head-t"];

/// Every pair of labels in `view` that lie over each other.
pub fn crossings(view: &View, text_px: f64) -> Vec<Crossing> {
    let mut out = Vec::new();
    for (i, a) in view.labels.iter().enumerate() {
        for b in &view.labels[i + 1..] {
            let (across, down) = a.reserved(text_px).into_each_other(b.reserved(text_px));
            if across > TOUCHING && down > TOUCHING {
                out.push(Crossing {
                    width: view.width,
                    a: a.clone(),
                    b: b.clone(),
                    across,
                    down,
                });
            }
        }
    }
    out
}

/// The closest two labels came to each other without touching, as the
/// smaller of the two gaps that keep them apart. [`None`] when a view has
/// fewer than two labels to compare.
pub fn tightest(view: &View, text_px: f64) -> Option<(f64, Label, Label)> {
    let mut best: Option<(f64, Label, Label)> = None;
    for (i, a) in view.labels.iter().enumerate() {
        for b in &view.labels[i + 1..] {
            let (across, down) = a.reserved(text_px).into_each_other(b.reserved(text_px));
            // the gap is along whichever axis actually separates them
            let gap = (-across).max(-down);
            if best.as_ref().is_none_or(|(g, _, _)| gap < *g) {
                best = Some((gap, a.clone(), b.clone()));
            }
        }
    }
    best
}

/// How far inside its reserved box a label's paint stayed, over a view.
/// Negative means the paint reached outside the box, which the specimen
/// sheet measured at up to 0.13 px on the served faces.
fn ink_margin(view: &View) -> Option<(f64, Label)> {
    let mut worst: Option<(f64, Label)> = None;
    for label in &view.labels {
        let Some(ink) = label.ink else { continue };
        let margin = (ink.left - label.left).min(label.right - ink.right);
        if worst.as_ref().is_none_or(|(m, _)| margin < *m) {
            worst = Some((margin, label.clone()));
        }
    }
    worst
}

/// What the run found, in the words the report prints.
pub fn verdict(capture: &Capture) -> Vec<String> {
    let mut out = vec![format!(
        "The chart's labels as {} drew them, measured with the browser's own \
         getComputedTextLength rather than the advance tables.",
        capture.binary
    )];
    let drawing: Vec<&View> = capture.views.iter().filter(|v| !v.blocked).collect();
    for view in &drawing {
        let crossings = crossings(view, capture.text_px);
        out.push(format!(
            "  {} px: {} labels, {}.",
            view.width,
            view.labels.len(),
            match (view.rendered_by.as_str(), view.hydrated) {
                ("op-site", true) => "drawn by the element, which adopted the pre-render",
                ("op-site", false) => "drawn by the element, which replaced the pre-render",
                (_, true) => "the pre-render, adopted as it stood",
                _ => "the pre-render the page was served, untouched",
            }
        ));
        for row in ROWS {
            if !view.labels.iter().any(|l| l.class == row) {
                out.push(format!(
                    "    no {row} at this width, so the check is thinner here than it reads."
                ));
            }
        }
        if crossings.is_empty() {
            match tightest(view, capture.text_px) {
                Some((gap, a, b)) => out.push(format!(
                    "    no two overlap; the closest are {:?} and {:?}, {gap:.3} px apart.",
                    a.text, b.text
                )),
                None => out.push("    fewer than two labels, so nothing was compared.".to_owned()),
            }
        }
        for c in &crossings {
            out.push(format!(
                "    {:?} ({}) lies over {:?} ({}) by {:.3} px across and {:.3} px down.",
                c.a.text, c.a.class, c.b.text, c.b.class, c.across, c.down
            ));
        }
        if let Some((margin, label)) = ink_margin(view) {
            out.push(if margin < 0.0 {
                format!(
                    "    the paint reached {:.3} px outside its box at worst, on {:?}, \
                     which is the side bearing the specimen sheet measured.",
                    -margin, label.text
                )
            } else {
                format!(
                    "    the paint stayed {margin:.3} px inside its box at worst, on {:?}.",
                    label.text
                )
            });
        }
    }
    for view in capture.views.iter().filter(|v| v.blocked) {
        let pinned: Vec<&Label> = view.labels.iter().filter(|l| l.pinned.is_some()).collect();
        out.push(format!(
            "  {} px with the served faces blocked: {} labels, {} of them pinned by textLength.",
            view.width,
            view.labels.len(),
            pinned.len()
        ));
        if pinned.is_empty() {
            out.push(
                "    nothing was pinned, so this load proves nothing about the slot.".to_owned(),
            );
        }
        for label in pinned {
            let want = label.pinned.unwrap_or_default();
            let drawn = label.right - label.left;
            out.push(format!(
                "    {:?} ({}) is pinned to {want:.2} px and drew {drawn:.2}, {}.",
                label.text,
                label.class,
                if (drawn - want).abs() <= TOUCHING {
                    "which the browser honoured"
                } else {
                    "which it did not"
                }
            ));
        }
    }
    out
}

/// Every complaint the run has: an overlap anywhere, a pinned label the
/// browser did not honour, or a view too empty to be evidence.
pub fn faults(capture: &Capture) -> Vec<String> {
    let mut out = Vec::new();
    for view in &capture.views {
        if !view.blocked {
            for c in crossings(view, capture.text_px) {
                out.push(format!(
                    "at {} px {:?} ({}) lies over {:?} ({}) by {:.3} px across and {:.3} down",
                    c.width, c.a.text, c.a.class, c.b.text, c.b.class, c.across, c.down
                ));
            }
            if view.labels.len() < 2 {
                out.push(format!(
                    "at {} px the page drew {} labels, which is not a check",
                    view.width,
                    view.labels.len()
                ));
            }
        }
        for label in view.labels.iter().filter(|l| l.pinned.is_some()) {
            let want = label.pinned.unwrap_or_default();
            let drawn = label.right - label.left;
            if (drawn - want).abs() > TOUCHING {
                out.push(format!(
                    "at {} px {:?} is pinned to {want:.2} px and drew {drawn:.2}",
                    view.width, label.text
                ));
            }
        }
    }
    out
}

/// Read what the capture script wrote.
pub fn read_capture(path: &std::path::Path) -> Result<Capture, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let value = parse(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let where_ = || path.display().to_string();
    let at = |v: &Value, key: &str| -> Result<Value, String> {
        match v {
            Value::Object(fields) => fields
                .iter()
                .find(|(n, _)| n == key)
                .map(|(_, v)| v.clone())
                .ok_or_else(|| format!("{}: no {key}", where_())),
            other => Err(format!(
                "{}: {key} wanted an object, found {}",
                where_(),
                other.kind()
            )),
        }
    };
    let string = |v: &Value, key: &str| -> Result<String, String> {
        match at(v, key)? {
            Value::String(s) => Ok(s),
            other => Err(format!("{}: {key} is {}", where_(), other.kind())),
        }
    };
    let number = |v: &Value, key: &str| -> Result<f64, String> {
        match at(v, key)? {
            Value::Number(n) => Ok(n),
            other => Err(format!("{}: {key} is {}", where_(), other.kind())),
        }
    };
    let flag = |v: &Value, key: &str| -> Result<bool, String> {
        match at(v, key)? {
            Value::Bool(b) => Ok(b),
            other => Err(format!("{}: {key} is {}", where_(), other.kind())),
        }
    };
    let maybe_number = |v: &Value, key: &str| -> Option<f64> {
        match at(v, key) {
            Ok(Value::Number(n)) => Some(n),
            _ => None,
        }
    };
    let list = |v: &Value, key: &str| -> Result<Vec<Value>, String> {
        match at(v, key)? {
            Value::Array(items) => Ok(items),
            other => Err(format!("{}: {key} is {}", where_(), other.kind())),
        }
    };

    let mut views = Vec::new();
    for view in list(&value, "views")? {
        let mut labels = Vec::new();
        for label in list(&view, "labels")? {
            let ink = match (
                maybe_number(&label, "ink_left"),
                maybe_number(&label, "ink_right"),
                maybe_number(&label, "ink_top"),
                maybe_number(&label, "ink_bottom"),
            ) {
                (Some(left), Some(right), Some(top), Some(bottom)) => Some(Box2 {
                    left,
                    right,
                    top,
                    bottom,
                }),
                _ => None,
            };
            labels.push(Label {
                class: string(&label, "class")?,
                text: string(&label, "text")?,
                left: number(&label, "left")?,
                right: number(&label, "right")?,
                baseline: number(&label, "baseline")?,
                ink,
                pinned: maybe_number(&label, "pinned"),
            });
        }
        views.push(View {
            width: number(&view, "width")?,
            hydrated: flag(&view, "hydrated")?,
            rendered_by: string(&view, "rendered_by")?,
            blocked: flag(&view, "blocked")?,
            labels,
        });
    }
    if views.is_empty() {
        return Err(format!("{}: no views", where_()));
    }
    Ok(Capture {
        browser: string(&value, "browser")?,
        binary: string(&value, "binary")?,
        text_px: number(&value, "text_px")?,
        views,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn label(class: &str, text: &str, left: f64, right: f64, baseline: f64) -> Label {
        Label {
            class: class.to_owned(),
            text: text.to_owned(),
            left,
            right,
            baseline,
            ink: None,
            pinned: None,
        }
    }

    fn view(labels: Vec<Label>) -> View {
        View {
            width: 360.0,
            hydrated: true,
            rendered_by: "op-site".to_owned(),
            blocked: false,
            labels,
        }
    }

    /// Two labels on one baseline whose advances run into each other are
    /// an overlap, and the report says by how much. The same two moved a
    /// hair apart are not, so the check is not simply always unhappy.
    #[test]
    fn labels_that_run_into_each_other_are_found_and_ones_that_clear_are_not() {
        let over = view(vec![
            label("axis tick-label", "0s", 10.0, 24.0, 100.0),
            label("axis tick-label", "1s", 20.0, 34.0, 100.0),
        ]);
        let found = crossings(&over, 12.0);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(
            (found[0].across - 4.0).abs() < 1e-9,
            "{:?}",
            found[0].across
        );
        let clear = view(vec![
            label("axis tick-label", "0s", 10.0, 24.0, 100.0),
            label("axis tick-label", "1s", 24.1, 38.0, 100.0),
        ]);
        assert!(crossings(&clear, 12.0).is_empty());
    }

    /// Rows are kept apart by the em box, so two labels that share x on
    /// different baselines do not overlap, and the same two brought within
    /// one em box of each other do.
    #[test]
    fn a_row_below_another_clears_it_by_the_em_box_and_not_by_luck() {
        let apart = view(vec![
            label("endlabel", "palette", 10.0, 60.0, 100.0),
            label("mark-label", "abort", 10.0, 60.0, 113.0),
        ]);
        assert!(crossings(&apart, 12.0).is_empty(), "12.0 px apart");
        let together = view(vec![
            label("endlabel", "palette", 10.0, 60.0, 100.0),
            label("mark-label", "abort", 10.0, 60.0, 104.0),
        ]);
        assert_eq!(crossings(&together, 12.0).len(), 1);
    }

    /// Edge to edge is not one over the other, which is what lets a row
    /// pack labels against a gap without the check calling it a fault.
    #[test]
    fn boxes_that_touch_are_not_boxes_over_each_other() {
        let touching = view(vec![
            label("axis tick-label", "0s", 10.0, 24.0, 100.0),
            label("axis tick-label", "1s", 24.0, 38.0, 100.0),
        ]);
        assert!(crossings(&touching, 12.0).is_empty());
    }

    /// A page that drew almost nothing must not read as a pass. The
    /// faults list says so, because a check whose evidence has quietly
    /// vanished is the failure most likely to go unnoticed.
    #[test]
    fn a_view_with_nothing_in_it_is_a_fault_and_not_a_pass() {
        let empty = view(vec![label("axis", "%", 0.0, 10.0, 20.0)]);
        let capture = Capture {
            browser: "chrome".to_owned(),
            binary: "chrome (test)".to_owned(),
            text_px: 12.0,
            views: vec![empty],
        };
        let faults = faults(&capture);
        assert_eq!(faults.len(), 1, "{faults:?}");
        assert!(faults[0].contains("1 labels"), "{faults:?}");
        // and the verdict names every row that went missing, so the report
        // reads as thin rather than as clean
        let said = verdict(&capture).join("\n");
        for row in ["axis tick-label", "endlabel", "head-t"] {
            assert!(said.contains(&format!("no {row} at this width")), "{said}");
        }
    }

    /// A pinned label the browser did not honour is a fault, and one it
    /// did is not.
    #[test]
    fn a_pinned_label_is_checked_against_what_the_browser_drew() {
        let mut honoured = label("axis", "100", 10.0, 31.6, 50.0);
        honoured.pinned = Some(21.6);
        let mut ignored = label("axis", "100", 10.0, 35.0, 70.0);
        ignored.pinned = Some(21.6);
        let capture = Capture {
            browser: "chrome".to_owned(),
            binary: "chrome (test)".to_owned(),
            text_px: 12.0,
            views: vec![View {
                width: 360.0,
                hydrated: false,
                rendered_by: "op-pages".to_owned(),
                blocked: true,
                labels: vec![honoured, ignored],
            }],
        };
        let faults = faults(&capture);
        assert_eq!(faults.len(), 1, "{faults:?}");
        assert!(faults[0].contains("drew 25.00"), "{faults:?}");
        let said = verdict(&capture).join("\n");
        assert!(said.contains("which the browser honoured"), "{said}");
        assert!(said.contains("which it did not"), "{said}");
    }
}
