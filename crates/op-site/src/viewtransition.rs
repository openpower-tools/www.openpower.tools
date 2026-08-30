//! Runs DOM mutations inside `document.startViewTransition` when the API
//! exists and the user has not asked for reduced motion, so visual state
//! changes cross-fade (300ms, see `styles/theme.css`) instead of popping.
//! Everywhere else the mutation runs directly.

use wasm_bindgen::closure::Closure;
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

/// Applies `mutate` inside a view transition when possible, else directly.
pub fn run(mutate: impl FnOnce() + 'static) {
    let context = web_sys::window().and_then(|w| {
        let d = w.document()?;
        let f = start_function(&w, &d)?;
        Some((d, f))
    });
    match context {
        Some((document, start)) => {
            let callback = Closure::once_into_js(mutate);
            if start.call1(document.as_ref(), &callback).is_err() {
                // The mutation must happen regardless of animation plumbing.
                let _ = js_sys::Function::from(callback).call0(&JsValue::NULL);
            }
        }
        None => mutate(),
    }
}
