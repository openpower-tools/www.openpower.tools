//! Compare the images two interaction reports drew: every image the second
//! run redrew must be one a reader could not tell from the first's.
//!
//!     cargo run --release -p op-verify --bin compare-frames DIR_A DIR_B
//!
//! Exit 0 when every image reproduced, 1 when one did not or when the
//! second tree has no image in common with the first, 2 when the arguments
//! were not two directories.

use op_verify::frames;
use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [first, again] = args.as_slice() else {
        eprintln!("usage: compare-frames DIR_A DIR_B");
        return ExitCode::from(2);
    };
    let (first, again) = (Path::new(first), Path::new(again));
    for dir in [first, again] {
        if !dir.is_dir() {
            eprintln!("{}: not a directory", dir.display());
            return ExitCode::from(2);
        }
    }
    ExitCode::from(frames::compare(first, again).report())
}
