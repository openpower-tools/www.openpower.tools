//! Measures the chart's labels as a browser drew them, and says whether
//! any two of them lie over each other.
//!
//! `op-chart` makes the same check against its own advance tables, which
//! proves the placement agrees with the ruler it was placed by. This one
//! reads the labels out of the built site's own page, with the site's own
//! stylesheet, and measures them from the glyphs the browser laid out, so
//! a rule that changed the size or the family would show here and cannot
//! show there. It also reads one load with the served faces blocked, the
//! case the tables are wrong for, and asks whether the labels that must
//! fit a fixed slot stayed pinned to it.
//!
//! `cargo run -p op-verify --bin labels-measure -- reports/specimens`

use op_verify::labels;

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = std::path::PathBuf::from(
        args.next()
            .unwrap_or_else(|| "reports/specimens".to_owned()),
    );
    if args.next().is_some() {
        eprintln!("usage: labels-measure [DIRECTORY]");
        std::process::exit(2);
    }
    match run(&dir) {
        Ok((lines, faults)) => {
            for line in &lines {
                println!("{line}");
            }
            if !faults.is_empty() {
                for fault in &faults {
                    eprintln!("labels: {fault}");
                }
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("labels: {e}");
            std::process::exit(1);
        }
    }
}

/// Read the capture, write the verdict beside it, and hand back both the
/// verdict and whatever the run has to complain about.
fn run(dir: &std::path::Path) -> Result<(Vec<String>, Vec<String>), String> {
    let capture = labels::read_capture(&dir.join("labels-capture.json"))?;
    let verdict = labels::verdict(&capture);
    let faults = labels::faults(&capture);
    let json = dir.join("labels-measured.json");
    std::fs::write(&json, measured_json(&capture, &verdict, &faults))
        .map_err(|e| format!("{}: {e}", json.display()))?;
    let mut out = vec![format!(
        "== the chart's labels in a browser ({}, {})",
        capture.browser, capture.binary
    )];
    out.extend(verdict.iter().map(|l| format!("   {l}")));
    out.push(format!("   json: {}", json.display()));
    Ok((out, faults))
}

/// The verdict and every label it was reached from, so a later run can be
/// compared against this one without driving a browser again.
fn measured_json(capture: &labels::Capture, verdict: &[String], faults: &[String]) -> String {
    // Rust's own debug form of a string is a JSON string for the text
    // this writes, which is ascii label text and verdict prose
    let quote = |s: &str| format!("{s:?}");
    let mut out = String::from("{\n");
    out.push_str(&format!("  \"browser\": {},\n", quote(&capture.browser)));
    out.push_str(&format!("  \"binary\": {},\n", quote(&capture.binary)));
    out.push_str(&format!("  \"text_px\": {:.4},\n", capture.text_px));
    out.push_str("  \"verdict\": [\n");
    for (i, line) in verdict.iter().enumerate() {
        let comma = if i + 1 == verdict.len() { "" } else { "," };
        out.push_str(&format!("   {}{comma}\n", quote(line)));
    }
    out.push_str("  ],\n  \"faults\": [\n");
    for (i, line) in faults.iter().enumerate() {
        let comma = if i + 1 == faults.len() { "" } else { "," };
        out.push_str(&format!("   {}{comma}\n", quote(line)));
    }
    out.push_str("  ],\n  \"views\": [\n");
    for (i, view) in capture.views.iter().enumerate() {
        let comma = if i + 1 == capture.views.len() {
            ""
        } else {
            ","
        };
        out.push_str(&format!(
            "   {{\"width\": {:.1}, \"hydrated\": {}, \"rendered_by\": {}, \"blocked\": {}, \
             \"labels\": [\n",
            view.width,
            view.hydrated,
            quote(&view.rendered_by),
            view.blocked
        ));
        for (j, label) in view.labels.iter().enumerate() {
            let last = if j + 1 == view.labels.len() { "" } else { "," };
            out.push_str(&format!(
                "    {{\"class\": {}, \"text\": {}, \"left\": {:.4}, \"right\": {:.4}, \
                 \"baseline\": {:.4}{}}}{last}\n",
                quote(&label.class),
                quote(&label.text),
                label.left,
                label.right,
                label.baseline,
                match label.pinned {
                    Some(px) => format!(", \"pinned\": {px:.4}"),
                    None => String::new(),
                }
            ));
        }
        out.push_str(&format!("   ]}}{comma}\n"));
    }
    out.push_str("  ]\n}\n");
    out
}
