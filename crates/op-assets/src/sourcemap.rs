//! Debug source maps for the shipped wasm, so the browser inspector
//! shows the original Rust.
//!
//! The release profile emits line-table DWARF (`debug =
//! "line-tables-only"` with `trim-paths = "all"`, so no build-machine
//! paths leak) and `data-keep-debug` carries it through wasm-bindgen.
//! This module turns the embedded DWARF into a standard source map
//! (via the `wasm2map` crate), appends the `sourceMappingURL` custom
//! section itself (the URL must be the absolute final one), and
//! recomputes the subresource-integrity digest Trunk stamped on the
//! wasm preload, which the appended section would otherwise
//! invalidate.
//!
//! Inspectors would otherwise consume TWO debug channels: the source
//! map, whose `sources` URLs we control, and the embedded DWARF itself
//! (Chrome's DWARF support and the C/C++ debugging extension), which
//! fetches raw compilation paths resolved against the origin -
//! `crates/...`, `/rustc/<hash>/...`, `/cargo/registry/...` - claiming
//! half a dozen top-level URL directories. The DWARF paths cannot be
//! rewritten in place cheaply (the stdlib's are baked into the
//! prebuilt std rlibs, and re-serialising every debug section means
//! rebuilding all their internal offsets), so the shipped wasm keeps
//! ONE channel: the map is generated from the DWARF, then the
//! `.debug_*` sections are STRIPPED (set `OP_ASSETS_KEEP_DWARF=1` to
//! keep them for local extension-based debugging). Stripping is safe
//! for the map because the debug sections sit after the code section,
//! which the strip asserts, so no mapped offset moves.
//!
//! Everything the map references is served under the single `/src/`
//! prefix: workspace crates at `/src/crates/...` (their text also
//! embedded as `sourcesContent`, so our own code resolves even where
//! the site is not being served), the stdlib at `/src/rust/library/...`
//! (both its DWARF shapes fold to one file), std's vendored deps under
//! `/src/rust/library/vendor/`, and registry crates at
//! `/src/vendor/<crate>-<version>/`. Licence files travel with every
//! served crate root. DevTools fetches the map and the sources only
//! while the inspector is open; regular visitors pay for none of it.

use std::path::{Component, Path, PathBuf};

/// Where source paths resolve from.
pub struct Roots<'a> {
    /// The workspace this site is built from.
    pub workspace: &'a Path,
    /// `<sysroot>/lib/rustlib/src/rust` from the rust-src component.
    pub rust_src: &'a Path,
    /// `<CARGO_HOME>/registry/src`.
    pub registry_src: &'a Path,
}

/// The outcome of resolving a map: the rewritten JSON, the files to
/// copy into staging as `(absolute source, staging-relative dest)`,
/// and the entries left as bare labels.
pub struct Resolved {
    pub map: String,
    pub copies: Vec<(PathBuf, String)>,
    pub unresolved: Vec<String>,
}

/// A source path located on disk: where it comes from, the
/// staging-relative path it is served at (its normalised literal DWARF
/// path), the crate root its licences travel from, and whether its
/// text is embedded in the map.
struct Located {
    from: PathBuf,
    dest: String,
    licence_root: Option<(PathBuf, String)>,
    embed: bool,
}

/// Rewrites every resolvable `sources` entry to its served URL and
/// collects the copy instructions (see the module docs for the URL
/// scheme and why it mirrors the DWARF paths verbatim). A stdlib,
/// std-vendored or registry path that cannot be found fails the build:
/// those classes are deterministic, so missing means the rust-src
/// component or the registry cache is absent. Anything else (in
/// practice a handful of units whose crate roots `trim-paths` erased
/// to bare `src/...`) is left untouched: a visible, honest label.
pub fn resolve_sources(map_json: &str, roots: &Roots) -> Resolved {
    let mut map: serde_json::Value = serde_json::from_str(map_json).expect("source map is JSON");
    let mut copies: Vec<(PathBuf, String)> = Vec::new();
    let mut licence_roots: Vec<(PathBuf, String)> = Vec::new();
    let mut unresolved = Vec::new();
    let mut missing = Vec::new();
    let mut contents = Vec::new();

    let sources = map
        .get_mut("sources")
        .and_then(|s| s.as_array_mut())
        .expect("map has sources");
    for entry in sources.iter_mut() {
        let Some(original) = entry.as_str().map(str::to_owned) else {
            contents.push(serde_json::Value::Null);
            continue;
        };
        let mut content = serde_json::Value::Null;
        match locate(&original, roots) {
            Ok(Some(located)) => {
                if located.embed {
                    content = std::fs::read_to_string(&located.from)
                        .map(serde_json::Value::String)
                        .unwrap_or(serde_json::Value::Null);
                }
                if let Some(root) = located.licence_root {
                    licence_roots.push(root);
                }
                *entry = serde_json::Value::String(format!("/{}", located.dest));
                copies.push((located.from, located.dest));
            }
            Ok(None) => unresolved.push(original),
            Err(()) => missing.push(original),
        }
        contents.push(content);
    }
    assert!(
        missing.is_empty(),
        "deterministic source classes failed to resolve (is the rust-src \
         component installed and the registry cache populated?): {missing:?}"
    );

    map["sourcesContent"] = serde_json::Value::Array(contents);

    licence_roots.sort();
    licence_roots.dedup();
    for (root, dest) in licence_roots {
        for name in licence_files(&root) {
            copies.push((root.join(&name), format!("{dest}/{name}")));
        }
    }
    copies.sort();
    copies.dedup();

    Resolved {
        map: map.to_string(),
        copies,
        unresolved,
    }
}

/// Classifies one DWARF path. `Ok(Some)` = serve it; `Ok(None)` = bare
/// label; `Err` = a deterministic class whose file is missing.
fn locate(original: &str, roots: &Roots) -> Result<Option<Located>, ()> {
    if let Some(relative) = workspace_relative(original, roots.workspace) {
        return Ok(Some(Located {
            from: roots.workspace.join(&relative),
            dest: format!("src/{relative}"),
            licence_root: Some((roots.workspace.to_owned(), "src/crates".to_owned())),
            embed: true,
        }));
    }
    if let Some(rest) = original.strip_prefix("/rust/deps/") {
        let Some(rest) = normalize(rest) else {
            return Ok(None);
        };
        let from = roots.rust_src.join("library/vendor").join(&rest);
        if !from.is_file() {
            return Err(());
        }
        let crate_dir = rest.split('/').next().expect("vendored crate dir");
        return Ok(Some(Located {
            from,
            licence_root: Some((
                roots.rust_src.join("library/vendor").join(crate_dir),
                format!("src/rust/library/vendor/{crate_dir}"),
            )),
            dest: format!("src/rust/library/vendor/{rest}"),
            embed: false,
        }));
    }
    if let Some(library) = stdlib_form(original) {
        let Some(library) = normalize(&library) else {
            return Ok(None);
        };
        if !library.starts_with("library/") {
            return Ok(None);
        }
        let from = roots.rust_src.join(&library);
        if !from.is_file() {
            return Err(());
        }
        return Ok(Some(Located {
            from,
            // both stdlib forms fold to one served file (the /rustc/
            // hash carries no information the toolchain pin lacks)
            dest: format!("src/rust/{library}"),
            // the toolchain licence set is placed once, at /src/rust/
            licence_root: None,
            embed: false,
        }));
    }
    if let Some(rest) = original.strip_prefix("/cargo/registry/") {
        let Some(rest) = normalize(rest) else {
            return Ok(None);
        };
        let mut parts = rest.splitn(3, '/');
        let (Some(first), Some(second), Some(tail)) = (parts.next(), parts.next(), parts.next())
        else {
            return Ok(None);
        };
        let (crate_dir, file) = if first == "src" {
            let Some(split) = tail.split_once('/') else {
                return Ok(None);
            };
            split
        } else {
            (second, tail)
        };
        let Some(root) = find_registry_crate(roots.registry_src, crate_dir) else {
            return Err(());
        };
        let from = root.join(file);
        if !from.is_file() {
            return Err(());
        }
        return Ok(Some(Located {
            from,
            dest: format!("src/vendor/{crate_dir}/{file}"),
            licence_root: Some((root, format!("src/vendor/{crate_dir}"))),
            embed: false,
        }));
    }
    Ok(None)
}

/// The rust-src relative path of the two stdlib shapes: `library/...`
/// directly, or the same behind `/rustc/<hash>/`. Paths may contain
/// `..` hops - stdarch is reached that way - which the caller folds.
fn stdlib_form(path: &str) -> Option<String> {
    if let Some(rest) = path.strip_prefix("/rustc/") {
        let (_hash, rest) = rest.split_once('/')?;
        return Some(rest.to_owned());
    }
    path.starts_with("library/").then(|| path.to_owned())
}

/// Locates `<crate>-<version>` under any index dir of the local
/// registry cache (the index dir named in the DWARF need not match the
/// local one; the crate dirname is unambiguous under a lockfile).
fn find_registry_crate(registry_src: &Path, crate_dir: &str) -> Option<PathBuf> {
    let entries = std::fs::read_dir(registry_src).ok()?;
    for index_dir in entries.filter_map(|e| e.ok()) {
        let candidate = index_dir.path().join(crate_dir);
        if candidate.is_dir() {
            return Some(candidate);
        }
    }
    None
}

/// Folds `.` and `..` segments; `None` when the path escapes its root
/// or carries non-normal components.
fn normalize(path: &str) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();
    for component in Path::new(path).components() {
        match component {
            Component::Normal(part) => parts.push(part.to_str()?),
            Component::ParentDir => {
                parts.pop()?;
            }
            Component::CurDir => {}
            Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

fn licence_files(root: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| {
            let upper = n.to_uppercase();
            upper.starts_with("LICENSE")
                || upper.starts_with("LICENCE")
                || upper.starts_with("COPYING")
                || upper.starts_with("COPYRIGHT")
        })
        .collect();
    names.sort();
    names
}

/// No parent/root components: safe to join and to serve.
fn clean(path: &str) -> bool {
    !path.is_empty()
        && Path::new(path)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
}

/// The workspace-relative path of `path` when it denotes a real file in
/// the workspace (either already relative, or absolute underneath it).
fn workspace_relative(path: &str, workspace: &Path) -> Option<String> {
    let p = Path::new(path);
    let relative = if p.is_absolute() {
        p.strip_prefix(workspace).ok()?
    } else {
        p
    };
    let relative = relative.to_str()?;
    (clean(relative) && workspace.join(relative).is_file()).then(|| relative.to_owned())
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

/// One top-level section of a wasm binary.
struct WasmSection {
    id: u8,
    name: Option<String>,
    /// Byte range of the whole section (id byte through payload end).
    range: std::ops::Range<usize>,
}

fn decode_uleb(bytes: &[u8], at: &mut usize) -> u64 {
    let (mut value, mut shift) = (0u64, 0u32);
    loop {
        let byte = bytes[*at];
        *at += 1;
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return value;
        }
        shift += 7;
    }
}

fn wasm_sections(bytes: &[u8]) -> Vec<WasmSection> {
    assert!(
        bytes.len() >= 8 && &bytes[..4] == b"\0asm",
        "not a wasm binary"
    );
    let mut sections = Vec::new();
    let mut at = 8;
    while at < bytes.len() {
        let start = at;
        let id = bytes[at];
        at += 1;
        let size = decode_uleb(bytes, &mut at) as usize;
        let payload = at..at + size;
        let name = (id == 0).then(|| {
            let mut cursor = payload.start;
            let len = decode_uleb(bytes, &mut cursor) as usize;
            String::from_utf8_lossy(&bytes[cursor..cursor + len]).into_owned()
        });
        at = payload.end;
        sections.push(WasmSection {
            id,
            name,
            range: start..at,
        });
    }
    assert_eq!(at, bytes.len(), "trailing garbage after last section");
    sections
}

/// Removes every `.debug_*` custom section. The source map's offsets
/// point into the code section, so this is only sound while the debug
/// sections all sit behind it - asserted, so a layout change fails the
/// build instead of silently skewing every mapping.
pub fn strip_debug_sections(bytes: &[u8]) -> Vec<u8> {
    let sections = wasm_sections(bytes);
    let code_end = sections
        .iter()
        .find(|s| s.id == 10)
        .map(|s| s.range.end)
        .expect("wasm has a code section");
    let mut out = bytes[..8].to_vec();
    for section in &sections {
        let debug = section
            .name
            .as_deref()
            .is_some_and(|n| n.starts_with(".debug_"));
        if debug {
            assert!(
                section.range.start >= code_end,
                "{} precedes the code section; stripping would skew the source map",
                section.name.as_deref().unwrap_or_default()
            );
            continue;
        }
        out.extend_from_slice(&bytes[section.range.clone()]);
    }
    out
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

    fn fake_roots(tag: &str) -> (PathBuf, PathBuf) {
        let base = std::env::temp_dir().join(format!("op-assets-{tag}-{}", std::process::id()));
        let rust_src = base.join("rust-src");
        let registry = base.join("registry-src");
        for (dir, file, text) in [
            (&rust_src, "library/core/src/x.rs", "// core"),
            (
                &rust_src,
                "library/vendor/dlmalloc-0.2.14/src/d.rs",
                "// dl",
            ),
            (
                &rust_src,
                "library/stdarch/crates/core_arch/src/wasm32/memory.rs",
                "// stdarch",
            ),
            (
                &rust_src,
                "library/vendor/dlmalloc-0.2.14/LICENSE-APACHE",
                "ap",
            ),
            (
                &registry,
                "index.crates.io-abc/wasm-bindgen-0.2.127/src/lib.rs",
                "// wb",
            ),
            (
                &registry,
                "index.crates.io-abc/wasm-bindgen-0.2.127/LICENSE-MIT",
                "mit",
            ),
        ] {
            let path = dir.join(file);
            std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdirs");
            std::fs::write(path, text).expect("write");
        }
        (rust_src, registry)
    }

    #[test]
    fn every_source_serves_at_its_literal_dwarf_path() {
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .expect("workspace");
        let (rust_src, registry) = fake_roots("resolve");
        let ours = "crates/op-assets/src/sourcemap.rs";
        let rustc_hash = "0123456789abcdef0123456789abcdef01234567";
        let map = serde_json::json!({
            "version": 3,
            "sources": [
                ours,
                workspace.join(ours).to_str().expect("utf8"),
                "library/core/src/x.rs",
                format!("/rustc/{rustc_hash}/library/core/src/x.rs"),
                "library/core/src/../../stdarch/crates/core_arch/src/wasm32/memory.rs",
                "/rust/deps/dlmalloc-0.2.14/src/d.rs",
                "/cargo/registry/src/index.crates.io-abc/wasm-bindgen-0.2.127/src/lib.rs",
                "/cargo/registry/25cdd57fae9f0462/wasm-bindgen-0.2.127/src/lib.rs",
                "src/lib.rs",
                "../outside/escape.rs",
            ],
            "mappings": "AAAA",
        });
        let resolved = resolve_sources(
            &map.to_string(),
            &Roots {
                workspace: &workspace,
                rust_src: &rust_src,
                registry_src: &registry,
            },
        );
        let value: serde_json::Value = serde_json::from_str(&resolved.map).expect("json");
        let sources: Vec<&str> = value["sources"]
            .as_array()
            .expect("array")
            .iter()
            .map(|s| s.as_str().expect("str"))
            .collect();
        assert_eq!(
            sources,
            vec![
                format!("/src/{ours}"),
                format!("/src/{ours}"),
                "/src/rust/library/core/src/x.rs".to_owned(),
                "/src/rust/library/core/src/x.rs".to_owned(),
                "/src/rust/library/stdarch/crates/core_arch/src/wasm32/memory.rs".to_owned(),
                "/src/rust/library/vendor/dlmalloc-0.2.14/src/d.rs".to_owned(),
                "/src/vendor/wasm-bindgen-0.2.127/src/lib.rs".to_owned(),
                "/src/vendor/wasm-bindgen-0.2.127/src/lib.rs".to_owned(),
                "src/lib.rs".to_owned(),
                "../outside/escape.rs".to_owned(),
            ]
        );
        let _ = rustc_hash;
        assert_eq!(
            resolved.unresolved,
            vec!["src/lib.rs", "../outside/escape.rs"]
        );

        let contents = value["sourcesContent"].as_array().expect("contents");
        assert_eq!(contents.len(), sources.len(), "content aligns with sources");
        let own_text = std::fs::read_to_string(workspace.join(ours)).expect("read own");
        assert_eq!(contents[0].as_str(), Some(own_text.as_str()));
        assert_eq!(contents[1].as_str(), Some(own_text.as_str()));
        assert!(contents[2..].iter().all(|c| c.is_null()));

        let dests: Vec<&str> = resolved.copies.iter().map(|(_, to)| to.as_str()).collect();
        let ours_dest = format!("src/{ours}");
        for expected in [
            ours_dest.as_str(),
            "src/rust/library/core/src/x.rs",
            "src/rust/library/stdarch/crates/core_arch/src/wasm32/memory.rs",
            "src/rust/library/vendor/dlmalloc-0.2.14/src/d.rs",
            "src/vendor/wasm-bindgen-0.2.127/src/lib.rs",
            // licences travel to every served crate root, and the
            // repo's own licences cover the /src/crates/ tree
            "src/crates/LICENSE.md",
            "src/rust/library/vendor/dlmalloc-0.2.14/LICENSE-APACHE",
            "src/vendor/wasm-bindgen-0.2.127/LICENSE-MIT",
        ] {
            assert!(
                dests.contains(&expected),
                "missing copy {expected}: {dests:?}"
            );
        }
        assert!(
            dests.iter().all(|d| d.starts_with("src/")),
            "everything serves under /src/: {dests:?}"
        );
        let base = rust_src.parent().expect("base");
        std::fs::remove_dir_all(base).expect("cleanup");
    }

    #[test]
    #[should_panic(expected = "deterministic source classes")]
    fn a_missing_stdlib_source_fails_the_build() {
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .expect("workspace");
        let (rust_src, registry) = fake_roots("missing");
        let map = serde_json::json!({
            "version": 3,
            "sources": ["library/core/src/does_not_exist.rs"],
            "mappings": "AAAA",
        });
        let outcome = std::panic::catch_unwind(|| {
            resolve_sources(
                &map.to_string(),
                &Roots {
                    workspace: &workspace,
                    rust_src: &rust_src,
                    registry_src: &registry,
                },
            )
        });
        std::fs::remove_dir_all(rust_src.parent().expect("base")).expect("cleanup");
        match outcome {
            Err(panic) => std::panic::resume_unwind(panic),
            Ok(_) => panic!("resolution unexpectedly succeeded"),
        }
    }

    fn leb(mut value: u64) -> Vec<u8> {
        let mut out = Vec::new();
        loop {
            let byte = (value & 0x7f) as u8;
            value >>= 7;
            if value == 0 {
                out.push(byte);
                break;
            }
            out.push(byte | 0x80);
        }
        out
    }

    fn section(id: u8, name: Option<&str>, body: &[u8]) -> Vec<u8> {
        let mut payload = Vec::new();
        if let Some(name) = name {
            payload.extend(leb(name.len() as u64));
            payload.extend_from_slice(name.as_bytes());
        }
        payload.extend_from_slice(body);
        let mut out = vec![id];
        out.extend(leb(payload.len() as u64));
        out.extend(payload);
        out
    }

    fn wasm_with(sections: &[Vec<u8>]) -> Vec<u8> {
        let mut bytes = b"\0asm\x01\0\0\0".to_vec();
        for s in sections {
            bytes.extend_from_slice(s);
        }
        bytes
    }

    #[test]
    fn stripping_removes_debug_sections_and_nothing_else() {
        let wasm = wasm_with(&[
            section(1, None, &[0]),
            section(10, None, &[1, 2, 3]),
            section(0, Some("name"), b"n"),
            section(0, Some(".debug_line"), &vec![9u8; 300]),
            section(0, Some(".debug_str"), b"s"),
        ]);
        let stripped = strip_debug_sections(&wasm);
        let expected = wasm_with(&[
            section(1, None, &[0]),
            section(10, None, &[1, 2, 3]),
            section(0, Some("name"), b"n"),
        ]);
        assert_eq!(stripped, expected);
        assert_eq!(strip_debug_sections(&stripped), expected, "idempotent");
    }

    #[test]
    #[should_panic(expected = "precedes the code section")]
    fn a_debug_section_before_code_refuses_to_strip() {
        let wasm = wasm_with(&[
            section(0, Some(".debug_line"), b"x"),
            section(10, None, &[1]),
        ]);
        strip_debug_sections(&wasm);
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
