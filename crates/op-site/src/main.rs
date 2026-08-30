//! www.openpower.tools
//!
//! The page is composed in `index.html` out of custom elements; each element's
//! behaviour lives in [`components`] and is registered here when the wasm
//! module starts. Trunk builds this binary for `wasm32-unknown-unknown` and
//! injects the loader into `index.html`.

mod colour;
mod components;
mod html;
mod theme;

#[cfg(test)]
mod palette;

fn main() {
    console_error_panic_hook::set_once();
    for definition in components::DEFINITIONS {
        op_webc::define(definition);
    }
}
