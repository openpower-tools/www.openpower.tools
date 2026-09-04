//! Compare two `--checks-json` dumps of the interaction report.
//!
//! The check names, their outcomes and the wording around each
//! measurement must match exactly, while the numbers inside a detail may
//! differ by up to one frame plus their own printed rounding, or by one
//! part in a thousand for a big figure, whichever is larger. Anything
//! else is a regression. Artefacts are held to a perceptual standard
//! instead, by [`crate::frames`].

use crate::Outcome;
use op_chart::data::json::{self, Value};
use std::path::Path;

/// One frame of the synthetic clock.
pub const FRAME: f64 = 1.0 / 60.0;

/// One check the report ran, as it dumped it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Check {
    pub name: String,
    pub ok: bool,
    pub detail: String,
}

/// One control's checks, under the tag the report gave it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Control {
    pub tag: String,
    pub checks: Vec<Check>,
}

/// Read a dump, naming the file in anything it has to refuse.
pub fn read(path: &Path) -> Result<Vec<Control>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    parse(&text).map_err(|e| format!("{}: {e}", path.display()))
}

/// Read the array of controls a dump holds.
pub fn parse(text: &str) -> Result<Vec<Control>, String> {
    let value = json::parse(text).map_err(|e| e.to_string())?;
    let Value::Array(items) = value else {
        return Err(format!("the dump is {}, not an array", value.kind()));
    };
    items
        .iter()
        .enumerate()
        .map(|(i, item)| control(i, item))
        .collect()
}

fn control(i: usize, item: &Value) -> Result<Control, String> {
    let Value::Object(fields) = item else {
        return Err(format!("control {i} is {}, not an object", item.kind()));
    };
    let tag = text(fields, "tag").ok_or_else(|| format!("control {i} has no tag"))?;
    let Some(Value::Array(listed)) = field(fields, "checks") else {
        return Err(format!("{tag} has no array of checks"));
    };
    let checks = listed
        .iter()
        .map(|c| check(&tag, c))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Control {
        tag: tag.clone(),
        checks,
    })
}

fn check(tag: &str, value: &Value) -> Result<Check, String> {
    let Value::Object(fields) = value else {
        return Err(format!("{tag} has a check that is {}", value.kind()));
    };
    let name = text(fields, "name").ok_or_else(|| format!("{tag} has a check with no name"))?;
    let Some(&Value::Bool(ok)) = field(fields, "ok") else {
        return Err(format!("{tag}: {name}: no outcome"));
    };
    let detail = text(fields, "detail").ok_or_else(|| format!("{tag}: {name}: no detail"))?;
    Ok(Check { name, ok, detail })
}

fn field<'a>(fields: &'a [(String, Value)], name: &str) -> Option<&'a Value> {
    fields.iter().find(|(k, _)| k == name).map(|(_, v)| v)
}

fn text(fields: &[(String, Value)], name: &str) -> Option<String> {
    match field(fields, name)? {
        Value::String(s) => Some(s.clone()),
        _ => None,
    }
}

/// The detail with every number replaced, so the wording can be compared.
pub fn skeleton(detail: &str) -> String {
    let mut out = String::with_capacity(detail.len());
    let mut at = 0;
    for (start, end) in spans(detail) {
        out.push_str(&detail[at..start]);
        out.push('#');
        at = end;
    }
    out.push_str(&detail[at..]);
    out
}

/// Every number in the detail, exactly as it was printed.
pub fn numbers(detail: &str) -> Vec<&str> {
    spans(detail)
        .into_iter()
        .map(|(start, end)| &detail[start..end])
        .collect()
}

/// Where each number sits. The grammar is a digit run with an optional
/// sign, an optional point with an optional fraction, and an optional
/// lower-case exponent with no plus sign: what the report prints, and
/// nothing more, so a hyphenated range reads as two numbers and a hex
/// hash reads as several.
fn spans(detail: &str) -> Vec<(usize, usize)> {
    let bytes = detail.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match number_at(bytes, i) {
            Some(end) => {
                out.push((i, end));
                i = end;
            }
            // never mid-character: a sign or a digit is one byte of its own
            None => i += 1,
        }
    }
    out
}

fn number_at(bytes: &[u8], start: usize) -> Option<usize> {
    let mut i = start;
    if bytes.get(i) == Some(&b'-') {
        i += 1;
    }
    if !digits(bytes, &mut i) {
        return None;
    }
    if bytes.get(i) == Some(&b'.') {
        i += 1;
    }
    digits(bytes, &mut i);
    if bytes.get(i) == Some(&b'e') {
        let mut after = i + 1;
        if bytes.get(after) == Some(&b'-') {
            after += 1;
        }
        if digits(bytes, &mut after) {
            i = after;
        }
    }
    Some(i)
}

fn digits(bytes: &[u8], i: &mut usize) -> bool {
    let from = *i;
    while bytes.get(*i).is_some_and(u8::is_ascii_digit) {
        *i += 1;
    }
    *i > from
}

/// How far two printings of one measurement may sit apart: one frame, plus
/// the rounding of the coarser of the two, so 3.00 and 3.02 are one frame
/// apart as printed. A number written without decimals is a count or a
/// colour channel, and those must match exactly.
pub fn tolerance(a: &str, b: &str) -> f64 {
    let decimals = |t: &str| t.split_once('.').map_or(0, |(_, fraction)| fraction.len());
    let places = decimals(a).max(decimals(b));
    if places == 0 {
        return 0.0;
    }
    FRAME + 10f64.powi(-(places as i32))
}

/// What differs between two printings of one detail, or `None` when the
/// wording is the same and every measurement is close enough.
pub fn detail_differs(a: &str, b: &str) -> Option<String> {
    if skeleton(a) != skeleton(b) {
        return Some("wording".to_owned());
    }
    // equal skeletons means one number here for each number there
    for (x, y) in numbers(a).into_iter().zip(numbers(b)) {
        // the grammar above is a subset of Rust's own float grammar, so
        // neither of these can fail; a refusal beats a panic if it ever does
        let (Ok(fx), Ok(fy)) = (x.parse::<f64>(), y.parse::<f64>()) else {
            return Some(format!("{x} vs {y}, not readable as numbers"));
        };
        if (fx - fy).abs() > tolerance(x, y).max(fx.abs() / 1000.0) {
            return Some(format!("{x} vs {y}, further apart than one frame"));
        }
    }
    None
}

/// The controls by tag, in the order the dump lists them, a repeated tag
/// standing for the later of the two.
fn by_tag(controls: &[Control]) -> Vec<(&str, &[Check])> {
    let mut out: Vec<(&str, &[Check])> = Vec::new();
    for control in controls {
        match out.iter_mut().find(|(tag, _)| *tag == control.tag) {
            Some(seen) => seen.1 = &control.checks,
            None => out.push((&control.tag, &control.checks)),
        }
    }
    out
}

/// Hold every control of the second dump against the first. `first_path`
/// only names the file in the message for a control that is missing.
pub fn compare(first: &[Control], again: &[Control], first_path: &str) -> Outcome {
    let (first, again) = (by_tag(first), by_tag(again));
    let names = |checks: &[Check]| checks.iter().map(|c| c.name.clone()).collect::<Vec<_>>();
    let mut differences = Vec::new();
    let mut bad: Vec<&str> = Vec::new();
    for (tag, checks) in &again {
        let Some((_, others)) = first.iter().find(|(seen, _)| seen == tag) else {
            differences.push(format!("{tag}: absent from {first_path}"));
            bad.push(tag);
            continue;
        };
        let (there, here) = (names(others), names(checks));
        if there != here {
            differences.push(format!(
                "{tag}: different checks: [{}] then [{}]",
                there.join(", "),
                here.join(", ")
            ));
            bad.push(tag);
            continue;
        }
        for (a, b) in others.iter().zip(checks.iter()) {
            if a.ok != b.ok {
                differences.push(format!(
                    "{tag}: {}: {} then {} ({} | {})",
                    a.name, a.ok, b.ok, a.detail, b.detail
                ));
                bad.push(tag);
            } else if let Some(why) = detail_differs(&a.detail, &b.detail) {
                differences.push(format!(
                    "{tag}: {}: {why} ({} | {})",
                    a.name, a.detail, b.detail
                ));
                bad.push(tag);
            }
        }
    }
    if bad.is_empty() {
        let total: usize = again.iter().map(|(_, checks)| checks.len()).sum();
        return Outcome {
            differences,
            summary: format!(
                "{total} checks over {} controls reproduced: same outcomes, measurements within one frame",
                again.len()
            ),
            failed: false,
        };
    }
    bad.sort_unstable();
    bad.dedup();
    Outcome {
        summary: format!("controls that did not reproduce: {}", bad.join(", ")),
        differences,
        failed: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(name: &str, ok: bool, detail: &str) -> Check {
        Check {
            name: name.to_owned(),
            ok,
            detail: detail.to_owned(),
        }
    }

    /// One control with one check, so a pair of details can be run through
    /// the whole comparison rather than through `detail_differs` alone.
    fn run(a: &Check, b: &Check) -> Outcome {
        let one = |c: &Check| {
            vec![Control {
                tag: "opt-switch".to_owned(),
                checks: vec![c.clone()],
            }]
        };
        compare(&one(a), &one(b), "first.json")
    }

    fn measured(detail: &str) -> Check {
        check("settles within the snap clock", true, detail)
    }

    #[test]
    fn one_frame_apart_in_seconds_passes() {
        let outcome = run(
            &measured("flight cleared at 3.00s"),
            &measured("flight cleared at 3.02s"),
        );
        assert!(!outcome.failed, "{outcome:?}");
        assert_eq!(
            outcome.summary,
            "1 checks over 1 controls reproduced: same outcomes, measurements within one frame"
        );
    }

    #[test]
    fn a_fifth_of_a_second_fails() {
        let outcome = run(
            &measured("flight cleared at 3.00s"),
            &measured("flight cleared at 3.20s"),
        );
        assert!(outcome.failed);
        assert_eq!(
            outcome.differences,
            [
                "opt-switch: settles within the snap clock: 3.00 vs 3.20, further apart than one frame (flight cleared at 3.00s | flight cleared at 3.20s)"
            ]
        );
    }

    #[test]
    fn a_hundredth_of_a_pixel_passes() {
        let outcome = run(&measured("max gap 0.24 pts"), &measured("max gap 0.25 pts"));
        assert!(!outcome.failed, "{outcome:?}");
    }

    #[test]
    fn a_colour_channel_written_without_decimals_fails_by_one() {
        let outcome = run(
            &measured("mid-flight green 198"),
            &measured("mid-flight green 199"),
        );
        assert!(outcome.failed);
        assert!(outcome.differences[0].contains("198 vs 199"), "{outcome:?}");
        // no decimals means no allowance at all, whatever the size of the
        // number and whether or not an exponent moved the point
        assert_eq!(tolerance("198", "199"), 0.0);
        assert_eq!(tolerance("1e-3", "2e-3"), 0.0);
    }

    /// The relative term, which lets a big figure drift where the frame
    /// term alone would not: 0.04 is past a frame plus a hundredth, and
    /// 0.06 is past a thousandth of fifty as well.
    #[test]
    fn a_large_measurement_may_drift_one_part_in_a_thousand() {
        assert!(
            !run(
                &measured("summary 50.00 chars"),
                &measured("summary 50.04 chars")
            )
            .failed
        );
        assert!(
            run(
                &measured("summary 50.00 chars"),
                &measured("summary 50.06 chars")
            )
            .failed
        );
    }

    #[test]
    fn a_changed_outcome_fails() {
        let outcome = run(
            &check("preview reaches legible opacity", true, "peak 0.9"),
            &check("preview reaches legible opacity", false, "peak 0.9"),
        );
        assert!(outcome.failed);
        assert_eq!(
            outcome.differences,
            ["opt-switch: preview reaches legible opacity: true then false (peak 0.9 | peak 0.9)"]
        );
    }

    #[test]
    fn a_changed_wording_fails() {
        let outcome = run(
            &measured("flight cleared at 3.00s"),
            &measured("flight cleared by 3.00s"),
        );
        assert!(outcome.failed);
        assert!(outcome.differences[0].contains("wording"), "{outcome:?}");
    }

    #[test]
    fn a_missing_control_fails() {
        let outcome = compare(
            &[],
            &[Control {
                tag: "opt-switch".to_owned(),
                checks: vec![measured("at 0.00s")],
            }],
            "first.json",
        );
        assert!(outcome.failed);
        assert_eq!(outcome.differences, ["opt-switch: absent from first.json"]);
        assert_eq!(
            outcome.summary,
            "controls that did not reproduce: opt-switch"
        );
    }

    #[test]
    fn a_different_number_of_checks_fails() {
        let one = vec![Control {
            tag: "opt-switch".to_owned(),
            checks: vec![measured("at 0.00s")],
        }];
        let two = vec![Control {
            tag: "opt-switch".to_owned(),
            checks: vec![measured("at 0.00s"), check("palette arrived", true, "")],
        }];
        let outcome = compare(&one, &two, "first.json");
        assert!(outcome.failed);
        assert!(
            outcome.differences[0].contains("different checks"),
            "{outcome:?}"
        );
    }

    #[test]
    fn numbers_are_read_as_the_report_prints_them() {
        assert_eq!(numbers("max 0.020 s behind, 234 samples"), ["0.020", "234"]);
        assert_eq!(numbers("chart ahead by at most -0.000 s"), ["-0.000"]);
        assert_eq!(
            numbers("settled when the blend ended (2.8-3.4s)"),
            ["2.8", "-3.4"]
        );
        assert_eq!(numbers("scaled 1e-3 of a step"), ["1e-3"]);
        assert_eq!(numbers("no numbers at all"), [] as [&str; 0]);
        assert_eq!(
            skeleton("first positions [18, 17]"),
            "first positions [#, #]"
        );
    }

    #[test]
    fn a_dump_is_read_and_a_broken_one_is_refused() {
        let dump = r#"[{"tag":"opt-switch","kind":"toggle","clock":"synthetic",
            "checks":[{"name":"armed","ok":true,"detail":"at 0.00s"}]}]"#;
        assert_eq!(
            parse(dump).expect("the dump reads"),
            [Control {
                tag: "opt-switch".to_owned(),
                checks: vec![check("armed", true, "at 0.00s")],
            }]
        );
        assert_eq!(
            parse(r#"[{"tag":"opt-switch"}]"#),
            Err("opt-switch has no array of checks".to_owned())
        );
        assert_eq!(
            parse(r#"{"tag":"opt-switch"}"#),
            Err("the dump is an object, not an array".to_owned())
        );
        assert!(parse("not json").is_err());
    }
}
