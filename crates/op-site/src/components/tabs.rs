//! `<opt-tabs>` and `<opt-tab label="...">`: a tabbed region. The panels are
//! light-DOM `<opt-tab>` children; the tablist is rendered in shadow DOM from
//! their labels, with click and arrow-key activation. Shadow ids cannot be
//! referenced from light DOM, so the panels carry `role="tabpanel"` without
//! an `aria-controls` linkage; selection state still reads correctly.

use op_webc::{CustomElement, ElementDefinition};
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use web_sys::{Element, HtmlElement};

use super::{BASE_CSS, shadow_root};
use crate::html::escape;

pub const DEFINITION: ElementDefinition = ElementDefinition {
    tag: "opt-tabs",
    observed_attributes: &[],
    create: |host| {
        Box::new(Tabs {
            host,
            listeners: Vec::new(),
        })
    },
};

pub const TAB_DEFINITION: ElementDefinition = ElementDefinition {
    tag: "opt-tab",
    observed_attributes: &[],
    create: |host| Box::new(Tab { host }),
};

struct Tabs {
    host: HtmlElement,
    /// Event closures, kept alive for the element's lifetime.
    listeners: Vec<Closure<dyn FnMut(web_sys::Event)>>,
}

struct Tab {
    host: HtmlElement,
}

/// Shows panel `index`, hides the rest, and mirrors the state onto the
/// shadow tablist buttons.
fn activate(host: &HtmlElement, index: u32) {
    if let Some(shadow) = host.shadow_root()
        && let Ok(buttons) = shadow.query_selector_all("[role=tab]")
    {
        for i in 0..buttons.length() {
            let Some(button) = buttons.item(i).and_then(|n| n.dyn_into::<Element>().ok()) else {
                continue;
            };
            let selected = i == index;
            let _ = button.set_attribute("aria-selected", if selected { "true" } else { "false" });
            let _ = button.set_attribute("tabindex", if selected { "0" } else { "-1" });
        }
    }
    if let Ok(panels) = host.query_selector_all(":scope > opt-tab") {
        for i in 0..panels.length() {
            let Some(panel) = panels.item(i).and_then(|n| n.dyn_into::<Element>().ok()) else {
                continue;
            };
            if i == index {
                let _ = panel.remove_attribute("hidden");
            } else {
                let _ = panel.set_attribute("hidden", "");
            }
        }
    }
}

/// The index the tablist currently marks selected.
fn selected_index(host: &HtmlElement) -> u32 {
    host.shadow_root()
        .and_then(|s| s.query_selector("[role=tab][aria-selected=true]").ok())
        .flatten()
        .and_then(|b| b.get_attribute("data-index"))
        .and_then(|i| i.parse().ok())
        .unwrap_or(0)
}

fn focus_button(host: &HtmlElement, index: u32) {
    if let Some(button) = host
        .shadow_root()
        .and_then(|s| s.query_selector(&format!("[data-index=\"{index}\"]")).ok())
        .flatten()
        .and_then(|b| b.dyn_into::<HtmlElement>().ok())
    {
        let _ = button.focus();
    }
}

impl CustomElement for Tabs {
    fn connected(&mut self) {
        let mut labels = Vec::new();
        if let Ok(panels) = self.host.query_selector_all(":scope > opt-tab") {
            for i in 0..panels.length() {
                let label = panels
                    .item(i)
                    .and_then(|n| n.dyn_into::<Element>().ok())
                    .and_then(|t| t.get_attribute("label"))
                    .unwrap_or_else(|| format!("Tab {}", i + 1));
                labels.push(label);
            }
        }
        let mut buttons = String::new();
        for (i, label) in labels.iter().enumerate() {
            buttons.push_str(&format!(
                "<button type=\"button\" role=\"tab\" data-index=\"{i}\" aria-selected=\"{sel}\" tabindex=\"{ti}\">{label}</button>",
                sel = if i == 0 { "true" } else { "false" },
                ti = if i == 0 { "0" } else { "-1" },
                label = escape(label),
            ));
        }
        let shadow = shadow_root(&self.host);
        shadow.set_inner_html(&format!(
            "<style>{BASE_CSS}
:host {{ display: block; margin: 1rem 0; }}
[role=tablist] {{
  display: flex;
  gap: 0.25rem;
  border-bottom: 1px solid var(--op-border);
}}
button {{
  font: inherit;
  background: none;
  border: none;
  border-bottom: 2px solid transparent;
  margin-bottom: -1px;
  color: var(--op-muted);
  padding: 0.3rem 0.9rem;
  cursor: pointer;
}}
button[aria-selected=\"true\"] {{
  color: var(--op-text);
  border-bottom-color: var(--op-accent);
}}
button:hover {{ color: var(--op-text); }}
</style>
<div role=\"tablist\"></div>
<slot></slot>"
        ));
        let Some(tablist) = shadow.query_selector("[role=tablist]").ok().flatten() else {
            return;
        };
        tablist.set_inner_html(&buttons);
        activate(&self.host, 0);

        let count = labels.len() as u32;
        let click = Closure::<dyn FnMut(web_sys::Event)>::new({
            let host = self.host.clone();
            move |event: web_sys::Event| {
                let Some(button) = event
                    .target()
                    .and_then(|t| t.dyn_into::<Element>().ok())
                    .and_then(|t| t.closest("[role=tab]").ok().flatten())
                else {
                    return;
                };
                if let Some(index) = button
                    .get_attribute("data-index")
                    .and_then(|i| i.parse().ok())
                {
                    activate(&host, index);
                }
            }
        });
        let keydown = Closure::<dyn FnMut(web_sys::Event)>::new({
            let host = self.host.clone();
            move |event: web_sys::Event| {
                let Some(key_event) = event.dyn_ref::<web_sys::KeyboardEvent>() else {
                    return;
                };
                let step: i64 = match key_event.key().as_str() {
                    "ArrowRight" => 1,
                    "ArrowLeft" => -1,
                    _ => return,
                };
                event.prevent_default();
                if count == 0 {
                    return;
                }
                let current = i64::from(selected_index(&host));
                let next = (current + step).rem_euclid(i64::from(count)) as u32;
                activate(&host, next);
                focus_button(&host, next);
            }
        });
        let _ = tablist.add_event_listener_with_callback("click", click.as_ref().unchecked_ref());
        let _ =
            tablist.add_event_listener_with_callback("keydown", keydown.as_ref().unchecked_ref());
        self.listeners.push(click);
        self.listeners.push(keydown);
    }
}

impl CustomElement for Tab {
    fn connected(&mut self) {
        let _ = self.host.set_attribute("role", "tabpanel");
        shadow_root(&self.host).set_inner_html(&format!(
            "<style>{BASE_CSS}
:host {{ display: block; padding: 0.75rem 0.25rem; }}
:host([hidden]) {{ display: none; }}
</style>
<slot></slot>"
        ));
    }
}
