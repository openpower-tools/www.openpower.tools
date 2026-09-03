//! Chart geometry and SVG emission shared by the page build (native) and
//! the elements (wasm). Everything here is pure: a [`Spec`] in, an SVG
//! string and a [`Layout`] out. Colours never appear as literals of this
//! crate's own; classes map to the site's tokens in the consumer's
//! stylesheet, so a theme change re-styles with zero re-render.

pub mod labels;
pub mod layout;
pub mod render;
pub mod spec;
pub mod ticks;

pub use labels::{marker_path, marker_samples, spread};
pub use layout::Layout;
pub use render::{Rendered, escape, render};
pub use spec::{Chapter, Series, Spec};
pub use ticks::{tick_increment, tick_step, ticks};
