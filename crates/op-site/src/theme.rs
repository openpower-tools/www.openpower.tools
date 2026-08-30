//! Colour theme selection: follow the system (`Auto`) or pin `Light`/`Dark`.
//!
//! The choice is stored in `localStorage` under [`STORAGE_KEY`] and applied as
//! `data-theme` on `<html>`, which `styles/theme.css` uses to override the
//! `prefers-color-scheme` default. `index.html` contains a three-line inline
//! script that applies the stored value before first paint; it must agree
//! with [`STORAGE_KEY`] and the stored values here (a test checks that).

use web_sys::Storage;

pub const STORAGE_KEY: &str = "op-theme";
const ATTRIBUTE: &str = "data-theme";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Auto,
    Light,
    Dark,
}

impl Mode {
    /// Interprets a stored value; anything unrecognised means `Auto`.
    pub fn parse(stored: Option<&str>) -> Self {
        match stored {
            Some("light") => Self::Light,
            Some("dark") => Self::Dark,
            _ => Self::Auto,
        }
    }

    /// Value persisted to storage and used for `data-theme`; `None` for `Auto`.
    pub fn stored_value(self) -> Option<&'static str> {
        match self {
            Self::Auto => None,
            Self::Light => Some("light"),
            Self::Dark => Some("dark"),
        }
    }

    /// The mode a click on the toggle moves to: Auto -> Light -> Dark -> Auto.
    pub fn next(self) -> Self {
        match self {
            Self::Auto => Self::Light,
            Self::Light => Self::Dark,
            Self::Dark => Self::Auto,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::Light => "Light",
            Self::Dark => "Dark",
        }
    }

    /// Visible button text.
    pub fn label(self) -> String {
        format!("Theme: {}", self.name())
    }

    /// Accessible description of the button's current state and action.
    pub fn description(self) -> String {
        let now = match self {
            Self::Auto => "Auto, following your system preference",
            Self::Light => "Light",
            Self::Dark => "Dark",
        };
        format!("Colour theme: {now}. Activate to switch to {}.", self.next().name())
    }
}

fn storage() -> Option<Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}

/// The stored mode, `Auto` if nothing is stored or storage is unavailable.
pub fn current() -> Mode {
    let stored = storage().and_then(|s| s.get_item(STORAGE_KEY).ok().flatten());
    Mode::parse(stored.as_deref())
}

/// Applies `mode` to the document and persists it.
pub fn apply(mode: Mode) {
    if let Some(root) = web_sys::window().and_then(|w| w.document()).and_then(|d| d.document_element()) {
        match mode.stored_value() {
            Some(value) => root.set_attribute(ATTRIBUTE, value).expect("set data-theme"),
            None => root.remove_attribute(ATTRIBUTE).expect("remove data-theme"),
        }
    }
    if let Some(storage) = storage() {
        // Storage can fail (quota, private mode); the in-page theme still applies.
        let _ = match mode.stored_value() {
            Some(value) => storage.set_item(STORAGE_KEY, value),
            None => storage.remove_item(STORAGE_KEY),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_values_round_trip_and_unknown_values_mean_auto() {
        for mode in [Mode::Auto, Mode::Light, Mode::Dark] {
            assert_eq!(Mode::parse(mode.stored_value()), mode);
        }
        for junk in [Some(""), Some("LIGHT"), Some("system"), Some("dark "), None] {
            assert_eq!(Mode::parse(junk), Mode::Auto, "{junk:?}");
        }
    }

    #[test]
    fn next_cycles_through_all_modes_exactly_once() {
        let mut seen = vec![Mode::Auto];
        let mut mode = Mode::Auto;
        for _ in 0..3 {
            mode = mode.next();
            seen.push(mode);
        }
        assert_eq!(seen, [Mode::Auto, Mode::Light, Mode::Dark, Mode::Auto]);
    }

    #[test]
    fn labels_and_descriptions_name_the_current_and_next_mode() {
        for mode in [Mode::Auto, Mode::Light, Mode::Dark] {
            assert_eq!(mode.label(), format!("Theme: {}", mode.name()));
            let description = mode.description();
            assert!(description.contains(mode.name()), "{description}");
            assert!(description.ends_with(&format!("switch to {}.", mode.next().name())), "{description}");
        }
    }

    /// The pre-paint script in index.html must read the same key and values.
    #[test]
    fn index_html_prepaint_script_agrees_with_storage_contract() {
        let index = include_str!("../../../index.html");
        assert!(index.contains(&format!("localStorage.getItem(\"{STORAGE_KEY}\")")), "index.html does not read {STORAGE_KEY}");
        for value in [Mode::Light, Mode::Dark].map(|m| m.stored_value().unwrap()) {
            assert!(index.contains(&format!("t===\"{value}\"")), "index.html does not recognise {value:?}");
        }
        assert!(index.contains("document.documentElement.dataset.theme=t"), "index.html does not set data-theme");
    }
}
