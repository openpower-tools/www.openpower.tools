//! Does a second interaction report reproduce the first?
//!
//! The synthetic clock makes a run reproducible, not bit-identical: two
//! runs take the same decisions and measure the same quantities to within
//! a frame, while a transition's last floating-point bit still lands
//! either side of a frame boundary now and then. So [`checks`] holds the
//! decisions and the wording to the letter and the measurements to a
//! frame, and [`frames`] holds the artefacts to what a reader would
//! notice rather than to their bytes.
//!
//! Native only, and dependency-light on purpose: the JSON comes from
//! op-chart's reader and the colour maths from op-colour, so the numbers
//! here are the ones the site itself is built and tested with.

pub mod checks;
pub mod frames;

/// What a comparison found: a line for each thing that differed, a closing
/// line for the reader, and whether the run should fail.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outcome {
    pub differences: Vec<String>,
    pub summary: String,
    pub failed: bool,
}

impl Outcome {
    /// Print every line, differences first, and give the exit code.
    pub fn report(&self) -> u8 {
        for line in &self.differences {
            println!("{line}");
        }
        println!("{}", self.summary);
        u8::from(self.failed)
    }
}
