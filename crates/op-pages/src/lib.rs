//! The page registry: every generated page of the site.
//!
//! Bodies are namespaced XML sources (see [`lower`]) validated and
//! lowered to HTML at emit time.
//!
//! Bodies are HTML fragments in `pages/`, composed of the custom elements
//! defined in `op-site` plus light-DOM content. The `op-pages` binary (a
//! Trunk `post_build` hook that runs after `op-assets`) wraps each fragment
//! in the staged `index.html`'s head - which carries the hashed script, style,
//! font pack and fallback references - so every page loads the same wasm and
//! assets from clean URLs like `/components/button/`.

pub mod lower;
pub use lower::{OPT_NS, lower};

/// One generated page.
pub struct Page {
    /// URL path segment(s) under the site root, without slashes at the ends.
    pub slug: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    /// Body fragment; rendered between the shared nav and footer.
    pub body: &'static str,
}

macro_rules! page {
    ($slug:literal, $title:literal, $description:literal, $file:literal) => {
        Page {
            slug: $slug,
            title: $title,
            description: $description,
            body: include_str!(concat!("../../../pages/", $file)),
        }
    };
}

/// Every generated page. The home page stays a Trunk source file; these are
/// emitted beside it.
pub const PAGES: &[Page] = &[
    page!(
        "specimen",
        "Palette specimen",
        "Every design token with its value and WCAG contrast, the site's elements in both themes, and the typography tiers.",
        "specimen.xml"
    ),
    page!(
        "components",
        "Components",
        "The web components the site is built from, each with its states and variants.",
        "components/index.xml"
    ),
    page!(
        "components/palette",
        "Component palette",
        "Every component rendered live, side by side.",
        "components/palette/index.xml"
    ),
    page!(
        "component/callout",
        "Callout",
        "Highlighted blocks for notes, tips and warnings.",
        "component/callout/index.xml"
    ),
    page!(
        "component/badge",
        "Badge",
        "Compact status markers with variant-coloured dots.",
        "component/badge/index.xml"
    ),
    page!(
        "component/card",
        "Card",
        "Titled content containers with optional link headings and footers.",
        "component/card/index.xml"
    ),
    page!(
        "component/details",
        "Details",
        "Collapsible sections built on the native details element.",
        "component/details/index.xml"
    ),
    page!(
        "component/key-value",
        "Key-value",
        "Label/value pairs for specs and metadata.",
        "component/key-value/index.xml"
    ),
    page!(
        "component/table",
        "Table",
        "Tabular data in a scrollable, token-styled frame.",
        "component/table/index.xml"
    ),
    page!(
        "component/source",
        "Source",
        "Labelled code blocks with a copy control.",
        "component/source/index.xml"
    ),
    page!(
        "component/timeline",
        "Timeline",
        "Dated events on a vertical line with status dots.",
        "component/timeline/index.xml"
    ),
    page!(
        "component/empty-state",
        "Empty state",
        "Placeholders for content that does not exist yet.",
        "component/empty-state/index.xml"
    ),
    page!(
        "component/button",
        "Button",
        "Action buttons with primary and danger weights.",
        "component/button/index.xml"
    ),
    page!(
        "component/input",
        "Input",
        "Labelled text inputs and textareas.",
        "component/input/index.xml"
    ),
    page!(
        "component/select",
        "Select",
        "Labelled native selects styled with the theme tokens.",
        "component/select/index.xml"
    ),
    page!(
        "component/checkbox",
        "Checkbox",
        "Native checkboxes with their labels.",
        "component/checkbox/index.xml"
    ),
    page!(
        "component/switch",
        "Switch",
        "Native checkboxes drawn as switches.",
        "component/switch/index.xml"
    ),
    page!(
        "component/tabs",
        "Tabs",
        "Tabbed regions with keyboard activation.",
        "component/tabs/index.xml"
    ),
    page!(
        "component/breadcrumb",
        "Breadcrumb",
        "Where-am-I trails with stylesheet separators.",
        "component/breadcrumb/index.xml"
    ),
    page!(
        "component/pagination",
        "Pagination",
        "Page navigation for split lists.",
        "component/pagination/index.xml"
    ),
    page!(
        "component/tooltip",
        "Tooltip",
        "Hover and focus annotations for terms.",
        "component/tooltip/index.xml"
    ),
    page!(
        "component/scene",
        "Scene",
        "Embedded A-Frame scenes with Rust-driven behaviour.",
        "component/scene/index.xml"
    ),
    page!(
        "component/kpi",
        "KPI",
        "Single measurements: big value, small label, status colour.",
        "component/kpi/index.xml"
    ),
    page!(
        "component/steps",
        "Steps",
        "Numbered procedures with done, current and error states.",
        "component/steps/index.xml"
    ),
    page!(
        "component/tree",
        "Tree",
        "Collapsible hierarchies on native details elements.",
        "component/tree/index.xml"
    ),
    page!(
        "component/chip",
        "Chip",
        "Interactive filter tokens: toggleable and removable.",
        "component/chip/index.xml"
    ),
    page!(
        "component/progress",
        "Progress",
        "Determinate and indeterminate progress bars.",
        "component/progress/index.xml"
    ),
];

/// The shared site navigation, used verbatim on generated pages and mirrored
/// by the home page (a test keeps them identical).
pub fn nav_markup() -> String {
    let mut items = String::new();
    for (href, label) in [
        ("/", "Home"),
        ("/components/", "Components"),
        ("/specimen/", "Specimen"),
    ] {
        items.push_str(&format!("<li><a href=\"{href}\">{label}</a></li>"));
    }
    format!("<opt-site-nav><ul>{items}</ul></opt-site-nav>")
}

/// The home page's navigation: Home only. The section indexes
/// (components, specimen) are deliberately not linked from the main
/// page; they remain reachable by URL and from the generated pages,
/// which use the full [`nav_markup`].
pub fn home_nav_markup() -> String {
    "<opt-site-nav><ul><li><a href=\"/\">Home</a></li></ul></opt-site-nav>".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_are_unique_clean_and_bodies_nonempty() {
        let mut slugs: Vec<&str> = PAGES.iter().map(|p| p.slug).collect();
        for page in PAGES {
            assert!(
                !page.slug.is_empty()
                    && !page.slug.starts_with('/')
                    && !page.slug.ends_with('/')
                    && !page.slug.contains(".."),
                "bad slug {}",
                page.slug
            );
            assert!(!page.title.is_empty() && !page.description.is_empty());
            assert!(page.body.contains('<'), "{}: body looks empty", page.slug);
        }
        slugs.sort_unstable();
        slugs.dedup();
        assert_eq!(slugs.len(), PAGES.len(), "duplicate slugs");
    }

    #[test]
    fn home_page_nav_matches_the_generated_nav() {
        let home = include_str!("../../../index.html");
        let nav = home_nav_markup();
        let start = home.find("<opt-site-nav>").expect("home nav");
        let end = home.find("</opt-site-nav>").expect("home nav end") + "</opt-site-nav>".len();
        assert_eq!(
            &home[start..end],
            nav,
            "home nav drifted from op_pages::home_nav_markup()"
        );
    }

    #[test]
    fn nav_links_cover_home_and_every_top_level_section() {
        let nav = nav_markup();
        assert!(nav.starts_with("<opt-site-nav>") && nav.ends_with("</opt-site-nav>"));
        for href in ["\"/\"", "\"/components/\"", "\"/specimen/\""] {
            assert!(nav.contains(href), "nav lacks {href}");
        }
    }
}
