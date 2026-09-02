//! `<opt-theme-toggle>`: a fixed-position button flipping Light and Dark.
//! The choice is remembered across visits (see `crate::theme`).
//!
//! Resting state is unemphasised: a quiet raised background, no border,
//! and a semantic glyph for the current theme (moon for dark, sun for
//! light). On hover or keyboard focus the control raises (slight lift and
//! shadow), takes an accent outline, and the glyph morphs into the text
//! label - the same reveal pattern as the switch. The accessible name is
//! always present via aria-label, so the icon-only resting state loses
//! nothing.
//! A `data-preview` attribute on the host forces the revealed state, so
//! specimen pages can show it without interaction.

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

const SUN: &str = "<svg viewBox=\"0 0 24 24\" aria-hidden=\"true\"><circle cx=\"12\" cy=\"12\" r=\"5\" fill=\"currentColor\"/><g stroke=\"currentColor\" stroke-width=\"2\" stroke-linecap=\"round\"><line x1=\"12\" y1=\"1.5\" x2=\"12\" y2=\"4.5\"/><line x1=\"12\" y1=\"19.5\" x2=\"12\" y2=\"22.5\"/><line x1=\"1.5\" y1=\"12\" x2=\"4.5\" y2=\"12\"/><line x1=\"19.5\" y1=\"12\" x2=\"22.5\" y2=\"12\"/><line x1=\"4.6\" y1=\"4.6\" x2=\"6.7\" y2=\"6.7\"/><line x1=\"17.3\" y1=\"17.3\" x2=\"19.4\" y2=\"19.4\"/><line x1=\"4.6\" y1=\"19.4\" x2=\"6.7\" y2=\"17.3\"/><line x1=\"17.3\" y1=\"6.7\" x2=\"19.4\" y2=\"4.6\"/></g></svg>";

const MOON: &str = "<svg viewBox=\"0 0 24 24\" aria-hidden=\"true\"><path fill=\"currentColor\" d=\"M20.5 14.5A8.5 8.5 0 0 1 9.5 3.5a8.5 8.5 0 1 0 11 11z\"/></svg>";

fn show(button: &Element, mode: Mode) {
    if let Some(icon) = button.query_selector(".icon").ok().flatten() {
        icon.set_inner_html(match mode {
            Mode::Dark => MOON,
            Mode::Light => SUN,
        });
    }
    if let Some(label) = button.query_selector(".label").ok().flatten() {
        label.set_text_content(Some(&mode.label()));
    }
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
  display: inline-flex;
  align-items: center;
  font: inherit;
  font-size: 0.875rem;
  line-height: 1.1rem;
  color: var(--op-text);
  background: var(--op-raised);
  border: 0;
  border-radius: 0.375rem;
  padding: 0.4rem 0.55rem;
  cursor: pointer;
  opacity: 0.85;
  outline: 2px solid transparent;
  outline-offset: 2px;
  transition: opacity 0.15s ease, translate 0.15s ease, box-shadow 0.15s ease,
    outline-color 0.15s ease;
}}
.icon {{
  display: inline-flex;
  width: 1.1rem;
  height: 1.1rem;
  max-width: 1.1rem;
  opacity: 1;
  overflow: hidden;
  transition: max-width 0.2s ease, opacity 0.15s ease;
}}
.icon svg {{ width: 1.1rem; height: 1.1rem; }}
.label {{
  max-width: 0;
  opacity: 0;
  overflow: hidden;
  white-space: nowrap;
  transition: max-width 0.2s ease, opacity 0.15s ease;
}}
button:hover, button:focus-visible, :host([data-preview]) button {{
  opacity: 1;
  translate: 0 -1px;
  box-shadow: 0 2px 10px color-mix(in srgb, var(--op-text) 18%, transparent);
  outline-color: var(--op-accent);
}}
button:hover .icon, button:focus-visible .icon,
:host([data-preview]) button .icon {{
  max-width: 0;
  opacity: 0;
}}
button:hover .label, button:focus-visible .label,
:host([data-preview]) button .label {{
  max-width: 10rem;
  opacity: 1;
}}
@media (prefers-reduced-motion: reduce) {{
  button, .icon, .label {{ transition: none; }}
}}
</style>
<button type=\"button\"><span class=\"icon\" aria-hidden=\"true\"></span><span class=\"label\"></span></button>"
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
