//! Trunk `post_build` hook.
//!
//! 1. Measures each embedded face from its own font tables and computes the
//!    fit against the recorded target geometry of the original it replaces,
//!    so metric numbers can never go stale when fonts are updated.
//! 2. Copies every face into the Trunk staging directory as its own
//!    content-hashed woff2 and generates one `fonts-<hash>.css` holding
//!    the metric-fitted `local()` fallback faces (from the same targets
//!    against the frozen metrics of the Arial-class system designs)
//!    followed by the real `@font-face` url() rules.
//! 3. Injects `<link rel="preload" as="font">` for the first-paint faces
//!    plus the stylesheet link, so text renders styled with no runtime
//!    font code at all: no wasm registration, no flash beyond a
//!    same-metrics letterform swap in the first frames.
//!
//! Browsers apply `size-adjust` to override metrics as well (verified by
//! measurement in Chromium), so every override written here is the target box
//! divided by the computed size adjustment.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256, Sha384};

mod manifest;
mod sourcemap;
use manifest::{
    FitTarget, MANIFEST, ManifestEntry, PLEX_SANS_TARGET, PRAGMATA_TARGET, REF_STRING, SYS_TARGET,
};

/// One loaded face: CSS descriptors, computed fit metrics, bytes, and
/// the manifest entry it came from.
struct Face {
    entry: &'static ManifestEntry,
    /// `(size-adjust, ascent-override, descent-override)`; `None` = as is.
    metrics: Option<(String, String, String)>,
    bytes: Vec<u8>,
}

/// Faces fetched before first paint: what the first view of any page
/// renders with (body text, bold headings, code). The rest load lazily
/// as glyphs demand them, behind the metric-fitted fallbacks.
const PRELOAD: &[(&str, &str, &str)] = &[
    ("IBM Plex Sans", "400", "normal"),
    ("IBM Plex Sans", "700", "normal"),
    ("Barlow Semi Condensed", "700", "normal"),
    ("Iosevka SS08", "400", "normal"),
];

/// Advance width of [`REF_STRING`] at a 100px em, measured from the font's
/// cmap and hmtx tables (kerning is not applied; these faces do not kern the
/// reference string materially, and the same convention was used to measure
/// the targets).
/// Width of [`REF_STRING`] at a 100px em, measured from the font's tables.
struct Measured {
    ref_width: f64,
}

fn measure(font_bytes: &[u8]) -> Measured {
    let face = ttf_parser::Face::parse(font_bytes, 0).expect("parse font");
    let upem = f64::from(face.units_per_em());
    let mut units = 0.0;
    for c in REF_STRING.chars() {
        let glyph = face
            .glyph_index(c)
            .unwrap_or_else(|| panic!("font lacks {c:?} from the reference string"));
        let advance = face
            .glyph_hor_advance(glyph)
            .unwrap_or_else(|| panic!("no advance for {c:?}"));
        units += f64::from(advance);
    }
    Measured {
        ref_width: units / upem * 100.0,
    }
}

fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

fn round3(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

/// `(size_adjust, ascent_override, descent_override)` percentages fitting a
/// face of measured reference width `measured` to `target`.
fn fit_percentages(target: &FitTarget, measured: f64) -> (f64, f64, f64) {
    let scale = target.ref_width / measured;
    (
        round1(scale * 100.0),
        round1(target.ascent_pct / scale),
        round1(target.descent_pct / scale),
    )
}

fn load_faces(assets: &Path) -> (Vec<Face>, Option<RenderedStandin>) {
    let mut heading_standin = None;
    let faces = MANIFEST
        .iter()
        .map(|entry| {
            let path = assets.join(entry.path);
            let woff2 = std::fs::read(&path)
                .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
            assert!(woff2.starts_with(b"wOF2"), "{} is not woff2", entry.path);
            let metrics = entry.fit.map(|target| {
                let ttf = woff2_patched::convert_woff2_to_ttf(&mut woff2.as_slice())
                    .unwrap_or_else(|e| panic!("cannot decode {}: {e:?}", entry.path));
                let measured = measure(&ttf);
                let (s, a, d) = fit_percentages(&target, measured.ref_width);
                if entry.family == "Barlow Semi Condensed"
                    && entry.weight == "400"
                    && entry.style == "normal"
                {
                    let scale = target.ref_width / measured.ref_width;
                    heading_standin = Some(RenderedStandin {
                        ref_width: measured.ref_width * scale,
                    });
                }
                (format!("{s}%"), format!("{a}%"), format!("{d}%"))
            });
            Face {
                entry,
                metrics,
                bytes: woff2,
            }
        })
        .collect();
    (faces, heading_standin)
}

/// The frozen reference widths of the system font classes the fallbacks bind
/// to (Arial Narrow, Arial and the 0.60em-advance monospace class), measured
/// with [`REF_STRING`] at a 100px em from their Liberation metric clones.
/// Consolas is excluded from the mono sources on purpose: its 0.55em advance
/// does not fit the shared adjustment.
struct FallbackDef {
    family: &'static str,
    sources: &'static [&'static str],
    base_ref_width: f64,
    /// x-height and cap height of the base class at a 100px em, measured from
    /// the Liberation metric clones. Present only where the base's aspect
    /// differs enough from its replacement that pure width fitting looks
    /// oversized, and an optical compromise is wanted.
    base_optics: Option<(f64, f64)>,
    target: FitTarget,
}

const FALLBACKS: &[FallbackDef] = &[
    FallbackDef {
        family: "op-heading-fallback",
        sources: &["Arial Narrow", "Liberation Sans Narrow", "Roboto Condensed"],
        base_ref_width: 1887.0,
        base_optics: Some((53.2, 69.5)),
        target: SYS_TARGET,
    },
    FallbackDef {
        family: "op-body-fallback",
        sources: &["Arial", "Liberation Sans", "Helvetica Neue", "Roboto"],
        base_ref_width: 2301.0,
        base_optics: None,
        target: PLEX_SANS_TARGET,
    },
    FallbackDef {
        family: "op-mono-fallback",
        sources: &[
            "Menlo",
            "DejaVu Sans Mono",
            "Liberation Mono",
            "Courier New",
        ],
        base_ref_width: 2700.0,
        base_optics: None,
        target: PRAGMATA_TARGET,
    },
];

/// Optical size of the heading fallback relative to its base class, chosen
/// by eye on 2026-08-31 from a rendered ladder against Barlow Semi Condensed
/// (103% to 110.8%): pure width parity (110.8%) made the Arial-Narrow class
/// visibly oversized, cap parity undersized it, and 104.5% felt right. The
/// width this gives up is recovered with letter tracking, computed below from
/// the rendered stand-in so it never goes stale.
const HEADING_FALLBACK_SIZE: f64 = 1.045;

/// The rendered reference-string width of the stand-in the heading fallback
/// swaps into: its measured table width scaled by its own size adjustment.
struct RenderedStandin {
    ref_width: f64,
}

/// Extra tracking (em per character) applied to headings while the fallback
/// face is showing, sized so the fallback's lines run as long as the rendered
/// stand-in's despite the smaller optical scale.
fn heading_fallback_tracking(def: &FallbackDef, standin: &RenderedStandin) -> f64 {
    let fallback_width = def.base_ref_width * HEADING_FALLBACK_SIZE;
    let deficit_per_100em = standin.ref_width - fallback_width;
    let chars = REF_STRING.chars().count() as f64;
    (deficit_per_100em / chars / 100.0).max(0.0)
}

fn fallback_css(heading_standin: Option<&RenderedStandin>) -> String {
    let mut css = String::from(
        "/* Generated by op-assets: metric-fitted local() fallback faces. The\n   overrides are target boxes divided by the size adjustment, because\n   browsers scale override metrics by size-adjust too. The heading\n   fallback uses an optical size chosen by eye against the rendered\n   stand-in, with letter tracking recovering the width; the tracking\n   drops to zero once the font pack installs (data-op-fonts). Do not\n   edit. */\n",
    );
    for def in FALLBACKS {
        let scale = if def.base_optics.is_some() {
            HEADING_FALLBACK_SIZE
        } else {
            def.target.ref_width / def.base_ref_width
        };
        let s = round1(scale * 100.0);
        let a = round1(def.target.ascent_pct / scale);
        let d = round1(def.target.descent_pct / scale);
        let sources: Vec<String> = def
            .sources
            .iter()
            .map(|s| format!("local(\"{s}\")"))
            .collect();
        css.push_str(&format!(
            "\n@font-face {{\n  font-family: \"{}\";\n  src: {};\n  size-adjust: {s}%;\n  ascent-override: {a}%;\n  descent-override: {d}%;\n  line-gap-override: 0%;\n}}\n",
            def.family,
            sources.join(", "),
        ));
    }
    if let Some(standin) = heading_standin {
        let tracking = heading_fallback_tracking(&FALLBACKS[0], standin);
        css.push_str(&format!(
            "\n:root {{\n  --op-heading-fallback-tracking: {}em;\n}}\n\n:root[data-op-fonts=\"ready\"] {{\n  --op-heading-fallback-tracking: 0em;\n}}\n",
            round3(tracking)
        ));
    }
    css
}

fn short_hash(bytes: &[u8]) -> String {
    let hash = Sha256::digest(bytes);
    format!(
        "{:x}",
        u128::from_be_bytes(hash[..16].try_into().expect("16 bytes"))
    )
}

/// Content-hashed staging file name for a face.
fn face_file_name(face: &Face) -> String {
    let stem = Path::new(face.entry.path)
        .file_stem()
        .and_then(|s| s.to_str())
        .expect("face path has a stem")
        .to_lowercase();
    format!("font-{stem}-{}.woff2", short_hash(&face.bytes))
}

/// The `@font-face` rules for the real faces: plain woff2 URLs with
/// `font-display: swap` (the metric-fitted fallbacks cover the wait
/// without layout shift) and, for fitted families, the same override
/// descriptors the runtime registration used to set.
fn faces_css(faces: &[Face]) -> String {
    let mut css = String::from(
        "\n/* The shipped faces. Fitted families carry the size-adjust and box\n   overrides computed at build time from their own tables. */\n",
    );
    for face in faces {
        css.push_str(&format!(
            "\n@font-face {{\n  font-family: \"{}\";\n  src: url(\"/{}\") format(\"woff2\");\n  font-weight: {};\n  font-style: {};\n  font-display: swap;\n",
            face.entry.family,
            face_file_name(face),
            face.entry.weight,
            face.entry.style,
        ));
        if let Some((size_adjust, ascent, descent)) = &face.metrics {
            css.push_str(&format!(
                "  size-adjust: {size_adjust};\n  ascent-override: {ascent};\n  descent-override: {descent};\n  line-gap-override: 0%;\n"
            ));
        }
        css.push_str("}\n");
    }
    css
}

/// Head links: preloads for the first-paint faces, then the stylesheet.
/// Absolute paths: generated pages live at nested URLs. `crossorigin`
/// is required on font preloads even same-origin, or the browser
/// fetches the file twice.
fn head_links(faces: &[Face], css_name: &str) -> String {
    let mut links = String::new();
    for spec in PRELOAD {
        let face = faces
            .iter()
            .find(|f| (f.entry.family, f.entry.weight, f.entry.style) == *spec)
            .unwrap_or_else(|| panic!("preload face {spec:?} not in the manifest"));
        links.push_str(&format!(
            "<link rel=\"preload\" as=\"font\" type=\"font/woff2\" href=\"/{}\" crossorigin />",
            face_file_name(face)
        ));
    }
    links.push_str(&format!("<link rel=\"stylesheet\" href=\"/{css_name}\" />"));
    links
}

fn main() {
    let staging = std::env::var_os("TRUNK_STAGING_DIR")
        .map(PathBuf::from)
        .expect("TRUNK_STAGING_DIR is set by Trunk for hooks");
    let assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets");

    let (faces, heading_standin) = load_faces(&assets);
    for face in &faces {
        std::fs::write(staging.join(face_file_name(face)), &face.bytes).expect("write face");
    }

    let css = format!(
        "{}{}",
        fallback_css(heading_standin.as_ref()),
        faces_css(&faces)
    );
    let css_name = format!("fonts-{}.css", short_hash(css.as_bytes()));
    std::fs::write(staging.join(&css_name), &css).expect("write fonts css");

    let index = staging.join("index.html");
    let html = std::fs::read_to_string(&index).expect("read staged index.html");
    assert!(!html.contains("as=\"font\""), "font links already present");
    let html = html.replacen(
        "</head>",
        &format!("{}</head>", head_links(&faces, &css_name)),
        1,
    );
    assert!(
        html.contains("as=\"font\""),
        "no </head> in staged index.html"
    );
    std::fs::write(&index, html).expect("write staged index.html");

    println!(
        "op-assets: emitted {} woff2 files ({} preloaded) and {css_name}",
        faces.len(),
        PRELOAD.len(),
    );

    emit_sourcemap(&staging);
}

/// Maps the staged wasm's DWARF to a browser source map, serves the
/// workspace sources it references, patches the binary's
/// sourceMappingURL and refreshes the preload's integrity digest.
/// See `sourcemap` for the whole story.
fn emit_sourcemap(staging: &Path) {
    let wasm_path = std::fs::read_dir(staging)
        .expect("read staging dir")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .find(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with("_bg.wasm"))
        })
        .expect("staged *_bg.wasm");
    let wasm_name = wasm_path
        .file_name()
        .and_then(|n| n.to_str())
        .expect("wasm file name")
        .to_owned();

    let mapper = wasm2map::WASM::load(&wasm_path)
        .expect("wasm carries DWARF (debug=line-tables-only + keep-debug)");
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root");
    let sysroot = std::process::Command::new("rustc")
        .args(["--print", "sysroot"])
        .output()
        .expect("run rustc");
    let rust_src = Path::new(String::from_utf8(sysroot.stdout).expect("utf8").trim())
        .join("lib/rustlib/src/rust");
    assert!(
        rust_src.join("library").is_dir(),
        "rust-src component missing at {} (rust-toolchain.toml lists it; run any cargo command to install)",
        rust_src.display()
    );
    let registry_src = std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cargo")))
        .expect("CARGO_HOME or HOME set")
        .join("registry/src");
    let resolved = sourcemap::resolve_sources(
        &mapper.map_v3(),
        &sourcemap::Roots {
            workspace: &workspace,
            rust_src: &rust_src,
            registry_src: &registry_src,
        },
    );
    let map_name = format!("{wasm_name}.map");
    std::fs::write(staging.join(&map_name), &resolved.map).expect("write source map");
    let served = resolved.copies.len();
    for (from, to) in resolved.copies {
        let dest = staging.join(&to);
        std::fs::create_dir_all(dest.parent().expect("src dir")).expect("create src dir");
        std::fs::copy(&from, &dest)
            .unwrap_or_else(|e| panic!("copy {} to staging: {e}", from.display()));
    }
    // The stdlib licences do not ride in the rust-src component; the
    // toolchain doc dir carries the library copyright inventory and the
    // individual licence texts. Serve them beside the sources.
    let doc = rust_src
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .expect("sysroot from rust-src")
        .join("share/doc/rust");
    let rust_root = staging.join("src/rust");
    let library_copyright = doc.join("COPYRIGHT-library.html");
    assert!(
        library_copyright.is_file(),
        "toolchain lacks COPYRIGHT-library.html at {}",
        doc.display()
    );
    std::fs::create_dir_all(rust_root.join("licenses")).expect("licence dirs");
    std::fs::copy(&library_copyright, rust_root.join("COPYRIGHT-library.html"))
        .expect("copy library copyright");
    for entry in std::fs::read_dir(doc.join("licenses")).expect("read licence texts") {
        let from = entry.expect("licence entry").path();
        if from.is_file()
            && let Some(name) = from.file_name()
        {
            std::fs::copy(&from, rust_root.join("licenses").join(name)).expect("copy licence");
        }
    }

    if !resolved.unresolved.is_empty() {
        println!(
            "op-assets: {} map sources stay bare labels (crate roots erased by trim-paths): {:?} ...",
            resolved.unresolved.len(),
            &resolved.unresolved[..resolved.unresolved.len().min(3)]
        );
    }

    let mut wasm = std::fs::read(&wasm_path).expect("read staged wasm");
    assert!(
        !wasm
            .windows(b"sourceMappingURL".len())
            .any(|w| w == b"sourceMappingURL"),
        "wasm already carries a sourceMappingURL section"
    );
    wasm.extend_from_slice(&sourcemap::source_mapping_section(&format!("/{map_name}")));
    std::fs::write(&wasm_path, &wasm).expect("write patched wasm");

    use base64::Engine as _;
    let digest = base64::engine::general_purpose::STANDARD.encode(Sha384::digest(&wasm));
    let index = staging.join("index.html");
    let html = std::fs::read_to_string(&index).expect("read staged index.html");
    let html =
        sourcemap::rewrite_integrity(&html, &format!("/{wasm_name}"), &format!("sha384-{digest}"));
    std::fs::write(&index, html).expect("write staged index.html");

    println!(
        "op-assets: source map {map_name}, {served} sources served under /src/, wasm integrity refreshed"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assets() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("assets")
    }

    /// Every manifest entry must point at a real woff2 file in the assets.
    #[test]
    fn manifest_files_exist_and_are_woff2() {
        for entry in MANIFEST {
            let path = assets().join(entry.path);
            let bytes =
                std::fs::read(&path).unwrap_or_else(|e| panic!("missing {}: {e}", path.display()));
            assert!(bytes.len() > 10_000, "{} looks truncated", entry.path);
            assert_eq!(&bytes[..4], b"wOF2", "{} is not woff2", entry.path);
        }
    }

    /// The computed fits must land in sane ranges, and Iosevka SS08 must come
    /// out at unity scale: it shares PragmataPro's character advance, which is
    /// exactly why it was chosen. A drift here means the font changed.
    #[test]
    fn computed_fits_match_design_expectations() {
        let (faces, _) = load_faces(&assets());
        let metric = |family: &str| {
            faces
                .iter()
                .find(|f| {
                    f.entry.family == family && f.entry.style == "normal" && f.entry.weight == "400"
                })
                .and_then(|f| f.metrics.clone())
                .unwrap_or_else(|| panic!("{family} has no computed metrics"))
        };
        let parse = |s: &str| s.trim_end_matches('%').parse::<f64>().expect("percent");
        let (s, a, d) = metric("Iosevka SS08");
        assert!(
            (parse(&s) - 100.0).abs() <= 2.0,
            "SS08 scale {s} drifted from unity"
        );
        assert!(
            (parse(&a) - PRAGMATA_TARGET.ascent_pct).abs() <= 3.0,
            "SS08 ascent {a}"
        );
        assert!(
            (parse(&d) - PRAGMATA_TARGET.descent_pct).abs() <= 1.0,
            "SS08 descent {d}"
        );
        let (s, _, _) = metric("Barlow Semi Condensed");
        let s = parse(&s);
        assert!(
            (104.0..=114.0).contains(&s),
            "Barlow scale {s} outside expectation"
        );
        assert!(
            faces
                .iter()
                .filter(|f| f.entry.family == "IBM Plex Sans")
                .all(|f| f.metrics.is_none())
        );
    }

    /// The fallback block must contain only local() sources and one rule
    /// per fallback family, and generation must be deterministic so the
    /// content hash is reproducible.
    #[test]
    fn fallback_css_is_local_only_and_deterministic() {
        let (faces, standin) = load_faces(&assets());
        let css = fallback_css(standin.as_ref());
        assert_eq!(css.matches("@font-face").count(), FALLBACKS.len());
        assert!(!css.contains("url("));
        for def in FALLBACKS {
            assert!(css.contains(def.family));
        }
        assert_eq!(css, fallback_css(standin.as_ref()));
        assert_eq!(faces_css(&faces), faces_css(&faces));
    }

    /// The face rules must cover the whole manifest with hashed woff2
    /// URLs, swap display, and override descriptors exactly where a fit
    /// was computed; every preload spec must resolve to a real face.
    #[test]
    fn faces_css_and_preloads_cover_the_manifest() {
        let (faces, _) = load_faces(&assets());
        let css = faces_css(&faces);
        assert_eq!(css.matches("@font-face").count(), MANIFEST.len());
        assert_eq!(css.matches("url(\"/font-").count(), MANIFEST.len());
        assert_eq!(css.matches("font-display: swap").count(), MANIFEST.len());
        let fitted = faces.iter().filter(|f| f.metrics.is_some()).count();
        assert_eq!(css.matches("size-adjust:").count(), fitted);
        assert_eq!(fitted, MANIFEST.iter().filter(|e| e.fit.is_some()).count());
        let links = head_links(&faces, "fonts-test.css");
        assert_eq!(links.matches("rel=\"preload\"").count(), PRELOAD.len());
        assert_eq!(links.matches("crossorigin").count(), PRELOAD.len());
        assert!(links.ends_with("<link rel=\"stylesheet\" href=\"/fonts-test.css\" />"));
        for face in &faces {
            let name = face_file_name(face);
            assert!(name.starts_with("font-") && name.ends_with(".woff2"));
            assert!(!name.contains(' '), "{name}");
        }
    }

    /// The heading tracking rule keys off data-op-fonts="ready", which
    /// index.html must arm from document.fonts.ready now that no wasm
    /// registration path exists.
    #[test]
    fn index_html_arms_the_fonts_ready_attribute() {
        let index = include_str!("../../../index.html");
        assert!(
            index.contains("document.fonts.ready"),
            "index.html lacks the fonts.ready script"
        );
        assert!(
            index.contains("dataset.opFonts=\"ready\""),
            "index.html does not set data-op-fonts=ready"
        );
    }

    /// The heading fallback uses the eye-chosen optical size, and the
    /// tracking computed to recover the width sits in a plausible band.
    #[test]
    fn heading_fallback_size_and_tracking_are_coherent() {
        let (_, standin) = load_faces(&assets());
        let standin = standin.expect("Barlow provides the rendered stand-in width");
        let heading = &FALLBACKS[0];
        assert!(heading.base_optics.is_some());
        let tracking = heading_fallback_tracking(heading, &standin);
        assert!(
            (0.005..=0.06).contains(&tracking),
            "tracking {tracking}em implausible"
        );
        let css = fallback_css(Some(&standin));
        assert!(
            css.contains("size-adjust: 104.5%;"),
            "heading size not applied"
        );
        assert!(
            css.contains("--op-heading-fallback-tracking:"),
            "tracking rule missing"
        );
        assert!(
            css.contains("data-op-fonts=\"ready\""),
            "ready zeroing missing"
        );
    }

    /// The mono fallback adjustment must fit the 0.60em-advance class.
    #[test]
    fn fallback_fits_are_sane() {
        for def in FALLBACKS {
            let (s, a, d) = fit_percentages(&def.target, def.base_ref_width);
            assert!((70.0..=125.0).contains(&s), "{}: scale {s}", def.family);
            assert!(a > 50.0 && d > 5.0, "{}: box {a}/{d}", def.family);
        }
        let (s, _, _) = fit_percentages(&PRAGMATA_TARGET, 2700.0);
        assert!((82.0..=85.0).contains(&s));
    }
}
