//! `<opt-switch>`: a native checkbox drawn as a switch. The control is light-DOM
//! markup (a label wrapping its input keeps the association without ids), so
//! it works without wasm and submits in forms; the styling lives in
//! `styles/theme.css` because shadow CSS cannot reach nested slotted
//! content. The element provides the layout box and the selector scope.

use op_webc::{CustomElement, ElementDefinition};
use web_sys::HtmlElement;

use super::{BASE_CSS, shadow_root};

pub const DEFINITION: ElementDefinition = ElementDefinition {
    tag: "opt-switch",
    observed_attributes: &[],
    create: |host| Box::new(Switch { host }),
};

struct Switch {
    host: HtmlElement,
}

impl CustomElement for Switch {
    fn connected(&mut self) {
        shadow_root(&self.host).set_inner_html(&format!(
            "<style>{BASE_CSS}
:host {{ display: block; margin: 1rem 0; }}
</style>
<slot></slot>"
        ));
    }
}
