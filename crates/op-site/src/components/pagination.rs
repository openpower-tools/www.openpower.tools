//! `<opt-pagination>`: page navigation for split lists. The pages are a
//! light-DOM `<ol>` of links (with `aria-current="page"` on the current one
//! and plain spans for gaps), styled from `styles/theme.css`.

use op_webc::{CustomElement, ElementDefinition};
use web_sys::HtmlElement;

use super::{BASE_CSS, shadow_root};

pub const DEFINITION: ElementDefinition = ElementDefinition {
    source: op_webc::here!(),
    tag: "opt-pagination",
    observed_attributes: &[],
    properties: &[],
    create: |host| Box::new(Pagination { host }),
};

struct Pagination {
    host: HtmlElement,
}

impl CustomElement for Pagination {
    fn connected(&mut self) {
        shadow_root(&self.host).set_inner_html(&format!(
            "<style>{BASE_CSS}
:host {{ display: block; margin: 1rem 0; }}
</style>
<nav aria-label=\"Pages\"><slot></slot></nav>"
        ));
    }
}
