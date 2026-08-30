//! `<op-callout variant="note|tip|warning|danger" heading="...">`: a
//! highlighted block for notes, tips and warnings. Content is light-DOM.
//!
//! Severity is triple-coded - the variant word set in the severity colour
//! where a heading would sit, an icon and a colour - so meaning never rests
//! on colour alone (WCAG 1.4.1). The icon shapes follow the ISO 3864 shape
//! grammar as registered in ISO 7010: an equilateral triangle for hazard
//! warnings (W001), a circle with a diagonal bar for prohibition-grade
//! danger (P001), a circled i for information, and the square of the
//! safe-condition series for tips. The hues stay in the matching ISO colour
//! families (amber, red, blue, green) but use the theme's contrast-checked
//! status tokens instead of raw signal colours.
//!
//! Severe variants (warning, danger) read differently from advisory ones
//! at a glance: their icon block sits on the left, while notes and tips
//! carry theirs on the right beside a thin stripe. In both, the icon is
//! knocked out of a solid block of the severity colour in the page
//! background colour, echoing an ISO sign; the side and the stripe keep the
//! two groups apart even before colour is read.

use op_webc::{CustomElement, ElementDefinition};
use web_sys::HtmlElement;

use super::{BASE_CSS, shadow_root};
use crate::html::escape;

pub const DEFINITION: ElementDefinition = ElementDefinition {
    tag: "op-callout",
    observed_attributes: &["variant", "heading"],
    create: |host| Box::new(Callout { host }),
};

struct Callout {
    host: HtmlElement,
}

const VARIANTS: &[&str] = &["note", "tip", "warning", "danger"];

/// The icon for a variant, as stroke elements in a 24x24 viewBox.
fn glyph(variant: &str) -> &'static str {
    match variant {
        // Safe condition (ISO 3864 square) carrying a check.
        "tip" => {
            r#"<rect x="3.5" y="3.5" width="17" height="17" rx="1"/><path d="M7.5 12.4l3.1 3.1 5.9-5.9"/>"#
        }
        // General warning, after ISO 7010 W001: triangle and exclamation.
        "warning" => {
            r#"<path d="M12 3.4 21.5 19.9H2.5Z"/><path d="M12 9.8v4.4"/><path d="M12 17.3v.01" stroke-width="2.6"/>"#
        }
        // General prohibition, after ISO 7010 P001: circle and diagonal bar.
        "danger" => r#"<circle cx="12" cy="12" r="9.2"/><path d="M5.5 5.5l13 13"/>"#,
        // Information: a circled i.
        _ => {
            r#"<circle cx="12" cy="12" r="9.2"/><path d="M12 11v5.4"/><path d="M12 7.6v.01" stroke-width="2.6"/>"#
        }
    }
}

impl Callout {
    fn render(&self) {
        let variant = self
            .host
            .get_attribute("variant")
            .filter(|v| VARIANTS.contains(&v.as_str()))
            .unwrap_or_else(|| "note".to_owned());
        let heading = self.host.get_attribute("heading");
        let heading_markup = heading
            .as_deref()
            .map(|h| format!("<p class=\"heading\">{}</p>", escape(h)))
            .unwrap_or_default();
        // The stripe or block, the label and the icon carry the variant
        // colour. The label sits where a heading would, as text (4.5:1 on the
        // page background, enforced by palette.rs); icons are decorative
        // (aria-hidden) because the label word is always visible. The knocked
        // out icon inherits the same 4.5:1 pair, read the other way round.
        let stripe = match variant.as_str() {
            "tip" => "var(--op-status-ok)",
            "warning" => "var(--op-status-warning)",
            "danger" => "var(--op-status-danger)",
            _ => "var(--op-status-info)",
        };
        let icon = glyph(&variant);
        let severe = matches!(variant.as_str(), "warning" | "danger");
        // Severe icons sit left; advisory icons sit right beside the thin
        // stripe. Both are knocked out of a solid block of the colour.
        let (gutter_side, frame_padding) = if severe {
            ("left", "0.4rem 1rem 0.4rem 3.1rem")
        } else {
            ("right", "0.4rem 3.1rem 0.4rem 1rem")
        };
        let stripe_css = if severe {
            String::new()
        } else {
            format!(
                "border-left: 0.25rem solid {stripe};
  "
            )
        };
        shadow_root(&self.host).set_inner_html(&format!(
            "<style>{BASE_CSS}
:host {{ display: block; margin: 1rem 0; }}
.frame {{
  position: relative;
  background: var(--op-raised);
  {stripe_css}padding: {frame_padding};
}}
.gutter {{
  position: absolute;
  {gutter_side}: 0;
  top: 0;
  bottom: 0;
  width: 2.1rem;
  background: {stripe};
  display: flex;
  align-items: center;
  justify-content: center;
}}
.glyph {{
  width: 1.3rem;
  height: 1.3rem;
  stroke: var(--op-bg);
  fill: none;
  stroke-width: 1.7;
  stroke-linecap: round;
  stroke-linejoin: round;
}}
.heading {{ font-weight: 700; margin: 0.15rem 0 0.25rem; }}
.variant {{
  margin: 0 0 0.1rem;
  font-size: 0.75rem;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.08em;
  color: {stripe};
}}
</style>
<div class=\"frame\"><span class=\"gutter\" aria-hidden=\"true\"><svg class=\"glyph\" viewBox=\"0 0 24 24\">{icon}</svg></span><p class=\"variant\">{variant}</p>{heading_markup}<slot></slot></div>"
        ));
    }
}

impl CustomElement for Callout {
    fn connected(&mut self) {
        self.render();
    }

    fn attribute_changed(&mut self, _n: &str, _o: Option<String>, _v: Option<String>) {
        self.render();
    }
}
