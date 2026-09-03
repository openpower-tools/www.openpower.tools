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

pub mod attention;
pub mod machine;

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

/// A workspace-relative Rust source position, as `file!()`/`line!()`
/// report it for workspace members (`crates/...`) - the same shape the
/// site serves under `/src/`.
pub struct SourceLocation {
    pub path: &'static str,
    pub line: u32,
}

/// Captures the invocation site as a [`SourceLocation`]. Use it for
/// [`ElementDefinition::source`] so the inspector's jump-to-definition
/// on a custom element opens the Rust component.
#[macro_export]
macro_rules! here {
    () => {
        $crate::SourceLocation {
            path: file!(),
            line: line!(),
        }
    };
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
    /// Where this definition lives ([`here!`]); the shim maps the
    /// element's generated class onto it so inspectors jump straight to
    /// the Rust source, which the site serves under `/src/`.
    pub source: SourceLocation,
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
        ElementHandle {
            inner: (self.create)(host),
        }
    }
}

/// The JavaScript shim, kept as a Rust constant so tests can check it against
/// the exported method names, and inlined below for wasm-bindgen.
pub const SHIM: &str = r#"
const VLQ_CHARS = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
function vlq(value) {
  let x = value < 0 ? (-value << 1) | 1 : value << 1;
  let out = "";
  do {
    let digit = x & 31;
    x >>>= 5;
    if (x) digit |= 32;
    out += VLQ_CHARS[digit];
  } while (x);
  return out;
}
export function defineElement(tag, observedAttributes, cls, path, line) {
  const rust = (el) => (el.__rust ??= cls.create(el));
  const make = (HTMLElement, observedAttributes, rust) => class extends HTMLElement {
    constructor() {
      super();
      // ElementInternals: custom states for CSS (:state()) and default
      // ARIA semantics, owned by the element rather than stamped on it.
      this.__internals = this.attachInternals();
    }
    static get observedAttributes() { return observedAttributes; }
    connectedCallback() { rust(this).connected(); }
    disconnectedCallback() { rust(this).disconnected(); }
    adoptedCallback() { rust(this).adopted(); }
    attributeChangedCallback(name, oldValue, newValue) {
      rust(this).attributeChanged(name, oldValue, newValue);
    }
  };
  const body = "export default " + make.toString() + ";";
  // Every line of the module maps to the element's DEFINITION in its
  // Rust source (served under /src/), so the inspector's
  // jump-to-definition opens the component instead of this glue. The
  // module is loaded from a blob URL rather than eval'd: source maps
  // are only applied to scripts that have a real, fetchable URL.
  const mappings = ["AA" + vlq(line - 1) + "A"];
  for (let i = body.split("\n").length; i > 1; i--) mappings.push("AAAA");
  const map = {
    version: 3,
    file: tag + ".js",
    sources: ["/src/" + path],
    sourcesContent: [null],
    names: [],
    mappings: mappings.join(";"),
  };
  // No //# sourceURL: it would replace the blob's real URL with an
  // unfetchable name, and an unfetchable script is exactly what stops
  // the inspector resolving the map.
  const source = body
    + "\n//# sourceMappingURL=data:application/json;base64," + btoa(JSON.stringify(map));
  const define = (factory) =>
    customElements.define(tag, factory(HTMLElement, observedAttributes, rust));
  try {
    const url = URL.createObjectURL(new Blob([source], { type: "text/javascript" }));
    import(url).then(
      (module) => define(module.default),
      // A Content-Security-Policy may forbid blob: scripts; the plain
      // class behaves identically, only the jump target is lost.
      () => define(make),
    );
  } catch (_blocked) {
    define(make);
  }
}
"#;

#[wasm_bindgen(inline_js = r#"
const VLQ_CHARS = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
function vlq(value) {
  let x = value < 0 ? (-value << 1) | 1 : value << 1;
  let out = "";
  do {
    let digit = x & 31;
    x >>>= 5;
    if (x) digit |= 32;
    out += VLQ_CHARS[digit];
  } while (x);
  return out;
}
export function defineElement(tag, observedAttributes, cls, path, line) {
  const rust = (el) => (el.__rust ??= cls.create(el));
  const make = (HTMLElement, observedAttributes, rust) => class extends HTMLElement {
    constructor() {
      super();
      // ElementInternals: custom states for CSS (:state()) and default
      // ARIA semantics, owned by the element rather than stamped on it.
      this.__internals = this.attachInternals();
    }
    static get observedAttributes() { return observedAttributes; }
    connectedCallback() { rust(this).connected(); }
    disconnectedCallback() { rust(this).disconnected(); }
    adoptedCallback() { rust(this).adopted(); }
    attributeChangedCallback(name, oldValue, newValue) {
      rust(this).attributeChanged(name, oldValue, newValue);
    }
  };
  const body = "export default " + make.toString() + ";";
  // Every line of the module maps to the element's DEFINITION in its
  // Rust source (served under /src/), so the inspector's
  // jump-to-definition opens the component instead of this glue. The
  // module is loaded from a blob URL rather than eval'd: source maps
  // are only applied to scripts that have a real, fetchable URL.
  const mappings = ["AA" + vlq(line - 1) + "A"];
  for (let i = body.split("\n").length; i > 1; i--) mappings.push("AAAA");
  const map = {
    version: 3,
    file: tag + ".js",
    sources: ["/src/" + path],
    sourcesContent: [null],
    names: [],
    mappings: mappings.join(";"),
  };
  // No //# sourceURL: it would replace the blob's real URL with an
  // unfetchable name, and an unfetchable script is exactly what stops
  // the inspector resolving the map.
  const source = body
    + "\n//# sourceMappingURL=data:application/json;base64," + btoa(JSON.stringify(map));
  const define = (factory) =>
    customElements.define(tag, factory(HTMLElement, observedAttributes, rust));
  try {
    const url = URL.createObjectURL(new Blob([source], { type: "text/javascript" }));
    import(url).then(
      (module) => define(module.default),
      // A Content-Security-Policy may forbid blob: scripts; the plain
      // class behaves identically, only the jump target is lost.
      () => define(make),
    );
  } catch (_blocked) {
    define(make);
  }
}
"#)]
extern "C" {
    #[wasm_bindgen(js_name = defineElement)]
    fn define_element(
        tag: &str,
        observed_attributes: js_sys::Array,
        cls: ElementClass,
        path: &str,
        line: u32,
    );
}

/// The `ElementInternals` the shim attached to `host` in its constructor
/// (as a plain value: the pinned web-sys has no binding for it yet).
pub fn internals(host: &HtmlElement) -> Option<JsValue> {
    js_sys::Reflect::get(host, &JsValue::from_str("__internals"))
        .ok()
        .filter(|v| !v.is_undefined() && !v.is_null())
}

/// Sets or clears a custom state on `host`, selectable as `:state(name)`
/// from outside and `:host(:state(name))` inside the shadow tree. Custom
/// states are how an element exposes INTERNAL state to CSS; attributes
/// remain the author's configuration surface.
pub fn set_state(host: &HtmlElement, name: &str, on: bool) {
    let Some(internals) = internals(host) else {
        return;
    };
    let Ok(states) = js_sys::Reflect::get(&internals, &JsValue::from_str("states")) else {
        return;
    };
    let method = if on { "add" } else { "delete" };
    if let Ok(function) = js_sys::Reflect::get(&states, &JsValue::from_str(method))
        && let Ok(function) = function.dyn_into::<js_sys::Function>()
    {
        let _ = function.call1(&states, &JsValue::from_str(name));
    }
}

/// Registers `definition` with the browser's custom element registry.
pub fn define(definition: &ElementDefinition) {
    let observed = definition
        .observed_attributes
        .iter()
        .map(|a| JsValue::from_str(a))
        .collect();
    define_element(
        definition.tag,
        observed,
        ElementClass {
            create: definition.create,
        },
        definition.source.path,
        definition.source.line,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The inline_js literal must stay identical to `SHIM`; this reads the
    /// source file so a drift between the two copies fails the build.
    #[test]
    fn inline_js_matches_shim_constant() {
        let source = include_str!("lib.rs");
        let signature = format!(
            "export function {}(tag, observedAttributes, cls, path, line) {{",
            "defineElement"
        );
        let occurrences = source.matches(&signature).count();
        assert_eq!(
            occurrences, 2,
            "expected the shim text once in SHIM and once in inline_js"
        );
        let inline_start =
            source.find("inline_js = r#\"").expect("inline_js literal") + "inline_js = r#\"".len();
        let inline_end = source[inline_start..]
            .find("\"#)]")
            .expect("end of inline_js")
            + inline_start;
        assert_eq!(&source[inline_start..inline_end], SHIM);
    }

    /// Every method the shim calls on the handle must exist as a wasm-bindgen
    /// export with that JS name.
    #[test]
    fn shim_only_calls_exported_methods() {
        for method in [
            "connected()",
            "disconnected()",
            "adopted()",
            "attributeChanged(name, oldValue, newValue)",
        ] {
            assert!(
                SHIM.contains(&format!(".{method}")),
                "shim does not call {method}"
            );
        }
        assert!(SHIM.contains("cls.create(el)"));
        assert!(
            SHIM.contains("sourceMappingURL=data:application/json;base64,"),
            "shim does not attach the per-element source map"
        );
        assert!(
            SHIM.contains("URL.createObjectURL") && SHIM.contains("import(url)"),
            "the class module must load from a real URL: source maps are \
             not applied to eval'd scripts"
        );
        assert!(
            !SHIM.contains("new Function"),
            "eval'd classes defeat the source map"
        );
        assert!(
            SHIM.contains("this.__internals = this.attachInternals();"),
            "the shim must attach ElementInternals for custom states"
        );
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
        let definition = ElementDefinition {
            tag: "op-example",
            observed_attributes: &["heading"],
            create,
            source: crate::here!(),
        };
        assert!(
            definition.tag.contains('-'),
            "custom element tags must contain a hyphen"
        );
        assert_eq!(definition.observed_attributes, &["heading"]);
        assert!(
            definition
                .source
                .path
                .ends_with("crates/op-webc/src/lib.rs"),
            "{}",
            definition.source.path
        );
        assert!(definition.source.line > 0);
    }
}
