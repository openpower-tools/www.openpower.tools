//! Did the page build emit the same bytes twice?
//!
//! The pre-rendered chart is what a reader without scripts is served, and
//! it comes out of the page build rather than the element. Everything on
//! that path is a pure function of the data block, so a second emission
//! has to be byte identical; anything else means something is reading a
//! clock, an iteration order, or the filesystem.
//!
//! `cargo run -p op-verify --bin compare-pages -- dist dist-again`

use op_verify::pages;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [first, second] = args.as_slice() else {
        eprintln!("usage: compare-pages FIRST SECOND");
        std::process::exit(2);
    };
    match pages::verdict(std::path::Path::new(first), std::path::Path::new(second)) {
        Ok((lines, same)) => {
            for line in lines {
                println!("{line}");
            }
            if !same {
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("compare-pages: {e}");
            std::process::exit(1);
        }
    }
}
