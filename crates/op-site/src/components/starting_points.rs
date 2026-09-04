//! `<opt-starting-points heading="...">`: a titled list. The list itself is
//! light-DOM content written in `index.html` and projected through a slot, so
//! links can be edited without touching Rust.

use op_webc::{CustomElement, ElementDefinition};
use web_sys::HtmlElement;

use super::{BASE_CSS, shadow_root};
use crate::html::escape;

pub const DEFINITION: ElementDefinition = ElementDefinition {
    source: op_webc::here!(),
    tag: "opt-starting-points",
    observed_attributes: &["heading"],
    properties: &[],
    create: |host| Box::new(StartingPoints { host }),
};

struct StartingPoints {
    host: HtmlElement,
}

impl StartingPoints {
    fn render(&self) {
        let heading = self
            .host
            .get_attribute("heading")
            .unwrap_or_else(|| "Starting points".to_owned());
        shadow_root(&self.host).set_inner_html(&format!(
            "<style>{BASE_CSS}
::slotted(ul) {{ margin: 0; padding-left: 1.25rem; }}
</style>
<h2>{}</h2><slot></slot>",
            escape(&heading)
        ));
    }
}

impl CustomElement for StartingPoints {
    fn connected(&mut self) {
        self.render();
    }

    fn attribute_changed(&mut self, _name: &str, _old: Option<String>, _new: Option<String>) {
        self.render();
    }
}
