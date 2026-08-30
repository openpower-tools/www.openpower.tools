//! sRGB colour maths shared by the palette tests and the specimen element.
//!
//! WCAG 2.x contrast ratio: (L1 + 0.05) / (L2 + 0.05) with relative luminance
//! L from sRGB.

/// An sRGB colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    /// Parses `#RRGGBB`.
    pub fn from_hex(text: &str) -> Option<Self> {
        let hex = text.trim().strip_prefix('#')?;
        if hex.len() != 6 {
            return None;
        }
        let channel = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
        Some(Self(channel(0)?, channel(2)?, channel(4)?))
    }

    /// Parses the `rgb(r, g, b)` / `rgba(r, g, b, a)` form browsers return from
    /// `getComputedStyle`; alpha is ignored.
    pub fn from_css_rgb(text: &str) -> Option<Self> {
        let inner = text.trim().strip_prefix("rgb")?.trim_start_matches('a');
        let inner = inner.strip_prefix('(')?.strip_suffix(')')?;
        let mut parts = inner.split([',', '/', ' ']).filter(|s| !s.is_empty());
        let mut channel = || parts.next()?.trim().parse::<u8>().ok();
        Some(Self(channel()?, channel()?, channel()?))
    }

    pub fn to_hex(self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.0, self.1, self.2)
    }

    /// Relative luminance per WCAG 2.x.
    pub fn luminance(self) -> f64 {
        fn linear(c: u8) -> f64 {
            let c = f64::from(c) / 255.0;
            if c <= 0.03928 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        }
        0.2126 * linear(self.0) + 0.7152 * linear(self.1) + 0.0722 * linear(self.2)
    }
}

/// WCAG contrast ratio between two colours, always >= 1.
pub fn contrast(a: Rgb, b: Rgb) -> f64 {
    let (la, lb) = (a.luminance(), b.luminance());
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hex_and_css_rgb_forms() {
        assert_eq!(Rgb::from_hex("#0A1546"), Some(Rgb(10, 21, 70)));
        assert_eq!(Rgb::from_hex(" #ffffff "), Some(Rgb(255, 255, 255)));
        assert_eq!(Rgb::from_hex("#fff"), None);
        assert_eq!(Rgb::from_hex("0A1546"), None);
        assert_eq!(Rgb::from_css_rgb("rgb(2, 2, 2)"), Some(Rgb(2, 2, 2)));
        assert_eq!(
            Rgb::from_css_rgb("rgba(233, 240, 248, 0.5)"),
            Some(Rgb(233, 240, 248))
        );
        assert_eq!(
            Rgb::from_css_rgb("rgb(233 240 248 / 1)"),
            Some(Rgb(233, 240, 248))
        );
        assert_eq!(Rgb::from_css_rgb("transparent"), None);
        assert_eq!(Rgb(2, 2, 2).to_hex(), "#020202");
        assert_eq!(
            Rgb::from_hex(&Rgb(171, 205, 239).to_hex()),
            Some(Rgb(171, 205, 239))
        );
    }

    #[test]
    fn luminance_and_contrast_match_reference_values() {
        let white = Rgb::from_hex("#FFFFFF").unwrap();
        let black = Rgb::from_hex("#000000").unwrap();
        assert!((white.luminance() - 1.0).abs() < 1e-9);
        assert!(black.luminance().abs() < 1e-9);
        assert!((contrast(black, white) - 21.0).abs() < 1e-9);
        assert!((contrast(white, black) - 21.0).abs() < 1e-9);
        // Published reference points: #767676 is the lightest grey that passes AA
        // on white at 4.54:1, and #777777 just fails at 4.48:1.
        assert!((contrast(Rgb::from_hex("#767676").unwrap(), white) - 4.54).abs() < 0.01);
        assert!((contrast(Rgb::from_hex("#777777").unwrap(), white) - 4.48).abs() < 0.01);
    }
}
