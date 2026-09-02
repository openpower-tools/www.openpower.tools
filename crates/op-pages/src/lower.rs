//! Authoring-time XML namespace, lowered at build time.
//!
//! Page sources are well-formed XML whose component vocabulary lives in
//! the `opt` namespace (`xmlns:opt` = [`OPT_NS`]). This module resolves
//! namespaces properly (the prefix is rebindable, as XML intends),
//! validates every `opt` element and attribute against the vocabulary
//! that `op-site` actually defines, and lowers `opt:name` to the
//! `<opt-name>` custom elements browsers require (custom element names
//! cannot contain a colon, and elements outside the HTML namespace never
//! upgrade -- hence authoring-time namespace, build-time lowering, with
//! checking instead of mysterious breakage).

use quick_xml::events::Event;
use quick_xml::name::ResolveResult;
use quick_xml::reader::NsReader;
use std::collections::BTreeMap;

pub const OPT_NS: &str = "https://www.openpower.tools/ns/opt";

/// HTML void elements: serialised without closing tags.
const VOID: &[&str] = &[
    "br", "hr", "img", "input", "meta", "link", "wbr", "col", "embed",
];

/// Globally acceptable attributes on `opt` elements, beyond each
/// element's own list.
const GLOBAL_ATTRS: &[&str] = &[
    "class", "id", "slot", "style", "title", "role", "hidden", "tabindex", "lang", "dir",
];

/// Content attributes accepted by elements beyond their observed
/// (reactive) attributes. Kept explicit so validation stays honest.
const EXTRA_ATTRS: &[(&str, &[&str])] = &[
    // Read once at connect time rather than observed for reactivity.
    ("table", &["dense", "lined"]),
    ("tab", &["label"]),
];

fn vocabulary() -> BTreeMap<String, Vec<String>> {
    let mut map = BTreeMap::new();
    for def in op_site::components::DEFINITIONS {
        let local = def
            .tag
            .strip_prefix("opt-")
            .expect("component tags carry the opt- prefix")
            .to_string();
        let mut attrs: Vec<String> = def
            .observed_attributes
            .iter()
            .map(|a| a.to_string())
            .collect();
        for (tag, extra) in EXTRA_ATTRS {
            if *tag == local {
                attrs.extend(extra.iter().map(|a| a.to_string()));
            }
        }
        attrs.extend(GLOBAL_ATTRS.iter().map(|a| a.to_string()));
        map.insert(local, attrs);
    }
    map
}

fn attr_allowed(allowed: &[String], name: &str) -> bool {
    name.starts_with("data-") || name.starts_with("aria-") || allowed.iter().any(|a| a == name)
}

fn escape_text(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Lower one page source. Returns the HTML fragment, or every violation.
pub fn lower(source: &str) -> Result<String, Vec<String>> {
    let vocab = vocabulary();
    let mut reader = NsReader::from_str(source);
    reader.config_mut().trim_text(false);
    let mut out = String::with_capacity(source.len());
    let mut errors: Vec<String> = Vec::new();
    let mut root_skipped = false;

    loop {
        match reader.read_resolved_event() {
            Err(e) => {
                errors.push(format!("XML parse error: {e}"));
                break;
            }
            Ok((resolve, event)) => {
                let in_opt = matches!(&resolve, ResolveResult::Bound(ns) if ns.as_ref() == OPT_NS.as_bytes());
                match event {
                    Event::Start(ref e) | Event::Empty(ref e) => {
                        let local = String::from_utf8_lossy(e.local_name().as_ref()).into_owned();
                        let empty = matches!(event, Event::Empty(_));
                        if in_opt && local == "body" && !root_skipped {
                            root_skipped = true;
                            continue;
                        }
                        let tag = if in_opt {
                            match vocab.get(&local) {
                                Some(allowed) => {
                                    for attr in e.attributes().flatten() {
                                        let key =
                                            String::from_utf8_lossy(attr.key.as_ref()).into_owned();
                                        if key.starts_with("xmlns") {
                                            continue;
                                        }
                                        if !attr_allowed(allowed, &key) {
                                            errors.push(format!(
                                                "<opt:{local}> does not accept attribute \"{key}\" (allowed: {})",
                                                allowed.join(", ")
                                            ));
                                        }
                                    }
                                    format!("opt-{local}")
                                }
                                None => {
                                    errors.push(format!(
                                        "unknown element <opt:{local}> (not in the opt vocabulary)"
                                    ));
                                    format!("opt-{local}")
                                }
                            }
                        } else {
                            local.clone()
                        };
                        out.push('<');
                        out.push_str(&tag);
                        for attr in e.attributes().flatten() {
                            let key = String::from_utf8_lossy(attr.key.as_ref());
                            if key.starts_with("xmlns") {
                                continue;
                            }
                            let value = String::from_utf8_lossy(&attr.value);
                            out.push(' ');
                            out.push_str(&key);
                            out.push_str("=\"");
                            out.push_str(&value);
                            out.push('"');
                        }
                        out.push('>');
                        if empty && !(!in_opt && VOID.contains(&local.as_str())) {
                            out.push_str(&format!("</{tag}>"));
                        }
                    }
                    Event::End(ref e) => {
                        let local = String::from_utf8_lossy(e.local_name().as_ref()).into_owned();
                        if in_opt && local == "body" {
                            continue;
                        }
                        let tag = if in_opt {
                            format!("opt-{local}")
                        } else {
                            local
                        };
                        out.push_str(&format!("</{tag}>"));
                    }
                    Event::Text(t) => match t.unescape() {
                        Ok(text) => out.push_str(&escape_text(&text)),
                        Err(e) => errors.push(format!("text decode error: {e}")),
                    },
                    Event::Comment(c) => {
                        out.push_str("<!--");
                        out.push_str(&String::from_utf8_lossy(c.as_ref()));
                        out.push_str("-->");
                    }
                    Event::CData(c) => {
                        out.push_str(&escape_text(&String::from_utf8_lossy(c.as_ref())))
                    }
                    Event::Decl(_) | Event::PI(_) | Event::DocType(_) => {}
                    Event::Eof => break,
                }
            }
        }
    }
    if errors.is_empty() {
        Ok(out)
    } else {
        Err(errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lowers_namespace_elements_and_passthrough_html() {
        let src = r#"<opt:body xmlns:opt="https://www.openpower.tools/ns/opt">
<p>plain</p><opt:badge variant="ok">verified</opt:badge><br/>
</opt:body>"#;
        let html = lower(src).expect("valid");
        assert!(html.contains("<p>plain</p>"));
        assert!(html.contains("<opt-badge variant=\"ok\">verified</opt-badge>"));
        assert!(html.contains("<br>"));
        assert!(!html.contains("opt:badge"));
        assert!(!html.contains("xmlns"));
    }

    #[test]
    fn prefix_is_rebindable_as_xml_intends() {
        let src = r#"<x:body xmlns:x="https://www.openpower.tools/ns/opt"><x:badge variant="ok">y</x:badge></x:body>"#;
        let html = lower(src).expect("valid");
        assert!(html.contains("<opt-badge variant=\"ok\">y</opt-badge>"));
    }

    #[test]
    fn unknown_elements_and_attributes_are_rejected() {
        let src = r#"<opt:body xmlns:opt="https://www.openpower.tools/ns/opt"><opt:nonsense/><opt:badge bogus="1">x</opt:badge></opt:body>"#;
        let errs = lower(src).expect_err("must fail");
        assert!(
            errs.iter()
                .any(|e| e.contains("unknown element <opt:nonsense>"))
        );
        assert!(
            errs.iter()
                .any(|e| e.contains("<opt:badge> does not accept attribute \"bogus\""))
        );
    }

    #[test]
    fn empty_component_elements_get_closing_tags() {
        let src =
            r#"<opt:body xmlns:opt="https://www.openpower.tools/ns/opt"><opt:scene/></opt:body>"#;
        let html = lower(src).expect("valid");
        assert_eq!(html.trim(), "<opt-scene></opt-scene>");
    }

    #[test]
    fn every_registered_page_lowers_cleanly() {
        for page in crate::PAGES {
            if let Err(errors) = lower(page.body) {
                panic!(
                    "page {} failed validation:\n  {}",
                    page.slug,
                    errors.join("\n  ")
                );
            }
        }
    }
}
