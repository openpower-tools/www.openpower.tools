//! `<opt-kpi label="..." value="..." unit="..." state="neutral|info|ok|warning|danger">`:
//! a single measurement, data-forward: big value, small label, optional unit,
//! the value coloured by an optional state (contrast-tested status tokens).
//! Several opt-kpi elements sit side by side in a row and wrap.

use op_webc::{CustomElement, ElementDefinition};
use web_sys::HtmlElement;

use super::{BASE_CSS, shadow_root};
use crate::html::escape;

pub const DEFINITION: ElementDefinition = ElementDefinition {
    source: op_webc::here!(),
    tag: "opt-kpi",
    observed_attributes: &["label", "value", "unit", "state"],
    properties: &[],
    create: |host| Box::new(Kpi { host }),
};

struct Kpi {
    host: HtmlElement,
}

const STATES: &[&str] = &["neutral", "info", "ok", "warning", "danger"];

/// The status a KPI shows: a valid `state` attribute wins, otherwise the
/// severity of the contained term (scheme, value), otherwise none.
fn state_of(attr: Option<&str>, term: Option<(&str, &str)>) -> Option<&'static str> {
    if let Some(s) = attr
        && let Some(known) = STATES.iter().find(|k| **k == s)
    {
        return Some(known);
    }
    term.map(|(scheme, value)| op_terms::severity_of(scheme, value).name())
}

impl Kpi {
    fn render(&self) {
        let attr = |name: &str| self.host.get_attribute(name).unwrap_or_default();
        let label = attr("label");
        let value = attr("value");
        let unit = attr("unit");
        // A contained <opt-term scheme value> is the semantic classification;
        // the KPI is one projection of it. An explicit state attribute is
        // the escape hatch for values that are not terms.
        let term = self
            .host
            .query_selector("opt-term")
            .ok()
            .flatten()
            .and_then(|t| Some((t.get_attribute("scheme")?, t.get_attribute("value")?)));
        let state = state_of(
            self.host.get_attribute("state").as_deref(),
            term.as_ref().map(|(s, v)| (s.as_str(), v.as_str())),
        );
        // No state: the value stays on the text token. With one, the value
        // takes the status colour (4.5:1 on bg and raised, per palette.rs).
        let value_colour = match state {
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

#[cfg(test)]
mod tests {
    use super::state_of;

    #[test]
    fn a_valid_state_attribute_wins_over_the_term() {
        assert_eq!(
            state_of(Some("warning"), Some(("outcome", "pass"))),
            Some("warning")
        );
    }

    #[test]
    fn the_contained_term_projects_its_severity() {
        assert_eq!(state_of(None, Some(("outcome", "pass"))), Some("ok"));
        assert_eq!(state_of(None, Some(("outcome", "fail"))), Some("danger"));
        assert_eq!(state_of(None, Some(("support", "patched"))), Some("info"));
        assert_eq!(state_of(None, Some(("flight", "Back"))), Some("warning"));
    }

    #[test]
    fn an_unknown_attribute_falls_through_to_the_term_then_to_none() {
        assert_eq!(
            state_of(Some("loud"), Some(("outcome", "skipped"))),
            Some("neutral")
        );
        assert_eq!(state_of(Some("loud"), None), None);
        assert_eq!(state_of(None, None), None);
    }

    #[test]
    fn an_unknown_term_is_neutral_not_a_crash() {
        assert_eq!(state_of(None, Some(("outcome", "maybe"))), Some("neutral"));
    }
}
