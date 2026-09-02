//! `<opt-tooltip text="...">`: a small hover/focus annotation on the slotted
//! trigger. Shown on hover and focus-within; the text is also mirrored into
//! the host's `title` attribute as the fallback for touch and assistive
//! tech, since shadow ids cannot be referenced by light-DOM aria.

use op_webc::{CustomElement, ElementDefinition};
use web_sys::HtmlElement;

use super::{BASE_CSS, shadow_root};
use crate::html::escape;

pub const DEFINITION: ElementDefinition = ElementDefinition {
    tag: "opt-tooltip",
    observed_attributes: &["text"],
    create: |host| Box::new(Tooltip { host }),
};

struct Tooltip {
    host: HtmlElement,
}

impl Tooltip {
    fn render(&self) {
        let text = self.host.get_attribute("text").unwrap_or_default();
        let _ = self.host.set_attribute("title", &text);
        shadow_root(&self.host).set_inner_html(&format!(
            "<style>{BASE_CSS}
:host {{ display: inline-block; position: relative; }}
.tip {{
  position: absolute;
  bottom: calc(100% + 0.4rem);
  left: 50%;
  transform: translateX(-50%);
  background: var(--op-raised);
  color: var(--op-text);
  border: 1px solid var(--op-border-strong);
  border-radius: 0.25rem;
  padding: 0.15rem 0.6rem;
  font-size: 0.8rem;
  white-space: nowrap;
  visibility: hidden;
  opacity: 0;
  transition: opacity 120ms ease;
  pointer-events: none;
}}
:host(:hover) .tip,
:host(:focus-within) .tip {{
  visibility: visible;
  opacity: 1;
}}
</style>
<slot></slot><span class=\"tip\" role=\"tooltip\">{text}</span>",
            text = escape(&text),
        ));
    }
}

impl CustomElement for Tooltip {
    fn connected(&mut self) {
        self.render();
    }

    fn attribute_changed(&mut self, _n: &str, _o: Option<String>, _v: Option<String>) {
        self.render();
    }
}
