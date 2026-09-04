//! `<opt-badge variant="neutral|ok|warning|danger|info">`: a compact status
//! marker. The label is light-DOM text.

use op_webc::{CustomElement, ElementDefinition};
use web_sys::HtmlElement;

use super::{BASE_CSS, shadow_root};

pub const DEFINITION: ElementDefinition = ElementDefinition {
    source: op_webc::here!(),
    tag: "opt-badge",
    observed_attributes: &["variant"],
    properties: &[],
    create: |host| Box::new(Badge { host }),
};

struct Badge {
    host: HtmlElement,
}

const VARIANTS: &[&str] = &["neutral", "ok", "warning", "danger", "info"];

impl Badge {
    fn render(&self) {
        let variant = self
            .host
            .get_attribute("variant")
            .filter(|v| VARIANTS.contains(&v.as_str()))
            .unwrap_or_else(|| "neutral".to_owned());
        // The dot and label are the data ink; the faint outline only bounds
        // the badge's extent in running prose.
        let dot = match variant.as_str() {
            "ok" => "var(--op-status-ok)",
            "warning" => "var(--op-status-warning)",
            "danger" => "var(--op-status-danger)",
            "info" => "var(--op-status-info)",
            _ => "var(--op-status-neutral)",
        };
        shadow_root(&self.host).set_inner_html(&format!(
            "<style>{BASE_CSS}
:host {{
  display: inline-flex;
  align-items: center;
  gap: 0.35em;
  border: 1px solid var(--op-border);
  border-radius: 1em;
  padding: 0.05em 0.7em 0.05em 0.5em;
  font-size: 0.8em;
  white-space: nowrap;
}}
.dot {{
  width: 0.55em;
  height: 0.55em;
  border-radius: 50%;
  background: {dot};
}}
</style>
<span class=\"dot\" aria-hidden=\"true\"></span><slot></slot>"
        ));
    }
}

impl CustomElement for Badge {
    fn connected(&mut self) {
        self.render();
    }

    fn attribute_changed(&mut self, _n: &str, _o: Option<String>, _v: Option<String>) {
        self.render();
    }
}
