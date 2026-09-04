//! `<opt-site-header heading="..." tagline="...">`: the page title. Both
//! attributes are observed, so changing them re-renders.

use op_webc::{CustomElement, ElementDefinition};
use web_sys::HtmlElement;

use super::{BASE_CSS, shadow_root};
use crate::html::escape;

pub const DEFINITION: ElementDefinition = ElementDefinition {
    source: op_webc::here!(),
    tag: "opt-site-header",
    observed_attributes: &["heading", "tagline"],
    properties: &[],
    create: |host| Box::new(SiteHeader { host }),
};

struct SiteHeader {
    host: HtmlElement,
}

impl SiteHeader {
    fn render(&self) {
        let heading = self.host.get_attribute("heading").unwrap_or_default();
        let tagline = self.host.get_attribute("tagline").unwrap_or_default();
        shadow_root(&self.host).set_inner_html(&format!(
            "<style>{BASE_CSS}
header {{ margin-bottom: 1.5rem; }}
h1 {{ color: var(--op-text); }}
h1::after {{
  content: \"\";
  display: block;
  width: 3rem;
  height: 0.2rem;
  margin-top: 0.4rem;
  background: var(--op-highlight);
}}
p {{ color: var(--op-muted); }}
</style>
<header><h1>{}</h1><p>{}</p></header>",
            escape(&heading),
            escape(&tagline)
        ));
    }
}

impl CustomElement for SiteHeader {
    fn connected(&mut self) {
        self.render();
    }

    fn attribute_changed(&mut self, _name: &str, _old: Option<String>, _new: Option<String>) {
        self.render();
    }
}
