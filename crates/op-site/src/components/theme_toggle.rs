//! `<opt-theme-toggle>`: a fixed-position button cycling Auto, Light and Dark.
//! The choice is remembered across visits (see `crate::theme`).

use op_webc::{CustomElement, ElementDefinition};
use wasm_bindgen::prelude::*;
use web_sys::{Element, Event, HtmlElement};

use super::{BASE_CSS, shadow_root};
use crate::theme::{self, Mode};

pub const DEFINITION: ElementDefinition = ElementDefinition {
    tag: "opt-theme-toggle",
    observed_attributes: &[],
    create: |host| {
        Box::new(ThemeToggle {
            host,
            on_click: None,
            on_scheme_change: None,
        })
    },
};

struct ThemeToggle {
    host: HtmlElement,
    /// Kept alive for as long as the element exists.
    on_click: Option<Closure<dyn FnMut(Event)>>,
    /// Updates the label if the system preference changes while no explicit
    /// choice is stored.
    on_scheme_change: Option<Closure<dyn FnMut(Event)>>,
}

fn show(button: &Element, mode: Mode) {
    button.set_text_content(Some(&mode.label()));
    button
        .set_attribute("aria-label", &mode.description())
        .expect("set aria-label");
    button
        .set_attribute("title", &mode.description())
        .expect("set title");
}

impl CustomElement for ThemeToggle {
    fn connected(&mut self) {
        if self.on_click.is_some() {
            return; // reconnected: the shadow tree and listener still exist
        }
        let shadow = shadow_root(&self.host);
        shadow.set_inner_html(&format!(
            "<style>{BASE_CSS}
:host {{
  position: fixed;
  top: 0.75rem;
  right: 0.75rem;
  z-index: 10;
}}
button {{
  font: inherit;
  font-size: 0.875rem;
  color: var(--op-text);
  background: var(--op-surface);
  border: 1px solid var(--op-border-strong);
  border-radius: 0.375rem;
  padding: 0.35rem 0.7rem;
  cursor: pointer;
}}
button:hover {{ border-color: var(--op-accent); }}
</style>
<button type=\"button\"></button>"
        ));
        let button = shadow
            .query_selector("button")
            .expect("query")
            .expect("button in template");
        // Start on whatever is in effect: the stored choice, else the system
        // preference. Nothing is written until the user toggles.
        show(&button, theme::current());

        if let Some(mql) = web_sys::window().and_then(|w| {
            w.match_media("(prefers-color-scheme: light)")
                .ok()
                .flatten()
        }) {
            let target = button.clone();
            let closure = Closure::<dyn FnMut(Event)>::new(move |_event| {
                if theme::stored().is_none() {
                    show(&target, theme::current());
                }
            });
            let _ =
                mql.add_event_listener_with_callback("change", closure.as_ref().unchecked_ref());
            self.on_scheme_change = Some(closure);
        }

        let target = button.clone();
        let closure = Closure::<dyn FnMut(Event)>::new(move |_event| {
            let next = theme::current().opposite();
            let target = target.clone();
            // Cross-fade the palette change like the font swap.
            crate::viewtransition::run(move || {
                theme::choose(next);
                show(&target, next);
            });
        });
        button
            .add_event_listener_with_callback("click", closure.as_ref().unchecked_ref())
            .expect("add click listener");
        self.on_click = Some(closure);
    }
}
