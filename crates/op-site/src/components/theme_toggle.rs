//! `<opt-theme-toggle>`: the theme control as a switch - the site's own
//! on/off slider metaphor with the IEC numerals swapped for theme
//! glyphs. The thumb carries the CURRENT theme's icon (moon for dark,
//! sun for light) knocked out in the page background colour, exactly
//! like opt-switch's numeral thumb; hovering plays the switch's slow
//! ghost preview - a dimmed thumb bearing the OTHER icon slides toward
//! the state a click would set, fading out before it ever looks real.
//! The action is named by a tooltip (title) and aria-label; role=switch
//! with aria-checked (checked = dark) carries the semantics, and
//! keyboard focus mirrors hover affordances with an accent outline.
//!
//! A click flips the thumb at once but the palette blends in slowly
//! (`theme::EASE_MS`, exponential - see the easing block in
//! `styles/theme.css`), so the page creeps toward the other theme
//! rather than snapping. A second click inside that window aborts:
//! the stored choice and the thumb return, and the palette glides
//! back from wherever the blend was. The choice persists via
//! `crate::theme`.

use std::cell::RefCell;
use std::rc::Rc;

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
            ease: Rc::default(),
        })
    },
};

struct ThemeToggle {
    host: HtmlElement,
    /// Kept alive for as long as the element exists.
    on_click: Option<Closure<dyn FnMut(Event)>>,
    /// Updates the control if the system preference changes while no
    /// explicit choice is stored.
    on_scheme_change: Option<Closure<dyn FnMut(Event)>>,
    /// The in-flight palette blend, shared with the click handler.
    ease: Rc<RefCell<Ease>>,
}

#[derive(Default)]
struct Ease {
    /// `Some(origin)` while a click's blend is in flight; the next
    /// click then aborts back to `origin` instead of toggling onward.
    origin: Option<Mode>,
    /// Handle of the pending settle timeout.
    timer: Option<i32>,
    /// Keeps the settle callback alive until it is replaced by the
    /// next blend (never dropped from inside its own invocation).
    settle: Option<Closure<dyn FnMut()>>,
}

const SUN: &str = "<svg viewBox=\"0 0 24 24\" aria-hidden=\"true\"><circle cx=\"12\" cy=\"12\" r=\"5\" fill=\"currentColor\"/><g stroke=\"currentColor\" stroke-width=\"2\" stroke-linecap=\"round\"><line x1=\"12\" y1=\"1.5\" x2=\"12\" y2=\"4.5\"/><line x1=\"12\" y1=\"19.5\" x2=\"12\" y2=\"22.5\"/><line x1=\"1.5\" y1=\"12\" x2=\"4.5\" y2=\"12\"/><line x1=\"19.5\" y1=\"12\" x2=\"22.5\" y2=\"12\"/><line x1=\"4.6\" y1=\"4.6\" x2=\"6.7\" y2=\"6.7\"/><line x1=\"17.3\" y1=\"17.3\" x2=\"19.4\" y2=\"19.4\"/><line x1=\"4.6\" y1=\"19.4\" x2=\"6.7\" y2=\"17.3\"/><line x1=\"17.3\" y1=\"6.7\" x2=\"19.4\" y2=\"4.6\"/></g></svg>";

const MOON: &str = "<svg viewBox=\"0 0 24 24\" aria-hidden=\"true\"><path fill=\"currentColor\" d=\"M20.5 14.5A8.5 8.5 0 0 1 9.5 3.5a8.5 8.5 0 1 0 11 11z\"/></svg>";

fn show(button: &Element, mode: Mode) {
    let _ = button.set_attribute(
        "data-mode",
        match mode {
            Mode::Dark => "dark",
            Mode::Light => "light",
        },
    );
    let _ = button.set_attribute(
        "aria-checked",
        if matches!(mode, Mode::Dark) {
            "true"
        } else {
            "false"
        },
    );
    if let Some(thumb) = button.query_selector(".thumb").ok().flatten() {
        thumb.set_inner_html(match mode {
            Mode::Dark => MOON,
            Mode::Light => SUN,
        });
    }
    if let Some(ghost) = button.query_selector(".ghost").ok().flatten() {
        ghost.set_inner_html(match mode {
            Mode::Dark => SUN,
            Mode::Light => MOON,
        });
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
  position: relative;
  display: inline-block;
  width: 2.6rem;
  height: 1.4rem;
  padding: 0;
  border: 1px solid var(--op-border-strong);
  border-radius: 0.8rem;
  background: var(--op-raised);
  cursor: pointer;
  outline: 2px solid transparent;
  outline-offset: 2px;
  transition: outline-color 0.15s ease;
}}
button:hover, button:focus-visible {{ outline-color: var(--op-accent); }}
.thumb, .ghost {{
  position: absolute;
  top: 50%;
  translate: 0 -50%;
  width: 1.1rem;
  height: 1.1rem;
  border-radius: 50%;
  display: inline-flex;
  align-items: center;
  justify-content: center;
}}
.thumb {{
  left: 0.12rem;
  z-index: 1;
  background: var(--op-text);
  color: var(--op-bg);
  transition: left 0.2s ease;
}}
.thumb svg, .ghost svg {{ width: 0.8rem; height: 0.8rem; }}
button[data-mode=\"dark\"] .thumb {{ left: calc(100% - 1.22rem); }}
.ghost {{
  z-index: 0;
  opacity: 0;
  background: color-mix(in srgb, var(--op-text) 40%, transparent);
  color: var(--op-bg);
}}
@keyframes ghost-to-dark {{
  0% {{ left: 0.12rem; opacity: 0; }}
  35% {{ opacity: 0.6; }}
  100% {{ left: calc(100% - 1.22rem); opacity: 0; }}
}}
@keyframes ghost-to-light {{
  0% {{ left: calc(100% - 1.22rem); opacity: 0; }}
  35% {{ opacity: 0.6; }}
  100% {{ left: 0.12rem; opacity: 0; }}
}}
button[data-mode=\"light\"]:hover .ghost, button[data-mode=\"light\"]:focus-visible .ghost {{
  animation: ghost-to-dark 1.6s ease-in-out infinite;
}}
button[data-mode=\"dark\"]:hover .ghost, button[data-mode=\"dark\"]:focus-visible .ghost {{
  animation: ghost-to-light 1.6s ease-in-out infinite;
}}
@media (prefers-reduced-motion: reduce) {{
  .thumb {{ transition: none; }}
  .ghost {{ animation: none !important; }}
}}
</style>
<button type=\"button\" role=\"switch\"><span class=\"ghost\" aria-hidden=\"true\"></span><span class=\"thumb\" aria-hidden=\"true\"></span></button>"
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

        let ease = self.ease.clone();
        let target = button.clone();
        let closure = Closure::<dyn FnMut(Event)>::new(move |_event| {
            let window = web_sys::window().expect("window");
            let mut state = ease.borrow_mut();
            if let Some(handle) = state.timer.take() {
                window.clear_timeout_with_handle(handle);
            }
            match state.origin.take() {
                // Second click mid-blend: abort. The easing attribute
                // stays armed, so the palette glides back from wherever
                // the blend was; CSS transition reversing shortens the
                // return in proportion to how far it had got.
                Some(origin) => {
                    theme::choose(origin);
                    show(&target, origin);
                }
                // First click: store and show the new theme at once
                // (the thumb slides now) while the palette blends in
                // slowly behind it.
                None => {
                    let origin = theme::current();
                    let next = origin.opposite();
                    theme::begin_easing();
                    theme::choose(next);
                    show(&target, next);
                    let describing = next.easing_description();
                    let _ = target.set_attribute("aria-label", &describing);
                    let _ = target.set_attribute("title", &describing);
                    state.origin = Some(origin);
                }
            }
            // Either way a blend is now in flight; settle once it has
            // run out (abort reversals finish sooner - the attribute
            // lingering a moment longer changes nothing).
            let ease_for_settle = ease.clone();
            let button_for_settle = target.clone();
            let settle = Closure::<dyn FnMut()>::new(move || {
                let mut state = ease_for_settle.borrow_mut();
                state.timer = None;
                state.origin = None;
                theme::end_easing();
                show(&button_for_settle, theme::current());
            });
            state.timer = window
                .set_timeout_with_callback_and_timeout_and_arguments_0(
                    settle.as_ref().unchecked_ref(),
                    theme::EASE_MS + 200,
                )
                .ok();
            state.settle = Some(settle);
        });
        button
            .add_event_listener_with_callback("click", closure.as_ref().unchecked_ref())
            .expect("add click listener");
        self.on_click = Some(closure);
    }
}
