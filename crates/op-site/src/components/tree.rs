//! `<opt-tree>`: a collapsible hierarchy - partition layouts, device trees.
//! The content is light-DOM nested lists; branch nodes are native
//! `<details>/<summary>` pairs, so collapsing works without wasm and stays
//! keyboard-accessible for free. Guide lines and markers come from
//! `styles/theme.css`.

use op_webc::{CustomElement, ElementDefinition};
use web_sys::HtmlElement;

use super::{BASE_CSS, shadow_root};

pub const DEFINITION: ElementDefinition = ElementDefinition {
    source: op_webc::here!(),
    tag: "opt-tree",
    observed_attributes: &[],
    properties: &[],
    create: |host| Box::new(Tree { host }),
};

struct Tree {
    host: HtmlElement,
}

impl CustomElement for Tree {
    fn connected(&mut self) {
        shadow_root(&self.host).set_inner_html(&format!(
            "<style>{BASE_CSS}
:host {{ display: block; margin: 1rem 0; font-size: 0.9rem; }}
</style>
<slot></slot>"
        ));
    }
}
