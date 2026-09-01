//! `<op-steps>`: a numbered procedure. The steps are a light-DOM `<ol>`;
//! each `<li>` may carry `data-state="done|current|error"` (unmarked steps
//! read as upcoming). Numbering, bubbles and the connecting line are drawn
//! from `styles/theme.css` with CSS counters, since shadow CSS cannot reach
//! the slotted list's items; states colour the bubble with the status
//! tokens and the text carries the meaning.

use op_webc::{CustomElement, ElementDefinition};
use web_sys::HtmlElement;

use super::{BASE_CSS, shadow_root};

pub const DEFINITION: ElementDefinition = ElementDefinition {
    tag: "op-steps",
    observed_attributes: &[],
    create: |host| Box::new(Steps { host }),
};

struct Steps {
    host: HtmlElement,
}

impl CustomElement for Steps {
    fn connected(&mut self) {
        shadow_root(&self.host).set_inner_html(&format!(
            "<style>{BASE_CSS}
:host {{ display: block; margin: 1rem 0; }}
::slotted(ol) {{
  list-style: none;
  margin: 0;
  padding: 0;
}}
</style>
<slot></slot>"
        ));
    }
}
