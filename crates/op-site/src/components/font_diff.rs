//! `<op-font-diff>`: differential rendering of two font families.
//!
//! The same sample is shown three times: once in family A with no fallback,
//! once in family B with no fallback (so a missing font is visible instead of
//! silently substituted), and once as an overlay with B drawn in the accent
//! colour at reduced opacity over A, so metric drift and letterform
//! differences appear as fringes. A status line reports which families
//! actually resolved in this browser, measured by layout width rather than
//! `document.fonts.check`, which only answers for set-registered faces.
//!
//! Attributes (all observed): `family-a`, `family-b`, `label-a`, `label-b`,
//! `sample`, `font-size` (any CSS length, default `1.5rem`).

use op_webc::{CustomElement, ElementDefinition};
use web_sys::HtmlElement;

use crate::html::escape;

pub const DEFINITION: ElementDefinition = ElementDefinition {
    tag: "op-font-diff",
    observed_attributes: &[
        "family-a",
        "family-b",
        "label-a",
        "label-b",
        "sample",
        "font-size",
    ],
    create: |host| Box::new(FontDiff { host }),
};

struct FontDiff {
    host: HtmlElement,
}

/// CSS generic family keywords that must not be quoted.
const GENERIC_FAMILIES: &[&str] = &[
    "system-ui",
    "ui-monospace",
    "ui-sans-serif",
    "ui-serif",
    "monospace",
    "sans-serif",
    "serif",
    "cursive",
    "fantasy",
];

/// Renders a family name as a CSS `font-family` value: quoted unless it is a
/// generic keyword.
fn css_family(family: &str) -> String {
    if GENERIC_FAMILIES.contains(&family) {
        family.to_owned()
    } else {
        format!("'{}'", family.replace(['\'', '"'], ""))
    }
}

fn row(class: &str, label: &str, family: &str, sample: &str) -> String {
    format!(
        "<div class=\"row\"><span class=\"label\">{}</span><span class=\"sample {class}\" style=\"font-family: {}\">{}</span></div>",
        escape(label),
        css_family(family),
        escape(sample),
    )
}

impl FontDiff {
    fn attr(&self, name: &str, default: &str) -> String {
        self.host
            .get_attribute(name)
            .unwrap_or_else(|| default.to_owned())
    }

    fn render(&self) {
        let family_a = self.attr("family-a", "system-ui");
        let family_b = self.attr("family-b", "system-ui");
        let label_a = self.attr("label-a", &family_a);
        let label_b = self.attr("label-b", &family_b);
        let sample = self.attr(
            "sample",
            "The quick brown fox jumps over the lazy dog 0123456789",
        );
        let font_size = self.attr("font-size", "1.5rem");
        let shadow = super::shadow_root(&self.host);
        shadow.set_inner_html(&format!(
            "<style>{base}
:host {{ margin: 1rem 0 1.5rem; }}
.frame {{
  background: var(--op-surface);
  border: 1px solid var(--op-border-strong);
  border-radius: 0.5rem;
  padding: 0.75rem 1rem;
}}
.row {{ display: grid; gap: 0.15rem; margin: 0.5rem 0; }}
.label {{ font-size: 0.75rem; color: var(--op-muted); }}
.sample {{ font-size: {font_size}; line-height: 1.35; overflow-wrap: anywhere; font-variant-ligatures: contextual; }}
.overlay {{ position: relative; }}
.overlay .b {{ position: absolute; inset: 0; color: var(--op-accent); opacity: 0.65; }}
.status {{ font-size: 0.75rem; color: var(--op-muted); margin-top: 0.5rem; }}
</style>
<div class=\"frame\">
{row_a}
{row_b}
<div class=\"row\"><span class=\"label\">overlay: {lb} in accent over {la}</span>
<div class=\"overlay\">
<div class=\"sample a\" style=\"font-family: {fa}\">{s}</div>
<div class=\"sample b\" aria-hidden=\"true\" style=\"font-family: {fb}\">{s}</div>
</div></div>
<p class=\"status\"></p>
</div>",
            base = super::BASE_CSS,
            row_a = row("a", &label_a, &family_a, &sample),
            row_b = row("b", &label_b, &family_b, &sample),
            la = escape(&label_a),
            lb = escape(&label_b),
            fa = css_family(&family_a),
            fb = css_family(&family_b),
            s = escape(&sample),
        ));
        let host = self.host.clone();
        let (fam_a, fam_b) = (family_a.clone(), family_b.clone());
        if let Some(document) = web_sys::window().and_then(|w| w.document())
            && let Ok(ready) = document.fonts().ready()
        {
            wasm_bindgen_futures::spawn_local(async move {
                let _ = wasm_bindgen_futures::JsFuture::from(ready).await;
                let describe = |family: &str| {
                    if crate::fontprobe::family_resolves(family) {
                        "resolves"
                    } else {
                        "does not resolve (its line shows a fallback)"
                    }
                };
                let text = format!(
                    "in this browser: {} {}; {} {}",
                    fam_a,
                    describe(&fam_a),
                    fam_b,
                    describe(&fam_b),
                );
                if let Some(shadow) = host.shadow_root()
                    && let Ok(Some(status)) = shadow.query_selector(".status")
                {
                    status.set_text_content(Some(&text));
                }
            });
        }
    }
}

impl CustomElement for FontDiff {
    fn connected(&mut self) {
        self.render();
    }

    fn attribute_changed(&mut self, _name: &str, _old: Option<String>, _new: Option<String>) {
        self.render();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generic_families_stay_unquoted_and_names_are_single_quoted() {
        // Single quotes: the value is interpolated into double-quoted HTML
        // style attributes, where a double quote would end the attribute.
        assert_eq!(css_family("system-ui"), "system-ui");
        assert_eq!(css_family("ui-monospace"), "ui-monospace");
        assert_eq!(css_family("Sys 2.0"), "'Sys 2.0'");
        assert_eq!(css_family("PragmataPro Liga"), "'PragmataPro Liga'");
        assert_eq!(
            css_family("evil'fam\"ily"),
            "'evilfamily'",
            "quotes stripped, not injected"
        );
    }

    #[test]
    fn rows_escape_content_and_carry_the_family_without_breaking_the_attribute() {
        let markup = row("a", "Label <x>", "Sys 2.0", "a & b");
        assert!(markup.contains("Label &lt;x&gt;"));
        assert!(markup.contains("a &amp; b"));
        assert!(markup.contains("style=\"font-family: 'Sys 2.0'\""));
        assert!(
            !markup.contains("font-family: \""),
            "double quote inside a style attribute"
        );
    }
}
