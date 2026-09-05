//! Chart geometry and SVG emission shared by the page build (native) and
//! the elements (wasm). Everything here is pure: a [`Spec`] in, an SVG
//! string and a [`Layout`] out. Colours never appear as literals of this
//! crate's own; classes map to the site's tokens in the consumer's
//! stylesheet, so a theme change re-styles with zero re-render.
//!
//! [`data`] reads the JSON block a chart carries, hashes it so a
//! pre-render can be told from a stale one, and turns it into a [`Spec`].
//!
//! [`advances`] is the one piece of data this crate does not compute: the
//! advance widths of the served faces, measured by `op-assets` (which owns
//! the fonts and the parser) and committed here as a generated source, so
//! text can be measured without a font, a build script or a dependency.
//! [`labels`] turns those advances into the width of a label and places
//! rows of labels from the widths, which is how the emitter knows what
//! fits (decision 14).

pub mod advances;
pub mod data;
pub mod labels;
pub mod layout;
pub mod render;
pub mod spec;
pub mod ticks;

pub use data::{Data, Error, escape_script, hash, hash_hex};
pub use labels::{
    ASCENT, DESCENT, Face, TEXT_PX, Wanted, marker_path, marker_samples, place, text_width,
};
pub use layout::Layout;
pub use render::{Aria, Rendered, announced, escape, render, render_with};
pub use spec::{Band, Chapter, Mark, Series, Spec};
pub use ticks::{tick_increment, tick_step, ticks};
