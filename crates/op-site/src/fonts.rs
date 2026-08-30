//! Every webfont, embedded in the wasm binary and registered at runtime with
//! the CSS Font Loading API. No `@font-face` rules and no fetchable font URLs
//! exist; the stacks in `styles/theme.css` and on the specimen page end in
//! system fallbacks for the no-wasm case.
//!
//! All embedded faces are SIL OFL 1.1 (licences alongside the files, see
//! `crates/op-site/assets/fonts/README.md`). Iosevka and Space Grotesk are
//! public stand-ins for the licensed commercial faces PragmataPro and Sys 2.0,
//! which are kept out of the repository for now; stacks list the commercial
//! family first, so browsers with those fonts installed locally use them.
//! Iosevka is subsetted to the site's Latin and technical ranges (its licence
//! declares no Reserved Font Name, so subsetting under the same name is
//! permitted); Space Grotesk is the official variable build.

use web_sys::{FontFace, FontFaceDescriptors};

struct EmbeddedFace {
    family: &'static str,
    /// A CSS `font-weight` value: a single weight or a variable range.
    weight: &'static str,
    style: &'static str,
    bytes: &'static [u8],
}

macro_rules! face {
    ($family:literal, $weight:literal, $style:literal, $path:literal) => {
        EmbeddedFace {
            family: $family,
            weight: $weight,
            style: $style,
            bytes: include_bytes!(concat!("../assets/fonts/", $path)),
        }
    };
}

const FACES: &[EmbeddedFace] = &[
    face!("B612", "400", "normal", "b612/B612-Regular.woff2"),
    face!("B612", "700", "normal", "b612/B612-Bold.woff2"),
    face!("B612", "400", "italic", "b612/B612-Italic.woff2"),
    face!("B612", "700", "italic", "b612/B612-BoldItalic.woff2"),
    face!("B612 Mono", "400", "normal", "b612/B612Mono-Regular.woff2"),
    face!("B612 Mono", "700", "normal", "b612/B612Mono-Bold.woff2"),
    face!("B612 Mono", "400", "italic", "b612/B612Mono-Italic.woff2"),
    face!(
        "B612 Mono",
        "700",
        "italic",
        "b612/B612Mono-BoldItalic.woff2"
    ),
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
        "IBM Plex Mono",
        "400",
        "normal",
        "plex-mono/IBMPlexMono-Regular.woff2"
    ),
    face!(
        "IBM Plex Mono",
        "700",
        "normal",
        "plex-mono/IBMPlexMono-Bold.woff2"
    ),
    face!(
        "IBM Plex Mono",
        "400",
        "italic",
        "plex-mono/IBMPlexMono-Italic.woff2"
    ),
    face!(
        "IBM Plex Serif",
        "400",
        "normal",
        "plex-serif/IBMPlexSerif-Regular.woff2"
    ),
    face!(
        "IBM Plex Serif",
        "700",
        "normal",
        "plex-serif/IBMPlexSerif-Bold.woff2"
    ),
    face!(
        "IBM Plex Serif",
        "400",
        "italic",
        "plex-serif/IBMPlexSerif-Italic.woff2"
    ),
    face!("Iosevka", "400", "normal", "iosevka/Iosevka-Regular.woff2"),
    face!("Iosevka", "700", "normal", "iosevka/Iosevka-Bold.woff2"),
    face!("Iosevka", "400", "italic", "iosevka/Iosevka-Italic.woff2"),
    face!(
        "Iosevka",
        "700",
        "italic",
        "iosevka/Iosevka-BoldItalic.woff2"
    ),
    face!(
        "Space Grotesk",
        "300 700",
        "normal",
        "space-grotesk/SpaceGrotesk-Variable.woff2"
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
        assert_eq!(FACES.len(), 23);
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
        assert_eq!(count("B612"), 4);
        assert_eq!(count("B612 Mono"), 4);
        assert_eq!(count("IBM Plex Sans"), 4);
        assert_eq!(count("IBM Plex Mono"), 3);
        assert_eq!(count("IBM Plex Serif"), 3);
        assert_eq!(count("Iosevka"), 4);
        assert_eq!(count("Space Grotesk"), 1);
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
