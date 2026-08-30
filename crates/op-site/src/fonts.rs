//! Every webfont, embedded in the wasm binary and registered at runtime with
//! the CSS Font Loading API. No `@font-face` rules and no fetchable font URLs
//! exist; the stacks in `styles/theme.css` and on the specimen page end in
//! system fallbacks for the no-wasm case.
//!
//! Only the faces the chosen typography actually uses are embedded; every
//! token in `styles/theme.css` ends in a curated system-font tail for
//! environments where this registration path never runs (no JavaScript, no
//! WebAssembly, or no CSS Font Loading API).
//! All embedded faces are SIL OFL 1.1 (licences alongside the files, see
//! `crates/op-site/assets/fonts/README.md`). Iosevka and Space Grotesk are
//! public stand-ins for the licensed commercial faces PragmataPro and Sys 2.0,
//! which are kept out of the repository for now; stacks list the commercial
//! family first, so browsers with those fonts installed locally use them.
//! The stand-ins are fitted to their originals: Iosevka SS08 is the
//! "PragmataPro Style" stylistic set with PragmataPro's vertical metrics
//! applied, and Barlow Semi Condensed is scaled and metric-overridden to
//! Sys 2.0's measured geometry. Iosevka is subsetted to the site's Latin and
//! technical ranges (its licence declares no Reserved Font Name, so
//! subsetting under the same name is permitted).

use wasm_bindgen::JsValue;
use web_sys::{FontFace, FontFaceDescriptors};

struct EmbeddedFace {
    family: &'static str,
    /// A CSS `font-weight` value: a single weight or a variable range.
    weight: &'static str,
    style: &'static str,
    /// `(sizeAdjust, ascentOverride, descentOverride)` percentages applied at
    /// registration, used to fit a stand-in to its original's measured
    /// geometry; the line gap override is always "0%" when metrics are given.
    metrics: Option<(&'static str, &'static str, &'static str)>,
    bytes: &'static [u8],
}

macro_rules! face {
    ($family:literal, $weight:literal, $style:literal, $path:literal) => {
        face!($family, $weight, $style, $path, None)
    };
    ($family:literal, $weight:literal, $style:literal, $path:literal, $metrics:expr) => {
        EmbeddedFace {
            family: $family,
            weight: $weight,
            style: $style,
            metrics: $metrics,
            bytes: include_bytes!(concat!("../assets/fonts/", $path)),
        }
    };
}

/// Fits Barlow Semi Condensed to Sys 2.0: canvas-measured against the
/// original, a 107% scale puts width, x-height and cap height within about
/// 2.5% of Sys, and the overrides clone Sys's ascent/descent box.
const SYS_FIT: Option<(&str, &str, &str)> = Some(("107%", "98%", "18%"));

/// Fits Iosevka SS08 (the "PragmataPro Style" stylistic set) to PragmataPro's
/// ascent/descent box. No size-adjust: the two families already share the
/// same character advance, which matters more for code than a small x-height
/// difference.
const PRAGMATA_FIT: Option<(&str, &str, &str)> = Some(("100%", "92%", "18%"));

const FACES: &[EmbeddedFace] = &[
    face!(
        "IBM Plex Sans",
        "400",
        "normal",
        "plex-sans/IBMPlexSans-Regular.woff2"
    ),
    face!(
        "IBM Plex Sans",
        "600",
        "normal",
        "plex-sans/IBMPlexSans-SemiBold.woff2"
    ),
    face!(
        "IBM Plex Sans",
        "700",
        "normal",
        "plex-sans/IBMPlexSans-Bold.woff2"
    ),
    face!(
        "IBM Plex Sans",
        "400",
        "italic",
        "plex-sans/IBMPlexSans-Italic.woff2"
    ),
    face!(
        "Iosevka SS08",
        "400",
        "normal",
        "iosevka-ss08/IosevkaSS08-Regular.woff2",
        PRAGMATA_FIT
    ),
    face!(
        "Iosevka SS08",
        "700",
        "normal",
        "iosevka-ss08/IosevkaSS08-Bold.woff2",
        PRAGMATA_FIT
    ),
    face!(
        "Iosevka SS08",
        "400",
        "italic",
        "iosevka-ss08/IosevkaSS08-Italic.woff2",
        PRAGMATA_FIT
    ),
    face!(
        "Iosevka SS08",
        "700",
        "italic",
        "iosevka-ss08/IosevkaSS08-BoldItalic.woff2",
        PRAGMATA_FIT
    ),
    face!(
        "Barlow Semi Condensed",
        "400",
        "normal",
        "barlow-semi-condensed/BarlowSemiCondensed-Regular.woff2",
        SYS_FIT
    ),
    face!(
        "Barlow Semi Condensed",
        "700",
        "normal",
        "barlow-semi-condensed/BarlowSemiCondensed-Bold.woff2",
        SYS_FIT
    ),
    face!(
        "Barlow Semi Condensed",
        "400",
        "italic",
        "barlow-semi-condensed/BarlowSemiCondensed-Italic.woff2",
        SYS_FIT
    ),
    face!(
        "Barlow Semi Condensed",
        "700",
        "italic",
        "barlow-semi-condensed/BarlowSemiCondensed-BoldItalic.woff2",
        SYS_FIT
    ),
];

/// Registers every embedded face with `document.fonts`. A face that fails to
/// register is simply absent from the set; every stack ends in system
/// fallbacks.
pub fn install() {
    let Some(fonts) = web_sys::window()
        .and_then(|w| w.document())
        .map(|d| d.fonts())
    else {
        return;
    };
    for face in FACES {
        let descriptors = FontFaceDescriptors::new();
        descriptors.set_weight(face.weight);
        descriptors.set_style(face.style);
        if let Some((size_adjust, ascent, descent)) = face.metrics {
            // These descriptors are recent additions; set them on the
            // dictionary object directly so the web-sys version does not
            // matter.
            for (key, value) in [
                ("sizeAdjust", size_adjust),
                ("ascentOverride", ascent),
                ("descentOverride", descent),
                ("lineGapOverride", "0%"),
            ] {
                let _ = js_sys::Reflect::set(
                    descriptors.as_ref(),
                    &JsValue::from_str(key),
                    &JsValue::from_str(value),
                );
            }
        }
        if let Ok(font_face) =
            FontFace::new_with_u8_array_and_descriptors(face.family, face.bytes, &descriptors)
        {
            let _ = fonts.add(&font_face);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_embedded_face_is_woff2_and_nonempty() {
        assert_eq!(FACES.len(), 12);
        for face in FACES {
            assert!(
                face.bytes.len() > 10_000,
                "{} {} looks truncated",
                face.family,
                face.weight
            );
            assert_eq!(
                &face.bytes[..4],
                b"wOF2",
                "{} {} is not woff2",
                face.family,
                face.weight
            );
        }
    }

    #[test]
    fn families_weights_and_styles_are_consistent() {
        let count = |family: &str| FACES.iter().filter(|f| f.family == family).count();
        assert_eq!(count("IBM Plex Sans"), 4);
        assert_eq!(count("Iosevka SS08"), 4);
        assert_eq!(count("Barlow Semi Condensed"), 4);
        for f in FACES {
            assert!(matches!(f.style, "normal" | "italic"));
            let weights: Vec<&str> = f.weight.split_whitespace().collect();
            assert!(
                matches!(weights.len(), 1 | 2),
                "{}: bad weight {}",
                f.family,
                f.weight
            );
            for w in weights {
                let w: u16 = w.parse().expect("numeric weight");
                assert!((100..=900).contains(&w));
            }
        }
    }
}
