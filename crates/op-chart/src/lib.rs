//! Chart geometry and SVG emission shared by the page build (native) and
//! the elements (wasm). Everything here is pure: a [`Spec`] in, an SVG
//! string and a [`Layout`] out. Colours never appear as literals of this
//! crate's own; classes map to the site's tokens in the consumer's
//! stylesheet, so a theme change re-styles with zero re-render.
//!
//! [`data`] reads the JSON block a chart carries, hashes it so a
//! pre-render can be told from a stale one, and turns it into a [`Spec`].

pub mod data;
pub mod labels;
pub mod layout;
pub mod render;
pub mod spec;
pub mod ticks;

pub use data::{Data, Error, escape_script, hash, hash_hex};
pub use labels::{marker_path, marker_samples, spread};
pub use layout::Layout;
pub use render::{Rendered, escape, render};
pub use spec::{Band, Chapter, Mark, Series, Spec};
pub use ticks::{tick_increment, tick_step, ticks};
