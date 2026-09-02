//! The webfont manifest: every face the site ships, the frozen metric
//! targets of the licensed originals they stand in for, and the
//! measurement reference string. Formerly the static half of the
//! `op-fontpack` container crate; the pack itself is gone - faces are
//! served as plain preloaded woff2 files declared in generated CSS.

/// Static description of a face in the source tree, used by the encoder.
pub struct ManifestEntry {
    pub family: &'static str,
    pub weight: &'static str,
    pub style: &'static str,
    /// The original this face replaces; `None` embeds the face unfitted.
    /// `op-assets` computes the size-adjust and box overrides at build time
    /// from the face's own tables against this target.
    pub fit: Option<FitTarget>,
    /// Path relative to `crates/op-assets/assets`.
    pub path: &'static str,
}

/// Reference string for width measurement; the same string was used to
/// canvas-measure the licensed originals.
pub const REF_STRING: &str = "OpenPOWER firmware ports and tools 0123456789";

/// Metric targets for a fitted family: the geometry of the original it
/// replaces, measured at a 100px em on 2026-08-31 from the licensed fonts
/// (which are deliberately not in this repository). These are frozen designs;
/// the volatile side, our embedded files, is measured from the actual font
/// tables at build time by `op-assets`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FitTarget {
    /// Advance width of [`REF_STRING`] at a 100px em.
    pub ref_width: f64,
    /// Ascent as a percentage of the em.
    pub ascent_pct: f64,
    /// Descent as a percentage of the em.
    pub descent_pct: f64,
}

/// Sys 2.0.
pub const SYS_TARGET: FitTarget = FitTarget {
    ref_width: 2091.0,
    ascent_pct: 98.0,
    descent_pct: 18.0,
};

/// PragmataPro Liga.
pub const PRAGMATA_TARGET: FitTarget = FitTarget {
    ref_width: 2250.0,
    ascent_pct: 92.0,
    descent_pct: 18.0,
};

/// IBM Plex Sans (embedded as-is; the target exists for its fallback face).
pub const PLEX_SANS_TARGET: FitTarget = FitTarget {
    ref_width: 2328.0,
    ascent_pct: 103.0,
    descent_pct: 28.0,
};

/// Every face the site ships, in registration order.
pub const MANIFEST: &[ManifestEntry] = &[
    ManifestEntry {
        family: "IBM Plex Sans",
        weight: "400",
        style: "normal",
        fit: None,
        path: "plex-sans/IBMPlexSans-Regular.woff2",
    },
    ManifestEntry {
        family: "IBM Plex Sans",
        weight: "600",
        style: "normal",
        fit: None,
        path: "plex-sans/IBMPlexSans-SemiBold.woff2",
    },
    ManifestEntry {
        family: "IBM Plex Sans",
        weight: "700",
        style: "normal",
        fit: None,
        path: "plex-sans/IBMPlexSans-Bold.woff2",
    },
    ManifestEntry {
        family: "IBM Plex Sans",
        weight: "400",
        style: "italic",
        fit: None,
        path: "plex-sans/IBMPlexSans-Italic.woff2",
    },
    ManifestEntry {
        family: "Iosevka SS08",
        weight: "400",
        style: "normal",
        fit: Some(PRAGMATA_TARGET),
        path: "iosevka-ss08/IosevkaSS08-Regular.woff2",
    },
    ManifestEntry {
        family: "Iosevka SS08",
        weight: "700",
        style: "normal",
        fit: Some(PRAGMATA_TARGET),
        path: "iosevka-ss08/IosevkaSS08-Bold.woff2",
    },
    ManifestEntry {
        family: "Iosevka SS08",
        weight: "400",
        style: "italic",
        fit: Some(PRAGMATA_TARGET),
        path: "iosevka-ss08/IosevkaSS08-Italic.woff2",
    },
    ManifestEntry {
        family: "Iosevka SS08",
        weight: "700",
        style: "italic",
        fit: Some(PRAGMATA_TARGET),
        path: "iosevka-ss08/IosevkaSS08-BoldItalic.woff2",
    },
    ManifestEntry {
        family: "Barlow Semi Condensed",
        weight: "400",
        style: "normal",
        fit: Some(SYS_TARGET),
        path: "barlow-semi-condensed/BarlowSemiCondensed-Regular.woff2",
    },
    ManifestEntry {
        family: "Barlow Semi Condensed",
        weight: "700",
        style: "normal",
        fit: Some(SYS_TARGET),
        path: "barlow-semi-condensed/BarlowSemiCondensed-Bold.woff2",
    },
    ManifestEntry {
        family: "Barlow Semi Condensed",
        weight: "400",
        style: "italic",
        fit: Some(SYS_TARGET),
        path: "barlow-semi-condensed/BarlowSemiCondensed-Italic.woff2",
    },
    ManifestEntry {
        family: "Barlow Semi Condensed",
        weight: "700",
        style: "italic",
        fit: Some(SYS_TARGET),
        path: "barlow-semi-condensed/BarlowSemiCondensed-BoldItalic.woff2",
    },
];
