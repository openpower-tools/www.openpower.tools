//! `<opt-palette-specimen>`: every palette token and the site's elements,
//! rendered in both themes side by side for design preview. It is the body
//! of <https://www.openpower.tools/specimen/> (`specimen/index.html`, a
//! second Trunk target).
//!
//! It renders into the light DOM rather than a shadow root so that the
//! `.opt-theme-dark` and `.opt-theme-light` token scopes in `styles/theme.css`
//! apply to its two columns; the real site elements placed inside each column
//! pick up that column's tokens through inheritance.

use op_webc::{CustomElement, ElementDefinition};
use wasm_bindgen::JsCast;
use web_sys::{Element, HtmlElement};

use crate::colour::{self, Rgb};

pub const DEFINITION: ElementDefinition = ElementDefinition {
    tag: "opt-palette-specimen",
    observed_attributes: &[],
    create: |host| {
        Box::new(Specimen {
            host,
            on_fonts_loadingdone: None,
        })
    },
};

struct Specimen {
    host: HtmlElement,
    /// Kept alive for as long as the element exists.
    on_fonts_loadingdone: Option<wasm_bindgen::prelude::Closure<dyn FnMut(web_sys::Event)>>,
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
    (
        "--op-raised",
        "raised background (callouts)",
        Role::Backdrop,
    ),
    ("--op-pole-base", "barberpole mix base", Role::Decoration),
    ("--op-text", "body text", Role::Text),
    ("--op-muted", "secondary text", Role::Text),
    ("--op-link", "links", Role::Text),
    ("--op-link-hover", "links on hover", Role::Text),
    ("--op-accent", "hover borders and rules", Role::Ui),
    ("--op-focus", "focus ring", Role::Ui),
    ("--op-border-strong", "control borders", Role::Ui),
    ("--op-status-neutral", "neutral status marker", Role::Ui),
    ("--op-status-info", "note and info markers", Role::Ui),
    ("--op-status-ok", "success markers", Role::Ui),
    ("--op-status-warning", "warning markers", Role::Ui),
    ("--op-status-danger", "danger markers", Role::Ui),
    ("--op-border", "separators", Role::Decoration),
    ("--op-highlight", "title rule", Role::Decoration),
];

const STYLE: &str = "
opt-palette-specimen { display: block; margin-top: 1.5rem; }
opt-palette-specimen .columns {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(19rem, 1fr));
  gap: 1rem;
}
opt-palette-specimen .column {
  background: var(--op-bg);
  color: var(--op-text);
  padding: 1rem;
  border: 1px solid var(--op-border-strong);
  border-radius: 0.5rem;
}
opt-palette-specimen h2 { margin: 0 0 0.25rem; font-size: 1.125rem; }
opt-palette-specimen .muted { color: var(--op-muted); }
opt-palette-specimen .swatches {
  list-style: none;
  margin: 0.75rem 0 1rem;
  padding: 0;
  display: grid;
  gap: 0.45rem;
}
opt-palette-specimen .swatch {
  display: flex;
  align-items: center;
  gap: 0.6rem;
  font-size: 0.8rem;
  line-height: 1.3;
}
opt-palette-specimen .chip {
  flex: none;
  width: 2.25rem;
  height: 2.25rem;
  border-radius: 0.3rem;
  border: 1px solid var(--op-border-strong);
}
opt-palette-specimen .meta { display: grid; }
opt-palette-specimen .meta .role { color: var(--op-muted); }
opt-palette-specimen .meta .ratio { color: var(--op-muted); }
opt-palette-specimen pre,
opt-palette-specimen code {
  font-family: var(--op-font-mono);
  background: var(--op-code-bg);
  border-radius: 0.3rem;
}
opt-palette-specimen code { padding: 0 0.25em; }
opt-palette-specimen pre { padding: 0.5rem 0.75rem; overflow-x: auto; }
opt-palette-specimen .sample-button {
  font: inherit;
  font-size: 0.875rem;
  color: var(--op-text);
  background: var(--op-surface);
  border: 1px solid var(--op-border-strong);
  border-radius: 0.375rem;
  padding: 0.35rem 0.7rem;
  cursor: pointer;
}
opt-palette-specimen .sample-button:hover { border-color: var(--op-accent); }
opt-palette-specimen .focus-sample {
  outline: 2px solid var(--op-focus);
  outline-offset: 2px;
  padding: 0 0.25rem;
}
opt-palette-specimen .surface-sample {
  background: var(--op-surface);
  padding: 0.5rem 0.75rem;
  border-radius: 0.375rem;
}
opt-palette-specimen hr.rule { border: 0; border-top: 1px solid var(--op-border); margin: 0.75rem 0; }
opt-palette-specimen hr.rule.strong { border-top-color: var(--op-border-strong); }
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
<opt-site-header heading=\"Heading\" tagline=\"Tagline in secondary text under the title rule.\"></opt-site-header>
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
            "opt-theme-dark",
            "Dark (default)",
            "Derived from the Worcester palette."
        ),
        column(
            "opt-theme-light",
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
    render_typography(host);
}

impl CustomElement for Specimen {
    fn connected(&mut self) {
        // Re-annotate whenever the stylesheet finishes loading faces (they
        // arrive lazily as glyphs demand them), so the badges track reality.
        if self.on_fonts_loadingdone.is_none()
            && let Some(document) = web_sys::window().and_then(|w| w.document())
        {
            let closure =
                wasm_bindgen::prelude::Closure::<dyn FnMut(web_sys::Event)>::new(move |_event| {
                    annotate_face_status()
                });
            let _ = document
                .fonts()
                .add_event_listener_with_callback("loadingdone", closure.as_ref().unchecked_ref());
            self.on_fonts_loadingdone = Some(closure);
        }
        render(&self.host);
    }
}

/// One typography candidate: label, note, and the three stacks.
struct TypeOption {
    id: &'static str,
    label: &'static str,
    note: &'static str,
    heading: &'static str,
    body: &'static str,
    mono: &'static str,
    /// Families whose availability the badge reports.
    check: &'static [&'static str],
}

const TYPE_OPTIONS: &[TypeOption] = &[
    TypeOption {
        id: "site",
        label: "Site rendering: the fitted embedded faces",
        note: "Barlow Semi Condensed fitted to Sys 2.0 for headings, IBM Plex Sans body, Iosevka SS08 fitted to PragmataPro for code; identical on every machine.",
        heading: "'Barlow Semi Condensed', system-ui, sans-serif",
        body: "'IBM Plex Sans', system-ui, sans-serif",
        mono: "'Iosevka SS08', ui-monospace, monospace",
        check: &["Barlow Semi Condensed", "IBM Plex Sans", "Iosevka SS08"],
    },
    TypeOption {
        id: "fallback",
        label: "Before the pack arrives, and wherever it never can",
        note: "Metric-fitted local() faces: the swap to the embedded faces changes letterforms without moving layout.",
        heading: "'opt-heading-fallback', system-ui, sans-serif",
        body: "'opt-body-fallback', system-ui, sans-serif",
        mono: "'opt-mono-fallback', ui-monospace, monospace",
        check: &[],
    },
];

const TYPE_STYLE: &str = "
opt-palette-specimen .type-option {
  background: var(--op-surface);
  border: 1px solid var(--op-border-strong);
  border-radius: 0.5rem;
  padding: 0.75rem 1rem;
  margin: 0.75rem 0;
}
opt-palette-specimen .type-option h3 { margin: 0; font-size: 1rem; }
opt-palette-specimen .type-option .t-h {
  font-size: 1.4rem;
  font-weight: 700;
  margin: 0.5rem 0 0.25rem;
}
opt-palette-specimen .type-option .t-b { margin: 0.25rem 0; }
opt-palette-specimen .type-option .t-m {
  font-variant-ligatures: contextual;
  margin: 0.25rem 0 0;
  background: var(--op-code-bg);
  padding: 0.3rem 0.5rem;
  border-radius: 0.3rem;
  white-space: pre-wrap;
}
";

fn type_option_markup(option: &TypeOption) -> String {
    format!(
        "<article class=\"type-option\" id=\"type-{}\">
<h3>{}</h3>
<p class=\"muted\">{} <span class=\"face-status\" data-families=\"{}\"></span></p>
<p class=\"t-h\" style=\"font-family: {}\">OpenPOWER firmware, ports and tools</p>
<p class=\"t-b\" style=\"font-family: {}\">Owner-controlled POWER9 systems: the Talos II and Blackbird boot from fully inspectable firmware. 0123456789 Il1 O0 — <em>italic</em>, <strong>bold</strong>.</p>
<p class=\"t-m\" style=\"font-family: {}\">pflash -E -p /tmp/talos.pnor &amp;&amp; echo ok  # =&gt; != === 0xDEADBEEF fi ffi</p>
</article>",
        option.id,
        option.label,
        option.note,
        option.check.join("|"),
        option.heading,
        option.body,
        option.mono,
    )
}

fn annotate_face_status() {
    let Some(document) = web_sys::window().and_then(|w| w.document()) else {
        return;
    };
    let Ok(nodes) = document.query_selector_all("opt-palette-specimen .face-status") else {
        return;
    };
    for index in 0..nodes.length() {
        let Some(el) = nodes.item(index).and_then(|n| n.dyn_into::<Element>().ok()) else {
            continue;
        };
        let families = el.get_attribute("data-families").unwrap_or_default();
        if families.is_empty() {
            continue;
        }
        let status: Vec<String> = families
            .split('|')
            .map(|family| {
                let resolves = crate::fontprobe::family_resolves(family);
                format!(
                    "{family}: {}",
                    if resolves { "resolves" } else { "not here" }
                )
            })
            .collect();
        el.set_text_content(Some(&status.join(" / ")));
    }
}

fn render_typography(host: &HtmlElement) {
    let cards: String = TYPE_OPTIONS.iter().map(type_option_markup).collect();
    let section = format!(
        "<style>{TYPE_STYLE}</style>
<h2>Typography</h2>
<p class=\"muted\">The typography as everyone sees it: the fitted embedded faces, and the metric-fitted fallbacks that hold layout until the font pack arrives. Locally installed fonts are deliberately not used.</p>
{cards}"
    );
    let current = host.inner_html();
    host.set_inner_html(&format!("{current}{section}"));
    let document = web_sys::window()
        .and_then(|w| w.document())
        .expect("document");
    let ready = document.fonts().ready().expect("fonts.ready");
    wasm_bindgen_futures::spawn_local(async move {
        let _ = wasm_bindgen_futures::JsFuture::from(ready).await;
        annotate_face_status();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fonts arrive only through the pack and the generated fallback
    /// stylesheet; the source stylesheet declares no font sources at all.
    #[test]
    fn stylesheet_declares_no_font_sources() {
        let css = include_str!("../../../../styles/theme.css");
        assert!(
            !css.contains("@font-face"),
            "unexpected @font-face in theme.css"
        );
        assert!(!css.contains("url("), "unexpected url() in theme.css");
        assert!(
            !css.contains(".woff"),
            "unexpected font file reference in theme.css"
        );
    }

    #[test]
    fn type_options_reference_families_from_their_own_stacks() {
        assert!(TYPE_OPTIONS.len() >= 2);
        for option in TYPE_OPTIONS {
            for family in option.check {
                let quoted = format!("'{family}'");
                assert!(
                    option.heading.contains(&quoted)
                        || option.body.contains(&quoted)
                        || option.mono.contains(&quoted),
                    "{}: checked family {family} is not in any stack",
                    option.id
                );
            }
            for stack in [option.heading, option.body, option.mono] {
                assert!(
                    stack.ends_with("sans-serif")
                        || stack.ends_with("monospace")
                        || stack.ends_with("serif"),
                    "{}: stack {stack:?} lacks a generic fallback",
                    option.id
                );
            }
        }
        let markup = type_option_markup(&TYPE_OPTIONS[0]);
        assert!(markup.contains("face-status"));
        assert!(markup.contains(TYPE_OPTIONS[0].label));
    }

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
        let markup = column("opt-theme-dark", "Dark", "source");
        for (name, role, _) in TOKENS {
            assert_eq!(
                markup.matches(&format!("data-token=\"{name}\"")).count(),
                1,
                "{name}"
            );
            assert!(markup.contains(role), "{role}");
        }
        assert!(markup.contains("<opt-site-header"));
        assert!(markup.starts_with("<section class=\"opt-theme-dark column\">"));
    }
}
