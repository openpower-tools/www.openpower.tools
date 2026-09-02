//! `<opt-callout variant="note|tip|warning|danger" heading="...">`: a
//! highlighted block for notes, tips and warnings. Content is light-DOM.
//!
//! Severity is signalled by the heading, set in small caps in the
//! severity colour, with the icon and the frame reinforcing it. There is no variant label - a
//! callout without a heading shows no label at all - so the author's
//! wording must say what the colour says (WCAG 1.4.1: shape and words,
//! never colour alone). The icon shapes follow the ISO 3864 shape
//! grammar as registered in ISO 7010 (see `components::iso`): an equilateral
//! triangle for hazard warnings (W001), a circle with a diagonal bar for
//! prohibition-grade danger (P001), a circled i for information, and the
//! square of the safe-condition series for tips. The hues stay in the
//! matching ISO colour families (amber, red, blue, green) but use the
//! theme's contrast-checked status tokens instead of raw signal colours.
//!
//! Every variant anchors on a hairline colour stripe at the left edge
//! (max(0.025rem, 1px), so it never vanishes on low-density screens) and
//! watermarks its dimmed icon on the right. Warnings and danger add a
//! barberpole mixed from their own status token toward the theme's pole
//! base - black in the dark theme, white in the light - behind a
//! translucent scrim: a faint texture under warning's scrim, visible
//! bands above and below danger's.

use op_webc::{CustomElement, ElementDefinition};
use web_sys::HtmlElement;

use super::{BASE_CSS, iso, shadow_root};
use crate::html::escape;

pub const DEFINITION: ElementDefinition = ElementDefinition {
    source: op_webc::here!(),
    tag: "opt-callout",
    observed_attributes: &["variant", "heading"],
    create: |host| Box::new(Callout { host }),
};

struct Callout {
    host: HtmlElement,
}

const VARIANTS: &[&str] = &["note", "tip", "warning", "danger"];

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
        // The stripe, the heading and the icon carry the variant colour.
        // The heading is text (4.5:1 on the page and raised backgrounds,
        // enforced by palette.rs); the icon is decorative (aria-hidden), so
        // the author's wording must carry the severity in words. The severe
        // scrim is 90% of the raised background, so the tested raised pairs
        // hold under it within their margins.
        let stripe = match variant.as_str() {
            "tip" => "var(--op-status-ok)",
            "warning" => "var(--op-status-warning)",
            "danger" => "var(--op-status-danger)",
            _ => "var(--op-status-info)",
        };
        let icon = iso::glyph(&variant);
        // Every variant watermarks its icon on the right; the left edge
        // belongs to the stripe alone.
        let icons_markup =
            format!("<svg class=\"glyph\" viewBox=\"0 0 24 24\" aria-hidden=\"true\">{icon}</svg>");
        // Right padding = icon (2rem) + icon inset (0.75rem) + half an icon
        // of clearance (1rem) = 3.75rem: text never crowds the watermark.
        let severe = matches!(variant.as_str(), "warning" | "danger");
        let (variant_css, scrim_open, scrim_close) = if severe {
            // Warning's pole hides under the full-bleed scrim (a faint
            // texture); danger's shows as bands above and below the scrim.
            // Both anchor on the solid stripe at the left edge.
            let frame_padding = if variant.as_str() == "danger" {
                "0.5rem 0"
            } else {
                "0"
            };
            (
                format!(
                    ".frame {{
  border-left: max(0.025rem, 1px) solid {stripe};
  background: repeating-linear-gradient(
    45deg,
    color-mix(in srgb, {stripe} 45%, var(--op-pole-base)) 0 0.75rem,
    color-mix(in srgb, {stripe} 22%, var(--op-pole-base)) 0.75rem 1.5rem
  );
  padding: {frame_padding};
}}
.scrim {{
  position: relative;
  z-index: 0;
  background: color-mix(in srgb, var(--op-raised) 90%, transparent);
  padding: 0.4rem 3.75rem 0.4rem 1rem;
}}"
                ),
                "<div class=\"scrim\">",
                "</div>",
            )
        } else {
            (
                format!(
                    ".frame {{
  position: relative;
  z-index: 0;
  background: var(--op-raised);
  border-left: max(0.025rem, 1px) solid {stripe};
  padding: 0.4rem 3.75rem 0.4rem 1rem;
}}"
                ),
                "",
                "",
            )
        };
        shadow_root(&self.host).set_inner_html(&format!(
            "<style>{BASE_CSS}
:host {{ display: block; margin: 1rem 0; }}
{variant_css}
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
}}
.heading {{
  font-weight: 700;
  margin: 0 0 0.25rem;
  color: {stripe};
  font-variant-caps: small-caps;
  letter-spacing: 0.03em;
}}
</style>
<div class=\"frame\">{scrim_open}{icons_markup}{heading_markup}<slot></slot>{scrim_close}</div>"
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
