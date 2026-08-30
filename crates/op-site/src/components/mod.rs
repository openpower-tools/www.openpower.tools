//! The custom elements that make up the site. `index.html` composes the page
//! from these tags; nothing here is instantiated from Rust.

mod badge;
mod build_status;
mod callout;
mod card;
mod details;
mod empty_state;
mod key_value;
mod palette_specimen;
mod site_footer;
mod site_header;
mod site_nav;
mod source;
mod starting_points;
mod table;
mod theme_toggle;
mod timeline;

use op_webc::ElementDefinition;
use web_sys::{HtmlElement, ShadowRoot, ShadowRootInit, ShadowRootMode};

/// Every element the site defines, registered in order by `main`.
pub const DEFINITIONS: &[ElementDefinition] = &[
    theme_toggle::DEFINITION,
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

/// Attaches an open shadow root to `host` (once) and returns it.
pub fn shadow_root(host: &HtmlElement) -> ShadowRoot {
    if let Some(existing) = host.shadow_root() {
        return existing;
    }
    host.attach_shadow(&ShadowRootInit::new(ShadowRootMode::Open))
        .expect("attach shadow root")
}
