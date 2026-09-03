//! Shared visual parts for switch-like controls.
//!
//! A switch-like control is a track with three parts: the solid
//! **thumb** that shows the current setting, a **preview** ghost that
//! plays where a click would go while the control has attention, and
//! (for controls whose change takes time) a **progress** ghost that
//! travels on the site's blend clock while the change is in flight.
//! Every control that uses this idiom - the site theme toggle, the
//! native-checkbox switch - gets its geometry, its contrast pairing,
//! its reduced-motion behaviour and its clocks from here, so they
//! cannot drift apart.
//!
//! The one structural rule this crate exists to enforce: **each part
//! has exactly one driver.** The preview is keyframe-animated, the
//! thumb and the progress ghost are transitioned, and no part is ever
//! both, because a property coming off a CSS animation jumps to its
//! new base value instead of transitioning (which is how a control can
//! look finished while nothing has moved). A test parses the emitted
//! CSS and fails if that rule is broken.
//!
//! States are addressed either as custom states on the host
//! (`:host(:state(flight))`, the standard way for a custom element to
//! expose internal state) or as selector suffixes on the track
//! (`:checked`, `:hover`) for native controls. Clocks come from the
//! motion tokens declared on `:root` (`--op-motion-*`), which inherit
//! through shadow roots; colours come from the palette tokens.
//! Geometry is in percentages of the track, so a part's own font size
//! (a numeral, an icon) can never distort it.

/// Track size, in `em` of the track so a host scales it with `font-size`.
pub const TRACK_WIDTH_EM: f64 = 2.6;
pub const TRACK_HEIGHT_EM: f64 = 1.4;
/// Thumb diameter and inset, in the same em.
pub const THUMB_EM: f64 = 1.1;
pub const INSET_EM: f64 = 0.12;

// Geometry invariants, checked at compile time: the thumb fits the
// track with its inset on both sides, and clears the track height.
const _: () = assert!(THUMB_EM + 2.0 * INSET_EM < TRACK_WIDTH_EM);
const _: () = assert!(THUMB_EM < TRACK_HEIGHT_EM);

fn pct(v: f64) -> String {
    format!("{v:.3}%")
}

/// Thumb height as a percentage of the track height.
pub fn thumb_height() -> String {
    pct(THUMB_EM / TRACK_HEIGHT_EM * 100.0)
}
/// Inset from the track's left edge, as a percentage of its width.
pub fn off_left() -> String {
    pct(INSET_EM / TRACK_WIDTH_EM * 100.0)
}
/// Thumb `left` on the on side, as a percentage of the track width.
pub fn on_left() -> String {
    pct((TRACK_WIDTH_EM - THUMB_EM - INSET_EM) / TRACK_WIDTH_EM * 100.0)
}

/// Fills for the two settings and the ink knocked out of them.
pub struct Look<'a> {
    pub off_fill: &'a str,
    pub on_fill: &'a str,
    pub ink: &'a str,
}

/// How a state is addressed.
#[derive(Clone, Copy)]
pub enum At<'a> {
    /// A suffix on the track selector, e.g. `:checked` or `:hover`.
    Suffix(&'a str),
    /// A custom state on the shadow host, e.g. `flight` for
    /// `:host(:state(flight))`.
    HostState(&'a str),
}

/// How a control's states and parts are addressed. Part suffixes are
/// appended to `track`; a shadow tree uses descendant selectors
/// (`" .thumb"`), a light-DOM control uses pseudo-elements
/// (`"::before"`).
pub struct Selectors<'a> {
    pub track: &'a str,
    pub on: At<'a>,
    /// Any of these means "has attention" and enables the preview.
    pub attention: &'a [At<'a>],
    /// "A change is in flight", if the control has one.
    pub flight: Option<At<'a>>,
    pub thumb: &'a str,
    pub preview: &'a str,
    pub progress: Option<&'a str>,
    /// Prefix for keyframe names, unique per stylesheet scope.
    pub keyframes: &'a str,
}

/// The parts as shadow markup, in paint order, each exposed as a
/// `part` so a page can theme them (`::part(thumb)`); the component
/// fills in their content.
pub const SHADOW_MARKUP: &str = "<span class=\"preview\" part=\"preview\" aria-hidden=\"true\"></span><span class=\"ghost\" part=\"progress\" aria-hidden=\"true\"></span><span class=\"thumb\" part=\"thumb\" aria-hidden=\"true\"></span>";

/// A selector for the track under a set of state conditions.
fn track_under(track: &str, conditions: &[(At, bool)]) -> String {
    let mut host = String::new();
    let mut suffix = String::new();
    for (at, positive) in conditions {
        match at {
            At::Suffix(s) => {
                if *positive {
                    suffix.push_str(s);
                } else {
                    suffix.push_str(&format!(":not({s})"));
                }
            }
            At::HostState(name) => {
                if *positive {
                    host.push_str(&format!(":state({name})"));
                } else {
                    host.push_str(&format!(":not(:state({name}))"));
                }
            }
        }
    }
    if host.is_empty() {
        format!("{track}{suffix}")
    } else {
        format!(":host({host}) {track}{suffix}")
    }
}

fn mix(fill: &str) -> String {
    format!("color-mix(in srgb, {fill} 85%, transparent)")
}

/// The complete CSS for one control's parts.
pub fn css(sel: &Selectors, look: &Look) -> String {
    let t = sel.track;
    let k = sel.keyframes;
    let (thumb, preview) = (sel.thumb, sel.preview);
    let under = |conditions: &[(At, bool)]| track_under(t, conditions);
    let attention_rules = |on: bool, part: &str| -> String {
        sel.attention
            .iter()
            .map(|a| {
                let mut conditions = vec![(sel.on, on), (*a, true)];
                if let Some(f) = sel.flight {
                    conditions.push((f, false));
                }
                format!("{}{part}", under(&conditions))
            })
            .collect::<Vec<_>>()
            .join(",\n")
    };
    let (h, off_l, on_l) = (thumb_height(), off_left(), on_left());
    let mut parts_list = format!("{t}{thumb}, {t}{preview}");
    if let Some(progress) = sel.progress {
        parts_list.push_str(&format!(", {t}{progress}"));
    }
    let on_track = under(&[(sel.on, true)]);
    let mut out = format!(
        "\
{t} {{
  position: relative;
  display: inline-block;
  /* content-box: the padding box IS the track geometry, so the parts'
     percentages resolve against exactly {TRACK_WIDTH_EM}em x {TRACK_HEIGHT_EM}em
     and the thumb stays a circle with symmetric insets */
  box-sizing: content-box;
  width: {TRACK_WIDTH_EM}em;
  height: {TRACK_HEIGHT_EM}em;
  margin: 0;
  padding: 0;
  border: 1px solid var(--op-border-strong);
  border-radius: {radius}em;
  background: var(--op-raised);
  cursor: pointer;
}}
{parts_list} {{
  position: absolute;
  top: 50%;
  translate: 0 -50%;
  left: {off_l};
  height: {h};
  aspect-ratio: 1;
  border-radius: 50%;
  display: flex;
  align-items: center;
  justify-content: center;
  box-sizing: border-box;
  color: {ink};
}}
{t}{thumb} {{
  z-index: 2;
  background: {off_fill};
  transition: left var(--op-motion-snap) ease, background var(--op-motion-snap) ease;
}}
{on_track}{thumb} {{
  left: {on_l};
  background: {on_fill};
}}
{t}{preview} {{
  z-index: 1;
  opacity: 0;
  background: {preview_from_off};
}}
{on_track}{preview} {{
  background: {preview_from_on};
}}
@keyframes {k}-preview-on {{
  0% {{ left: {off_l}; opacity: 0; }}
  22% {{ opacity: 0.9; }}
  70% {{ opacity: 0.9; }}
  100% {{ left: {on_l}; opacity: 0; }}
}}
@keyframes {k}-preview-off {{
  0% {{ left: {on_l}; opacity: 0; }}
  22% {{ opacity: 0.9; }}
  70% {{ opacity: 0.9; }}
  100% {{ left: {off_l}; opacity: 0; }}
}}
{preview_on_attention} {{
  animation: {k}-preview-on var(--op-motion-preview) ease-in-out infinite;
}}
{preview_off_attention} {{
  animation: {k}-preview-off var(--op-motion-preview) ease-in-out infinite;
}}
",
        radius = TRACK_HEIGHT_EM / 2.0,
        ink = look.ink,
        off_fill = look.off_fill,
        on_fill = look.on_fill,
        preview_from_off = mix(look.on_fill),
        preview_from_on = mix(look.off_fill),
        preview_on_attention = attention_rules(false, preview),
        preview_off_attention = attention_rules(true, preview),
    );
    if let (Some(progress), Some(flight)) = (sel.progress, sel.flight) {
        out.push_str(&format!(
            "\
{t}{progress} {{
  z-index: 1;
  opacity: 0;
  background: {left_on};
  transition: opacity var(--op-motion-fade) ease;
}}
{on_track}{progress} {{
  left: {on_l};
  background: {left_off};
}}
{in_flight}{progress} {{
  opacity: 0.9;
  transition-property: left, opacity;
  transition-duration: var(--op-motion-blend), var(--op-motion-fade);
  transition-timing-function: var(--op-motion-blend-curve), ease;
}}
",
            left_on = mix(look.on_fill),
            left_off = mix(look.off_fill),
            in_flight = under(&[(flight, true)]),
        ));
    }
    out.push_str(&format!(
        "\
@media (prefers-reduced-motion: reduce) {{
  {t}{preview} {{ animation: none !important; }}
  {preview_on_attention} {{ opacity: 0.9; left: {on_l}; }}
  {preview_off_attention} {{ opacity: 0.9; left: {off_l}; }}
",
        preview_on_attention = attention_rules(false, preview),
        preview_off_attention = attention_rules(true, preview),
    ));
    if let (Some(progress), Some(flight)) = (sel.progress, sel.flight) {
        out.push_str(&format!(
            "  {}{progress} {{ transition: none; }}\n",
            under(&[(flight, true)])
        ));
    }
    out.push_str("}\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOOK: Look = Look {
        off_fill: "var(--op-text)",
        on_fill: "var(--op-text)",
        ink: "var(--op-bg)",
    };

    fn shadow() -> Selectors<'static> {
        Selectors {
            track: "button",
            on: At::HostState("dark"),
            attention: &[At::HostState("attention")],
            flight: Some(At::HostState("flight")),
            thumb: " .thumb",
            preview: " .preview",
            progress: Some(" .ghost"),
            keyframes: "toggle",
        }
    }

    fn light_dom() -> Selectors<'static> {
        Selectors {
            track: "opt-switch input",
            on: At::Suffix(":checked"),
            attention: &[At::Suffix(":hover"), At::Suffix(":focus-visible")],
            flight: None,
            thumb: "::before",
            preview: "::after",
            progress: None,
            keyframes: "switch",
        }
    }

    /// `(selector, declarations)` for every rule outside @keyframes,
    /// including rules nested in @media blocks.
    fn rules(css: &str) -> Vec<(String, String)> {
        let mut out = Vec::new();
        let mut rest = css;
        while let Some(open) = rest.find('{') {
            let head = rest[..open].trim();
            let head = head.rsplit(['}', ';']).next().unwrap_or(head).trim();
            if head.starts_with("@keyframes") {
                let mut depth = 0;
                let mut end = open;
                for (i, c) in rest[open..].char_indices() {
                    match c {
                        '{' => depth += 1,
                        '}' => {
                            depth -= 1;
                            if depth == 0 {
                                end = open + i;
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                rest = &rest[end + 1..];
                continue;
            }
            if head.starts_with("@media") {
                rest = &rest[open + 1..];
                continue;
            }
            let close = rest[open..].find('}').expect("rule closes") + open;
            out.push((head.to_owned(), rest[open + 1..close].to_owned()));
            rest = &rest[close + 1..];
        }
        out
    }

    #[test]
    fn each_part_has_exactly_one_driver() {
        for sel in [shadow(), light_dom()] {
            let css = css(&sel, &LOOK);
            for (selector, decls) in rules(&css) {
                let animated = decls.contains("animation:");
                let transitioned = decls.contains("transition");
                assert!(
                    !(animated && transitioned),
                    "{selector} is both animated and transitioned"
                );
                if selector.contains(sel.preview) {
                    assert!(!transitioned, "preview must never transition: {selector}");
                }
                if selector.contains(sel.thumb)
                    || sel.progress.is_some_and(|p| selector.contains(p))
                {
                    assert!(!animated, "thumb/progress must never animate: {selector}");
                }
            }
        }
    }

    #[test]
    fn host_states_address_the_shadow_parts() {
        let css = css(&shadow(), &LOOK);
        assert!(css.contains(":host(:state(dark)) button .thumb {"));
        assert!(css.contains(":host(:state(flight)) button .ghost {"));
        assert!(
            css.contains(
                ":host(:not(:state(dark)):state(attention):not(:state(flight))) button .preview"
            ),
            "preview gated on attention and not-flight via host states"
        );
        assert!(
            !css.contains("data-"),
            "no attributes: state is custom state"
        );
    }

    #[test]
    fn the_preview_never_plays_during_a_flight() {
        let css = css(&shadow(), &LOOK);
        for (selector, decls) in rules(&css) {
            if decls.contains("animation: toggle-preview") {
                assert!(
                    selector.contains(":not(:state(flight))"),
                    "preview animation not gated on flight: {selector}"
                );
            }
        }
    }

    #[test]
    fn pseudo_element_selectors_keep_the_pseudo_element_last() {
        let css = css(&light_dom(), &LOOK);
        for (selector, _) in rules(&css) {
            for single in selector.split(',') {
                let single = single.trim();
                if let Some(at) = single.find("::") {
                    assert!(
                        single[at + 2..].find(':').is_none(),
                        "pseudo-class after pseudo-element: {single}"
                    );
                }
            }
        }
        assert!(css.contains("opt-switch input:not(:checked):hover::after"));
        assert!(css.contains("opt-switch input:checked::before"));
    }

    #[test]
    fn clocks_come_from_motion_tokens_only() {
        for sel in [shadow(), light_dom()] {
            let css = css(&sel, &LOOK);
            for (selector, decls) in rules(&css) {
                for line in decls.lines().map(str::trim) {
                    let carries_clock = line.starts_with("transition:")
                        || line.starts_with("transition-duration")
                        || line.starts_with("transition-timing-function")
                        || line.starts_with("animation:")
                        || line.starts_with("animation-duration");
                    if carries_clock {
                        assert!(
                            line.contains("var(--op-motion-") || line.contains("none"),
                            "{selector}: literal clock in `{line}`"
                        );
                    }
                }
            }
        }
        let css = css(&shadow(), &LOOK);
        for token in [
            "--op-motion-snap",
            "--op-motion-preview",
            "--op-motion-blend",
            "--op-motion-fade",
            "--op-motion-blend-curve",
        ] {
            assert!(css.contains(token), "{token} unused");
        }
    }

    #[test]
    fn geometry_is_font_size_independent_and_fits() {
        let css = css(&shadow(), &LOOK);
        // parts are sized in % of the track, never in em
        for (selector, decls) in rules(&css) {
            if selector.contains(".thumb")
                || selector.contains(".preview")
                || selector.contains(".ghost")
            {
                for line in decls.lines().map(str::trim) {
                    if line.starts_with("left:") || line.starts_with("height:") {
                        assert!(
                            line.contains('%'),
                            "{selector}: {line} must be a percentage"
                        );
                    }
                }
            }
        }
        // on + thumb + inset spans exactly the track
        let on: f64 = on_left().trim_end_matches('%').parse().expect("on");
        let off: f64 = off_left().trim_end_matches('%').parse().expect("off");
        let thumb_w = THUMB_EM / TRACK_WIDTH_EM * 100.0;
        assert!((on + thumb_w + off - 100.0).abs() < 0.01);
    }

    #[test]
    fn reduced_motion_keeps_a_static_preview() {
        let css = css(&shadow(), &LOOK);
        let reduced = css
            .split("@media (prefers-reduced-motion: reduce)")
            .nth(1)
            .expect("reduced block");
        assert!(reduced.contains("animation: none !important"));
        assert!(
            reduced.contains("opacity: 0.9"),
            "static preview must still show"
        );
        assert!(reduced.contains(":host(:state(flight)) button .ghost { transition: none; }"));
    }
}
