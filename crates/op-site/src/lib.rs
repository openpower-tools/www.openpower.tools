// SPDX-License-Identifier: GPL-3.0-or-later
//! Library target: exposes the component registry so build-time tooling
//! (op-pages' XML vocabulary validation) can consume the exact element
//! definitions the site registers at runtime. The wasm entry point stays
//! in `main.rs` with its own module tree.

mod colour;
pub mod components;
mod fontprobe;
mod html;
mod theme;
mod viewtransition;
