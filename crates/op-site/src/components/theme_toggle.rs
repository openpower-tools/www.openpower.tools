//! `<opt-theme-toggle>`: the theme control as a switch - the site's own
//! on/off slider with the IEC numerals swapped for theme glyphs. Built
//! from the shared switch parts (`op_parts`): the solid thumb carries
//! the CURRENT theme's icon knocked out in page-background colour, the
//! preview ghost plays where a click would go while the control has
//! attention, and the progress ghost travels on the palette blend's
//! clock while a change is in flight.
//!
//! Behaviour is the interaction machine in `op_webc::machine`; this
//! element only translates. Pointer and visible focus arrive through
//! the `Attention` controller, clicks arrive as `Activate`, and the
//! end of the palette blend (observed on `<html>` through
//! `theme::blend_finished`, forward or reversed) arrives as `Finished`.
//! Effects leave as custom states on the host - `dark`, `attention`,
//! `flight` - which the parts CSS keys off via `:host(:state(...))`
//! and a page may key off via `opt-theme-toggle:state(...)`; as
//! `role=switch` / `aria-checked` on the inner button; and as the
//! stored theme choice via `crate::theme`. Nothing timed depends on
//! `:hover` or on a timer, and no attribute carries internal state.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use op_parts::{At, Look, Selectors};
use op_webc::attention::Attention;
use op_webc::machine::{Description, Effect, Input, Machine};
use op_webc::{CustomElement, ElementDefinition, set_state};
use wasm_bindgen::prelude::*;
use web_sys::{Element, Event, HtmlElement};

use super::{BASE_CSS, shadow_root};
use crate::motion;
use crate::theme::{self, Mode};

pub const DEFINITION: ElementDefinition = ElementDefinition {
    source: op_webc::here!(),
    tag: "opt-theme-toggle",
    observed_attributes: &[],
    create: |host| Box::new(ThemeToggle { host, wiring: None }),
};

const SELECTORS: Selectors = Selectors {
    track: "button",
    on: At::HostState("dark"),
    attention: &[At::HostState("attention")],
    flight: Some(At::HostState("flight")),
    thumb: " .thumb",
    preview: " .preview",
    progress: Some(" .ghost"),
    keyframes: "opt-theme-toggle",
};

const LOOK: Look = Look {
    off_fill: "var(--op-text)",
    on_fill: "var(--op-text)",
    ink: "var(--op-bg)",
};

const SUN: &str = "<svg viewBox=\"0 0 24 24\" aria-hidden=\"true\"><circle cx=\"12\" cy=\"12\" r=\"5\" fill=\"currentColor\"/><g stroke=\"currentColor\" stroke-width=\"2\" stroke-linecap=\"round\"><line x1=\"12\" y1=\"1.5\" x2=\"12\" y2=\"4.5\"/><line x1=\"12\" y1=\"19.5\" x2=\"12\" y2=\"22.5\"/><line x1=\"1.5\" y1=\"12\" x2=\"4.5\" y2=\"12\"/><line x1=\"19.5\" y1=\"12\" x2=\"22.5\" y2=\"12\"/><line x1=\"4.6\" y1=\"4.6\" x2=\"6.7\" y2=\"6.7\"/><line x1=\"17.3\" y1=\"17.3\" x2=\"19.4\" y2=\"19.4\"/><line x1=\"4.6\" y1=\"19.4\" x2=\"6.7\" y2=\"17.3\"/><line x1=\"17.3\" y1=\"6.7\" x2=\"19.4\" y2=\"4.6\"/></g></svg>";

const MOON: &str = "<svg viewBox=\"0 0 24 24\" aria-hidden=\"true\"><path fill=\"currentColor\" d=\"M20.5 14.5A8.5 8.5 0 0 1 9.5 3.5a8.5 8.5 0 1 0 11 11z\"/></svg>";

fn mode_of(on: bool) -> Mode {
    if on { Mode::Dark } else { Mode::Light }
}

/// Everything that lives as long as the element: the machine, the DOM
/// handles the effects act on, and the listeners feeding inputs.
struct Wiring {
    /// The listeners below hold their own clones; these keep the
    /// machine alive with the element and available to future hooks.
    _machine: Rc<RefCell<Machine>>,
    /// Bumped per flight so a stale completion cannot settle a newer one.
    _generation: Rc<Cell<u32>>,
    _attention: Attention,
    _on_click: Closure<dyn FnMut(Event)>,
    _on_scheme_change: Option<Closure<dyn FnMut(Event)>>,
}

struct ThemeToggle {
    host: HtmlElement,
    wiring: Option<Wiring>,
}

/// The DOM side of the machine: applies effects to the host, the
/// button and the theme store.
#[derive(Clone)]
struct Surface {
    host: HtmlElement,
    button: Element,
}

impl Surface {
    /// Reflects the setting: custom state, icons, switch semantics.
    fn show(&self, on: bool) {
        set_state(&self.host, "dark", on);
        let mode = mode_of(on);
        let _ = self
            .button
            .set_attribute("aria-checked", if on { "true" } else { "false" });
        let (current, other) = match mode {
            Mode::Dark => (MOON, SUN),
            Mode::Light => (SUN, MOON),
        };
        if let Some(thumb) = self.button.query_selector(".thumb").ok().flatten() {
            thumb.set_inner_html(current);
        }
        // Preview and progress ghosts carry the icon opposite the thumb's:
        // the destination while idle, the outgoing theme in flight.
        for class in [".preview", ".ghost"] {
            if let Some(ghost) = self.button.query_selector(class).ok().flatten() {
                ghost.set_inner_html(other);
            }
        }
    }

    fn describe(&self, on: bool, description: Description) {
        let text = match description {
            Description::Settled => mode_of(on).description(),
            Description::Switching => mode_of(on).easing_description(),
        };
        let _ = self.button.set_attribute("aria-label", &text);
        let _ = self.button.set_attribute("title", &text);
    }

    fn apply(&self, on: bool, effect: Effect) {
        match effect {
            Effect::SetOn(value) => {
                theme::choose(mode_of(value));
                self.show(value);
            }
            Effect::Attention(present) => set_state(&self.host, "attention", present),
            Effect::Arm => {
                theme::begin_easing();
                set_state(&self.host, "flight", true);
            }
            Effect::Disarm => {
                theme::end_easing();
                set_state(&self.host, "flight", false);
            }
            Effect::Describe(description) => self.describe(on, description),
        }
    }
}

/// Feeds one input to the machine and performs the effects; after an
/// activation, watches the blend so its completion arrives as `Finished`.
fn dispatch(
    machine: &Rc<RefCell<Machine>>,
    generation: &Rc<Cell<u32>>,
    surface: &Surface,
    input: Input,
) {
    let effects = {
        let mut m = machine.borrow_mut();
        m.on(input)
    };
    let on = machine.borrow().on;
    for effect in effects {
        surface.apply(on, effect);
    }
    if input == Input::Activate && machine.borrow().in_flight() {
        watch_completion(machine, generation, surface);
    }
}

/// Awaits the palette blend's `finished` promises (which reject if a
/// newer activation replaces the transitions, in which case that
/// activation has started its own watch) and settles the machine. A
/// timer bounds the wait in case nothing was transitioning to observe.
fn watch_completion(machine: &Rc<RefCell<Machine>>, generation: &Rc<Cell<u32>>, surface: &Surface) {
    let this_flight = generation.get().wrapping_add(1);
    generation.set(this_flight);
    let finished = theme::blend_finished();
    let (machine, generation, surface) = (machine.clone(), generation.clone(), surface.clone());
    wasm_bindgen_futures::spawn_local(async move {
        let bound = js_sys::Promise::new(&mut |resolve, _| {
            if let Some(window) = web_sys::window() {
                let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
                    &resolve,
                    motion::BLEND_MS + 500,
                );
            }
        });
        let outcome = wasm_bindgen_futures::JsFuture::from(js_sys::Promise::race(
            &js_sys::Array::of2(&finished, &bound),
        ))
        .await;
        if outcome.is_ok() && generation.get() == this_flight {
            dispatch(&machine, &generation, &surface, Input::Finished);
        }
    });
}

impl CustomElement for ThemeToggle {
    fn connected(&mut self) {
        if self.wiring.is_some() {
            return; // reconnected: shadow tree and listeners still exist
        }
        let shadow = shadow_root(&self.host);
        shadow.set_inner_html(&format!(
            "<style>{BASE_CSS}
:host {{
  position: fixed;
  top: 0.75rem;
  right: 0.75rem;
  z-index: 10;
  font-size: 1rem;
}}
{parts}
button {{
  outline: 2px solid transparent;
  outline-offset: 2px;

}}
:host(:state(attention)) button {{ outline-color: var(--op-accent); }}
button svg {{ width: 0.8rem; height: 0.8rem; }}
</style>
<button type=\"button\" role=\"switch\">{markup}</button>",
            parts = op_parts::css(&SELECTORS, &LOOK),
            markup = op_parts::SHADOW_MARKUP,
        ));
        let button = shadow
            .query_selector("button")
            .expect("query")
            .expect("button in template");
        let surface = Surface {
            host: self.host.clone(),
            button: button.clone(),
        };
        // Start on whatever is in effect: the stored choice, else the
        // system preference. Nothing is written until the user acts.
        let on = theme::current() == Mode::Dark;
        let machine = Rc::new(RefCell::new(Machine::new(on)));
        let generation = Rc::new(Cell::new(0u32));
        surface.show(on);
        surface.describe(on, Description::Settled);

        let attention = {
            let (machine, generation, surface) =
                (machine.clone(), generation.clone(), surface.clone());
            Attention::attach(&button, move |present| {
                let input = if present {
                    Input::Attend
                } else {
                    Input::Neglect
                };
                dispatch(&machine, &generation, &surface, input);
            })
        };

        let on_click = {
            let (machine, generation, surface) =
                (machine.clone(), generation.clone(), surface.clone());
            Closure::<dyn FnMut(Event)>::new(move |_event| {
                dispatch(&machine, &generation, &surface, Input::Activate);
            })
        };
        button
            .add_event_listener_with_callback("click", on_click.as_ref().unchecked_ref())
            .expect("add click listener");

        // A system preference change while no choice is stored is not an
        // activation: the setting simply follows, instantly.
        let on_scheme_change = web_sys::window()
            .and_then(|w| {
                w.match_media("(prefers-color-scheme: light)")
                    .ok()
                    .flatten()
            })
            .map(|mql| {
                let (machine, surface) = (machine.clone(), surface.clone());
                let closure = Closure::<dyn FnMut(Event)>::new(move |_event| {
                    if theme::stored().is_none() {
                        let on = theme::current() == Mode::Dark;
                        machine.borrow_mut().on = on;
                        surface.show(on);
                        surface.describe(on, Description::Settled);
                    }
                });
                let _ = mql
                    .add_event_listener_with_callback("change", closure.as_ref().unchecked_ref());
                closure
            });

        self.wiring = Some(Wiring {
            _machine: machine,
            _generation: generation,
            _attention: attention,
            _on_click: on_click,
            _on_scheme_change: on_scheme_change,
        });
    }
}
