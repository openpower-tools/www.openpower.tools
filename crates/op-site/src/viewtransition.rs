//! Locates `document.startViewTransition` when the API exists and the
//! user has not asked for reduced motion, so visual state changes (the
//! font swap) cross-fade (300ms, see `styles/theme.css`) instead of
//! popping. The theme flip no longer uses this: its palette blend is a
//! long property transition of its own (see the easing block in
//! `styles/theme.css`).

use wasm_bindgen::{JsCast, JsValue};
use web_sys::{Document, Window};

/// The `startViewTransition` function, when present and welcome.
pub fn start_function(window: &Window, document: &Document) -> Option<js_sys::Function> {
    let reduced = window
        .match_media("(prefers-reduced-motion: reduce)")
        .ok()
        .flatten()
        .is_some_and(|mql| mql.matches());
    if reduced {
        return None;
    }
    js_sys::Reflect::get(document.as_ref(), &JsValue::from_str("startViewTransition"))
        .ok()?
        .dyn_into::<js_sys::Function>()
        .ok()
}
