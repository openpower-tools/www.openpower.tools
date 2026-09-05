//! Measures the kerning sweep and states what it found.
//!
//! Reads the positions the browser gave for every character of the De
//! Bruijn sequence, turns them into the kern of every ordered pair, and
//! says whether the advance table is a sound basis for placing labels or
//! where it is not. Writes the kerns as JSON and the verdict to both.
//!
//! `cargo run -p op-verify --bin sweep-measure -- reports/specimens`

use op_verify::{specimens, sweep};

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = std::path::PathBuf::from(
        args.next()
            .unwrap_or_else(|| "reports/specimens".to_owned()),
    );
    if args.next().is_some() {
        eprintln!("usage: sweep-measure [DIRECTORY]");
        std::process::exit(2);
    }
    match run(&dir) {
        Ok(lines) => {
            for line in lines {
                println!("{line}");
            }
        }
        Err(e) => {
            eprintln!("sweep: {e}");
            std::process::exit(1);
        }
    }
}

/// Measure the sweep, write the JSON, and give the verdict.
fn run(dir: &std::path::Path) -> Result<Vec<String>, String> {
    let text = sweep::text();
    let capture = sweep::read_capture(&dir.join("sweep-capture.json"))?;
    let swept = sweep::analyse(&text, &capture)?;
    // the strings the site's own charts draw, taken from the specimen's
    // own case table so the two cannot list different things
    let mut drawn: Vec<(String, String)> = specimens::cases()
        .into_iter()
        .filter(|c| c.kind != "shaping probe")
        .map(|c| (c.text.to_owned(), c.text.to_owned()))
        .collect();
    drawn.sort();
    drawn.dedup();
    let mut lines = sweep::verdict(&text, &swept, &drawn);
    lines.extend(cross_check(dir, &swept)?);
    let json = dir.join("sweep-measured.json");
    std::fs::write(&json, sweep::measured_json(&text, &capture, &swept, &lines))
        .map_err(|e| format!("{}: {e}", json.display()))?;
    let mut out = vec![format!(
        "== kerning sweep ({}, {})",
        capture.browser, capture.binary
    )];
    out.extend(lines.iter().map(|l| format!("   {l}")));
    out.push(format!("   json: {}", json.display()));
    Ok(out)
}

/// Hold the sweep against a measurement it had no part in: the specimen
/// capture laid every one of its strings out and reported the advance the
/// browser gave it. The sweep predicts that advance as the table's sum
/// plus the kerns of the string's own pairs, so the two agreeing is the
/// sweep checked end to end by a different reading of a different page.
fn cross_check(dir: &std::path::Path, swept: &[sweep::Swept]) -> Result<Vec<String>, String> {
    let path = dir.join("capture-light.json");
    if !path.exists() {
        return Ok(vec![
            "No specimen capture beside this one, so the sweep was not cross-checked against it."
                .to_owned(),
        ]);
    }
    let capture = specimens::read_capture(&path)?;
    let cases = specimens::cases();
    let page = specimens::layout(&cases);
    let mut worst = 0.0_f64;
    let mut worst_at = String::new();
    let mut checked = 0;
    for cell in &page.cells {
        if cell.variant != specimens::Variant::Shaped {
            continue;
        }
        let case = &cases[cell.case];
        let Some(face) = swept.iter().find(|s| s.face == case.face) else {
            continue;
        };
        let predicted = case.advance() + face.error_shaped(case.text);
        let measured = capture.advance(&cell.id)?;
        let apart = (predicted - measured).abs();
        if apart > worst {
            worst = apart;
            worst_at = format!("{:?} in {}", case.text, specimens::face_name(case.face));
        }
        checked += 1;
    }
    Ok(vec![format!(
        "Cross-check: the sweep predicts the specimen page's own laid-out advance for all {checked} of its \
         strings to within {worst:.4} px (worst on {worst_at}), measured off a different page in a \
         different reading."
    )])
}
