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
//! Every variant anchors on a colour stripe at the left edge. Notes and
//! tips keep the quiet frame with the dimmed watermark of the icon beside
//! the stripe; warnings and danger post their icon on each side, over a dark
//! barberpole mixed from their own status token (amber, red) behind a
//! translucent scrim: a faint texture under warning's scrim, visible
//! bands above and below danger's.

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
        // The stripe, the label and the icon carry the variant colour. The
        // label sits where a heading would, as text (4.5:1 on the page and
        // raised backgrounds, enforced by palette.rs); the icon is decorative
        // (aria-hidden) because the label word is always visible. The severe
        // scrim is 90% of the raised background, so the tested raised pairs
        // hold under it within their margins.
        let stripe = match variant.as_str() {
            "tip" => "var(--op-status-ok)",
            "warning" => "var(--op-status-warning)",
            "danger" => "var(--op-status-danger)",
            _ => "var(--op-status-info)",
        };
        let icon = iso::glyph(&variant);
        let glyph = |side: &str| {
            format!(
                "<svg class=\"glyph {side}\" viewBox=\"0 0 24 24\" aria-hidden=\"true\">{icon}</svg>"
            )
        };
        // Advisory icons watermark the left beside the stripe; warning and
        // danger post one on each side.
        let icons_markup = match variant.as_str() {
            "warning" | "danger" => format!("{}{}", glyph("left"), glyph("right")),
            _ => glyph("left"),
        };
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
  border-left: 0.25rem solid {stripe};
  background: repeating-linear-gradient(
    45deg,
    color-mix(in srgb, {stripe} 45%, #000) 0 0.75rem,
    color-mix(in srgb, {stripe} 22%, #000) 0.75rem 1.5rem
  );
  padding: {frame_padding};
}}
.scrim {{
  position: relative;
  z-index: 0;
  background: color-mix(in srgb, var(--op-raised) 90%, transparent);
  padding: 0.4rem 3.1rem 0.4rem 3.1rem;
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
  border-left: 0.25rem solid {stripe};
  padding: 0.4rem 1rem 0.4rem 3.1rem;
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
.glyph.left {{ left: 0.75rem; }}
.glyph.right {{ right: 0.75rem; }}
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
<div class=\"frame\">{scrim_open}{icons_markup}<p class=\"variant\">{variant}</p>{heading_markup}<slot></slot>{scrim_close}</div>"
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
