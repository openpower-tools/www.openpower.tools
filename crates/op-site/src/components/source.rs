//! `<opt-source label="...">`: a labelled frame for a light-DOM `<pre>` block
//! with a copy control. The label names the language, file or origin; the
//! button copies the pre's text (clipboard access needs a secure context).

use std::rc::Rc;

use op_webc::{CustomElement, ElementDefinition};
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use wasm_bindgen::closure::Closure;
use web_sys::HtmlElement;

use super::{BASE_CSS, shadow_root};
use crate::html::escape;

pub const DEFINITION: ElementDefinition = ElementDefinition {
    source: op_webc::here!(),
    tag: "opt-source",
    observed_attributes: &["label"],
    properties: &[],
    create: |host| {
        Box::new(Source {
            host,
            retained: Vec::new(),
            swallowed: Vec::new(),
        })
    },
};

/// A handler for the clipboard's rejected promise, shared with the click
/// closure that installs it.
type Swallow = Rc<Closure<dyn FnMut(JsValue)>>;

struct Source {
    host: HtmlElement,
    /// Event and timeout closures, kept alive for the element's lifetime.
    /// Re-renders append rather than replace, so a pending "Copied" reset
    /// can still fire safely after the label changes.
    retained: Vec<Closure<dyn FnMut()>>,
    /// The clipboard's rejection handlers, one per render, kept for the
    /// same reason.
    swallowed: Vec<Swallow>,
}

impl Source {
    fn render(&mut self) {
        let label = self.host.get_attribute("label").unwrap_or_default();
        let shadow = shadow_root(&self.host);
        shadow.set_inner_html(&format!(
            "<style>{BASE_CSS}
:host {{ display: block; margin: 1rem 0; }}
.frame {{
  border: 1px solid var(--op-border);
  border-radius: 0.375rem;
  overflow: hidden;
  background: var(--op-code-bg);
}}
.bar {{
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 1rem;
  padding: 0.3rem 0.6rem 0.3rem 1rem;
  border-bottom: 1px solid var(--op-border);
  font-family: var(--op-font-mono);
  font-size: 0.75rem;
  color: var(--op-muted);
}}
button {{
  font: inherit;
  color: var(--op-muted);
  background: none;
  border: 1px solid var(--op-border-strong);
  border-radius: 0.25rem;
  padding: 0.1rem 0.6rem;
  cursor: pointer;
}}
button:hover {{ color: var(--op-text); border-color: var(--op-accent); }}
::slotted(pre) {{
  margin: 0;
  padding: 0.6rem 1rem;
  overflow-x: auto;
  font-family: var(--op-font-mono);
  font-size: 0.85rem;
  font-variant-ligatures: contextual;
}}
</style>
<div class=\"frame\"><div class=\"bar\"><span>{label}</span><button type=\"button\">Copy</button></div><slot></slot></div>",
            label = escape(&label),
        ));
        let Some(button) = shadow.query_selector("button").ok().flatten() else {
            return;
        };
        let revert = Closure::<dyn FnMut()>::new({
            let button = button.clone();
            move || button.set_text_content(Some("Copy"))
        });
        let revert_fn = revert.as_ref().unchecked_ref::<js_sys::Function>().clone();
        // A browser may refuse the clipboard, and the refusal arrives as a
        // rejected promise: swallow it here, or it surfaces as an uncaught
        // error. The button's own label is the feedback either way.
        let swallow: Swallow = Rc::new(Closure::<dyn FnMut(JsValue)>::new(|_| {}));
        let click = Closure::<dyn FnMut()>::new({
            let host = self.host.clone();
            let button = button.clone();
            let swallow = Rc::clone(&swallow);
            move || {
                let Some(window) = web_sys::window() else {
                    return;
                };
                let text = host
                    .query_selector("pre")
                    .ok()
                    .flatten()
                    .and_then(|pre| pre.text_content())
                    .unwrap_or_default();
                let _ = window
                    .navigator()
                    .clipboard()
                    .write_text(&text)
                    .catch(&swallow);
                button.set_text_content(Some("Copied"));
                let _ =
                    window.set_timeout_with_callback_and_timeout_and_arguments_0(&revert_fn, 1500);
            }
        });
        let _ = button.add_event_listener_with_callback("click", click.as_ref().unchecked_ref());
        self.retained.push(click);
        self.retained.push(revert);
        self.swallowed.push(swallow);
    }
}

impl CustomElement for Source {
    fn connected(&mut self) {
        self.render();
    }

    fn attribute_changed(&mut self, _n: &str, _o: Option<String>, _v: Option<String>) {
        self.render();
    }
}
