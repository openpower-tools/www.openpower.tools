//! `<op-build-status>`: what is running, straight from the wasm module.

use op_webc::{CustomElement, ElementDefinition};
use web_sys::HtmlElement;

use super::{BASE_CSS, DEFINITIONS, shadow_root};

pub const DEFINITION: ElementDefinition = ElementDefinition {
    tag: "op-build-status",
    observed_attributes: &[],
    create: |host| Box::new(BuildStatus { host }),
};

struct BuildStatus {
    host: HtmlElement,
}

/// One line describing this build: crate version, target architecture and the
/// number of custom elements the module defines.
pub fn status_line() -> String {
    format!(
        "op-site {} ({}), {} custom elements defined in Rust",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::ARCH,
        DEFINITIONS.len()
    )
}

impl CustomElement for BuildStatus {
    fn connected(&mut self) {
        shadow_root(&self.host).set_inner_html(&format!(
            "<style>{BASE_CSS}</style><h2>Status</h2><p><code>{}</code></p>",
            status_line()
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_line_reports_version_arch_and_element_count() {
        let line = status_line();
        assert!(
            line.starts_with(&format!("op-site {} (", env!("CARGO_PKG_VERSION"))),
            "{line}"
        );
        assert!(
            line.contains(&format!("({})", std::env::consts::ARCH)),
            "{line}"
        );
        assert!(
            line.ends_with(&format!(
                "{} custom elements defined in Rust",
                DEFINITIONS.len()
            )),
            "{line}"
        );
        assert!(
            DEFINITIONS.len() >= 5,
            "expected the five site elements, found {}",
            DEFINITIONS.len()
        );
    }

    #[test]
    fn every_definition_has_a_unique_hyphenated_tag() {
        let mut tags: Vec<&str> = DEFINITIONS.iter().map(|d| d.tag).collect();
        for tag in &tags {
            assert!(tag.contains('-') && tag.starts_with("op-"), "bad tag {tag}");
        }
        tags.sort_unstable();
        tags.dedup();
        assert_eq!(tags.len(), DEFINITIONS.len(), "duplicate tag names");
    }

    #[test]
    fn pages_use_every_defined_element_and_no_undefined_op_tags() {
        let mut pages: Vec<(String, String)> = vec![(
            "index.html".to_owned(),
            include_str!("../../../../index.html").to_owned(),
        )];
        for page in op_pages::PAGES {
            pages.push((page.slug.to_owned(), page.body.to_owned()));
        }
        pages.push(("nav".to_owned(), op_pages::nav_markup()));
        for definition in DEFINITIONS {
            assert!(
                pages
                    .iter()
                    .any(|(_, html)| html.contains(&format!("<{}", definition.tag))),
                "no page uses <{}>",
                definition.tag
            );
        }
        for (page, html) in &pages {
            for tag in html
                .split('<')
                .filter_map(|s| s.strip_prefix("op-"))
                .map(|s| {
                    let end = s
                        .find(|c: char| c.is_whitespace() || c == '>')
                        .unwrap_or(s.len());
                    format!("op-{}", &s[..end])
                })
            {
                assert!(
                    DEFINITIONS.iter().any(|d| d.tag == tag),
                    "{page} uses undefined <{tag}>"
                );
            }
        }
    }
}
