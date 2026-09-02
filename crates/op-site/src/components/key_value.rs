//! `<opt-key-value>`: label/value pairs for specs and metadata. The `<dl>`
//! lives in the light DOM; the element lays it out as a two-column grid
//! (grid placement reaches the real children of the slotted list). The
//! dt/dd colours come from `styles/theme.css`, which shadow CSS cannot set.

use op_webc::{CustomElement, ElementDefinition};
use web_sys::HtmlElement;

use super::{BASE_CSS, shadow_root};

pub const DEFINITION: ElementDefinition = ElementDefinition {
    tag: "opt-key-value",
    observed_attributes: &[],
    create: |host| Box::new(KeyValue { host }),
};

struct KeyValue {
    host: HtmlElement,
}

impl CustomElement for KeyValue {
    fn connected(&mut self) {
        shadow_root(&self.host).set_inner_html(&format!(
            "<style>{BASE_CSS}
:host {{ display: block; margin: 1rem 0; }}
::slotted(dl) {{
  display: grid;
  grid-template-columns: max-content 1fr;
  gap: 0.3rem 1.5rem;
  margin: 0;
}}
</style>
<slot></slot>"
        ));
    }
}
