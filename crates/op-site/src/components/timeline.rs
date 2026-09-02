//! `<opt-timeline>`: dated events on a vertical line. The `<ol>` lives in the
//! light DOM; the line and inset are drawn here, while the per-item status
//! dots (which shadow CSS cannot reach) come from `styles/theme.css`, keyed
//! on each item's `data-status` attribute.

use op_webc::{CustomElement, ElementDefinition};
use web_sys::HtmlElement;

use super::{BASE_CSS, shadow_root};

pub const DEFINITION: ElementDefinition = ElementDefinition {
    source: op_webc::here!(),
    tag: "opt-timeline",
    observed_attributes: &[],
    create: |host| Box::new(Timeline { host }),
};

struct Timeline {
    host: HtmlElement,
}

impl CustomElement for Timeline {
    fn connected(&mut self) {
        shadow_root(&self.host).set_inner_html(&format!(
            "<style>{BASE_CSS}
:host {{ display: block; margin: 1rem 0; }}
/* theme.css places each item's dot at -1.25rem, centred on this border. */
::slotted(ol) {{
  list-style: none;
  margin: 0;
  padding: 0 0 0 1.25rem;
  border-left: 2px solid var(--op-border);
}}
</style>
<slot></slot>"
        ));
    }
}
