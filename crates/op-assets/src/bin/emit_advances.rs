//! Writes the advance tables `op-chart` compiles in.
//!
//! Run after any change to the served faces:
//! `cargo run -p op-assets --bin emit-advances`. A test in
//! `op-assets::advances` regenerates the same file and compares it byte for
//! byte, so a face that changes without this being run fails the workspace
//! test run rather than leaving the chart measuring a font it no longer
//! serves.

fn main() {
    let path = op_assets::advances::generated_path();
    let source = op_assets::advances::generate(&op_assets::advances::assets());
    std::fs::write(&path, &source)
        .unwrap_or_else(|e| panic!("cannot write {}: {e}", path.display()));
    println!(
        "op-assets: {} advances per face for {} faces, {} bytes to {}",
        op_assets::advances::COUNT,
        op_assets::advances::DRAWN.len(),
        source.len(),
        path.display()
    );
}
