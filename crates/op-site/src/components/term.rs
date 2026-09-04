//! `<opt-term scheme="..." value="...">`: a classification, named as a
//! term of one of the site's controlled vocabularies (`op_terms`). The
//! element carries the MEANING; its look - a badge in the severity the
//! term is contained in - is a projection derived from the vocabulary,
//! never chosen in markup. Light-DOM text, if any, is the label;
//! otherwise the term's value is shown. The term's severity is also
//! exposed as a custom state (`:state(ok)` and friends) so a container
//! such as opt-kpi can project it too.

use op_webc::{CustomElement, ElementDefinition, set_state};
use web_sys::HtmlElement;

use super::{BASE_CSS, shadow_root};
use crate::html::escape;

pub const DEFINITION: ElementDefinition = ElementDefinition {
    source: op_webc::here!(),
    tag: "opt-term",
    observed_attributes: &["scheme", "value"],
    properties: &[],
    create: |host| Box::new(TermElement { host }),
};

struct TermElement {
    host: HtmlElement,
}

impl TermElement {
    fn render(&self) {
        let scheme = self.host.get_attribute("scheme").unwrap_or_default();
        let value = self.host.get_attribute("value").unwrap_or_default();
        let term = op_terms::lookup(&scheme, &value);
        let severity = term.map_or(op_terms::Severity::Neutral, |t| t.broader);
        for s in ["neutral", "info", "ok", "warning", "danger"] {
            set_state(&self.host, s, s == severity.name());
        }
        set_state(&self.host, "unknown-term", term.is_none());
        let title = term
            .map(|t| t.description)
            .unwrap_or("not a term of this vocabulary");
        if self.host.get_attribute("title").is_none() {
            let _ = self.host.set_attribute("title", title);
        }
        let colour = format!("var(--op-status-{})", severity.name());
        shadow_root(&self.host).set_inner_html(&format!(
            "<style>{BASE_CSS}
:host {{ display: inline-flex; align-items: center; gap: 0.35em; padding: 0.05em 0.6em; border-radius: 1em; font-size: 0.8em; white-space: nowrap;
  background: color-mix(in srgb, {colour} 16%, transparent); color: var(--op-text); }}
:host(:state(unknown-term)) {{ outline: 1px dashed var(--op-status-danger); }}
.dot {{ width: 0.55em; height: 0.55em; border-radius: 50%; background: {colour}; }}
</style><span class=\"dot\" aria-hidden=\"true\"></span><slot>{}</slot>",
            escape(&value)
        ));
    }
}

impl CustomElement for TermElement {
    fn connected(&mut self) {
        self.render();
    }

    fn attribute_changed(&mut self, _n: &str, _o: Option<String>, _v: Option<String>) {
        self.render();
    }
}
