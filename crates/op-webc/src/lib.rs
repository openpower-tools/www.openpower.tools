//! Define Web Components (custom elements) in Rust.
//!
//! The browser's custom element registry requires a JavaScript class that
//! extends `HTMLElement`, and wasm-bindgen cannot export a Rust type as a
//! subclass of a JS class. So this crate carries the one piece of JavaScript in
//! the project: [`SHIM`], a generic class factory embedded with
//! `#[wasm_bindgen(inline_js)]`. wasm-bindgen emits it at build time as a
//! snippet next to the wasm glue; it defines the element class once per tag and
//! forwards every lifecycle callback to a Rust [`CustomElement`] instance held
//! in an [`ElementHandle`].
//!
//! The same forwarding pattern (a JS object whose methods call into a
//! wasm-bindgen-exported struct) is what an A-Frame component registration
//! (`AFRAME.registerComponent(name, { init, tick, ... })`) needs, so that can
//! be added later without changing this design.
//!
//! Constraint: callbacks run while the handle is mutably borrowed. An element
//! must not, inside a callback, synchronously trigger another callback on
//! itself (for example by setting one of its own observed attributes), or
//! wasm-bindgen will throw a "recursive use of an object" error.

use wasm_bindgen::prelude::*;
use web_sys::HtmlElement;

/// Behaviour of one custom element instance. All methods have empty defaults
/// so an element implements only what it needs.
pub trait CustomElement: 'static {
    /// The element was inserted into a document. May run more than once.
    fn connected(&mut self) {}
    /// The element was removed from a document.
    fn disconnected(&mut self) {}
    /// The element was moved to a new document.
    fn adopted(&mut self) {}
    /// One of [`ElementDefinition::observed_attributes`] changed. Runs before
    /// [`connected`](Self::connected) for attributes present at parse time.
    fn attribute_changed(&mut self, _name: &str, _old: Option<String>, _new: Option<String>) {}
}

/// Static description of a custom element class.
pub struct ElementDefinition {
    /// Tag name; must contain a hyphen, e.g. `op-theme-toggle`.
    pub tag: &'static str,
    /// Attributes for which [`CustomElement::attribute_changed`] fires.
    pub observed_attributes: &'static [&'static str],
    /// Creates the Rust state for one host element. Called lazily on the first
    /// lifecycle callback, never from the JS constructor, so the host may be
    /// inspected freely.
    pub create: fn(HtmlElement) -> Box<dyn CustomElement>,
}

/// Per-instance state, owned by the JS element as `this.__rust`.
#[wasm_bindgen]
pub struct ElementHandle {
    inner: Box<dyn CustomElement>,
}

#[wasm_bindgen]
impl ElementHandle {
    /// Forwarded from `connectedCallback`.
    pub fn connected(&mut self) {
        self.inner.connected();
    }

    /// Forwarded from `disconnectedCallback`.
    pub fn disconnected(&mut self) {
        self.inner.disconnected();
    }

    /// Forwarded from `adoptedCallback`.
    pub fn adopted(&mut self) {
        self.inner.adopted();
    }

    /// Forwarded from `attributeChangedCallback`.
    #[wasm_bindgen(js_name = attributeChanged)]
    pub fn attribute_changed(&mut self, name: String, old: Option<String>, new: Option<String>) {
        self.inner.attribute_changed(&name, old, new);
    }
}

/// Per-tag factory handed to the shim.
#[wasm_bindgen]
pub struct ElementClass {
    create: fn(HtmlElement) -> Box<dyn CustomElement>,
}

#[wasm_bindgen]
impl ElementClass {
    /// Creates the Rust state for `host`.
    pub fn create(&self, host: HtmlElement) -> ElementHandle {
        ElementHandle { inner: (self.create)(host) }
    }
}

/// The JavaScript shim, kept as a Rust constant so tests can check it against
/// the exported method names, and inlined below for wasm-bindgen.
pub const SHIM: &str = r#"
export function defineElement(tag, observedAttributes, cls) {
  class RustElement extends HTMLElement {
    static get observedAttributes() { return observedAttributes; }
    #rust() { return (this.__rust ??= cls.create(this)); }
    connectedCallback() { this.#rust().connected(); }
    disconnectedCallback() { this.#rust().disconnected(); }
    adoptedCallback() { this.#rust().adopted(); }
    attributeChangedCallback(name, oldValue, newValue) {
      this.#rust().attributeChanged(name, oldValue, newValue);
    }
  }
  customElements.define(tag, RustElement);
}
"#;

#[wasm_bindgen(inline_js = r#"
export function defineElement(tag, observedAttributes, cls) {
  class RustElement extends HTMLElement {
    static get observedAttributes() { return observedAttributes; }
    #rust() { return (this.__rust ??= cls.create(this)); }
    connectedCallback() { this.#rust().connected(); }
    disconnectedCallback() { this.#rust().disconnected(); }
    adoptedCallback() { this.#rust().adopted(); }
    attributeChangedCallback(name, oldValue, newValue) {
      this.#rust().attributeChanged(name, oldValue, newValue);
    }
  }
  customElements.define(tag, RustElement);
}
"#)]
extern "C" {
    #[wasm_bindgen(js_name = defineElement)]
    fn define_element(tag: &str, observed_attributes: js_sys::Array, cls: ElementClass);
}

/// Registers `definition` with the browser's custom element registry.
pub fn define(definition: &ElementDefinition) {
    let observed = definition.observed_attributes.iter().map(|a| JsValue::from_str(a)).collect();
    define_element(definition.tag, observed, ElementClass { create: definition.create });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The inline_js literal must stay identical to `SHIM`; this reads the
    /// source file so a drift between the two copies fails the build.
    #[test]
    fn inline_js_matches_shim_constant() {
        let source = include_str!("lib.rs");
        let signature = format!("export function {}(tag, observedAttributes, cls) {{", "defineElement");
        let occurrences = source.matches(&signature).count();
        assert_eq!(occurrences, 2, "expected the shim text once in SHIM and once in inline_js");
        let inline_start = source.find("inline_js = r#\"").expect("inline_js literal") + "inline_js = r#\"".len();
        let inline_end = source[inline_start..].find("\"#)]").expect("end of inline_js") + inline_start;
        assert_eq!(&source[inline_start..inline_end], SHIM);
    }

    /// Every method the shim calls on the handle must exist as a wasm-bindgen
    /// export with that JS name.
    #[test]
    fn shim_only_calls_exported_methods() {
        for method in ["connected()", "disconnected()", "adopted()", "attributeChanged(name, oldValue, newValue)"] {
            assert!(SHIM.contains(&format!(".{method}")), "shim does not call {method}");
        }
        assert!(SHIM.contains("cls.create(this)"));
        let source = include_str!("lib.rs");
        assert!(source.contains("pub fn connected(&mut self)"));
        assert!(source.contains("pub fn disconnected(&mut self)"));
        assert!(source.contains("pub fn adopted(&mut self)"));
        assert!(source.contains("js_name = attributeChanged"));
        assert!(source.contains("pub fn create(&self, host: HtmlElement) -> ElementHandle"));
    }

    #[test]
    fn definitions_require_hyphenated_tags() {
        fn create(_: HtmlElement) -> Box<dyn CustomElement> {
            struct Noop;
            impl CustomElement for Noop {}
            Box::new(Noop)
        }
        let definition = ElementDefinition { tag: "op-example", observed_attributes: &["heading"], create };
        assert!(definition.tag.contains('-'), "custom element tags must contain a hyphen");
        assert_eq!(definition.observed_attributes, &["heading"]);
    }
}
