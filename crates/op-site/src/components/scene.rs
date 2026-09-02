//! `<op-scene>`: an embedded A-Frame scene. The A-Frame runtime is vendored
//! (vendor/aframe-1.8.0.min.js, copied unhashed into the site root) and
//! loaded lazily on PROXIMITY, not presence: an IntersectionObserver with a
//! 50% root margin injects the script only when the scene is within half a
//! viewport of view, so pages never pay for the runtime the reader does not
//! reach (and the main page, which has no scene, never loads it at all).
//! Entities take their colours from the active theme's tokens at build
//! time, and the spinning box is driven by a tick component registered from
//! Rust through an inline_js bridge - the same pattern op-webc uses for
//! custom elements.
//!
//! A-Frame renders in the light DOM (a-scene has never been reliable inside
//! shadow roots), so this element owns its subtree instead of slotting it.

use op_webc::{CustomElement, ElementDefinition};
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::prelude::wasm_bindgen;
use web_sys::{
    Document, Element, HtmlElement, IntersectionObserver, IntersectionObserverEntry,
    IntersectionObserverInit, Window,
};

pub const DEFINITION: ElementDefinition = ElementDefinition {
    tag: "op-scene",
    observed_attributes: &[],
    create: |host| {
        Box::new(Scene {
            host,
            onload: None,
            tick: None,
            observer: None,
            on_near: None,
        })
    },
};

#[wasm_bindgen(inline_js = "export function register_op_spin(tick) {
  if (!window.AFRAME || window.AFRAME.components['op-spin']) { return; }
  window.AFRAME.registerComponent('op-spin', {
    tick: function (time, delta) { tick(this.el, time, delta); },
  });
}")]
extern "C" {
    fn register_op_spin(tick: &js_sys::Function);
}

/// The tick callback A-Frame calls with (element, time, delta).
type TickClosure = Closure<dyn FnMut(JsValue, f64, f64)>;
type NearClosure = Closure<dyn FnMut(js_sys::Array, IntersectionObserver)>;

struct Scene {
    host: HtmlElement,
    /// Kept alive for the element's lifetime.
    onload: Option<Closure<dyn FnMut()>>,
    tick: Option<TickClosure>,
    observer: Option<IntersectionObserver>,
    on_near: Option<NearClosure>,
}

/// Reads a custom property off the document root, so the scene matches the
/// active theme.
fn token(window: &Window, document: &Document, name: &str) -> String {
    document
        .document_element()
        .and_then(|root| window.get_computed_style(&root).ok().flatten())
        .and_then(|style| style.get_property_value(name).ok())
        .map(|value| value.trim().to_owned())
        .unwrap_or_default()
}

/// Builds the scene markup once AFRAME is available.
fn build(window: &Window, document: &Document, host: &HtmlElement, tick: &js_sys::Function) {
    register_op_spin(tick);
    let bg = token(window, document, "--op-bg");
    let surface = token(window, document, "--op-surface");
    let accent = token(window, document, "--op-accent");
    let info = token(window, document, "--op-status-info");
    let ok = token(window, document, "--op-status-ok");
    host.set_inner_html(&format!(
        "<a-scene embedded style=\"height: 320px; width: 100%;\">\
<a-sky color=\"{bg}\"></a-sky>\
<a-plane position=\"0 0 -4\" rotation=\"-90 0 0\" width=\"14\" height=\"14\" color=\"{surface}\"></a-plane>\
<a-box op-spin position=\"-1.2 1.1 -3.5\" color=\"{accent}\"></a-box>\
<a-sphere position=\"0 1.25 -5.5\" radius=\"1.25\" color=\"{info}\"></a-sphere>\
<a-cylinder position=\"1.4 0.75 -3\" radius=\"0.5\" height=\"1.5\" color=\"{ok}\"></a-cylinder>\
</a-scene>"
    ));
}

/// Kicks off the runtime load (or builds immediately when another scene
/// already loaded it). Shared by the proximity callback and the fallback
/// path when IntersectionObserver is unavailable.
fn start_loading(
    window: &Window,
    document: &Document,
    host: &HtmlElement,
    tick_fn: &js_sys::Function,
    onload_fn: &js_sys::Function,
) {
    let already_loaded =
        js_sys::Reflect::has(window, &JsValue::from_str("AFRAME")).unwrap_or(false);
    if already_loaded {
        build(window, document, host, tick_fn);
        return;
    }
    // Load the vendored runtime once; further op-scene elements attach to
    // the same script's load event.
    let script = match document.query_selector("script[data-op-aframe]") {
        Ok(Some(existing)) => existing,
        _ => {
            let Ok(script) = document.create_element("script") else {
                return;
            };
            let _ = script.set_attribute("src", "/aframe-1.8.0.min.js");
            let _ = script.set_attribute("data-op-aframe", "");
            if let Some(body) = document.body() {
                let _ = body.append_child(&script);
            }
            script
        }
    };
    let _ = script.add_event_listener_with_callback("load", onload_fn);
}

impl CustomElement for Scene {
    fn connected(&mut self) {
        let Some(window) = web_sys::window() else {
            return;
        };
        let Some(document) = window.document() else {
            return;
        };
        self.host
            .set_inner_html("<p class=\"loading\">Loading the scene\u{2026}</p>");

        // The Rust-side tick: spin the box that carries op-spin.
        let tick = TickClosure::new(move |el: JsValue, time: f64, _delta: f64| {
            if let Some(el) = el.dyn_ref::<Element>() {
                let yaw = (time * 0.03) % 360.0;
                let _ = el.set_attribute("rotation", &format!("12 {yaw:.2} 0"));
            }
        });
        let tick_fn: js_sys::Function = tick.as_ref().unchecked_ref::<js_sys::Function>().clone();
        self.tick = Some(tick);

        let onload = Closure::<dyn FnMut()>::new({
            let window = window.clone();
            let document = document.clone();
            let host = self.host.clone();
            let tick_fn = tick_fn.clone();
            move || build(&window, &document, &host, &tick_fn)
        });
        let onload_fn: js_sys::Function =
            onload.as_ref().unchecked_ref::<js_sys::Function>().clone();
        self.onload = Some(onload);

        // Resources load when the scene is LIKELY to be seen: within half a
        // viewport (rootMargin 50%). The observer fires once and disconnects.
        let on_near = NearClosure::new({
            let window = window.clone();
            let document = document.clone();
            let host = self.host.clone();
            let tick_fn = tick_fn.clone();
            let onload_fn = onload_fn.clone();
            move |entries: js_sys::Array, observer: IntersectionObserver| {
                let near = entries.iter().any(|e| {
                    e.dyn_ref::<IntersectionObserverEntry>()
                        .map(|entry| entry.is_intersecting())
                        .unwrap_or(false)
                });
                if !near {
                    return;
                }
                observer.disconnect();
                start_loading(&window, &document, &host, &tick_fn, &onload_fn);
            }
        });
        let init = IntersectionObserverInit::new();
        init.set_root_margin("50%");
        match IntersectionObserver::new_with_options(on_near.as_ref().unchecked_ref(), &init) {
            Ok(observer) => {
                observer.observe(&self.host);
                self.observer = Some(observer);
                self.on_near = Some(on_near);
            }
            Err(_) => {
                // No observer support: fall back to load-on-connect.
                start_loading(&window, &document, &self.host, &tick_fn, &onload_fn);
            }
        }
    }
}
