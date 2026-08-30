//! The custom elements that make up the site. `index.html` composes the page
//! from these tags; nothing here is instantiated from Rust.

mod build_status;
mod site_footer;
mod site_header;
mod starting_points;
mod theme_toggle;

use op_webc::ElementDefinition;
use web_sys::{HtmlElement, ShadowRoot, ShadowRootInit, ShadowRootMode};

/// Every element the site defines, registered in order by `main`.
pub const DEFINITIONS: &[ElementDefinition] = &[
    theme_toggle::DEFINITION,
    site_header::DEFINITION,
    starting_points::DEFINITION,
    build_status::DEFINITION,
    site_footer::DEFINITION,
];

/// Styles shared by every shadow root. Colours come from the `--op-*` custom
/// properties in `styles/theme.css`, which inherit across the shadow boundary.
pub const BASE_CSS: &str = "
:host { display: block; }
a { color: var(--op-link); }
a:hover { color: var(--op-link-hover); }
:focus-visible { outline: 2px solid var(--op-focus); outline-offset: 2px; }
h1, h2 { margin: 0; line-height: 1.2; }
h1 { font-size: 1.75rem; }
h2 { font-size: 1.125rem; margin-top: 1.5rem; margin-bottom: 0.5rem; }
p { margin: 0.5rem 0; }
code {
  font-family: ui-monospace, monospace;
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
