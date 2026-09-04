//! Compare two `--checks-json` dumps of the interaction report: the second
//! run must have taken the same decisions and measured the same quantities
//! to within a frame.
//!
//!     cargo run --release -p op-verify --bin compare-checks FIRST.json AGAIN.json
//!
//! Exit 0 when the second dump reproduced the first, 1 when a control did
//! not, 2 when the arguments or the files were not readable.

use op_verify::checks;
use std::path::Path;
use std::process::ExitCode;

fn run(first_path: &str, again_path: &str) -> Result<u8, String> {
    let first = checks::read(Path::new(first_path))?;
    let again = checks::read(Path::new(again_path))?;
    Ok(checks::compare(&first, &again, first_path).report())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [first, again] = args.as_slice() else {
        eprintln!("usage: compare-checks FIRST.json AGAIN.json");
        return ExitCode::from(2);
    };
    match run(first, again) {
        Ok(code) => ExitCode::from(code),
        Err(why) => {
            eprintln!("{why}");
            ExitCode::from(2)
        }
    }
}
