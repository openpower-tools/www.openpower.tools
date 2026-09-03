//! A reusable behaviour: tracks whether an element has attention -
//! pointer hover or visible focus - and reports transitions. The
//! reactive-controller idea from Lit, in Rust: a bundle of listeners
//! with its own state that a component attaches to an element and
//! drops with it.
//!
//! Hover and focus are inputs to a component's interaction machine,
//! not styling hooks: the machine exposes attention as a custom state
//! and the stylesheet keys off that, so timed effects never depend on
//! `:hover` directly (a property coming off a hover-driven animation
//! does not transition, which is how a control can look finished while
//! nothing moved).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use wasm_bindgen::prelude::*;
use web_sys::{Element, Event, EventTarget};

/// One named DOM listener kept alive with the controller.
type Listener = (&'static str, Closure<dyn FnMut(Event)>);

pub struct Attention {
    target: EventTarget,
    listeners: Vec<Listener>,
}

impl Attention {
    /// Starts tracking `element`; `on_change` receives `true` when
    /// attention arrives and `false` when both pointer and visible focus
    /// are gone.
    pub fn attach(element: &Element, on_change: impl FnMut(bool) + 'static) -> Self {
        let hovered = Rc::new(Cell::new(false));
        let focused = Rc::new(Cell::new(false));
        let last = Rc::new(Cell::new(false));
        let on_change: Rc<RefCell<dyn FnMut(bool)>> = Rc::new(RefCell::new(on_change));
        let notify = {
            let (hovered, focused, last, on_change) = (
                hovered.clone(),
                focused.clone(),
                last.clone(),
                on_change.clone(),
            );
            Rc::new(move || {
                let now = hovered.get() || focused.get();
                if now != last.get() {
                    last.set(now);
                    (on_change.borrow_mut())(now);
                }
            })
        };
        let target: EventTarget = element.clone().into();
        let element = element.clone();
        let mut listeners: Vec<Listener> = Vec::new();
        let mut listen = |name: &'static str, closure: Closure<dyn FnMut(Event)>| {
            let _ = target.add_event_listener_with_callback(name, closure.as_ref().unchecked_ref());
            listeners.push((name, closure));
        };
        {
            let (hovered, notify) = (hovered.clone(), notify.clone());
            listen(
                "pointerenter",
                Closure::new(move |_| {
                    hovered.set(true);
                    notify();
                }),
            );
        }
        {
            let (hovered, notify) = (hovered.clone(), notify.clone());
            listen(
                "pointerleave",
                Closure::new(move |_| {
                    hovered.set(false);
                    notify();
                }),
            );
        }
        {
            let (focused, notify, element) = (focused.clone(), notify.clone(), element.clone());
            listen(
                "focusin",
                Closure::new(move |_| {
                    // Only keyboard-style focus counts as attention; a
                    // pointer click focuses too but the pointer already
                    // supplies the attention.
                    focused.set(element.matches(":focus-visible").unwrap_or(false));
                    notify();
                }),
            );
        }
        {
            let (focused, notify) = (focused.clone(), notify.clone());
            listen(
                "focusout",
                Closure::new(move |_| {
                    focused.set(false);
                    notify();
                }),
            );
        }
        Self { target, listeners }
    }
}

impl Drop for Attention {
    fn drop(&mut self) {
        for (name, closure) in &self.listeners {
            let _ = self
                .target
                .remove_event_listener_with_callback(name, closure.as_ref().unchecked_ref());
        }
    }
}
