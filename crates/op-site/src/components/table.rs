//! `<opt-table>`: a scroll container and skin for a light-DOM `<table>`. The
//! cell and caption styles live in `styles/theme.css` (shadow CSS cannot
//! reach inside the slotted table). Row rules are opt-in via a `lined`
//! attribute and a `dense` attribute tightens the cell padding, both applied
//! from that stylesheet too.

use op_webc::{CustomElement, ElementDefinition};
use web_sys::HtmlElement;

use super::{BASE_CSS, shadow_root};

pub const DEFINITION: ElementDefinition = ElementDefinition {
    source: op_webc::here!(),
    tag: "opt-table",
    observed_attributes: &[],
    create: |host| Box::new(Table { host }),
};

struct Table {
    host: HtmlElement,
}

impl CustomElement for Table {
    fn connected(&mut self) {
        shadow_root(&self.host).set_inner_html(&format!(
            "<style>{BASE_CSS}
:host {{ display: block; margin: 1rem 0; }}
.scroll {{ overflow-x: auto; }}
::slotted(table) {{
  border-collapse: collapse;
  width: 100%;
  font-size: 0.9rem;
}}
</style>
<div class=\"scroll\"><slot></slot></div>"
        ));
    }
}
