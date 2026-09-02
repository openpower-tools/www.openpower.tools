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

/// Rewrites every resolvable `sources` entry to a served `/src/...`
/// URL and collects the copy instructions, so the inspector can browse
/// everything the wasm's line tables reference:
///
/// - workspace files -> `/src/<relative>`, with the text additionally
///   embedded in `sourcesContent` (our own code resolves even where
///   the site is not being served);
/// - stdlib paths (`library/...`, also in their `/rustc/<hash>/...`
///   form) -> `/src/rust/library/...` from the rust-src component;
/// - std's vendored deps (`/rust/deps/<crate>/...`) ->
///   `/src/rust/library/vendor/<crate>/...`, same component;
/// - locked registry crates (`/cargo/registry/src/<index>/<crate>/...`)
///   -> `/src/vendor/<crate>-<version>/...` from the local registry.
///
/// Licence files at each copied crate root travel along. A stdlib,
/// vendored or registry path that cannot be found is a build error
/// (the classes are deterministic; missing means the rust-src
/// component or the registry cache is absent). Anything else - in
/// practice a handful of units whose crate roots `trim-paths` erased
/// to bare `src/...` - is left untouched: a visible, honest label.
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
        let resolved = if let Some(relative) = workspace_relative(&original, roots.workspace) {
            let from = roots.workspace.join(&relative);
            content = std::fs::read_to_string(&from)
                .map(serde_json::Value::String)
                .unwrap_or(serde_json::Value::Null);
            copies.push((from, format!("src/{relative}")));
            Some(format!("/src/{relative}"))
        } else if let Some(stdlib) = stdlib_relative(&original) {
            let from = roots.rust_src.join(&stdlib);
            if from.is_file() {
                if let Some(root) = crate_root(&stdlib, "library/vendor/") {
                    licence_roots.push((roots.rust_src.join(&root), format!("src/rust/{root}")));
                } else {
                    licence_roots.push((roots.rust_src.to_owned(), "src/rust".to_owned()));
                }
                copies.push((from, format!("src/rust/{stdlib}")));
                Some(format!("/src/rust/{stdlib}"))
            } else {
                missing.push(original.clone());
                None
            }
        } else if let Some((crate_dir, rest)) = registry_relative(&original) {
            if let Some(root) = find_registry_crate(roots.registry_src, &crate_dir) {
                let from = root.join(&rest);
                if from.is_file() {
                    licence_roots.push((root, format!("src/vendor/{crate_dir}")));
                    copies.push((from, format!("src/vendor/{crate_dir}/{rest}")));
                    Some(format!("/src/vendor/{crate_dir}/{rest}"))
                } else {
                    missing.push(original.clone());
                    None
                }
            } else {
                missing.push(original.clone());
                None
            }
        } else {
            unresolved.push(original.clone());
            None
        };
        if let Some(url) = resolved {
            *entry = serde_json::Value::String(url);
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

/// rust-src-relative path for stdlib forms: `library/...` directly,
/// the same behind a `/rustc/<hash>/` prefix, and std's vendored deps
/// `/rust/deps/<crate>/...` (shipped under `library/vendor/`). Paths
/// may contain `..` hops (stdarch is reached that way); they are
/// folded before the `library/` check so the result is join-safe.
fn stdlib_relative(path: &str) -> Option<String> {
    let library = if let Some(rest) = path.strip_prefix("/rustc/") {
        let (_hash, rest) = rest.split_once('/')?;
        rest
    } else {
        path
    };
    if let Some(rest) = library.strip_prefix("/rust/deps/") {
        return Some(format!("library/vendor/{}", normalize(rest)?));
    }
    let library = normalize(library)?;
    library.starts_with("library/").then_some(library)
}

/// Splits a registry path into `(crate dir, file path)`. Two remap
/// forms exist: the older `/cargo/registry/src/<index dir>/<crate>-
/// <version>/...` and the newer `/cargo/registry/<hash>/<crate>-
/// <version>/...`; both lose their machine-specific component in the
/// served URL - a `<crate>-<version>` dirname is unambiguous under a
/// lockfile.
fn registry_relative(path: &str) -> Option<(String, String)> {
    let rest = path.strip_prefix("/cargo/registry/")?;
    let rest = normalize(rest)?;
    let mut parts = rest.splitn(3, '/');
    let (first, second, tail) = (parts.next()?, parts.next()?, parts.next()?);
    if first == "src" {
        let (crate_dir, file) = tail.split_once('/')?;
        Some((crate_dir.to_owned(), file.to_owned()))
    } else {
        let _registry_hash = first;
        Some((second.to_owned(), tail.to_owned()))
    }
}

/// Locates `<crate>-<version>` under any index dir of the local
/// registry cache.
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

/// First path component(s) identifying the crate a file belongs to:
/// `<prefix><crate>-<version>` for prefixed layouts, or
/// `<index>/<crate>` for the two-level registry layout.
fn crate_root(relative: &str, prefix: &str) -> Option<String> {
    let rest = relative.strip_prefix(prefix)?;
    let depth = if prefix.is_empty() { 2 } else { 1 };
    let parts: Vec<&str> = rest.splitn(depth + 1, '/').collect();
    (parts.len() > depth).then(|| format!("{prefix}{}", parts[..depth].join("/")))
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
            (&rust_src, "LICENSE-MIT", "mit"),
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
    fn every_deterministic_class_resolves_and_licences_travel() {
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .expect("workspace");
        let (rust_src, registry) = fake_roots("resolve");
        let ours = "crates/op-assets/src/sourcemap.rs";
        let map = serde_json::json!({
            "version": 3,
            "sources": [
                ours,
                workspace.join(ours).to_str().expect("utf8"),
                "library/core/src/x.rs",
                "/rustc/0123456789abcdef0123456789abcdef01234567/library/core/src/x.rs",
                "/rust/deps/dlmalloc-0.2.14/src/d.rs",
                "/cargo/registry/src/index.crates.io-abc/wasm-bindgen-0.2.127/src/lib.rs",
                "/cargo/registry/25cdd57fae9f0462/wasm-bindgen-0.2.127/src/lib.rs",
                "library/core/src/../../stdarch/crates/core_arch/src/wasm32/memory.rs",
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
                "/src/rust/library/vendor/dlmalloc-0.2.14/src/d.rs".to_owned(),
                "/src/vendor/wasm-bindgen-0.2.127/src/lib.rs".to_owned(),
                "/src/vendor/wasm-bindgen-0.2.127/src/lib.rs".to_owned(),
                "/src/rust/library/stdarch/crates/core_arch/src/wasm32/memory.rs".to_owned(),
                "src/lib.rs".to_owned(),
                "../outside/escape.rs".to_owned(),
            ]
        );
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
        for expected in [
            format!("src/{ours}").as_str(),
            "src/rust/library/core/src/x.rs",
            "src/rust/library/vendor/dlmalloc-0.2.14/src/d.rs",
            "src/vendor/wasm-bindgen-0.2.127/src/lib.rs",
            "src/rust/LICENSE-MIT",
            "src/rust/library/vendor/dlmalloc-0.2.14/LICENSE-APACHE",
            "src/vendor/wasm-bindgen-0.2.127/LICENSE-MIT",
        ] {
            assert!(
                dests.contains(&expected),
                "missing copy {expected}: {dests:?}"
            );
        }
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
