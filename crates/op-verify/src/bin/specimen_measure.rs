//! Measures the specimen captures and draws the contact sheets.
//!
//! Reads each theme's capture and the JSON beside it, cuts the picture
//! back into cells by the boxes the page was laid out with, and asks of
//! every one: how wide is the mark the browser actually painted, how does
//! that stand against the advance sum op-chart would have placed it by,
//! and how much ink does it put on its surface. Writes the numbers as
//! JSON and the picture a person reads as a PNG.
//!
//! `cargo run -p op-verify --bin specimen-measure -- reports/specimens`

use op_verify::{frames, sheet, specimens};

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = std::path::PathBuf::from(
        args.next()
            .unwrap_or_else(|| "reports/specimens".to_owned()),
    );
    let themes: Vec<String> = {
        let rest: Vec<String> = args.collect();
        if rest.is_empty() {
            vec!["light".to_owned(), "dark".to_owned()]
        } else {
            rest
        }
    };
    let cases = specimens::cases();
    let page = specimens::layout(&cases);
    let mut failed = false;
    for theme in &themes {
        match run(&dir, theme, &cases, &page) {
            Ok(lines) => {
                for line in lines {
                    println!("{line}");
                }
            }
            Err(e) => {
                eprintln!("{theme}: {e}");
                failed = true;
            }
        }
    }
    if failed {
        std::process::exit(1);
    }
}

/// One theme: measure it, write its JSON and its sheet, and say what it
/// came to.
fn run(
    dir: &std::path::Path,
    theme: &str,
    cases: &[specimens::Case],
    page: &specimens::Page,
) -> Result<Vec<String>, String> {
    let capture = specimens::read_capture(&dir.join(format!("capture-{theme}.json")))?;
    let image = frames::decode(&dir.join(&capture.image))?;
    let rows = specimens::measure_capture(&image, cases, page, &capture)?;
    let summary = specimens::summary(cases, &rows);
    let json = dir.join(format!("measured-{theme}.json"));
    std::fs::write(
        &json,
        specimens::measured_json(cases, &rows, &capture, &summary),
    )
    .map_err(|e| format!("{}: {e}", json.display()))?;
    let sheet_path = dir.join(format!("sheet-{theme}.png"));
    let canvas = sheet::contact_sheet(&image, cases, page, &capture, &rows, &summary)?;
    canvas.write(&sheet_path)?;
    let mut lines = vec![format!("== {theme}")];
    lines.extend(summary.iter().map(|l| format!("   {l}")));
    lines.push(format!(
        "   sheet: {} ({} by {})",
        sheet_path.display(),
        canvas.width,
        canvas.height
    ));
    lines.push(format!("   json:  {}", json.display()));
    Ok(lines)
}
