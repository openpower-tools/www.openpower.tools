//! Writes the specimen page and the manifest that goes with it.
//!
//! The page holds one cell per label the chart draws, in the real face,
//! weight and size, over the surface that label sits on, with the advance
//! sum op-chart would place it by written onto the cell. The manifest
//! says where every box is, so a capture can be cut into cells without
//! guessing and measured against that sum.
//!
//! It writes the kerning sweep's page too. Both pages are measured by
//! the same capture step against the same served faces, and both are
//! generated rather than written, so neither can drift from the tables
//! it is testing.
//!
//! `cargo run -p op-verify --bin specimen-page -- reports/specimens`
//! then `uv run tools/interaction_report/specimen_capture.py`.

use op_verify::{specimens, sweep};

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = std::path::PathBuf::from(
        args.next()
            .unwrap_or_else(|| "reports/specimens".to_owned()),
    );
    if args.next().is_some() {
        eprintln!("usage: specimen-page [DIRECTORY]");
        std::process::exit(2);
    }
    if let Err(e) = std::fs::create_dir_all(&dir) {
        eprintln!("cannot make {}: {e}", dir.display());
        std::process::exit(1);
    }
    let cases = specimens::cases();
    let page = specimens::layout(&cases);
    let text = sweep::text();
    let html = dir.join("specimen.html");
    let manifest = dir.join("specimen.json");
    let sweep_html = dir.join("sweep.html");
    let sweep_manifest = dir.join("sweep.json");
    for (path, body) in [
        (&html, specimens::page(&cases, &page)),
        (&manifest, specimens::manifest(&cases, &page)),
        (&sweep_html, sweep::page(&text)),
        (&sweep_manifest, sweep::manifest(&text)),
    ] {
        if let Err(e) = std::fs::write(path, &body) {
            eprintln!("cannot write {}: {e}", path.display());
            std::process::exit(1);
        }
    }
    println!(
        "specimen: {} cases in {} cells, {} by {} CSS px\n  {}\n  {}",
        cases.len(),
        page.cells.len(),
        page.width,
        page.height,
        html.display(),
        manifest.display()
    );
    println!(
        "sweep:    every one of the {} ordered pairs once, in {} characters, {} runs\n  {}\n  {}",
        op_chart::advances::COUNT * op_chart::advances::COUNT,
        text.chars().count(),
        sweep::faces().len() * sweep::Shaping::all().len(),
        sweep_html.display(),
        sweep_manifest.display()
    );
}
