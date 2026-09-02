// SPDX-License-Identifier: GPL-3.0-or-later
//! www.openpower.tools component library.
//!
//! The single module tree: the wasm binary in `main.rs` is a thin entry
//! point over this library, and build-time tooling (op-pages' XML
//! vocabulary validation) consumes [`components::DEFINITIONS`] and the
//! [`theme`] storage contract from here.

mod colour;
pub mod components;
mod fontprobe;
mod html;
pub mod theme;

#[cfg(test)]
mod palette;
