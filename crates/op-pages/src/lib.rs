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

pub mod caniuse;
pub use caniuse::{GeneratedPage, generated_pages};
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
        "projects",
        "Projects",
        "Projects under the openpower.tools umbrella; each lives at /project/{name}/.",
        "projects/index.xml"
    ),
    page!(
        "project/openpower-tools",
        "openpower-tools",
        "The umbrella project: this site, its component vocabulary, and the supporting tooling.",
        "project/openpower-tools/index.xml"
    ),
    page!(
        "project/openpower-tools/status",
        "openpower-tools status",
        "Aggregated build and distribution status for openpower-tools and related OpenPOWER work.",
        "project/openpower-tools/status/index.xml"
    ),
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

/// One top-level section of the site.
pub struct Section {
    /// Absolute href of the section index (trailing slash).
    pub href: &'static str,
    /// Short name used in the navigation and on the home index.
    pub label: &'static str,
    /// One-line summary, shown on the home index; kept identical to the
    /// section index page's own description (a test enforces it).
    pub description: &'static str,
    /// Whether the section appears in the shared navigation and, by
    /// consequence, in the home index listing. Sections opt out
    /// individually without overriding the navigation as a whole; they
    /// stay reachable by URL and from in-page links.
    pub in_nav: bool,
}

/// Every top-level section, nav-visible or not.
pub const SECTIONS: &[Section] = &[
    Section {
        href: "/",
        label: "Home",
        description: "Community-driven support for OpenPOWER / Talos II firmware and software ports.",
        in_nav: true,
    },
    Section {
        href: "/can-i-use/",
        label: "Can I use",
        description: "Evidence-backed support matrix for software on POWER, with live capability probes of your own browser.",
        in_nav: true,
    },
    Section {
        href: "/projects/",
        label: "Projects",
        description: "Projects under the openpower.tools umbrella; each lives at /project/{name}/.",
        in_nav: true,
    },
    Section {
        href: "/components/",
        label: "Components",
        description: "The web components the site is built from, each with its states and variants.",
        in_nav: false,
    },
    Section {
        href: "/specimen/",
        label: "Specimen",
        description: "Every design token with its value and WCAG contrast, the site's elements in both themes, and the typography tiers.",
        in_nav: false,
    },
];

/// The shared site navigation: every nav-visible section, in registry
/// order. Used verbatim on generated pages and mirrored by the home
/// page (a test keeps them identical).
pub fn nav_markup() -> String {
    let mut items = String::new();
    for section in SECTIONS.iter().filter(|s| s.in_nav) {
        items.push_str(&format!(
            "<li><a href=\"{}\">{}</a></li>",
            section.href, section.label
        ));
    }
    format!("<opt-site-nav><ul>{items}</ul></opt-site-nav>")
}

/// The home index's section listing: a card per nav-visible section
/// other than home itself, carrying its one-line description. Mirrored
/// verbatim by `index.html` (a test keeps them identical).
pub fn home_sections_markup() -> String {
    let mut cards = String::new();
    for section in SECTIONS.iter().filter(|s| s.in_nav && s.href != "/") {
        cards.push_str(&format!(
            "<opt-card heading=\"{}\" href=\"{}\"><p>{}</p></opt-card>",
            section.label, section.href, section.description
        ));
    }
    format!("<div class=\"op-gallery\">{cards}</div>")
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
    fn nav_lists_exactly_the_nav_visible_sections() {
        let nav = nav_markup();
        assert!(nav.starts_with("<opt-site-nav>") && nav.ends_with("</opt-site-nav>"));
        for section in SECTIONS {
            let href = format!("\"{}\"", section.href);
            assert_eq!(
                nav.contains(&href),
                section.in_nav,
                "{} has in_nav={} but the nav is {nav}",
                section.href,
                section.in_nav
            );
        }
    }

    #[test]
    fn home_page_nav_matches_the_generated_nav() {
        let home = include_str!("../../../index.html");
        let start = home.find("<opt-site-nav>").expect("home nav");
        let end = home.find("</opt-site-nav>").expect("home nav end") + "</opt-site-nav>".len();
        assert_eq!(
            &home[start..end],
            nav_markup(),
            "home nav drifted from op_pages::nav_markup()"
        );
    }

    #[test]
    fn home_page_lists_the_nav_visible_sections_and_only_those() {
        let home = include_str!("../../../index.html");
        assert!(
            home.contains(&home_sections_markup()),
            "home index drifted from op_pages::home_sections_markup()"
        );
        for section in SECTIONS.iter().filter(|s| !s.in_nav) {
            let link = format!("href=\"{}\"", section.href);
            assert!(
                !home.contains(&link),
                "{} is hidden from the nav but linked from the home page",
                section.href
            );
        }
    }

    #[test]
    fn sections_describe_their_index_pages_and_have_them() {
        let generated = generated_pages();
        for section in SECTIONS.iter().filter(|s| s.href != "/") {
            let slug = section.href.trim_matches('/');
            let description = PAGES
                .iter()
                .find(|p| p.slug == slug)
                .map(|p| p.description.to_owned())
                .or_else(|| {
                    generated
                        .iter()
                        .find(|p| p.slug == slug)
                        .map(|p| p.description.clone())
                })
                .unwrap_or_else(|| panic!("section {} has no index page", section.href));
            assert_eq!(
                description, section.description,
                "section {} description drifted from its index page",
                section.href
            );
        }
    }
}
