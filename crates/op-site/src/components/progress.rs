//! `<opt-progress value="0..100" label="..." state="ok|warning|danger">`: a
//! progress bar. With no `value` it renders indeterminate (a sweeping band,
//! stilled under prefers-reduced-motion). The fill takes the accent, or a
//! status token when `state` is set; the element carries the progressbar
//! role and aria values for assistive tech.

use op_webc::{CustomElement, ElementDefinition};
use web_sys::HtmlElement;

use super::{BASE_CSS, shadow_root};
use crate::html::escape;

pub const DEFINITION: ElementDefinition = ElementDefinition {
    source: op_webc::here!(),
    tag: "opt-progress",
    observed_attributes: &["value", "label", "state"],
    create: |host| Box::new(Progress { host }),
};

struct Progress {
    host: HtmlElement,
}

impl Progress {
    fn render(&self) {
        let label = self.host.get_attribute("label").unwrap_or_default();
        let value: Option<f64> = self
            .host
            .get_attribute("value")
            .and_then(|v| v.parse().ok())
            .map(|v: f64| v.clamp(0.0, 100.0));
        let fill = match self.host.get_attribute("state").as_deref() {
            Some("ok") => "var(--op-status-ok)",
            Some("warning") => "var(--op-status-warning)",
            Some("danger") => "var(--op-status-danger)",
            _ => "var(--op-accent)",
        };
        let _ = self.host.set_attribute("role", "progressbar");
        let _ = self.host.set_attribute("aria-valuemin", "0");
        let _ = self.host.set_attribute("aria-valuemax", "100");
        match value {
            Some(v) => {
                let _ = self.host.set_attribute("aria-valuenow", &format!("{v:.0}"));
            }
            None => {
                let _ = self.host.remove_attribute("aria-valuenow");
            }
        }
        if !label.is_empty() {
            let _ = self.host.set_attribute("aria-label", &label);
        }
        let label_markup = if label.is_empty() {
            String::new()
        } else {
            let percent = value
                .map(|v| format!("<span class=\"percent\">{v:.0}%</span>"))
                .unwrap_or_default();
            format!("<p class=\"label\">{}{percent}</p>", escape(&label))
        };
        let bar = match value {
            Some(v) => format!(
                "<div class=\"track\"><div class=\"fill\" style=\"width: {v}%\"></div></div>"
            ),
            None => "<div class=\"track\"><div class=\"fill sweep\"></div></div>".to_owned(),
        };
        shadow_root(&self.host).set_inner_html(&format!(
            "<style>{BASE_CSS}
:host {{ display: block; margin: 1rem 0; }}
.label {{
  display: flex;
  justify-content: space-between;
  gap: 1rem;
  margin: 0 0 0.25rem;
  font-size: 0.85rem;
  color: var(--op-muted);
}}
.percent {{ font-family: var(--op-font-mono); }}
.track {{
  height: 0.4rem;
  background: var(--op-code-bg);
  border-radius: 0.2rem;
  overflow: hidden;
}}
.fill {{
  height: 100%;
  background: {fill};
  border-radius: 0.2rem;
}}
.sweep {{ width: 30%; }}
@media (prefers-reduced-motion: no-preference) {{
  .sweep {{ animation: opt-progress-sweep 1.6s ease-in-out infinite; }}
}}
@keyframes opt-progress-sweep {{
  0% {{ margin-left: -30%; }}
  100% {{ margin-left: 100%; }}
}}
</style>
{label_markup}{bar}"
        ));
    }
}

impl CustomElement for Progress {
    fn connected(&mut self) {
        self.render();
    }

    fn attribute_changed(&mut self, _n: &str, _o: Option<String>, _v: Option<String>) {
        self.render();
    }
}
