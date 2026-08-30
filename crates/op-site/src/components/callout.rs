//! `<op-callout variant="note|tip|warning|danger" heading="...">`: a
//! highlighted block for notes, tips and warnings. Content is light-DOM.

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
        // The stripe colour carries the variant; text stays --op-text for AA.
        let stripe = match variant.as_str() {
            "tip" => "var(--op-status-ok)",
            "warning" => "var(--op-status-warning)",
            "danger" => "var(--op-status-danger)",
            _ => "var(--op-status-info)",
        };
        shadow_root(&self.host).set_inner_html(&format!(
            "<style>{BASE_CSS}
:host {{ display: block; margin: 1rem 0; }}
.frame {{
  border-left: 0.25rem solid {stripe};
  background: var(--op-surface);
  border-radius: 0 0.375rem 0.375rem 0;
  padding: 0.6rem 1rem;
}}
.heading {{ font-weight: 700; margin: 0 0 0.25rem; }}
.variant {{
  font-size: 0.7rem;
  text-transform: uppercase;
  letter-spacing: 0.08em;
  color: var(--op-muted);
}}
</style>
<div class=\"frame\"><span class=\"variant\">{variant}</span>{heading_markup}<slot></slot></div>"
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
