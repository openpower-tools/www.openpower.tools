//! Debug source maps for the shipped wasm, so the browser inspector
//! shows the original Rust.
//!
//! The release profile emits line-table DWARF (`debug =
//! "line-tables-only"` with `trim-paths = "all"`, so no build-machine
//! paths leak), `data-keep-debug` carries it through wasm-bindgen, and
//! wasm-opt runs with `-g` (`data-wasm-opt-params`) so `-Oz` updates
//! the line info instead of discarding it. This module then turns the
//! embedded DWARF into a standard source map (via the `wasm2map`
//! crate), serves every workspace source file it references under
//! `/src/`, appends the `sourceMappingURL` custom section itself (the
//! URL must be the absolute final one, so the patching is done here,
//! not by wasm2map), and recomputes the subresource-integrity digest
//! Trunk stamped on the wasm preload, which the appended section would
//! otherwise invalidate.
//!
//! DevTools fetches the map (and then the sources) only while the
//! inspector is open: regular visitors pay only for the DWARF bytes in
//! the wasm itself. Registry and rustc sources stay as bare paths in
//! the map; they are visible but not browsable, which is honest.

use std::path::{Component, Path, PathBuf};

/// Rewrites the map's `sources`: entries that resolve to files inside
/// the workspace become `/src/<relative>` URLs, returned as copy
/// instructions `(absolute file, staging-relative destination)`.
/// Everything else is left untouched.
pub fn rewrite_sources(map_json: &str, workspace: &Path) -> (String, Vec<(PathBuf, String)>) {
    let mut map: serde_json::Value = serde_json::from_str(map_json).expect("source map is JSON");
    let mut copies = Vec::new();
    if let Some(sources) = map.get_mut("sources").and_then(|s| s.as_array_mut()) {
        for entry in sources {
            let Some(original) = entry.as_str() else {
                continue;
            };
            if let Some(relative) = workspace_relative(original, workspace) {
                copies.push((workspace.join(&relative), format!("src/{relative}")));
                *entry = serde_json::Value::String(format!("/src/{relative}"));
            }
        }
    }
    copies.sort();
    copies.dedup();
    (map.to_string(), copies)
}

/// The workspace-relative path of `path` when it denotes a real file in
/// the workspace (either already relative, or absolute underneath it);
/// `None` for registry crates, rustc sources and anything traversing.
fn workspace_relative(path: &str, workspace: &Path) -> Option<String> {
    let p = Path::new(path);
    let relative = if p.is_absolute() {
        p.strip_prefix(workspace).ok()?
    } else {
        p
    };
    if relative
        .components()
        .any(|c| !matches!(c, Component::Normal(_)))
    {
        return None;
    }
    let relative = relative.to_str()?;
    workspace
        .join(relative)
        .is_file()
        .then(|| relative.to_owned())
}

fn unsigned_leb128(mut value: u64, out: &mut Vec<u8>) {
    loop {
        let byte = (value & 0x7f) as u8;
        value >>= 7;
        if value == 0 {
            out.push(byte);
            break;
        }
        out.push(byte | 0x80);
    }
}

/// The `sourceMappingURL` custom section (id 0) for `url`, appendable
/// verbatim to a wasm binary: name and payload are length-prefixed
/// (unsigned LEB128) per the WebAssembly binary format.
pub fn source_mapping_section(url: &str) -> Vec<u8> {
    const NAME: &[u8] = b"sourceMappingURL";
    let mut payload = Vec::new();
    unsigned_leb128(NAME.len() as u64, &mut payload);
    payload.extend_from_slice(NAME);
    unsigned_leb128(url.len() as u64, &mut payload);
    payload.extend_from_slice(url.as_bytes());
    let mut section = vec![0u8];
    unsigned_leb128(payload.len() as u64, &mut section);
    section.extend_from_slice(&payload);
    section
}

/// Replaces the `integrity` value inside the one tag whose `href`
/// matches; every other tag keeps its digest.
pub fn rewrite_integrity(html: &str, href: &str, integrity: &str) -> String {
    let marker = format!("href=\"{href}\"");
    let tag_start = html
        .find(&marker)
        .map(|at| html[..at].rfind('<').expect("href sits inside a tag"))
        .unwrap_or_else(|| panic!("no tag references {href}"));
    let tag_end = tag_start + html[tag_start..].find('>').expect("tag closes");
    let tag = &html[tag_start..tag_end];
    let value_start = tag_start
        + tag
            .find("integrity=\"")
            .unwrap_or_else(|| panic!("tag for {href} carries no integrity attribute"))
        + "integrity=\"".len();
    let value_end = value_start
        + html[value_start..]
            .find('"')
            .expect("integrity value closes");
    format!("{}{integrity}{}", &html[..value_start], &html[value_end..])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode_leb(bytes: &[u8]) -> (u64, usize) {
        let (mut value, mut shift, mut used) = (0u64, 0u32, 0usize);
        for byte in bytes {
            value |= u64::from(byte & 0x7f) << shift;
            used += 1;
            if byte & 0x80 == 0 {
                return (value, used);
            }
            shift += 7;
        }
        panic!("unterminated LEB128");
    }

    #[test]
    fn source_mapping_section_round_trips_including_multibyte_lengths() {
        let long_url = format!("/{}.map", "x".repeat(300));
        for url in ["/w.map", long_url.as_str()] {
            let section = source_mapping_section(url);
            assert_eq!(section[0], 0, "custom section id");
            let (payload_len, used) = decode_leb(&section[1..]);
            let payload = &section[1 + used..];
            assert_eq!(payload.len() as u64, payload_len);
            let (name_len, used) = decode_leb(payload);
            let name = &payload[used..used + name_len as usize];
            assert_eq!(name, b"sourceMappingURL");
            let rest = &payload[used + name_len as usize..];
            let (url_len, used) = decode_leb(rest);
            assert_eq!(&rest[used..], url.as_bytes());
            assert_eq!(url_len as usize, url.len());
        }
    }

    #[test]
    fn sources_inside_the_workspace_are_rewritten_and_copied_the_rest_kept() {
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let ours = "crates/op-assets/src/sourcemap.rs";
        let absolute = workspace.join(ours);
        let map = serde_json::json!({
            "version": 3,
            "sources": [
                ours,
                absolute.to_str().expect("utf8 path"),
                "index.crates.io-6f17d22bba15001f/wasm-bindgen-0.2.127/src/lib.rs",
                "/rustc/abcdef/library/core/src/option.rs",
                "../outside/escape.rs",
            ],
            "mappings": "AAAA",
        });
        let (rewritten, copies) = rewrite_sources(&map.to_string(), &workspace);
        let value: serde_json::Value = serde_json::from_str(&rewritten).expect("json");
        let sources: Vec<&str> = value["sources"]
            .as_array()
            .expect("array")
            .iter()
            .map(|s| s.as_str().expect("str"))
            .collect();
        assert_eq!(sources[0], format!("/src/{ours}"));
        assert_eq!(sources[1], format!("/src/{ours}"), "absolute form maps too");
        assert!(sources[2].starts_with("index.crates.io"), "registry kept");
        assert!(sources[3].starts_with("/rustc/"), "rustc kept");
        assert_eq!(sources[4], "../outside/escape.rs", "traversal kept as-is");
        assert_eq!(copies, vec![(absolute, format!("src/{ours}"))]);
    }

    #[test]
    fn integrity_rewrite_touches_only_the_matching_tag() {
        let html = "<head><link href=\"/a.js\" integrity=\"sha384-old1\"><link rel=\"preload\" href=\"/b.wasm\" crossorigin=\"anonymous\" integrity=\"sha384-old2\" as=\"fetch\"></head>";
        let out = rewrite_integrity(html, "/b.wasm", "sha384-new");
        assert!(out.contains("integrity=\"sha384-old1\""));
        assert!(out.contains("integrity=\"sha384-new\""));
        assert!(!out.contains("sha384-old2"));
    }

    /// Pinned SRI vector: sha384 of the empty input, base64 as browsers
    /// print it.
    #[test]
    fn sri_digest_matches_the_published_empty_vector() {
        use base64::Engine as _;
        use sha2::Digest as _;
        let digest = base64::engine::general_purpose::STANDARD.encode(sha2::Sha384::digest(b""));
        assert_eq!(
            digest,
            "OLBgp1GsljhM2TJ+sbHjaiH9txEUvgdDTAzHv2P24donTt6/529l+9Ua0vFImLlb"
        );
    }
}
