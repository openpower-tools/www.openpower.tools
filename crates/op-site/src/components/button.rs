//! `<op-button variant="default|primary|danger" disabled>`: an action
//! button. The label is light-DOM text; the button itself is shadow DOM, so
//! it does not participate in forms - it is for actions wired in script,
//! like the copy control on op-source. Clicks bubble out composed.
//!
//! Primary weight comes from a heavier accent border, not a fill: the accent
//! hues only reach 3:1 against the light background, so they may border text
//! but never carry it.

use op_webc::{CustomElement, ElementDefinition};
use web_sys::HtmlElement;

use super::{BASE_CSS, shadow_root};

pub const DEFINITION: ElementDefinition = ElementDefinition {
    tag: "op-button",
    observed_attributes: &["variant", "disabled"],
    create: |host| Box::new(Button { host }),
};

struct Button {
    host: HtmlElement,
}

const VARIANTS: &[&str] = &["default", "primary", "danger"];

impl Button {
    fn render(&self) {
        let variant = self
            .host
            .get_attribute("variant")
            .filter(|v| VARIANTS.contains(&v.as_str()))
            .unwrap_or_else(|| "default".to_owned());
        let disabled = if self.host.has_attribute("disabled") {
            " disabled"
        } else {
            ""
        };
        shadow_root(&self.host).set_inner_html(&format!(
            "<style>{BASE_CSS}
:host {{ display: inline-block; }}
button {{
  font: inherit;
  font-family: var(--op-font-sans);
  color: var(--op-text);
  background: none;
  border: 1px solid var(--op-border-strong);
  border-radius: 0.25rem;
  padding: 0.35rem 1rem;
  cursor: pointer;
}}
button.primary {{
  border: 2px solid var(--op-accent);
  padding: calc(0.35rem - 1px) calc(1rem - 1px);
  font-weight: 600;
}}
button.danger {{
  color: var(--op-status-danger);
  border-color: var(--op-status-danger);
}}
button:hover:not(:disabled) {{
  background: var(--op-code-bg);
  border-color: var(--op-accent);
}}
button.danger:hover:not(:disabled) {{
  background: var(--op-status-danger);
  border-color: var(--op-status-danger);
  color: var(--op-bg);
}}
button:disabled {{
  opacity: 0.45;
  cursor: not-allowed;
}}
</style>
<button type=\"button\" class=\"{variant}\"{disabled}><slot></slot></button>"
        ));
    }
}

impl CustomElement for Button {
    fn connected(&mut self) {
        self.render();
    }

    fn attribute_changed(&mut self, _n: &str, _o: Option<String>, _v: Option<String>) {
        self.render();
    }
}
