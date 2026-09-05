//! What the Trunk hook and the table generator share: the webfont
//! manifest, and the advance-width measurement `op-chart` compiles in.
//!
//! The hook itself is the `op-assets` bin; the generator is
//! `emit-advances`. Both read the same faces through [`manifest`], so
//! neither can measure a face the site does not serve.

pub mod advances;
pub mod manifest;
