//! Colour theme: dark or light.
//!
//! With no stored choice the site follows the system preference through CSS
//! alone (`prefers-color-scheme`); the toggle starts on whichever side that
//! resolves to, flips between the two, and remembers an explicit choice in
//! `localStorage` under [`STORAGE_KEY`], mirrored as `data-theme` on `<html>`.
//! `index.html` contains a three-line inline script that applies the stored
//! value before first paint; it must agree with [`STORAGE_KEY`] and the
//! stored values here (a test checks that).

use web_sys::Storage;

pub const STORAGE_KEY: &str = "op-theme";
const ATTRIBUTE: &str = "data-theme";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Dark,
    Light,
}

impl Mode {
    /// Interprets a stored value; anything unrecognised means no choice.
    pub fn parse(stored: Option<&str>) -> Option<Self> {
        match stored {
            Some("dark") => Some(Self::Dark),
            Some("light") => Some(Self::Light),
            _ => None,
        }
    }

    /// Value persisted to storage and used for `data-theme`.
    pub fn stored_value(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
        }
    }

    pub fn opposite(self) -> Self {
        match self {
            Self::Dark => Self::Light,
            Self::Light => Self::Dark,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Dark => "Dark",
            Self::Light => "Light",
        }
    }

    /// Visible button text: the theme currently in effect.
    pub fn label(self) -> String {
        format!("Theme: {}", self.name())
    }

    /// Accessible description of the button's current state and action.
    pub fn description(self) -> String {
        format!(
            "Colour theme: {}. Activate to switch to {}.",
            self.name(),
            self.opposite().name()
        )
    }
}

/// An explicit stored choice beats the system preference.
pub fn resolve(stored: Option<Mode>, system_prefers_light: bool) -> Mode {
    stored.unwrap_or(if system_prefers_light {
        Mode::Light
    } else {
        Mode::Dark
    })
}

fn storage() -> Option<Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}

/// The stored explicit choice, if any.
pub fn stored() -> Option<Mode> {
    let value = storage().and_then(|s| s.get_item(STORAGE_KEY).ok().flatten());
    Mode::parse(value.as_deref())
}

/// Whether the system currently prefers a light scheme.
pub fn system_prefers_light() -> bool {
    web_sys::window()
        .and_then(|w| {
            w.match_media("(prefers-color-scheme: light)")
                .ok()
                .flatten()
        })
        .is_some_and(|mql| mql.matches())
}

/// The theme currently in effect.
pub fn current() -> Mode {
    resolve(stored(), system_prefers_light())
}

/// Records an explicit user choice and reflects it on `<html>`.
pub fn choose(mode: Mode) {
    if let Some(root) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.document_element())
    {
        root.set_attribute(ATTRIBUTE, mode.stored_value())
            .expect("set data-theme");
    }
    if let Some(storage) = storage() {
        // Storage can fail (quota, private mode); the in-page theme still applies.
        let _ = storage.set_item(STORAGE_KEY, mode.stored_value());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_values_round_trip_and_unknown_values_mean_no_choice() {
        for mode in [Mode::Dark, Mode::Light] {
            assert_eq!(Mode::parse(Some(mode.stored_value())), Some(mode));
        }
        for junk in [
            Some(""),
            Some("LIGHT"),
            Some("auto"),
            Some("system"),
            Some("dark "),
            None,
        ] {
            assert_eq!(Mode::parse(junk), None, "{junk:?}");
        }
    }

    #[test]
    fn resolution_prefers_the_stored_choice_then_the_system() {
        assert_eq!(resolve(Some(Mode::Dark), true), Mode::Dark);
        assert_eq!(resolve(Some(Mode::Light), false), Mode::Light);
        assert_eq!(resolve(None, true), Mode::Light);
        assert_eq!(resolve(None, false), Mode::Dark);
    }

    #[test]
    fn opposite_is_an_involution_over_both_modes() {
        for mode in [Mode::Dark, Mode::Light] {
            assert_ne!(mode.opposite(), mode);
            assert_eq!(mode.opposite().opposite(), mode);
        }
    }

    #[test]
    fn labels_and_descriptions_name_the_current_and_next_mode() {
        for mode in [Mode::Dark, Mode::Light] {
            assert_eq!(mode.label(), format!("Theme: {}", mode.name()));
            let description = mode.description();
            assert!(description.contains(mode.name()), "{description}");
            assert!(
                description.ends_with(&format!("switch to {}.", mode.opposite().name())),
                "{description}"
            );
        }
    }

    /// The pre-paint script in both pages must read the same key and values.
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
            "page does not read {STORAGE_KEY}"
        );
        for mode in [Mode::Dark, Mode::Light] {
            assert!(
                index.contains(&format!("t===\"{}\"", mode.stored_value())),
                "page does not recognise {mode:?}"
            );
        }
        assert!(
            index.contains("document.documentElement.dataset.theme=t"),
            "page does not set data-theme"
        );
    }
}
