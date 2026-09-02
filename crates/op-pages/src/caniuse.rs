//! The can-i-use matrix: structured records rendered through the same
//! validated XML pipeline as the hand-written pages.
//!
//! One TOML record per item under `data/can-i-use/`, community-editable
//! by pull request. Every record carries evidence links and a
//! last-verified date; the generator refuses unknown statuses, and the
//! registry test refuses unregistered files, so the matrix cannot drift
//! silently. The generated page body is namespaced XML fed through
//! [`crate::lower`], which validates every element and attribute against
//! the live component vocabulary - the site eats its own dogfood.

use serde::Deserialize;

/// Registered records. include_str keeps the build hermetic; the test
/// below insists this list matches the data directory exactly.
const RAW: &[(&str, &str)] = &[
    ("box64", include_str!("../../../data/can-i-use/box64.toml")),
    (
        "chromium",
        include_str!("../../../data/can-i-use/chromium.toml"),
    ),
    (
        "firefox-jit",
        include_str!("../../../data/can-i-use/firefox-jit.toml"),
    ),
    (
        "freebsd",
        include_str!("../../../data/can-i-use/freebsd.toml"),
    ),
    (
        "gcc-llvm",
        include_str!("../../../data/can-i-use/gcc-llvm.toml"),
    ),
    ("kvm", include_str!("../../../data/can-i-use/kvm.toml")),
    (
        "qemu-user",
        include_str!("../../../data/can-i-use/qemu-user.toml"),
    ),
    ("rust", include_str!("../../../data/can-i-use/rust.toml")),
    (
        "ut2004",
        include_str!("../../../data/can-i-use/ut2004.toml"),
    ),
    (
        "webkitgtk",
        include_str!("../../../data/can-i-use/webkitgtk.toml"),
    ),
    (
        "wine-hangover",
        include_str!("../../../data/can-i-use/wine-hangover.toml"),
    ),
];

pub const STATUSES: &[(&str, &str, &str)] = &[
    // (status, badge variant, meaning)
    (
        "upstream",
        "ok",
        "works from the project or distribution itself",
    ),
    (
        "patched",
        "info",
        "works via maintained downstream patches or packages",
    ),
    (
        "in-progress",
        "warning",
        "active effort; usable with caveats or not yet usable",
    ),
    ("broken", "danger", "currently broken"),
    ("unsupported", "danger", "no support and none planned"),
    ("unknown", "neutral", "not yet verified - help wanted"),
];

#[derive(Deserialize)]
pub struct Item {
    pub id: String,
    pub title: String,
    pub category: String,
    pub status: String,
    pub headline: String,
    #[serde(default)]
    pub page_size: Option<String>,
    pub last_verified: String,
    #[serde(default)]
    pub channels: Vec<Channel>,
    #[serde(default)]
    pub evidence: Vec<Evidence>,
    #[serde(default)]
    pub notes: Option<String>,
}

#[derive(Deserialize)]
pub struct Channel {
    pub name: String,
    pub status: String,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Deserialize)]
pub struct Evidence {
    pub title: String,
    pub url: String,
}

/// A page produced from data rather than a source file.
pub struct GeneratedPage {
    pub slug: String,
    pub title: String,
    pub description: String,
    /// Namespaced XML, to be passed through [`crate::lower`].
    pub body_xml: String,
}

pub fn badge_variant(status: &str) -> &'static str {
    STATUSES
        .iter()
        .find(|(s, _, _)| *s == status)
        .map(|(_, v, _)| *v)
        .unwrap_or("neutral")
}

pub fn items() -> Vec<Item> {
    RAW.iter()
        .map(|(id, raw)| {
            let item: Item = toml::from_str(raw)
                .unwrap_or_else(|e| panic!("data/can-i-use/{id}.toml does not parse: {e}"));
            assert_eq!(&item.id, id, "record id must match its file name ({id})");
            assert!(
                STATUSES.iter().any(|(s, _, _)| *s == item.status),
                "{id}: unknown status {:?}",
                item.status
            );
            for channel in &item.channels {
                assert!(
                    STATUSES.iter().any(|(s, _, _)| *s == channel.status),
                    "{id}: unknown channel status {:?}",
                    channel.status
                );
            }
            assert!(
                !item.evidence.is_empty(),
                "{id}: records need evidence links"
            );
            item
        })
        .collect()
}

fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn badge(status: &str) -> String {
    format!(
        "<opt:badge variant=\"{}\">{}</opt:badge>",
        badge_variant(status),
        xml_escape(status)
    )
}

/// Build the matrix page as namespaced XML.
pub fn page_xml() -> String {
    let mut items = items();
    items.sort_by(|a, b| {
        (a.category.clone(), a.title.clone()).cmp(&(b.category.clone(), b.title.clone()))
    });

    let mut body = String::new();
    body.push_str("<opt:body xmlns:opt=\"https://www.openpower.tools/ns/opt\">\n");
    body.push_str(
        "<p>Can I use it on POWER? Answered with evidence, not vibes: every row \
carries links and a last-verified date, and the whole table is community-editable \
as one TOML file per row. The panel below is measured live in <em>your</em> \
browser on <em>your</em> machine - this site runs as WebAssembly, so it can \
ask rather than guess.</p>\n",
    );
    body.push_str("<h2>Your machine, right now</h2>\n<opt:machine-probes></opt:machine-probes>\n");
    body.push_str("<h2>The matrix</h2>\n<p>");
    for (status, _, meaning) in STATUSES {
        body.push_str(&badge(status));
        body.push_str(&format!(" {} &#160; ", xml_escape(meaning)));
    }
    body.push_str("</p>\n");

    let mut current_category = String::new();
    for item in &items {
        if item.category != current_category {
            if !current_category.is_empty() {
                body.push_str("</tbody></table></opt:table>\n");
            }
            current_category = item.category.clone();
            body.push_str(&format!("<h3>{}</h3>\n", xml_escape(&current_category)));
            body.push_str(
                "<opt:table lined=\"\"><table><thead><tr><th>What</th><th>Status</th>\
<th>Where</th><th>Page size</th><th>Evidence</th><th>Verified</th></tr></thead><tbody>\n",
            );
        }
        let channels = if item.channels.is_empty() {
            "&#8212;".to_owned()
        } else {
            item.channels
                .iter()
                .map(|c| {
                    let note = c
                        .note
                        .as_deref()
                        .map(|n| format!(" &#8212; {}", xml_escape(n)))
                        .unwrap_or_default();
                    format!("{} {}{note}", badge(&c.status), xml_escape(&c.name))
                })
                .collect::<Vec<_>>()
                .join("<br/>")
        };
        let evidence = item
            .evidence
            .iter()
            .map(|e| {
                format!(
                    "<a href=\"{}\">{}</a>",
                    xml_escape(&e.url),
                    xml_escape(&e.title)
                )
            })
            .collect::<Vec<_>>()
            .join("<br/>");
        let page_size = item
            .page_size
            .as_deref()
            .map(xml_escape)
            .unwrap_or_else(|| "&#8212;".to_owned());
        let notes = item
            .notes
            .as_deref()
            .map(|n| format!("<br/><small>{}</small>", xml_escape(n)))
            .unwrap_or_default();
        body.push_str(&format!(
            "<tr><td>{title}<br/><small>{headline}</small>{notes}</td><td>{status}</td>\
<td>{channels}</td><td>{page_size}</td><td>{evidence}</td><td>{verified}</td></tr>\n",
            title = xml_escape(&item.title),
            headline = xml_escape(&item.headline),
            status = badge(&item.status),
            verified = xml_escape(&item.last_verified),
        ));
    }
    if !current_category.is_empty() {
        body.push_str("</tbody></table></opt:table>\n");
    }
    body.push_str(
        "<p>Add or correct a row: edit <a href=\"https://github.com/openpower-tools/\
www.openpower.tools/tree/main/data/can-i-use\">data/can-i-use/</a> and open a pull \
request; unknown statuses and missing evidence fail the build.</p>\n",
    );
    body.push_str("</opt:body>\n");
    body
}

pub fn generated_pages() -> Vec<GeneratedPage> {
    vec![GeneratedPage {
        slug: "can-i-use".to_owned(),
        title: "Can I use it on POWER?".to_owned(),
        description:
            "Evidence-backed support matrix for software on POWER, with live capability probes of your own browser."
                .to_owned(),
        body_xml: page_xml(),
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_matches_the_data_directory() {
        let dir =
            std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/can-i-use"));
        let mut on_disk: Vec<String> = std::fs::read_dir(dir)
            .expect("data dir")
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                e.file_name()
                    .to_str()
                    .and_then(|n| n.strip_suffix(".toml").map(str::to_owned))
            })
            .collect();
        on_disk.sort();
        let mut registered: Vec<String> = RAW.iter().map(|(id, _)| id.to_string()).collect();
        registered.sort();
        assert_eq!(on_disk, registered, "register every record in caniuse::RAW");
    }

    #[test]
    fn records_parse_and_the_page_validates() {
        let n = items().len();
        assert!(n >= 10, "matrix should not shrink silently ({n})");
        let xml = page_xml();
        let html = crate::lower(&xml)
            .unwrap_or_else(|e| panic!("matrix fails validation:\n  {}", e.join("\n  ")));
        assert!(html.contains("<opt-machine-probes>"));
        assert!(html.contains("<opt-badge"));
        assert!(!html.contains("opt:"));
    }

    #[test]
    fn statuses_map_to_badge_variants() {
        for (status, variant, _) in STATUSES {
            assert_eq!(badge_variant(status), *variant);
        }
        assert_eq!(badge_variant("nonsense"), "neutral");
    }
}
