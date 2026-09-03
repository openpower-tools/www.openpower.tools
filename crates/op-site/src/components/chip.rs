//! `<opt-chip variant="neutral|ok|warning|danger|info" toggle pressed removable>`:
//! the interactive sibling of opt-badge, for filters and selections. With
//! `toggle`, clicking flips the pressed state (mirrored as aria-pressed);
//! with `removable`, a close control dispatches a composed, bubbling
//! `opt-chip-remove` event and removes the chip. Without either, it renders
//! as a static token.

use op_webc::{CustomElement, ElementDefinition};
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use web_sys::{CustomEvent, CustomEventInit, HtmlElement};

use super::{BASE_CSS, shadow_root};

pub const DEFINITION: ElementDefinition = ElementDefinition {
    source: op_webc::here!(),
    tag: "opt-chip",
    observed_attributes: &["variant", "toggle", "pressed", "removable"],
    create: |host| {
        Box::new(Chip {
            host,
            retained: Vec::new(),
        })
    },
};

struct Chip {
    host: HtmlElement,
    /// Event closures, kept alive for the element's lifetime; re-renders
    /// append rather than replace so in-flight handlers stay valid.
    retained: Vec<Closure<dyn FnMut()>>,
}

const VARIANTS: &[&str] = &["neutral", "ok", "warning", "danger", "info"];

impl Chip {
    fn render(&mut self) {
        let variant = self
            .host
            .get_attribute("variant")
            .filter(|v| VARIANTS.contains(&v.as_str()))
            .unwrap_or_else(|| "neutral".to_owned());
        let dot = match variant.as_str() {
            "ok" => "var(--op-status-ok)",
            "warning" => "var(--op-status-warning)",
            "danger" => "var(--op-status-danger)",
            "info" => "var(--op-status-info)",
            _ => "var(--op-status-neutral)",
        };
        let toggle = self.host.has_attribute("toggle");
        let pressed = self.host.has_attribute("pressed");
        let removable = self.host.has_attribute("removable");
        // The body is a real button only when it toggles; the dot and label
        // stay the data ink, the pressed state fills with the code
        // background and strengthens the border.
        let body = if toggle {
            format!(
                "<button type=\"button\" class=\"body\" aria-pressed=\"{}\"><span class=\"dot\" aria-hidden=\"true\"></span><slot></slot></button>",
                if pressed { "true" } else { "false" }
            )
        } else {
            "<span class=\"body\"><span class=\"dot\" aria-hidden=\"true\"></span><slot></slot></span>"
                .to_owned()
        };
        let remove = if removable {
            "<button type=\"button\" class=\"remove\" aria-label=\"Remove\">\u{00d7}</button>"
        } else {
            ""
        };
        shadow_root(&self.host).set_inner_html(&format!(
            "<style>{BASE_CSS}
:host {{
  display: inline-flex;
  align-items: stretch;
  border: 1px solid var(--op-border);
  border-radius: 1em;
  font-size: 0.8em;
  white-space: nowrap;
  background: {pressed_bg};
}}
.body {{
  display: inline-flex;
  align-items: center;
  gap: 0.35em;
  padding: 0.05em 0.7em 0.05em 0.5em;
  font: inherit;
  color: var(--op-text);
  background: none;
  border: none;
  border-radius: 1em 0 0 1em;
}}
button.body {{ cursor: pointer; }}
button.body:hover {{ color: var(--op-link-hover); }}
:host(:not([removable])) .body {{ border-radius: 1em; }}
.dot {{
  width: 0.55em;
  height: 0.55em;
  border-radius: 50%;
  background: {dot};
}}
.remove {{
  font: inherit;
  color: var(--op-muted);
  background: none;
  border: none;
  border-left: 1px solid var(--op-border);
  border-radius: 0 1em 1em 0;
  padding: 0 0.5em;
  cursor: pointer;
}}
.remove:hover {{ color: var(--op-status-danger); }}
</style>
{body}{remove}",
            pressed_bg = if pressed {
                "var(--op-code-bg)"
            } else {
                "transparent"
            },
        ));
        if pressed {
            let _ = self
                .host
                .style()
                .set_property("border-color", "var(--op-accent)");
        } else {
            let _ = self.host.style().remove_property("border-color");
        }
        let shadow = shadow_root(&self.host);
        if toggle && let Ok(Some(body_button)) = shadow.query_selector("button.body") {
            let click = Closure::<dyn FnMut()>::new({
                let host = self.host.clone();
                move || {
                    if host.has_attribute("pressed") {
                        let _ = host.remove_attribute("pressed");
                    } else {
                        let _ = host.set_attribute("pressed", "");
                    }
                }
            });
            let _ = body_button
                .add_event_listener_with_callback("click", click.as_ref().unchecked_ref());
            self.retained.push(click);
        }
        if removable && let Ok(Some(remove_button)) = shadow.query_selector("button.remove") {
            let click = Closure::<dyn FnMut()>::new({
                let host = self.host.clone();
                move || {
                    let init = CustomEventInit::new();
                    init.set_bubbles(true);
                    init.set_composed(true);
                    if let Ok(event) =
                        CustomEvent::new_with_event_init_dict("opt-chip-remove", &init)
                    {
                        let _ = host.dispatch_event(&event);
                    }
                    host.remove();
                }
            });
            let _ = remove_button
                .add_event_listener_with_callback("click", click.as_ref().unchecked_ref());
            self.retained.push(click);
        }
    }
}

impl CustomElement for Chip {
    fn connected(&mut self) {
        self.render();
    }

    fn attribute_changed(&mut self, _n: &str, _o: Option<String>, _v: Option<String>) {
        self.render();
    }
}
