//! `<opt-switch>`: a native checkbox drawn as a switch. The control is
//! light-DOM markup (a label wrapping its input keeps the association
//! without ids), so it keeps checkbox semantics, submits in forms and
//! still functions as a plain checkbox without wasm. Its representation
//! is the shared switch parts from `op_parts` - the same geometry,
//! contrast pairing, clocks and reduced-motion behaviour as the theme
//! toggle - addressed as pseudo-elements on the native input and
//! installed once as a document stylesheet, since shadow CSS cannot
//! reach nested slotted content. The thumb carries the IEC power
//! numeral for its setting; the preview carries the numeral a click
//! would set.

use op_parts::{At, Look, Selectors};
use op_webc::{CustomElement, ElementDefinition};
use web_sys::HtmlElement;

use super::{BASE_CSS, document_style, shadow_root};

pub const DEFINITION: ElementDefinition = ElementDefinition {
    source: op_webc::here!(),
    tag: "opt-switch",
    observed_attributes: &[],
    properties: &[],
    create: |host| Box::new(Switch { host }),
};

const SELECTORS: Selectors = Selectors {
    track: "opt-switch input",
    on: At::Suffix(":checked"),
    attention: &[At::Suffix(":hover"), At::Suffix(":focus-visible")],
    flight: None,
    thumb: "::before",
    preview: "::after",
    progress: None,
    keyframes: "opt-switch",
};

const LOOK: Look = Look {
    off_fill: "var(--op-border-strong)",
    on_fill: "var(--op-accent)",
    ink: "var(--op-bg)",
};

/// The document stylesheet: the shared parts plus what is specific to
/// this control - the native input's appearance reset, the numerals,
/// and the accent border when on.
pub fn stylesheet() -> String {
    format!(
        "{}\
opt-switch input {{
  appearance: none;
  vertical-align: middle;
}}
opt-switch input:checked {{ border-color: var(--op-accent); }}
opt-switch input::before, opt-switch input::after {{
  font-family: var(--op-font-mono);
  font-size: 0.6em;
  line-height: 1;
}}
opt-switch input::before {{ content: \"0\"; }}
opt-switch input:checked::before {{ content: \"1\"; }}
opt-switch input::after {{ content: \"1\"; }}
opt-switch input:checked::after {{ content: \"0\"; }}
",
        op_parts::css(&SELECTORS, &LOOK)
    )
}

struct Switch {
    host: HtmlElement,
}

impl CustomElement for Switch {
    fn connected(&mut self) {
        document_style("opt-switch-parts", &stylesheet());
        shadow_root(&self.host).set_inner_html(&format!(
            "<style>{BASE_CSS}
:host {{ display: block; margin: 1rem 0; }}
</style>
<slot></slot>"
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_stylesheet_is_the_shared_parts_plus_numerals() {
        let css = stylesheet();
        assert!(css.contains("opt-switch input:checked::before {\n  left:"));
        assert!(css.contains("@keyframes opt-switch-preview-on"));
        assert!(css.contains("content: \"1\""));
        assert!(css.contains("var(--op-motion-snap)"));
        assert!(
            !css.contains("data-"),
            "state comes from :checked, not attributes"
        );
    }
}
