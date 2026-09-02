//! The page registry: every generated page of the site.
//!
//! Bodies are HTML fragments in `pages/`, composed of the custom elements
//! defined in `op-site` plus light-DOM content. The `op-pages` binary (a
//! Trunk `post_build` hook that runs after `op-assets`) wraps each fragment
//! in the staged `index.html`'s head - which carries the hashed script, style,
//! font pack and fallback references - so every page loads the same wasm and
//! assets from clean URLs like `/components/button/`.

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
        "specimen.html"
    ),
    page!(
        "components",
        "Components",
        "The web components the site is built from, each with its states and variants.",
        "components/index.html"
    ),
    page!(
        "components/palette",
        "Component palette",
        "Every component rendered live, side by side.",
        "components/palette/index.html"
    ),
    page!(
        "components/callout",
        "Callout",
        "Highlighted blocks for notes, tips and warnings.",
        "components/callout/index.html"
    ),
    page!(
        "components/badge",
        "Badge",
        "Compact status markers with variant-coloured dots.",
        "components/badge/index.html"
    ),
    page!(
        "components/card",
        "Card",
        "Titled content containers with optional link headings and footers.",
        "components/card/index.html"
    ),
    page!(
        "components/details",
        "Details",
        "Collapsible sections built on the native details element.",
        "components/details/index.html"
    ),
    page!(
        "components/key-value",
        "Key-value",
        "Label/value pairs for specs and metadata.",
        "components/key-value/index.html"
    ),
    page!(
        "components/table",
        "Table",
        "Tabular data in a scrollable, token-styled frame.",
        "components/table/index.html"
    ),
    page!(
        "components/source",
        "Source",
        "Labelled code blocks with a copy control.",
        "components/source/index.html"
    ),
    page!(
        "components/timeline",
        "Timeline",
        "Dated events on a vertical line with status dots.",
        "components/timeline/index.html"
    ),
    page!(
        "components/empty-state",
        "Empty state",
        "Placeholders for content that does not exist yet.",
        "components/empty-state/index.html"
    ),
    page!(
        "components/button",
        "Button",
        "Action buttons with primary and danger weights.",
        "components/button/index.html"
    ),
    page!(
        "components/input",
        "Input",
        "Labelled text inputs and textareas.",
        "components/input/index.html"
    ),
    page!(
        "components/select",
        "Select",
        "Labelled native selects styled with the theme tokens.",
        "components/select/index.html"
    ),
    page!(
        "components/checkbox",
        "Checkbox",
        "Native checkboxes with their labels.",
        "components/checkbox/index.html"
    ),
    page!(
        "components/switch",
        "Switch",
        "Native checkboxes drawn as switches.",
        "components/switch/index.html"
    ),
    page!(
        "components/tabs",
        "Tabs",
        "Tabbed regions with keyboard activation.",
        "components/tabs/index.html"
    ),
    page!(
        "components/breadcrumb",
        "Breadcrumb",
        "Where-am-I trails with stylesheet separators.",
        "components/breadcrumb/index.html"
    ),
    page!(
        "components/pagination",
        "Pagination",
        "Page navigation for split lists.",
        "components/pagination/index.html"
    ),
    page!(
        "components/tooltip",
        "Tooltip",
        "Hover and focus annotations for terms.",
        "components/tooltip/index.html"
    ),
    page!(
        "components/scene",
        "Scene",
        "Embedded A-Frame scenes with Rust-driven behaviour.",
        "components/scene/index.html"
    ),
    page!(
        "components/kpi",
        "KPI",
        "Single measurements: big value, small label, status colour.",
        "components/kpi/index.html"
    ),
    page!(
        "components/steps",
        "Steps",
        "Numbered procedures with done, current and error states.",
        "components/steps/index.html"
    ),
    page!(
        "components/tree",
        "Tree",
        "Collapsible hierarchies on native details elements.",
        "components/tree/index.html"
    ),
    page!(
        "components/chip",
        "Chip",
        "Interactive filter tokens: toggleable and removable.",
        "components/chip/index.html"
    ),
    page!(
        "components/progress",
        "Progress",
        "Determinate and indeterminate progress bars.",
        "components/progress/index.html"
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
    format!("<op-site-nav><ul>{items}</ul></op-site-nav>")
}

/// The home page's navigation: Home only. The section indexes
/// (components, specimen) are deliberately not linked from the main
/// page; they remain reachable by URL and from the generated pages,
/// which use the full [`nav_markup`].
pub fn home_nav_markup() -> String {
    "<op-site-nav><ul><li><a href=\"/\">Home</a></li></ul></op-site-nav>".to_string()
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
        let start = home.find("<op-site-nav>").expect("home nav");
        let end = home.find("</op-site-nav>").expect("home nav end") + "</op-site-nav>".len();
        assert_eq!(
            &home[start..end],
            nav,
            "home nav drifted from op_pages::home_nav_markup()"
        );
    }

    #[test]
    fn nav_links_cover_home_and_every_top_level_section() {
        let nav = nav_markup();
        assert!(nav.starts_with("<op-site-nav>") && nav.ends_with("</op-site-nav>"));
        for href in ["\"/\"", "\"/components/\"", "\"/specimen/\""] {
            assert!(nav.contains(href), "nav lacks {href}");
        }
    }
}
