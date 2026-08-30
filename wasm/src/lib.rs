//! WebAssembly module for <https://www.openpower.tools>.
//!
//! This is a skeleton. It exposes a few small functions so the Lit front end
//! can confirm that the Rust -> wasm-bindgen -> Vite pipeline works end to
//! end. Real functionality will be added as the project takes shape.

use wasm_bindgen::prelude::*;

/// Runs once when the module is instantiated: route Rust panics to the
/// browser console instead of an opaque `unreachable` trap.
#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
}

/// Version of this crate, taken from `Cargo.toml` at compile time.
#[wasm_bindgen]
#[must_use]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}

/// Target architecture this module was compiled for (`wasm32` in the browser,
/// the host architecture when running the native test suite).
#[wasm_bindgen]
#[must_use]
pub fn target_arch() -> String {
    std::env::consts::ARCH.to_owned()
}

/// Human-readable one-line status combining [`version`] and [`target_arch`].
#[wasm_bindgen]
#[must_use]
pub fn status_line() -> String {
    format!("openpower-tools-wasm {} ({})", version(), target_arch())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_three_numeric_components() {
        let v = version();
        let parts: Vec<&str> = v.split('.').collect();
        assert_eq!(parts.len(), 3, "version {v:?} should be MAJOR.MINOR.PATCH");
        for part in parts {
            assert!(
                part.parse::<u64>().is_ok(),
                "component {part:?} of {v:?} is not numeric"
            );
        }
    }

    #[test]
    fn target_arch_matches_host_when_tested_natively() {
        assert_eq!(target_arch(), std::env::consts::ARCH);
        assert!(!target_arch().is_empty());
    }

    #[test]
    fn status_line_embeds_version_and_arch() {
        let line = status_line();
        assert_eq!(
            line,
            format!(
                "openpower-tools-wasm {} ({})",
                env!("CARGO_PKG_VERSION"),
                std::env::consts::ARCH
            )
        );
    }
}
