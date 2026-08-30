//! The font pack: a single binary container holding every webfont face, its
//! CSS descriptors and fit metrics.
//!
//! `op-assets` encodes the pack at build time (a Trunk `post_build` hook) from
//! the files in `crates/op-site/assets/fonts`; the site's wasm fetches and
//! decodes it after first paint and registers each face through the CSS Font
//! Loading API. One format module shared by both sides keeps them in
//! lockstep, and the container is deliberately not a font file: there are no
//! fetchable font URLs, matching the embedded-fonts policy.
//!
//! Layout, all integers little-endian:
//!
//! ```text
//! magic "OPF1" | u32 face count | faces... | data...
//! face: u16-length-prefixed strings (family, weight, style,
//!       size_adjust, ascent_override, descent_override; empty = no fit)
//!       then u32 data length; data segments follow in face order.
//! ```

/// One face: descriptors, fit metrics and the woff2 bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Face {
    pub family: String,
    pub weight: String,
    pub style: String,
    /// `(sizeAdjust, ascentOverride, descentOverride)`; `None` = register as is.
    pub metrics: Option<(String, String, String)>,
    pub bytes: Vec<u8>,
}

/// Static description of a face in the source tree, used by the encoder.
pub struct ManifestEntry {
    pub family: &'static str,
    pub weight: &'static str,
    pub style: &'static str,
    pub metrics: Option<(&'static str, &'static str, &'static str)>,
    /// Path relative to `crates/op-site/assets/fonts`.
    pub path: &'static str,
}

/// Fits Barlow Semi Condensed to Sys 2.0: canvas-measured against the
/// original, a 107% scale puts width, x-height and cap height within about
/// 2.5% of Sys, and the overrides clone Sys's ascent/descent box.
pub const SYS_FIT: Option<(&str, &str, &str)> = Some(("107%", "98%", "18%"));

/// Fits Iosevka SS08 (the "PragmataPro Style" stylistic set) to PragmataPro's
/// ascent/descent box. No size-adjust: the two families already share the
/// same character advance, which matters more for code than a small x-height
/// difference.
pub const PRAGMATA_FIT: Option<(&str, &str, &str)> = Some(("100%", "92%", "18%"));

/// Every face the site ships, in registration order.
pub const MANIFEST: &[ManifestEntry] = &[
    ManifestEntry {
        family: "IBM Plex Sans",
        weight: "400",
        style: "normal",
        metrics: None,
        path: "plex-sans/IBMPlexSans-Regular.woff2",
    },
    ManifestEntry {
        family: "IBM Plex Sans",
        weight: "600",
        style: "normal",
        metrics: None,
        path: "plex-sans/IBMPlexSans-SemiBold.woff2",
    },
    ManifestEntry {
        family: "IBM Plex Sans",
        weight: "700",
        style: "normal",
        metrics: None,
        path: "plex-sans/IBMPlexSans-Bold.woff2",
    },
    ManifestEntry {
        family: "IBM Plex Sans",
        weight: "400",
        style: "italic",
        metrics: None,
        path: "plex-sans/IBMPlexSans-Italic.woff2",
    },
    ManifestEntry {
        family: "Iosevka SS08",
        weight: "400",
        style: "normal",
        metrics: PRAGMATA_FIT,
        path: "iosevka-ss08/IosevkaSS08-Regular.woff2",
    },
    ManifestEntry {
        family: "Iosevka SS08",
        weight: "700",
        style: "normal",
        metrics: PRAGMATA_FIT,
        path: "iosevka-ss08/IosevkaSS08-Bold.woff2",
    },
    ManifestEntry {
        family: "Iosevka SS08",
        weight: "400",
        style: "italic",
        metrics: PRAGMATA_FIT,
        path: "iosevka-ss08/IosevkaSS08-Italic.woff2",
    },
    ManifestEntry {
        family: "Iosevka SS08",
        weight: "700",
        style: "italic",
        metrics: PRAGMATA_FIT,
        path: "iosevka-ss08/IosevkaSS08-BoldItalic.woff2",
    },
    ManifestEntry {
        family: "Barlow Semi Condensed",
        weight: "400",
        style: "normal",
        metrics: SYS_FIT,
        path: "barlow-semi-condensed/BarlowSemiCondensed-Regular.woff2",
    },
    ManifestEntry {
        family: "Barlow Semi Condensed",
        weight: "700",
        style: "normal",
        metrics: SYS_FIT,
        path: "barlow-semi-condensed/BarlowSemiCondensed-Bold.woff2",
    },
    ManifestEntry {
        family: "Barlow Semi Condensed",
        weight: "400",
        style: "italic",
        metrics: SYS_FIT,
        path: "barlow-semi-condensed/BarlowSemiCondensed-Italic.woff2",
    },
    ManifestEntry {
        family: "Barlow Semi Condensed",
        weight: "700",
        style: "italic",
        metrics: SYS_FIT,
        path: "barlow-semi-condensed/BarlowSemiCondensed-BoldItalic.woff2",
    },
];

pub const MAGIC: &[u8; 4] = b"OPF1";

fn push_str(out: &mut Vec<u8>, s: &str) {
    let len = u16::try_from(s.len()).expect("string fits u16");
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(s.as_bytes());
}

/// Encodes faces into a pack.
pub fn encode(faces: &[Face]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(
        &u32::try_from(faces.len())
            .expect("face count fits u32")
            .to_le_bytes(),
    );
    for face in faces {
        push_str(&mut out, &face.family);
        push_str(&mut out, &face.weight);
        push_str(&mut out, &face.style);
        let (a, b, c) = face
            .metrics
            .as_ref()
            .map(|(a, b, c)| (a.as_str(), b.as_str(), c.as_str()))
            .unwrap_or(("", "", ""));
        push_str(&mut out, a);
        push_str(&mut out, b);
        push_str(&mut out, c);
        out.extend_from_slice(
            &u32::try_from(face.bytes.len())
                .expect("face size fits u32")
                .to_le_bytes(),
        );
    }
    for face in faces {
        out.extend_from_slice(&face.bytes);
    }
    out
}

#[derive(Debug, PartialEq, Eq)]
pub struct DecodeError(pub &'static str);

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], DecodeError> {
        let end = self.pos.checked_add(n).ok_or(DecodeError("overflow"))?;
        let slice = self
            .data
            .get(self.pos..end)
            .ok_or(DecodeError("truncated"))?;
        self.pos = end;
        Ok(slice)
    }

    fn u16(&mut self) -> Result<u16, DecodeError> {
        Ok(u16::from_le_bytes(
            self.take(2)?.try_into().expect("2 bytes"),
        ))
    }

    fn u32(&mut self) -> Result<u32, DecodeError> {
        Ok(u32::from_le_bytes(
            self.take(4)?.try_into().expect("4 bytes"),
        ))
    }

    fn string(&mut self) -> Result<String, DecodeError> {
        let len = self.u16()? as usize;
        let bytes = self.take(len)?;
        String::from_utf8(bytes.to_vec()).map_err(|_| DecodeError("invalid utf-8"))
    }
}

/// Decodes a pack produced by [`encode`].
pub fn decode(data: &[u8]) -> Result<Vec<Face>, DecodeError> {
    let mut r = Reader { data, pos: 0 };
    if r.take(4)? != MAGIC {
        return Err(DecodeError("bad magic"));
    }
    let count = r.u32()? as usize;
    if count > 1024 {
        return Err(DecodeError("implausible face count"));
    }
    let mut headers = Vec::with_capacity(count);
    for _ in 0..count {
        let family = r.string()?;
        let weight = r.string()?;
        let style = r.string()?;
        let a = r.string()?;
        let b = r.string()?;
        let c = r.string()?;
        let len = r.u32()? as usize;
        let metrics = if a.is_empty() && b.is_empty() && c.is_empty() {
            None
        } else {
            Some((a, b, c))
        };
        headers.push((family, weight, style, metrics, len));
    }
    let mut faces = Vec::with_capacity(count);
    for (family, weight, style, metrics, len) in headers {
        let bytes = r.take(len)?.to_vec();
        faces.push(Face {
            family,
            weight,
            style,
            metrics,
            bytes,
        });
    }
    if r.pos != data.len() {
        return Err(DecodeError("trailing bytes"));
    }
    Ok(faces)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<Face> {
        vec![
            Face {
                family: "A".into(),
                weight: "400".into(),
                style: "normal".into(),
                metrics: None,
                bytes: vec![1, 2, 3],
            },
            Face {
                family: "B Face".into(),
                weight: "300 700".into(),
                style: "italic".into(),
                metrics: Some(("107%".into(), "98%".into(), "18%".into())),
                bytes: vec![0; 70_000],
            },
        ]
    }

    #[test]
    fn round_trips_faces_exactly() {
        let faces = sample();
        assert_eq!(decode(&encode(&faces)).expect("decode"), faces);
    }

    #[test]
    fn rejects_corruption() {
        let good = encode(&sample());
        assert_eq!(decode(&good[..3]), Err(DecodeError("truncated")));
        let mut bad_magic = good.clone();
        bad_magic[0] = b'X';
        assert_eq!(decode(&bad_magic), Err(DecodeError("bad magic")));
        let mut truncated = good.clone();
        truncated.truncate(good.len() - 1);
        assert_eq!(decode(&truncated), Err(DecodeError("truncated")));
        let mut trailing = good.clone();
        trailing.push(0);
        assert_eq!(decode(&trailing), Err(DecodeError("trailing bytes")));
        let mut absurd = good.clone();
        absurd[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(decode(&absurd), Err(DecodeError("implausible face count")));
    }

    #[test]
    fn manifest_is_consistent() {
        assert_eq!(MANIFEST.len(), 12);
        for entry in MANIFEST {
            assert!(matches!(entry.style, "normal" | "italic"));
            for w in entry.weight.split_whitespace() {
                let w: u16 = w.parse().expect("numeric weight");
                assert!((100..=900).contains(&w));
            }
            assert!(entry.path.ends_with(".woff2"));
            assert!(!entry.path.starts_with('/') && !entry.path.contains(".."));
        }
        let families: std::collections::BTreeSet<_> = MANIFEST.iter().map(|e| e.family).collect();
        assert_eq!(
            families.into_iter().collect::<Vec<_>>(),
            ["Barlow Semi Condensed", "IBM Plex Sans", "Iosevka SS08"]
        );
    }
}
