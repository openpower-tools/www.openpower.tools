//! sRGB, as 8-bit hex and as linear light.

/// An sRGB colour with components in 0..=1 (gamma encoded).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Srgb {
    pub r: f64,
    pub g: f64,
    pub b: f64,
}

/// Linear-light RGB with the sRGB primaries (D65).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Linear {
    pub r: f64,
    pub g: f64,
    pub b: f64,
}

fn decode(c: f64) -> f64 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn encode(c: f64) -> f64 {
    if c <= 0.003_130_8 {
        12.92 * c
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

impl Srgb {
    /// Parse `#RRGGBB` (case-insensitive).
    pub fn from_hex(hex: &str) -> Option<Self> {
        let h = hex.strip_prefix('#')?;
        if h.len() != 6 {
            return None;
        }
        let byte = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok();
        Some(Self {
            r: f64::from(byte(0)?) / 255.0,
            g: f64::from(byte(2)?) / 255.0,
            b: f64::from(byte(4)?) / 255.0,
        })
    }

    /// `#RRGGBB`, upper case, each channel rounded to the nearest byte.
    pub fn to_hex(self) -> String {
        let q = |c: f64| (c.clamp(0.0, 1.0) * 255.0).round() as u8;
        format!("#{:02X}{:02X}{:02X}", q(self.r), q(self.g), q(self.b))
    }

    /// Whether every channel lies inside the sRGB gamut, with a small
    /// tolerance for floating point.
    pub fn in_gamut(self) -> bool {
        let ok = |c: f64| (-1e-9..=1.0 + 1e-9).contains(&c);
        ok(self.r) && ok(self.g) && ok(self.b)
    }

    pub fn to_linear(self) -> Linear {
        Linear {
            r: decode(self.r),
            g: decode(self.g),
            b: decode(self.b),
        }
    }

    /// The channels quantised to 8 bits and back, which is what a browser
    /// will actually display for the hex form.
    pub fn quantised(self) -> Self {
        Self::from_hex(&self.to_hex()).expect("hex round trip")
    }
}

impl Linear {
    pub fn to_srgb(self) -> Srgb {
        Srgb {
            r: encode(self.r),
            g: encode(self.g),
            b: encode(self.b),
        }
    }

    /// CIE 1931 XYZ for the D65 white (the sRGB matrix from IEC 61966-2-1).
    pub fn to_xyz(self) -> [f64; 3] {
        [
            0.412_456_4 * self.r + 0.357_576_1 * self.g + 0.180_437_5 * self.b,
            0.212_672_9 * self.r + 0.715_152_2 * self.g + 0.072_175_0 * self.b,
            0.019_333_9 * self.r + 0.119_192_0 * self.g + 0.950_304_1 * self.b,
        ]
    }

    /// Relative luminance Y, as WCAG 2 defines it.
    pub fn luminance(self) -> f64 {
        0.2126 * self.r + 0.7152 * self.g + 0.0722 * self.b
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn hex_parses_and_prints() {
        let c = Srgb::from_hex("#2b415f").unwrap();
        assert_eq!(c.to_hex(), "#2B415F");
        assert!(Srgb::from_hex("2B415F").is_none());
        assert!(Srgb::from_hex("#2B41").is_none());
    }

    #[test]
    fn transfer_curve_matches_the_standard_at_its_anchors() {
        // IEC 61966-2-1: 0.04045 encoded is 0.0031308 linear, and mid grey
        // #808080 is 0.2158 linear (the familiar 21.6%).
        assert!((decode(0.04045) - 0.003_130_8).abs() < 1e-7);
        let mid = Srgb::from_hex("#808080").unwrap().to_linear();
        assert!((mid.r - 0.215_86).abs() < 1e-4, "{}", mid.r);
        // white is Y = 1 and XYZ is the D65 white point
        let w = Srgb::from_hex("#FFFFFF").unwrap().to_linear().to_xyz();
        assert!(
            (w[0] - 0.9505).abs() < 1e-3
                && (w[1] - 1.0).abs() < 1e-6
                && (w[2] - 1.089).abs() < 1e-3
        );
    }

    proptest! {
        #[test]
        fn linear_and_encoded_forms_round_trip(r in 0.0f64..=1.0, g in 0.0f64..=1.0, b in 0.0f64..=1.0) {
            let c = Srgb { r, g, b };
            let back = c.to_linear().to_srgb();
            prop_assert!((back.r - r).abs() < 1e-9 && (back.g - g).abs() < 1e-9 && (back.b - b).abs() < 1e-9);
        }
    }
}
