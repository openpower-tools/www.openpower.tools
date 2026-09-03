//! Motion tokens: the site's clocks.
//!
//! The values live in `styles/theme.css` as custom properties on
//! `:root` and inherit through every shadow root, which is how a
//! component's transition and the palette blend share one duration and
//! one curve with nothing copied. This module holds the names and the
//! expected values so a test keeps the stylesheet honest, plus the one
//! number the Rust side needs itself: the blend length, for a safety
//! bound on observing the blend's completion.

/// Every motion token with the value `styles/theme.css` must declare.
pub const TOKENS: &[(&str, &str)] = &[
    ("--op-motion-snap", "160ms"),
    ("--op-motion-fade", "250ms"),
    ("--op-motion-preview", "1.6s"),
    ("--op-motion-blend", "3s"),
    (
        "--op-motion-blend-curve",
        "cubic-bezier(0.9, 0.05, 0.85, 0.3)",
    ),
];

/// The blend's exponential curve, installed over the bezier where the
/// engine supports `linear()`.
pub const BLEND_CURVE: &str = "linear(0, 0.008 10%, 0.021 20%, 0.039 30%, 0.068 40%, 0.111 50%, 0.177 60%, 0.276 70%, 0.426 80%, 0.654 90%, 0.809 95%, 1)";

/// The blend length in milliseconds; equals the `--op-motion-blend`
/// token (tested).
pub const BLEND_MS: i32 = 3_000;

#[cfg(test)]
mod tests {
    use super::*;

    const CSS: &str = include_str!("../../../styles/theme.css");

    fn root_declares(name: &str, value: &str) -> bool {
        CSS.contains(&format!("  {name}: {value};"))
    }

    #[test]
    fn stylesheet_declares_every_motion_token() {
        for (name, value) in TOKENS {
            assert!(
                root_declares(name, value),
                "theme.css does not declare {name}: {value}"
            );
        }
    }

    #[test]
    fn the_exponential_curve_is_installed_behind_a_supports_gate() {
        let gate = CSS
            .find("@supports (animation-timing-function: linear(0, 1))")
            .expect("supports gate");
        let block = &CSS[gate..gate + 400];
        assert!(
            block.contains(&format!("--op-motion-blend-curve: {BLEND_CURVE};")),
            "linear() curve missing or drifted from BLEND_CURVE"
        );
    }

    #[test]
    fn the_palette_blend_reads_the_tokens() {
        let rule = CSS
            .split(":root[data-op-theme-easing]")
            .nth(1)
            .expect("blend rule")
            .split('}')
            .next()
            .expect("rule end");
        assert!(rule.contains("transition-duration: var(--op-motion-blend);"));
        assert!(rule.contains("transition-timing-function: var(--op-motion-blend-curve);"));
    }

    #[test]
    fn blend_ms_equals_the_blend_token() {
        let (_, value) = TOKENS
            .iter()
            .find(|(n, _)| *n == "--op-motion-blend")
            .expect("blend token");
        let seconds: f64 = value.trim_end_matches('s').parse().expect("seconds");
        assert_eq!((seconds * 1000.0) as i32, BLEND_MS);
    }

    #[test]
    fn reduced_motion_collapses_snaps_only() {
        let reduced = CSS
            .split("@media (prefers-reduced-motion: reduce)")
            .skip(1)
            .find(|b| b.contains("--op-motion-snap"))
            .expect("reduced-motion token override");
        let block = reduced.split('}').next().expect("block");
        assert!(block.contains("--op-motion-snap: 0s;"));
        assert!(
            !block.contains("--op-motion-blend"),
            "the blend is a colour fade, not motion; it must stay"
        );
    }
}
