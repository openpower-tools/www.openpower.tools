//! `<op-card heading="..." href="...">`: a titled content container; with
//! `href` the heading becomes a link. Body and an optional `footer` slot are
//! light-DOM.

use op_webc::{CustomElement, ElementDefinition};
use web_sys::HtmlElement;

use super::{BASE_CSS, shadow_root};
use crate::html::escape;

pub const DEFINITION: ElementDefinition = ElementDefinition {
    tag: "op-card",
    observed_attributes: &["heading", "href"],
    create: |host| Box::new(Card { host }),
};

struct Card {
    host: HtmlElement,
}

impl Card {
    fn render(&self) {
        let heading = self.host.get_attribute("heading").unwrap_or_default();
        let heading_markup = match self.host.get_attribute("href") {
            Some(href) if !href.is_empty() => format!(
                "<h3><a href=\"{}\">{}</a></h3>",
                escape(&href),
                escape(&heading)
            ),
            _ => format!("<h3>{}</h3>", escape(&heading)),
        };
        shadow_root(&self.host).set_inner_html(&format!(
            "<style>{BASE_CSS}
:host {{
  display: block;
  background: var(--op-surface);
  border: 1px solid var(--op-border-strong);
  border-radius: 0.5rem;
  padding: 0.9rem 1.1rem;
}}
:host(:hover) {{ border-color: var(--op-accent); }}
h3 {{ margin: 0 0 0.4rem; font-size: 1rem; font-family: var(--op-font-heading); }}
h3 a {{ text-decoration: none; }}
h3 a:hover {{ text-decoration: underline; }}
.footer {{
  margin-top: 0.6rem;
  padding-top: 0.5rem;
  border-top: 1px solid var(--op-border);
  font-size: 0.85rem;
  color: var(--op-muted);
}}
</style>
{heading_markup}<slot></slot><div class=\"footer\" part=\"footer\"><slot name=\"footer\"></slot></div>"
        ));
    }
}

impl CustomElement for Card {
    fn connected(&mut self) {
        self.render();
    }

    fn attribute_changed(&mut self, _n: &str, _o: Option<String>, _v: Option<String>) {
        self.render();
    }
}
