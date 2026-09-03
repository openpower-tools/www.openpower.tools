//! Colour mathematics, native only, for fitting and testing the palette.
//! Every algorithm here is checked in its tests against published
//! reference values from the algorithm's own authors, not against itself.

pub mod apca;
pub mod cvd;
pub mod lab;
pub mod oklab;
pub mod srgb;
pub mod wcag;

pub use apca::apca_lc;
pub use cvd::{Deficiency, simulate};
pub use lab::{Lab, ciede2000};
pub use oklab::{Oklab, Oklch};
pub use srgb::{Linear, Srgb};
pub use wcag::wcag_contrast;
