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
//!
//! One element is more than renamed: `<opt:chart>` is held back until its
//! end tag so its data block can be pre-rendered into a declarative shadow
//! root, which is why the loop below carries a chart buffer.

use op_site::components::chart::{DEFAULT_RATIO, DEFAULT_WIDTH, prerender};
use quick_xml::events::{BytesStart, Event};
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
    ("film", &["sheet", "title", "id", "video"]),
    ("machine", &["for"]),
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

/// An attribute value, escaped for the double quotes it is written in. The
/// ampersand goes first, or the escapes would be escaped in turn; the
/// angle brackets are not strictly required inside a value, and are written
/// anyway so that the four entities XML predefines all make the round trip
/// as themselves.
fn escape_attr(value: &str) -> String {
    escape_text(value).replace('"', "&quot;")
}

/// Reports every attribute of `e` the element does not accept. `reserved`
/// names attributes the lowering emits itself, which an author may not
/// write even when the general rules would let them through.
fn check_attrs(
    local: &str,
    allowed: &[String],
    reserved: &[&str],
    e: &BytesStart,
    errors: &mut Vec<String>,
) {
    for attr in e.attributes().flatten() {
        let key = String::from_utf8_lossy(attr.key.as_ref()).into_owned();
        if key.starts_with("xmlns") {
            continue;
        }
        if reserved.contains(&key.as_str()) || !attr_allowed(allowed, &key) {
            errors.push(format!(
                "<opt:{local}> does not accept attribute \"{key}\" (allowed: {})",
                allowed.join(", ")
            ));
        }
    }
}

/// Writes `e`'s attributes as they were authored, namespace declarations
/// excepted: they have done their work by now. The value makes the round
/// trip through its own text, so an XML entity arrives as HTML's spelling
/// of the same character and a value written in single quotes cannot end
/// the double-quoted attribute it lands in.
fn write_attrs(e: &BytesStart, out: &mut String) {
    for attr in e.attributes().flatten() {
        let key = String::from_utf8_lossy(attr.key.as_ref());
        if key.starts_with("xmlns") {
            continue;
        }
        let value = attr
            .unescape_value()
            .unwrap_or_else(|_| String::from_utf8_lossy(&attr.value));
        out.push(' ');
        out.push_str(&key);
        out.push_str("=\"");
        out.push_str(&escape_attr(&value));
        out.push('"');
    }
}

/// An `<opt:chart>` buffered until its end tag: the pre-render needs the
/// data block, and the block is a child, so the element cannot be emitted
/// as it is read.
struct Chart {
    /// The author's attributes, as authored, without `data-hash`.
    attrs: String,
    /// Pre-render geometry, `None` once a bad value has been reported.
    width: Option<f64>,
    ratio: Option<f64>,
    /// Open descendants: zero at the chart's own level, so an end tag
    /// arriving at zero is the chart's own.
    depth: usize,
    /// The raw text of the first `<script type="application/json">` child.
    block: Option<String>,
    /// While that script is open, the depth its end tag returns to.
    reading: Option<usize>,
    /// Every other child, lowered as usual and passed through.
    rest: String,
}

impl Chart {
    /// Reads the pre-render geometry off the start tag and keeps the
    /// element's own attributes for re-emission.
    fn new(e: &BytesStart, errors: &mut Vec<String>) -> Self {
        let mut attrs = String::new();
        write_attrs(e, &mut attrs);
        Self {
            attrs,
            width: number(e, "initial-width", DEFAULT_WIDTH, errors),
            ratio: number(e, "ratio", DEFAULT_RATIO, errors),
            depth: 0,
            block: None,
            reading: None,
            rest: String::new(),
        }
    }
}

/// `name` on `e` as a positive number: `fallback` when the attribute is
/// absent, and `None` with the reason recorded when it is neither.
fn number(e: &BytesStart, name: &str, fallback: f64, errors: &mut Vec<String>) -> Option<f64> {
    let Some(found) = e
        .attributes()
        .flatten()
        .find(|a| a.key.as_ref() == name.as_bytes())
    else {
        return Some(fallback);
    };
    let raw = String::from_utf8_lossy(&found.value).into_owned();
    match raw.trim().parse::<f64>() {
        Ok(n) if n.is_finite() && n > 0.0 => Some(n),
        Ok(_) => {
            errors.push(format!(
                "<opt:chart> needs a positive {name}, not \"{raw}\""
            ));
            None
        }
        Err(reason) => {
            errors.push(format!(
                "<opt:chart> cannot read {name}=\"{raw}\" as a number ({reason})"
            ));
            None
        }
    }
}

/// Whether `e` opens the `<script type="application/json">` a chart's data
/// block lives in.
fn is_json_script(e: &BytesStart) -> bool {
    e.attributes()
        .flatten()
        .any(|a| a.key.as_ref() == b"type" && a.value.as_ref() == b"application/json")
}

/// Where lowered markup goes: into a buffered chart's children, or
/// straight into the page.
fn sink<'a>(chart: &'a mut Option<Chart>, out: &'a mut String) -> &'a mut String {
    match chart {
        Some(c) => &mut c.rest,
        None => out,
    }
}

/// Text inside a chart's data block lands raw (script content is raw text,
/// and the block is hashed exactly as it was written); everywhere else it
/// is escaped.
fn push_text(chart: &mut Option<Chart>, out: &mut String, text: &str) {
    match chart {
        Some(c) if c.reading.is_some() => c.block.get_or_insert_default().push_str(text),
        other => sink(other, out).push_str(&escape_text(text)),
    }
}

/// Emits a buffered chart: the element carrying the pre-render's hash, the
/// declarative shadow root holding the finished chart, then the data block
/// and whatever else the author put inside.
fn finish_chart(chart: Chart, out: &mut String, errors: &mut Vec<String>) {
    let rendered = match (&chart.block, chart.width.zip(chart.ratio)) {
        (Some(block), Some((width, ratio))) => match prerender(block, width, ratio) {
            Ok(pre) => Some(pre),
            Err(reason) => {
                errors.push(format!(
                    "<opt:chart> has an unreadable data block: {reason}"
                ));
                None
            }
        },
        (None, Some(_)) => {
            errors.push(
                "<opt:chart> has no <script type=\"application/json\"> child holding its data"
                    .to_owned(),
            );
            None
        }
        // a bad initial-width or ratio was reported where it was read
        (_, None) => None,
    };
    out.push_str("<opt-chart");
    out.push_str(&chart.attrs);
    match rendered {
        Some(pre) => {
            out.push_str(&format!(" data-hash=\"{}\">", pre.hash));
            out.push_str("<template shadowrootmode=\"open\" shadowrootdelegatesfocus>");
            out.push_str(&pre.shadow);
            out.push_str("</template>");
            out.push_str(&pre.block);
        }
        None => out.push('>'),
    }
    out.push_str(&chart.rest);
    out.push_str("</opt-chart>");
}

/// Lower one page source. Returns the HTML fragment, or every violation.
pub fn lower(source: &str) -> Result<String, Vec<String>> {
    let vocab = vocabulary();
    let mut reader = NsReader::from_str(source);
    reader.config_mut().trim_text(false);
    let mut out = String::with_capacity(source.len());
    let mut errors: Vec<String> = Vec::new();
    let mut root_skipped = false;
    let mut chart: Option<Chart> = None;

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
                        if let Some(c) = chart.as_mut() {
                            // the first application/json script is the chart's data, not content
                            if !in_opt
                                && local == "script"
                                && c.block.is_none()
                                && is_json_script(e)
                            {
                                c.block = Some(String::new());
                                if !empty {
                                    c.reading = Some(c.depth);
                                    c.depth += 1;
                                }
                                continue;
                            }
                            if !empty {
                                c.depth += 1;
                            }
                        } else if in_opt
                            && local == "chart"
                            && let Some(allowed) = vocab.get(&local)
                        {
                            check_attrs(&local, allowed, &["data-hash"], e, &mut errors);
                            let buffered = Chart::new(e, &mut errors);
                            if empty {
                                finish_chart(buffered, &mut out, &mut errors);
                            } else {
                                chart = Some(buffered);
                            }
                            continue;
                        }
                        let tag = if in_opt {
                            match vocab.get(&local) {
                                Some(allowed) => {
                                    check_attrs(&local, allowed, &[], e, &mut errors);
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
                        let target = sink(&mut chart, &mut out);
                        target.push('<');
                        target.push_str(&tag);
                        write_attrs(e, target);
                        target.push('>');
                        if empty && !(!in_opt && VOID.contains(&local.as_str())) {
                            target.push_str(&format!("</{tag}>"));
                        }
                    }
                    Event::End(ref e) => {
                        let local = String::from_utf8_lossy(e.local_name().as_ref()).into_owned();
                        if in_opt && local == "body" {
                            continue;
                        }
                        if chart.as_ref().is_some_and(|c| c.depth == 0) {
                            let buffered = chart.take().expect("the chart being buffered");
                            finish_chart(buffered, &mut out, &mut errors);
                            continue;
                        }
                        if let Some(c) = chart.as_mut() {
                            c.depth -= 1;
                            if c.reading == Some(c.depth) {
                                // the data block ends here; it is emitted by the chart
                                c.reading = None;
                                continue;
                            }
                        }
                        let tag = if in_opt {
                            format!("opt-{local}")
                        } else {
                            local
                        };
                        sink(&mut chart, &mut out).push_str(&format!("</{tag}>"));
                    }
                    Event::Text(t) => match t.unescape() {
                        Ok(text) => push_text(&mut chart, &mut out, &text),
                        Err(e) => errors.push(format!("text decode error: {e}")),
                    },
                    Event::Comment(c) => {
                        let target = sink(&mut chart, &mut out);
                        target.push_str("<!--");
                        target.push_str(&String::from_utf8_lossy(c.as_ref()));
                        target.push_str("-->");
                    }
                    Event::CData(c) => {
                        push_text(&mut chart, &mut out, &String::from_utf8_lossy(c.as_ref()))
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

    /// A small valid data block: one series, three samples, a second and a half.
    const BLOCK: &str = r#"{"series": [{"id": "a", "label": "A", "unit": "%"}], "rows": [[0, 0], [0.75, 40], [1.5, 100]], "duration": 1.5}"#;

    /// The same, with the sequence that would close a script element sitting
    /// inside a label.
    const CLOSER_BLOCK: &str = r#"{"series": [{"id": "a", "label": "</script> in a label", "unit": "%"}], "rows": [[0, 0], [1.5, 100]], "duration": 1.5}"#;

    /// Wraps a fragment in the page root the lowering expects.
    fn page(fragment: &str) -> String {
        format!(r#"<opt:body xmlns:opt="https://www.openpower.tools/ns/opt">{fragment}</opt:body>"#)
    }

    /// A chart carrying `block` as its data, with `attrs` beyond `for`.
    fn chart(attrs: &str, block: &str) -> String {
        format!(
            "<opt:chart for=\"f\"{attrs}><script type=\"application/json\">{block}</script></opt:chart>"
        )
    }

    /// The emitted data block, as a browser would hand it to the element.
    fn emitted_block(html: &str) -> &str {
        const OPEN: &str = "<script type=\"application/json\">";
        let start = html.find(OPEN).expect("an emitted data block") + OPEN.len();
        let end = html[start..].find("</script>").expect("the block's end") + start;
        &html[start..end]
    }

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
    fn a_chart_lowers_to_a_declarative_shadow_root_holding_its_pre_render() {
        let html = lower(&page(&chart("", BLOCK))).expect("valid");
        // the literals are the documented defaults, stated here independently
        let pre = prerender(BLOCK, 640.0, 16.0 / 6.0).expect("the fixture block is valid");
        assert!(html.contains(&format!("<opt-chart for=\"f\" data-hash=\"{}\">", pre.hash)));
        assert!(html.contains("<template shadowrootmode=\"open\" shadowrootdelegatesfocus>"));
        assert!(
            html.contains(&pre.shadow),
            "the lowered shadow root is not the pre-render's"
        );
        assert_eq!(
            emitted_block(&html),
            BLOCK,
            "the block must reach the page exactly as written"
        );
        assert_eq!(op_chart::hash_hex(emitted_block(&html)), pre.hash);
        assert!(html.trim_end().ends_with("</opt-chart>"));
    }

    #[test]
    fn a_block_that_closes_a_script_is_escaped_and_hashes_as_emitted() {
        let html = lower(&page(&chart("", &escape_text(CLOSER_BLOCK)))).expect("valid");
        let emitted = emitted_block(&html);
        assert!(emitted.contains("<\\/script"), "{emitted}");
        assert_eq!(
            html.matches("</script>").count(),
            1,
            "the block closes its script element early"
        );
        assert_eq!(emitted.replace("<\\/script", "</script"), CLOSER_BLOCK);
        // the one invariant: the hash names the text the element reads back
        assert!(html.contains(&format!("data-hash=\"{}\"", op_chart::hash_hex(emitted))));
        // which is not the text the author wrote, here
        assert_ne!(
            op_chart::hash_hex(emitted),
            op_chart::hash_hex(CLOSER_BLOCK),
            "the escape moved no bytes: this block cannot prove the invariant"
        );
    }

    #[test]
    fn an_attribute_value_cannot_end_its_own_attribute() {
        // XML lets a value be written in single quotes, where a double quote
        // is just a character; HTML writes it in double quotes
        let src = r#"<opt:body xmlns:opt="https://www.openpower.tools/ns/opt"><opt:badge variant='a"b &amp; c &lt; d'>x</opt:badge></opt:body>"#;
        let html = lower(src).expect("valid");
        assert!(
            html.contains("<opt-badge variant=\"a&quot;b &amp; c &lt; d\">"),
            "{html}"
        );
        // the value is one attribute, not a quote followed by an unknown one
        assert_eq!(html.matches('"').count(), 2, "{html}");
        // and the ampersand is escaped once, not once per pass
        assert!(!html.contains("&amp;amp;"), "{html}");
    }

    #[test]
    fn other_children_of_a_chart_are_passed_through_after_the_template() {
        let html = lower(&page(&format!(
            "<opt:chart for=\"f\"><script type=\"application/json\">{BLOCK}</script><p>caption</p></opt:chart>"
        )))
        .expect("valid");
        let template = html.find("</template>").expect("the template");
        let passed = html.find("<p>caption</p>").expect("the other child");
        assert!(template < passed, "{html}");
    }

    #[test]
    fn a_chart_without_a_data_block_is_a_lowering_error() {
        let errors = lower(&page(
            "<opt:chart for=\"f\"><p>nothing to draw</p></opt:chart>",
        ))
        .expect_err("must fail");
        assert!(
            errors
                .iter()
                .any(|e| e.contains("<opt:chart>") && e.contains("application/json")),
            "{errors:?}"
        );
    }

    #[test]
    fn an_unreadable_data_block_reports_the_parsers_message() {
        let errors = lower(&page(&chart("", r#"{"nope": 1}"#))).expect_err("must fail");
        assert!(
            errors
                .iter()
                .any(|e| e.contains("<opt:chart>") && e.contains("unknown key \"nope\"")),
            "{errors:?}"
        );
    }

    #[test]
    fn initial_width_and_ratio_size_the_pre_render_and_leave_the_hash_alone() {
        let wide = lower(&page(&chart("", BLOCK))).expect("valid");
        let narrow = lower(&page(&chart(" initial-width=\"320\"", BLOCK))).expect("valid");
        let squat =
            lower(&page(&chart(" initial-width=\"320\" ratio=\"4\"", BLOCK))).expect("valid");
        assert!(wide.contains("viewBox=\"0 0 640 240\""), "{wide}");
        assert!(narrow.contains("viewBox=\"0 0 320 120\""), "{narrow}");
        assert!(squat.contains("viewBox=\"0 0 320 80\""), "{squat}");
        let hash = format!("data-hash=\"{}\"", op_chart::hash_hex(BLOCK));
        for html in [&wide, &narrow, &squat] {
            assert!(html.contains(&hash), "the geometry moved the hash: {html}");
        }
    }

    #[test]
    fn an_unusable_initial_width_or_ratio_is_a_lowering_error() {
        let errors = lower(&page(&chart(" initial-width=\"0\" ratio=\"wide\"", BLOCK)))
            .expect_err("must fail");
        assert!(
            errors
                .iter()
                .any(|e| e.contains("<opt:chart> needs a positive initial-width")),
            "{errors:?}"
        );
        assert!(
            errors
                .iter()
                .any(|e| e.contains("<opt:chart> cannot read ratio=\"wide\"")),
            "{errors:?}"
        );
    }

    #[test]
    fn a_chart_may_not_write_its_own_data_hash() {
        let errors =
            lower(&page(&chart(" data-hash=\"0000000000000000\"", BLOCK))).expect_err("must fail");
        assert!(
            errors
                .iter()
                .any(|e| e.contains("<opt:chart> does not accept attribute \"data-hash\"")),
            "{errors:?}"
        );
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
