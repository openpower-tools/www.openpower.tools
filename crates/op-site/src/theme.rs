//! Colour theme selection. The site is dark by default; `Light` pins the
//! light palette and `Auto` follows the system preference.
//!
//! The choice is stored in `localStorage` under [`STORAGE_KEY`] and applied as
//! `data-theme` on `<html>` (`"light"` or `"auto"`; absent means dark), which
//! `styles/theme.css` keys its token sets on. `index.html` contains a
//! three-line inline script that applies the stored value before first paint;
//! it must agree with [`STORAGE_KEY`] and the stored values here (a test
//! checks that).

use web_sys::Storage;

pub const STORAGE_KEY: &str = "op-theme";
const ATTRIBUTE: &str = "data-theme";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Dark,
    Light,
    Auto,
}

impl Mode {
    /// Interprets a stored value; anything unrecognised means the default, `Dark`.
    pub fn parse(stored: Option<&str>) -> Self {
        match stored {
            Some("light") => Self::Light,
            Some("auto") => Self::Auto,
            _ => Self::Dark,
        }
    }

    /// Value persisted to storage and used for `data-theme`; `None` for the
    /// default, `Dark`.
    pub fn stored_value(self) -> Option<&'static str> {
        match self {
            Self::Dark => None,
            Self::Light => Some("light"),
            Self::Auto => Some("auto"),
        }
    }

    /// The mode a click on the toggle moves to: Dark -> Light -> Auto -> Dark.
    pub fn next(self) -> Self {
        match self {
            Self::Dark => Self::Light,
            Self::Light => Self::Auto,
            Self::Auto => Self::Dark,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Dark => "Dark",
            Self::Light => "Light",
            Self::Auto => "Auto",
        }
    }

    /// Visible button text.
    pub fn label(self) -> String {
        format!("Theme: {}", self.name())
    }

    /// Accessible description of the button's current state and action.
    pub fn description(self) -> String {
        let now = match self {
            Self::Dark => "Dark",
            Self::Light => "Light",
            Self::Auto => "Auto, following your system preference",
        };
        format!(
            "Colour theme: {now}. Activate to switch to {}.",
            self.next().name()
        )
    }
}

fn storage() -> Option<Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}

/// The stored mode, `Dark` if nothing is stored or storage is unavailable.
pub fn current() -> Mode {
    let stored = storage().and_then(|s| s.get_item(STORAGE_KEY).ok().flatten());
    Mode::parse(stored.as_deref())
}

/// Applies `mode` to the document and persists it.
pub fn apply(mode: Mode) {
    if let Some(root) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.document_element())
    {
        match mode.stored_value() {
            Some(value) => root
                .set_attribute(ATTRIBUTE, value)
                .expect("set data-theme"),
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
    fn stored_values_round_trip_and_unknown_values_mean_dark() {
        for mode in [Mode::Dark, Mode::Light, Mode::Auto] {
            assert_eq!(Mode::parse(mode.stored_value()), mode);
        }
        for junk in [
            Some(""),
            Some("LIGHT"),
            Some("system"),
            Some("auto "),
            Some("dark"),
            None,
        ] {
            assert_eq!(Mode::parse(junk), Mode::Dark, "{junk:?}");
        }
    }

    #[test]
    fn next_cycles_through_all_modes_exactly_once() {
        let mut seen = vec![Mode::Dark];
        let mut mode = Mode::Dark;
        for _ in 0..3 {
            mode = mode.next();
            seen.push(mode);
        }
        assert_eq!(seen, [Mode::Dark, Mode::Light, Mode::Auto, Mode::Dark]);
    }

    #[test]
    fn labels_and_descriptions_name_the_current_and_next_mode() {
        for mode in [Mode::Dark, Mode::Light, Mode::Auto] {
            assert_eq!(mode.label(), format!("Theme: {}", mode.name()));
            let description = mode.description();
            assert!(description.contains(mode.name()), "{description}");
            assert!(
                description.ends_with(&format!("switch to {}.", mode.next().name())),
                "{description}"
            );
        }
    }

    /// The pre-paint script in index.html must read the same key and values.
    #[test]
    fn index_html_prepaint_script_agrees_with_storage_contract() {
        for index in [
            include_str!("../../../index.html"),
            include_str!("../../../specimen/index.html"),
        ] {
            check_prepaint_script(index);
        }
    }

    fn check_prepaint_script(index: &str) {
        assert!(
            index.contains(&format!("localStorage.getItem(\"{STORAGE_KEY}\")")),
            "index.html does not read {STORAGE_KEY}"
        );
        for value in [Mode::Light, Mode::Auto].map(|m| m.stored_value().unwrap()) {
            assert!(
                index.contains(&format!("t===\"{value}\"")),
                "index.html does not recognise {value:?}"
            );
        }
        assert!(
            index.contains("document.documentElement.dataset.theme=t"),
            "index.html does not set data-theme"
        );
    }
}
