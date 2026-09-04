//! `<opt-site-nav>`: the site navigation. Links live in the light DOM (an
//! unordered list of anchors) so they exist without wasm; the element styles
//! them and marks the link matching the current URL with `aria-current`.

use op_webc::{CustomElement, ElementDefinition};
use wasm_bindgen::JsCast;
use web_sys::HtmlElement;

use super::{BASE_CSS, shadow_root};

pub const DEFINITION: ElementDefinition = ElementDefinition {
    source: op_webc::here!(),
    tag: "opt-site-nav",
    observed_attributes: &[],
    properties: &[],
    create: |host| Box::new(SiteNav { host }),
};

struct SiteNav {
    host: HtmlElement,
}

impl CustomElement for SiteNav {
    fn connected(&mut self) {
        shadow_root(&self.host).set_inner_html(&format!(
            "<style>{BASE_CSS}
:host {{
  display: block;
  margin: 0 0 1.5rem;
  font-size: 0.9rem;
}}
::slotted(ul) {{
  list-style: none;
  display: flex;
  gap: 1.25rem;
  margin: 0;
  padding: 0 0 0.5rem;
  border-bottom: 1px solid var(--op-border);
}}
</style>
<nav aria-label=\"Site\"><slot></slot></nav>"
        ));
        let path = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.location())
            .and_then(|l| l.pathname().ok())
            .unwrap_or_else(|| "/".to_owned());
        if let Ok(links) = self.host.query_selector_all("a") {
            for index in 0..links.length() {
                let Some(link) = links.item(index) else {
                    continue;
                };
                let Ok(link) = link.dyn_into::<web_sys::Element>() else {
                    continue;
                };
                let href = link.get_attribute("href").unwrap_or_default();
                let matches = if href == "/" {
                    path == "/" || path == "/index.html"
                } else {
                    path == href || path == href.trim_end_matches('/')
                };
                if matches {
                    let _ = link.set_attribute("aria-current", "page");
                } else if href != "/" && path.starts_with(href.as_str()) {
                    // An ancestor section of the current page.
                    let _ = link.set_attribute("aria-current", "true");
                }
            }
        }
    }
}
