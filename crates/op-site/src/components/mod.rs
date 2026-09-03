//! The custom elements that make up the site. `index.html` composes the page
//! from these tags; nothing here is instantiated from Rust.

mod badge;
mod breadcrumb;
mod build_status;
mod button;
mod callout;
mod card;
mod checkbox;
mod chip;
mod details;
mod empty_state;
mod film;
mod input;
mod iso;
mod key_value;
mod kpi;
mod machine_diagram;
mod machine_probes;
mod pagination;
mod palette_specimen;
mod progress;
mod scene;
mod select_field;
mod site_footer;
mod site_header;
mod site_nav;
mod source;
mod starting_points;
mod steps;
mod switch;
mod table;
mod tabs;
mod theme_toggle;
mod timeline;
mod tooltip;
mod tree;

use op_webc::ElementDefinition;
use web_sys::{HtmlElement, ShadowRoot, ShadowRootInit, ShadowRootMode};

/// Every element the site defines, registered in order by `main`.
pub const DEFINITIONS: &[ElementDefinition] = &[
    theme_toggle::DEFINITION,
    film::DEFINITION,
    machine_diagram::DEFINITION,
    site_header::DEFINITION,
    starting_points::DEFINITION,
    build_status::DEFINITION,
    site_footer::DEFINITION,
    palette_specimen::DEFINITION,
    site_nav::DEFINITION,
    callout::DEFINITION,
    badge::DEFINITION,
    card::DEFINITION,
    details::DEFINITION,
    key_value::DEFINITION,
    table::DEFINITION,
    source::DEFINITION,
    timeline::DEFINITION,
    empty_state::DEFINITION,
    button::DEFINITION,
    input::DEFINITION,
    select_field::DEFINITION,
    checkbox::DEFINITION,
    switch::DEFINITION,
    tabs::DEFINITION,
    tabs::TAB_DEFINITION,
    breadcrumb::DEFINITION,
    pagination::DEFINITION,
    tooltip::DEFINITION,
    scene::DEFINITION,
    kpi::DEFINITION,
    machine_probes::DEFINITION,
    steps::DEFINITION,
    tree::DEFINITION,
    chip::DEFINITION,
    progress::DEFINITION,
];

/// Styles shared by every shadow root. Colours come from the `--op-*` custom
/// properties in `styles/theme.css`, which inherit across the shadow boundary.
pub const BASE_CSS: &str = "
:host { display: block; }
a { color: var(--op-link); }
a:hover { color: var(--op-link-hover); }
:focus-visible { outline: 2px solid var(--op-focus); outline-offset: 2px; }
h1, h2 { margin: 0; line-height: 1.2; font-family: var(--op-font-heading); letter-spacing: var(--op-heading-fallback-tracking, 0em); }
h1 { font-size: 1.75rem; }
h2 { font-size: 1.125rem; margin-top: 1.5rem; margin-bottom: 0.5rem; }
p { margin: 0.5rem 0; }
code {
  font-family: var(--op-font-mono);
  font-variant-ligatures: contextual;
  background: var(--op-code-bg);
  padding: 0 0.25em;
  border-radius: 0.2em;
}
";

/// Installs `css` as a document-level stylesheet once, keyed by `id`.
/// For components whose representation must reach light-DOM content
/// (a native control they wrap): the component still owns the CSS in
/// Rust, it just lands in the document instead of a shadow tree.
pub fn document_style(id: &str, css: &str) {
    let Some(document) = web_sys::window().and_then(|w| w.document()) else {
        return;
    };
    if document.get_element_by_id(id).is_some() {
        return;
    }
    let Ok(style) = document.create_element("style") else {
        return;
    };
    style.set_id(id);
    style.set_text_content(Some(css));
    if let Some(head) = document.head() {
        let _ = head.append_child(&style);
    }
}

/// Attaches an open shadow root to `host` (once) and returns it.
pub fn shadow_root(host: &HtmlElement) -> ShadowRoot {
    if let Some(existing) = host.shadow_root() {
        return existing;
    }
    host.attach_shadow(&ShadowRootInit::new(ShadowRootMode::Open))
        .expect("attach shadow root")
}

#[cfg(test)]
mod definition_tests {
    use super::*;

    /// Every definition's source location (`op_webc::here!`) must name
    /// a real workspace file: the shim maps the element's class onto it
    /// and the site serves it under `/src/`, so a wrong path breaks the
    /// inspector's jump-to-definition silently.
    /// Every element must say how the interaction report exercises it
    /// (`data/interaction-contract.json`), even if only to declare it
    /// static: a control nobody thought to drive is exactly how a
    /// hover-only defect ships.
    #[test]
    fn every_definition_is_in_the_interaction_contract() {
        let contract = include_str!("../../../../data/interaction-contract.json");
        for definition in DEFINITIONS {
            assert!(
                contract.contains(&format!("\"tag\": \"{}\"", definition.tag)),
                "{} is missing from data/interaction-contract.json",
                definition.tag
            );
        }
    }

    #[test]
    fn every_definition_names_its_real_source_file() {
        let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        for definition in DEFINITIONS {
            let source = &definition.source;
            assert!(
                source.path.starts_with("crates/") && source.path.ends_with(".rs"),
                "{}: unexpected source shape {}",
                definition.tag,
                source.path
            );
            assert!(
                workspace.join(source.path).is_file(),
                "{}: {} does not exist",
                definition.tag,
                source.path
            );
            assert!(source.line > 0, "{}", definition.tag);
        }
    }
}
