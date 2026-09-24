//! The full site index at `/site-index/`: every page the site serves, as a
//! tree of its URL paths drawn with `opt-tree`. It is reached from the small
//! π in the footer rather than from the navigation, so pages hidden from the
//! nav stay findable. Built from the same registry the page build emits
//! from, so it cannot miss a page or list one that is not there.

use std::collections::BTreeMap;

use crate::caniuse::xml_escape as esc;
use crate::{GeneratedPage, OPT_NS, PAGES, SECTIONS};

pub const SLUG: &str = "site-index";
const TITLE: &str = "Site index";
const DESCRIPTION: &str = "Every page of openpower.tools, including those not in the navigation.";

/// One path segment. `page` is the title of the page at this path, if there
/// is one; a segment such as `component/` has children but no page.
#[derive(Default)]
struct Node {
    page: Option<String>,
    children: BTreeMap<String, Node>,
}

impl Node {
    fn insert(&mut self, slug: &str, title: &str) {
        let node = slug
            .split('/')
            .filter(|s| !s.is_empty())
            .fold(self, |node, segment| {
                node.children.entry(segment.to_owned()).or_default()
            });
        node.page = Some(title.to_owned());
    }

    /// This node as a list item: its label, then its children as a nested
    /// list inside an open disclosure when it has any.
    fn render(&self, href: &str, segment: &str, out: &mut String) {
        let mut label = format!("<code>{}</code>", esc(segment));
        if let Some(title) = &self.page {
            label.push_str(&format!(" <a href=\"{}\">{}</a>", esc(href), esc(title)));
        }
        if self.children.is_empty() {
            out.push_str(&format!("<li>{label}</li>\n"));
            return;
        }
        out.push_str(&format!(
            "<li><details open=\"\"><summary>{label}</summary>\n<ul>\n"
        ));
        for (name, child) in &self.children {
            child.render(&format!("{href}{name}/"), &format!("{name}/"), out);
        }
        out.push_str("</ul>\n</details></li>\n");
    }
}

/// The index page: the home page at the root, then every registered page,
/// the pages in `others` (the data-generated ones) and the index itself,
/// each under its path.
pub fn page(others: &[GeneratedPage]) -> GeneratedPage {
    let mut root = Node {
        page: Some(SECTIONS[0].label.to_owned()),
        ..Node::default()
    };
    for p in PAGES {
        root.insert(p.slug, p.title);
    }
    for p in others {
        root.insert(&p.slug, &p.title);
    }
    root.insert(SLUG, TITLE);

    let mut tree = String::new();
    root.render("/", "/", &mut tree);
    GeneratedPage {
        slug: SLUG.to_owned(),
        title: TITLE.to_owned(),
        description: DESCRIPTION.to_owned(),
        body_xml: format!(
            "<opt:body xmlns:opt=\"{OPT_NS}\">\n<opt:tree>\n<ul>\n{tree}</ul>\n</opt:tree>\n</opt:body>\n"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every link in `body` with the depth of list it sits in (1 = the
    /// outermost list).
    fn links_with_depth(body: &str) -> Vec<(String, usize)> {
        let mut out = Vec::new();
        let mut depth = 0usize;
        let mut rest = body;
        while let Some(i) = rest.find('<') {
            rest = &rest[i..];
            if rest.starts_with("<ul>") {
                depth += 1;
            } else if rest.starts_with("</ul>") {
                depth -= 1;
            } else if let Some(tail) = rest.strip_prefix("<a href=\"") {
                let href = &tail[..tail.find('"').expect("closing quote")];
                out.push((href.to_owned(), depth));
            }
            rest = &rest[1..];
        }
        out
    }

    fn index() -> GeneratedPage {
        crate::generated_pages()
            .into_iter()
            .find(|p| p.slug == SLUG)
            .expect("the site index is a generated page")
    }

    /// The index lists exactly the pages the build emits (every registered
    /// page, every generated page including itself) plus the home page, each
    /// once.
    #[test]
    fn lists_every_emitted_page_once() {
        let mut listed: Vec<String> = links_with_depth(&index().body_xml)
            .into_iter()
            .map(|(href, _)| href)
            .collect();
        listed.sort();
        let mut expected: Vec<String> = std::iter::once("/".to_owned())
            .chain(PAGES.iter().map(|p| format!("/{}/", p.slug)))
            .chain(
                crate::generated_pages()
                    .iter()
                    .map(|p| format!("/{}/", p.slug)),
            )
            .collect();
        expected.sort();
        assert_eq!(listed, expected);
    }

    /// Each page sits as deep in the tree as its path is long: the home page
    /// at the root, `/projects/` one level in, `/component/badge/` two.
    #[test]
    fn each_page_sits_at_the_depth_of_its_path() {
        let links = links_with_depth(&index().body_xml);
        assert!(links.len() > 10, "the index looks empty: {links:?}");
        for (href, depth) in links {
            let segments = href.split('/').filter(|s| !s.is_empty()).count();
            assert_eq!(depth, segments + 1, "{href} is at list depth {depth}");
        }
    }

    /// A branch with no page of its own (`component/`) is listed without a
    /// link, and its pages sit beneath it.
    #[test]
    fn pageless_segments_are_unlinked_branches() {
        let body = index().body_xml;
        assert!(
            body.contains("<summary><code>component/</code></summary>"),
            "{body}"
        );
        assert!(!body.contains("href=\"/component/\""), "{body}");
    }

    #[test]
    fn index_lowers_as_a_tree_and_escapes_its_text() {
        let others = vec![GeneratedPage {
            slug: "x".to_owned(),
            title: "a < b & c".to_owned(),
            description: String::new(),
            body_xml: String::new(),
        }];
        let html = crate::lower(&page(&others).body_xml).expect("the index lowers");
        assert!(html.contains("<opt-tree>"), "{html}");
        assert!(html.contains(">a &lt; b &amp; c</a>"), "{html}");
    }
}
