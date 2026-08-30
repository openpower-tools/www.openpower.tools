//! `<op-callout variant="note|tip|warning|danger" heading="...">`: a
//! highlighted block for notes, tips and warnings. Content is light-DOM.
//!
//! Severity is triple-coded - the variant word set in the severity colour
//! where a heading would sit, an icon and a colour - so meaning never rests
//! on colour alone (WCAG 1.4.1). The icon shapes follow the ISO 3864 shape
//! grammar as registered in ISO 7010 (see `components::iso`): an equilateral
//! triangle for hazard warnings (W001), a circle with a diagonal bar for
//! prohibition-grade danger (P001), a circled i for information, and the
//! square of the safe-condition series for tips. The hues stay in the
//! matching ISO colour families (amber, red, blue, green) but use the
//! theme's contrast-checked status tokens instead of raw signal colours.
//!
//! The frame itself is a severity ladder, spending more ink as the stakes
//! rise. Notes and tips: a thin stripe and a dimmed watermark of the icon on
//! the right. Warnings: the triangle posted on the left at full strength,
//! drawn with the warning amber as its border. Danger: the roundel knocked
//! out of solid blocks on both sides with the title and message centred
//! between them, like a hazard plate.

use op_webc::{CustomElement, ElementDefinition};
use web_sys::HtmlElement;

use super::{BASE_CSS, iso, shadow_root};
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

/// A solid side block with the icon knocked out of it (the danger plates).
fn block(side: &str, icon: &str) -> String {
    format!(
        "<span class=\"block {side}\" aria-hidden=\"true\"><svg class=\"glyph\" viewBox=\"0 0 24 24\">{icon}</svg></span>"
    )
}

const BLOCK_CSS: &str = "
.block {
  position: absolute;
  top: 0;
  bottom: 0;
  width: 2.1rem;
  display: flex;
  align-items: center;
  justify-content: center;
}
.block.left { left: 0; }
.block.right { right: 0; }
.glyph {
  width: 1.3rem;
  height: 1.3rem;
  stroke: var(--op-bg);
  fill: none;
  stroke-width: 1.7;
  stroke-linecap: round;
  stroke-linejoin: round;
}";

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
        // The frame, the label and the icon carry the variant colour. The
        // label sits where a heading would, as text (4.5:1 on the page and
        // raised backgrounds, enforced by palette.rs); icons and blocks are
        // decorative (aria-hidden) because the label word is always visible.
        // Knocked-out icons reuse the same pair, read the other way round.
        let stripe = match variant.as_str() {
            "tip" => "var(--op-status-ok)",
            "warning" => "var(--op-status-warning)",
            "danger" => "var(--op-status-danger)",
            _ => "var(--op-status-info)",
        };
        let icon = iso::glyph(&variant);
        let (variant_css, side_markup) = match variant.as_str() {
            // The triangle posted on the left at full strength, drawn with
            // the warning amber as its border, on the plain raised ground.
            "warning" => (
                format!(
                    ".frame {{ padding: 0.4rem 1rem 0.4rem 3.1rem; }}
.glyph {{
  position: absolute;
  left: 0.65rem;
  top: 50%;
  transform: translateY(-50%);
  width: 1.8rem;
  height: 1.8rem;
  stroke: {stripe};
  fill: none;
  stroke-width: 1.7;
  stroke-linecap: round;
  stroke-linejoin: round;
}}"
                ),
                format!(
                    "<svg class=\"glyph\" viewBox=\"0 0 24 24\" aria-hidden=\"true\">{icon}</svg>"
                ),
            ),
            // The roundel on both sides, title and message centred between
            // them: a hazard plate.
            "danger" => (
                format!(
                    ".frame {{ padding: 0.4rem 3.1rem; text-align: center; }}{BLOCK_CSS}
.block {{ background: {stripe}; }}"
                ),
                format!("{}{}", block("left", icon), block("right", icon)),
            ),
            // Advisory: thin stripe, dimmed watermark on the right.
            _ => (
                format!(
                    ".frame {{
  z-index: 0;
  border-left: 0.25rem solid {stripe};
  padding: 0.4rem 3.1rem 0.4rem 1rem;
}}
.glyph {{
  position: absolute;
  z-index: -1;
  top: 50%;
  right: 0.75rem;
  transform: translateY(-50%);
  width: 2rem;
  height: 2rem;
  stroke: {stripe};
  opacity: 0.45;
  fill: none;
  stroke-width: 1.7;
  stroke-linecap: round;
  stroke-linejoin: round;
}}"
                ),
                format!(
                    "<svg class=\"glyph\" viewBox=\"0 0 24 24\" aria-hidden=\"true\">{icon}</svg>"
                ),
            ),
        };
        shadow_root(&self.host).set_inner_html(&format!(
            "<style>{BASE_CSS}
:host {{ display: block; margin: 1rem 0; }}
.frame {{
  position: relative;
  background: var(--op-raised);
}}
{variant_css}
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
<div class=\"frame\">{side_markup}<p class=\"variant\">{variant}</p>{heading_markup}<slot></slot></div>"
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
