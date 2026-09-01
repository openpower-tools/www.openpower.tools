//! `<op-kpi label="..." value="..." unit="..." state="neutral|info|ok|warning|danger">`:
//! a single measurement, data-forward: big value, small label, optional unit,
//! the value coloured by an optional state (contrast-tested status tokens).
//! Several op-kpi elements sit side by side in a row and wrap.

use op_webc::{CustomElement, ElementDefinition};
use web_sys::HtmlElement;

use super::{BASE_CSS, shadow_root};
use crate::html::escape;

pub const DEFINITION: ElementDefinition = ElementDefinition {
    tag: "op-kpi",
    observed_attributes: &["label", "value", "unit", "state"],
    create: |host| Box::new(Kpi { host }),
};

struct Kpi {
    host: HtmlElement,
}

const STATES: &[&str] = &["neutral", "info", "ok", "warning", "danger"];

impl Kpi {
    fn render(&self) {
        let attr = |name: &str| self.host.get_attribute(name).unwrap_or_default();
        let label = attr("label");
        let value = attr("value");
        let unit = attr("unit");
        let state = self
            .host
            .get_attribute("state")
            .filter(|s| STATES.contains(&s.as_str()));
        // No state: the value stays on the text token. With one, the value
        // takes the status colour (4.5:1 on bg and raised, per palette.rs).
        let value_colour = match state.as_deref() {
            Some("info") => "var(--op-status-info)",
            Some("ok") => "var(--op-status-ok)",
            Some("warning") => "var(--op-status-warning)",
            Some("danger") => "var(--op-status-danger)",
            Some(_) => "var(--op-muted)",
            None => "var(--op-text)",
        };
        let unit_markup = if unit.is_empty() {
            String::new()
        } else {
            format!(" <span class=\"unit\">{}</span>", escape(&unit))
        };
        shadow_root(&self.host).set_inner_html(&format!(
            "<style>{BASE_CSS}
:host {{ display: inline-block; margin: 0.5rem 1.75rem 0.5rem 0; vertical-align: top; }}
.label {{
  margin: 0;
  font-size: 0.75rem;
  color: var(--op-muted);
  font-variant-caps: small-caps;
  letter-spacing: 0.03em;
}}
.value {{
  margin: 0;
  font-family: var(--op-font-mono);
  font-size: 1.6rem;
  line-height: 1.2;
  color: {value_colour};
}}
.unit {{ font-size: 0.9rem; color: var(--op-muted); }}
</style>
<p class=\"label\">{label}</p><p class=\"value\">{value}{unit_markup}</p>",
            label = escape(&label),
            value = escape(&value),
        ));
    }
}

impl CustomElement for Kpi {
    fn connected(&mut self) {
        self.render();
    }

    fn attribute_changed(&mut self, _n: &str, _o: Option<String>, _v: Option<String>) {
        self.render();
    }
}
