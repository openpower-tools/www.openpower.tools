//! `<opt-empty-state heading="...">`: a placeholder for content that does not
//! exist yet. The explanation (and usually a link to change that) is
//! light-DOM content.

use op_webc::{CustomElement, ElementDefinition};
use web_sys::HtmlElement;

use super::{BASE_CSS, shadow_root};
use crate::html::escape;

pub const DEFINITION: ElementDefinition = ElementDefinition {
    source: op_webc::here!(),
    tag: "opt-empty-state",
    observed_attributes: &["heading"],
    properties: &[],
    create: |host| Box::new(EmptyState { host }),
};

struct EmptyState {
    host: HtmlElement,
}

impl EmptyState {
    fn render(&self) {
        let heading = self
            .host
            .get_attribute("heading")
            .map(|h| format!("<p class=\"heading\">{}</p>", escape(&h)))
            .unwrap_or_default();
        shadow_root(&self.host).set_inner_html(&format!(
            "<style>{BASE_CSS}
:host {{ display: block; margin: 1rem 0; }}
.frame {{
  border: 1px dashed var(--op-border-strong);
  border-radius: 0.375rem;
  padding: 1.75rem 1rem;
  text-align: center;
  color: var(--op-muted);
}}
.heading {{
  font-family: var(--op-font-heading);
  letter-spacing: var(--op-heading-fallback-tracking, 0em);
  font-size: 1.1rem;
  color: var(--op-text);
  margin: 0 0 0.25rem;
}}
</style>
<div class=\"frame\">{heading}<slot></slot></div>"
        ));
    }
}

impl CustomElement for EmptyState {
    fn connected(&mut self) {
        self.render();
    }

    fn attribute_changed(&mut self, _n: &str, _o: Option<String>, _v: Option<String>) {
        self.render();
    }
}
