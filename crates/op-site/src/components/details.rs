//! `<opt-details summary="..." open>`: a collapsible section built on the
//! native `<details>` element, so the behaviour is free and accessible.

use op_webc::{CustomElement, ElementDefinition};
use web_sys::HtmlElement;

use super::{BASE_CSS, shadow_root};
use crate::html::escape;

pub const DEFINITION: ElementDefinition = ElementDefinition {
    tag: "opt-details",
    observed_attributes: &["summary", "open"],
    create: |host| Box::new(Details { host }),
};

struct Details {
    host: HtmlElement,
}

impl Details {
    fn render(&self) {
        let summary = self
            .host
            .get_attribute("summary")
            .unwrap_or_else(|| "Details".to_owned());
        let open = if self.host.has_attribute("open") {
            " open"
        } else {
            ""
        };
        shadow_root(&self.host).set_inner_html(&format!(
            "<style>{BASE_CSS}
:host {{ display: block; margin: 0.5rem 0; }}
details {{
  border: 1px solid var(--op-border);
  border-radius: 0.375rem;
  background: var(--op-surface);
}}
details[open] {{ border-color: var(--op-border-strong); }}
summary {{
  cursor: pointer;
  padding: 0.5rem 0.9rem;
  font-weight: 700;
}}
summary:hover {{ color: var(--op-link-hover); }}
.body {{ padding: 0 0.9rem 0.75rem; }}
</style>
<details{open}><summary>{}</summary><div class=\"body\"><slot></slot></div></details>",
            escape(&summary)
        ));
    }
}

impl CustomElement for Details {
    fn connected(&mut self) {
        self.render();
    }

    fn attribute_changed(&mut self, _n: &str, _o: Option<String>, _v: Option<String>) {
        self.render();
    }
}
