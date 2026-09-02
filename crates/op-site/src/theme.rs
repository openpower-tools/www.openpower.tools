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

/// Storage keys are namespaced (owner scheme, 2026-09-02, no migration
/// from the old flat key): reverse-domain site prefix, then versioned
/// storage and configuration scopes, then the setting path. Leaves under
/// the theme scope: `current` (the explicit choice, stored here) and
/// room for `default` and friends later.
pub const STORAGE_BASE: &str =
    "tools.openpower.sites.www.storage.version.1.configuration.version.1.ux.theme";
pub const STORAGE_KEY: &str =
    "tools.openpower.sites.www.storage.version.1.configuration.version.1.ux.theme.current";
const ATTRIBUTE: &str = "data-theme";

/// Attribute on `<html>` that arms the slow palette blend declared in
/// `styles/theme.css`: while present, the registered colour tokens
/// transition over [`EASE_MS`] on an exponential curve, so a
/// `data-theme` flip creeps in instead of snapping and a second click
/// can abort it. A contract test keeps the stylesheet in step.
pub const EASING_ATTRIBUTE: &str = "data-op-theme-easing";

/// How long an armed palette blend runs. The stylesheet's
/// transition-duration must match (tested); the toggle settles its
/// state slightly after this.
pub const EASE_MS: i32 = 3_000;

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

    /// Accessible description while a blend toward `self` is in
    /// flight: names the destination and the way back, since a second
    /// activation aborts.
    pub fn easing_description(self) -> String {
        format!(
            "Colour theme: switching to {}. Activate to return to {}.",
            self.name(),
            self.opposite().name()
        )
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

fn document_root() -> Option<web_sys::Element> {
    web_sys::window()?.document()?.document_element()
}

/// Arms the slow palette blend for subsequent theme flips.
pub fn begin_easing() {
    if let Some(root) = document_root() {
        root.set_attribute(EASING_ATTRIBUTE, "")
            .expect("set easing attribute");
    }
}

/// Disarms the blend so later theme changes (system preference shifts)
/// apply instantly again.
pub fn end_easing() {
    if let Some(root) = document_root() {
        let _ = root.remove_attribute(EASING_ATTRIBUTE);
    }
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

    #[test]
    fn storage_key_is_the_current_leaf_of_the_base() {
        assert_eq!(STORAGE_KEY, format!("{STORAGE_BASE}.current"));
    }

    /// The pre-paint script in both pages must read the same key and values.
    #[test]
    fn index_html_prepaint_script_agrees_with_storage_contract() {
        check_prepaint_script(include_str!("../../../index.html"));
    }

    #[test]
    fn easing_descriptions_name_the_target_and_the_way_back() {
        for target in [Mode::Dark, Mode::Light] {
            let description = target.easing_description();
            assert!(
                description.contains(&format!("switching to {}", target.name())),
                "{description}"
            );
            assert!(
                description.ends_with(&format!("return to {}.", target.opposite().name())),
                "{description}"
            );
        }
    }

    /// The stylesheet's easing machinery must cover exactly the colour
    /// tokens: every token declared with a hex value is registered as a
    /// typed <color> (with the dark palette as initial value, since dark
    /// is the :root default) and listed in the gated transition, and the
    /// gate matches [`EASING_ATTRIBUTE`] / [`EASE_MS`].
    #[test]
    fn easing_css_registers_and_transitions_every_colour_token() {
        let css = include_str!("../../../styles/theme.css");

        let mut declared: Vec<&str> = Vec::new();
        for line in css.lines() {
            if let Some((name, value)) = line.trim().split_once(':')
                && name.starts_with("--op-")
                && value.trim().starts_with('#')
            {
                declared.push(name.trim());
            }
        }
        declared.sort_unstable();
        declared.dedup();
        assert!(declared.len() > 10, "token scan looks broken: {declared:?}");

        let mut registered: Vec<(&str, &str)> = Vec::new();
        for segment in css.split("@property ").skip(1) {
            let name = segment.split('{').next().expect("name").trim();
            let body = segment
                .split('{')
                .nth(1)
                .expect("body")
                .split('}')
                .next()
                .expect("body end");
            assert!(
                body.contains("syntax: \"<color>\""),
                "{name} is not typed as <color>"
            );
            assert!(body.contains("inherits: true"), "{name} must inherit");
            let initial = body
                .split("initial-value:")
                .nth(1)
                .unwrap_or_else(|| panic!("{name} lacks initial-value"))
                .split(';')
                .next()
                .expect("initial end")
                .trim();
            registered.push((name, initial));
        }
        registered.sort_unstable();
        let names: Vec<&str> = registered.iter().map(|(n, _)| *n).collect();
        assert_eq!(names, declared, "registrations != hex-declared tokens");

        let dark_block = css
            .split(":root,")
            .nth(1)
            .expect("dark :root block")
            .split('}')
            .next()
            .expect("dark block end");
        for (name, initial) in &registered {
            let dark = dark_block
                .split(&format!("{name}:"))
                .nth(1)
                .unwrap_or_else(|| panic!("{name} missing from the dark palette"))
                .split(';')
                .next()
                .expect("value end")
                .trim();
            assert_eq!(
                initial, &dark,
                "{name} initial-value drifted from the dark palette"
            );
        }

        let rule = css
            .split(&format!(":root[{EASING_ATTRIBUTE}]"))
            .nth(1)
            .expect("easing rule")
            .split('}')
            .next()
            .expect("rule end");
        let mut transitioned: Vec<&str> = rule
            .split("transition-property:")
            .nth(1)
            .expect("transition-property")
            .split(';')
            .next()
            .expect("list end")
            .split(',')
            .map(str::trim)
            .collect();
        transitioned.sort_unstable();
        assert_eq!(transitioned, declared, "transition list != colour tokens");
        assert!(
            rule.contains(&format!("transition-duration: {}s", EASE_MS / 1000)),
            "stylesheet duration disagrees with EASE_MS"
        );
        assert!(
            rule.contains("cubic-bezier(") && rule.contains("linear("),
            "expected the exponential curve and its bezier fallback"
        );
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
