//! `<opt-breadcrumb>`: where-am-I navigation. The trail is a light-DOM `<ol>`
//! of links, the last carrying `aria-current="page"`; separators come from
//! `styles/theme.css` so they stay out of the accessible name.

use op_webc::{CustomElement, ElementDefinition};
use web_sys::HtmlElement;

use super::{BASE_CSS, shadow_root};

pub const DEFINITION: ElementDefinition = ElementDefinition {
    tag: "opt-breadcrumb",
    observed_attributes: &[],
    create: |host| Box::new(Breadcrumb { host }),
};

struct Breadcrumb {
    host: HtmlElement,
}

impl CustomElement for Breadcrumb {
    fn connected(&mut self) {
        shadow_root(&self.host).set_inner_html(&format!(
            "<style>{BASE_CSS}
:host {{ display: block; margin: 1rem 0; }}
</style>
<nav aria-label=\"Breadcrumb\"><slot></slot></nav>"
        ));
    }
}
