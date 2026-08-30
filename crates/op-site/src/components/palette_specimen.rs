//! `<op-palette-specimen>`: every palette token and the site's elements,
//! rendered in both themes side by side for design preview. It is the body
//! of <https://www.openpower.tools/specimen/> (`specimen/index.html`, a
//! second Trunk target).
//!
//! It renders into the light DOM rather than a shadow root so that the
//! `.op-theme-dark` and `.op-theme-light` token scopes in `styles/theme.css`
//! apply to its two columns; the real site elements placed inside each column
//! pick up that column's tokens through inheritance.

use op_webc::{CustomElement, ElementDefinition};
use wasm_bindgen::JsCast;
use web_sys::{Element, HtmlElement};

use crate::colour::{self, Rgb};

pub const DEFINITION: ElementDefinition = ElementDefinition {
    tag: "op-palette-specimen",
    observed_attributes: &[],
    create: |host| Box::new(Specimen { host }),
};

struct Specimen {
    host: HtmlElement,
}

/// How a token's contrast is reported.
#[derive(Clone, Copy)]
enum Role {
    /// Text: measured against the page background and the surface.
    Text,
    /// UI boundary or indicator: measured against background and surface.
    Ui,
    /// Something drawn behind text: measured with body text on it.
    Backdrop,
    /// Decoration only: no requirement, no ratio shown.
    Decoration,
}

const TOKENS: &[(&str, &str, Role)] = &[
    ("--op-bg", "page background", Role::Backdrop),
    ("--op-surface", "controls and panels", Role::Backdrop),
    ("--op-code-bg", "code background", Role::Backdrop),
    ("--op-text", "body text", Role::Text),
    ("--op-muted", "secondary text", Role::Text),
    ("--op-link", "links", Role::Text),
    ("--op-link-hover", "links on hover", Role::Text),
    ("--op-accent", "hover borders and rules", Role::Ui),
    ("--op-focus", "focus ring", Role::Ui),
    ("--op-border-strong", "control borders", Role::Ui),
    ("--op-border", "separators", Role::Decoration),
    ("--op-highlight", "title rule", Role::Decoration),
];

const STYLE: &str = "
op-palette-specimen { display: block; margin-top: 1.5rem; }
op-palette-specimen .columns {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(19rem, 1fr));
  gap: 1rem;
}
op-palette-specimen .column {
  background: var(--op-bg);
  color: var(--op-text);
  padding: 1rem;
  border: 1px solid var(--op-border-strong);
  border-radius: 0.5rem;
}
op-palette-specimen h2 { margin: 0 0 0.25rem; font-size: 1.125rem; }
op-palette-specimen .muted { color: var(--op-muted); }
op-palette-specimen .swatches {
  list-style: none;
  margin: 0.75rem 0 1rem;
  padding: 0;
  display: grid;
  gap: 0.45rem;
}
op-palette-specimen .swatch {
  display: flex;
  align-items: center;
  gap: 0.6rem;
  font-size: 0.8rem;
  line-height: 1.3;
}
op-palette-specimen .chip {
  flex: none;
  width: 2.25rem;
  height: 2.25rem;
  border-radius: 0.3rem;
  border: 1px solid var(--op-border-strong);
}
op-palette-specimen .meta { display: grid; }
op-palette-specimen .meta .role { color: var(--op-muted); }
op-palette-specimen .meta .ratio { color: var(--op-muted); }
op-palette-specimen pre,
op-palette-specimen code {
  font-family: ui-monospace, monospace;
  background: var(--op-code-bg);
  border-radius: 0.3rem;
}
op-palette-specimen code { padding: 0 0.25em; }
op-palette-specimen pre { padding: 0.5rem 0.75rem; overflow-x: auto; }
op-palette-specimen .sample-button {
  font: inherit;
  font-size: 0.875rem;
  color: var(--op-text);
  background: var(--op-surface);
  border: 1px solid var(--op-border-strong);
  border-radius: 0.375rem;
  padding: 0.35rem 0.7rem;
  cursor: pointer;
}
op-palette-specimen .sample-button:hover { border-color: var(--op-accent); }
op-palette-specimen .focus-sample {
  outline: 2px solid var(--op-focus);
  outline-offset: 2px;
  padding: 0 0.25rem;
}
op-palette-specimen .surface-sample {
  background: var(--op-surface);
  padding: 0.5rem 0.75rem;
  border-radius: 0.375rem;
}
op-palette-specimen hr.rule { border: 0; border-top: 1px solid var(--op-border); margin: 0.75rem 0; }
op-palette-specimen hr.rule.strong { border-top-color: var(--op-border-strong); }
";

fn column(class: &str, title: &str, source: &str) -> String {
    let swatches: String = TOKENS
        .iter()
        .map(|(name, role, _)| {
            format!(
                "<li class=\"swatch\" data-token=\"{name}\">\
<span class=\"chip\" style=\"background: var({name})\"></span>\
<span class=\"meta\"><code>{name}</code><span class=\"role\">{role}</span>\
<span class=\"hex\"></span><span class=\"ratio\"></span></span></li>"
            )
        })
        .collect();
    format!(
        "<section class=\"{class} column\">
<h2>{title}</h2>
<p class=\"muted\">{source}</p>
<ul class=\"swatches\">{swatches}</ul>
<op-site-header heading=\"Heading\" tagline=\"Tagline in secondary text under the title rule.\"></op-site-header>
<p>Body text with a <a href=\"#\">link</a>, <code>inline code</code> and <span class=\"muted\">secondary text</span>.</p>
<pre>preformatted block
on the code background</pre>
<p><button type=\"button\" class=\"sample-button\">Button</button> <span class=\"focus-sample\">focus ring</span></p>
<p class=\"surface-sample\">Text on a surface, as in controls and panels.</p>
<hr class=\"rule\"><hr class=\"rule strong\">
</section>"
    )
}

/// Reads a custom property from an element's computed style as a colour.
fn token_colour(element: &Element, token: &str) -> Option<Rgb> {
    let style = web_sys::window()?.get_computed_style(element).ok()??;
    let value = style.get_property_value(token).ok()?;
    let value = value.trim();
    Rgb::from_hex(value).or_else(|| Rgb::from_css_rgb(value))
}

fn ratio_text(role: Role, colour: Rgb, bg: Rgb, surface: Rgb, text: Rgb) -> String {
    match role {
        Role::Text | Role::Ui => {
            let need = if matches!(role, Role::Text) { 4.5 } else { 3.0 };
            format!(
                "{:.2}:1 on bg, {:.2}:1 on surface (needs {need}:1)",
                colour::contrast(colour, bg),
                colour::contrast(colour, surface)
            )
        }
        Role::Backdrop => format!(
            "body text on it {:.2}:1 (needs 4.5:1)",
            colour::contrast(text, colour)
        ),
        Role::Decoration => "decoration only".to_owned(),
    }
}

/// Fills in the hex value and contrast ratios for every swatch in `column`
/// from the column's computed tokens.
fn annotate(column: &Element) {
    let Some(bg) = token_colour(column, "--op-bg") else {
        return;
    };
    let Some(surface) = token_colour(column, "--op-surface") else {
        return;
    };
    let Some(text) = token_colour(column, "--op-text") else {
        return;
    };
    for (name, _, role) in TOKENS {
        let Some(colour) = token_colour(column, name) else {
            continue;
        };
        let Ok(Some(swatch)) = column.query_selector(&format!("[data-token=\"{name}\"]")) else {
            continue;
        };
        if let Ok(Some(hex)) = swatch.query_selector(".hex") {
            hex.set_text_content(Some(&colour.to_hex()));
        }
        if let Ok(Some(ratio)) = swatch.query_selector(".ratio") {
            ratio.set_text_content(Some(&ratio_text(*role, colour, bg, surface, text)));
        }
    }
}

fn render(host: &HtmlElement) {
    host.set_inner_html(&format!(
        "<style>{STYLE}</style>
<div class=\"columns\">{}{}</div>",
        column(
            "op-theme-dark",
            "Dark (default)",
            "Derived from the Worcester palette."
        ),
        column(
            "op-theme-light",
            "Light",
            "Derived from the Nottingham palette."
        ),
    ));
    let columns = host
        .query_selector_all("section.column")
        .expect("query columns");
    for index in 0..columns.length() {
        if let Some(column) = columns
            .item(index)
            .and_then(|n| n.dyn_into::<Element>().ok())
        {
            annotate(&column);
        }
    }
}

impl CustomElement for Specimen {
    fn connected(&mut self) {
        render(&self.host);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_css_token_has_a_specimen_entry_and_vice_versa() {
        let css = include_str!("../../../../styles/theme.css");
        let dark_block = &css[css.find(":root,").unwrap()..];
        let dark_block = &dark_block[..dark_block.find('}').unwrap()];
        let mut declared: Vec<&str> = dark_block
            .lines()
            .filter_map(|l| l.trim().split_once(':'))
            .map(|(name, _)| name.trim())
            .filter(|name| name.starts_with("--op-"))
            .collect();
        let mut listed: Vec<&str> = TOKENS.iter().map(|(name, _, _)| *name).collect();
        declared.sort_unstable();
        listed.sort_unstable();
        assert_eq!(
            declared, listed,
            "styles/theme.css tokens and TOKENS differ"
        );
    }

    #[test]
    fn ratio_text_reports_the_right_requirement_per_role() {
        let black = Rgb(0, 0, 0);
        let white = Rgb(255, 255, 255);
        let grey = Rgb(0x76, 0x76, 0x76);
        assert_eq!(
            ratio_text(Role::Text, black, white, white, black),
            "21.00:1 on bg, 21.00:1 on surface (needs 4.5:1)"
        );
        assert!(ratio_text(Role::Ui, grey, white, white, black).ends_with("(needs 3:1)"));
        assert_eq!(
            ratio_text(Role::Backdrop, white, white, white, black),
            "body text on it 21.00:1 (needs 4.5:1)"
        );
        assert_eq!(
            ratio_text(Role::Decoration, grey, white, white, black),
            "decoration only"
        );
    }

    #[test]
    fn columns_render_every_token_once() {
        let markup = column("op-theme-dark", "Dark", "source");
        for (name, role, _) in TOKENS {
            assert_eq!(
                markup.matches(&format!("data-token=\"{name}\"")).count(),
                1,
                "{name}"
            );
            assert!(markup.contains(role), "{role}");
        }
        assert!(markup.contains("<op-site-header"));
        assert!(markup.starts_with("<section class=\"op-theme-dark column\">"));
    }
}
