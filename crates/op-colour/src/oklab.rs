//! OKLab and OKLCH (Bjorn Ottosson, 2020), from and to linear sRGB.

use crate::srgb::{Linear, Srgb};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Oklab {
    pub l: f64,
    pub a: f64,
    pub b: f64,
}

/// Polar OKLab: lightness, chroma, hue in degrees (0..360).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Oklch {
    pub l: f64,
    pub c: f64,
    pub h: f64,
}

impl Oklab {
    pub fn from_linear(c: Linear) -> Self {
        let l = 0.412_221_470_8 * c.r + 0.536_332_536_3 * c.g + 0.051_445_992_9 * c.b;
        let m = 0.211_903_498_2 * c.r + 0.680_699_545_1 * c.g + 0.107_396_956_6 * c.b;
        let s = 0.088_302_461_9 * c.r + 0.281_718_837_6 * c.g + 0.629_978_700_5 * c.b;
        let (l, m, s) = (l.cbrt(), m.cbrt(), s.cbrt());
        Self {
            l: 0.210_454_255_3 * l + 0.793_617_785_0 * m - 0.004_072_046_8 * s,
            a: 1.977_998_495_1 * l - 2.428_592_205_0 * m + 0.450_593_709_9 * s,
            b: 0.025_904_037_1 * l + 0.782_771_766_2 * m - 0.808_675_766_0 * s,
        }
    }

    pub fn from_srgb(c: Srgb) -> Self {
        Self::from_linear(c.to_linear())
    }

    pub fn to_linear(self) -> Linear {
        let l = self.l + 0.396_337_777_4 * self.a + 0.215_803_757_3 * self.b;
        let m = self.l - 0.105_561_345_8 * self.a - 0.063_854_172_8 * self.b;
        let s = self.l - 0.089_484_177_5 * self.a - 1.291_485_548_0 * self.b;
        let (l, m, s) = (l * l * l, m * m * m, s * s * s);
        Linear {
            r: 4.076_741_662_1 * l - 3.307_711_591_3 * m + 0.230_969_929_2 * s,
            g: -1.268_438_004_6 * l + 2.609_757_401_1 * m - 0.341_319_396_5 * s,
            b: -0.004_196_086_3 * l - 0.703_418_614_8 * m + 1.707_614_701_0 * s,
        }
    }

    pub fn to_srgb(self) -> Srgb {
        self.to_linear().to_srgb()
    }

    pub fn to_oklch(self) -> Oklch {
        let c = self.a.hypot(self.b);
        let h = self.b.atan2(self.a).to_degrees().rem_euclid(360.0);
        Oklch { l: self.l, c, h }
    }
}

impl Oklch {
    pub fn to_oklab(self) -> Oklab {
        let (s, c) = self.h.to_radians().sin_cos();
        Oklab {
            l: self.l,
            a: self.c * c,
            b: self.c * s,
        }
    }

    pub fn to_srgb(self) -> Srgb {
        self.to_oklab().to_srgb()
    }

    pub fn from_srgb(c: Srgb) -> Self {
        Oklab::from_srgb(c).to_oklch()
    }

    /// The largest chroma at this lightness and hue, not above `cap`, whose
    /// sRGB form stays in gamut; found by bisection on chroma.
    pub fn max_in_gamut_chroma(l: f64, h: f64, cap: f64) -> f64 {
        let fits = |c: f64| Oklch { l, c, h }.to_srgb().in_gamut();
        if fits(cap) {
            return cap;
        }
        let (mut lo, mut hi) = (0.0, cap);
        for _ in 0..40 {
            let mid = (lo + hi) / 2.0;
            if fits(mid) {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        lo
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn close(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() <= tol
    }

    /// Ottosson's published table for the sRGB primaries and white.
    #[test]
    fn matches_ottossons_reference_values() {
        let cases = [
            ("#FFFFFF", (1.0, 0.0, 0.0)),
            ("#FF0000", (0.627_955, 0.224_863, 0.125_846)),
            ("#00FF00", (0.866_440, -0.233_888, 0.179_498)),
            ("#0000FF", (0.452_014, -0.032_457, -0.311_528)),
        ];
        for (hex, (l, a, b)) in cases {
            let o = Oklab::from_srgb(Srgb::from_hex(hex).unwrap());
            assert!(
                close(o.l, l, 1e-4) && close(o.a, a, 1e-4) && close(o.b, b, 1e-4),
                "{hex}: {o:?}"
            );
        }
    }

    #[test]
    fn hue_angles_land_where_expected() {
        // pure red sits at about 29 degrees, pure blue at about 264
        let red = Oklch::from_srgb(Srgb::from_hex("#FF0000").unwrap());
        let blue = Oklch::from_srgb(Srgb::from_hex("#0000FF").unwrap());
        assert!(close(red.h, 29.23, 0.1), "{}", red.h);
        assert!(close(blue.h, 264.05, 0.1), "{}", blue.h);
    }

    #[test]
    fn gamut_search_returns_the_cap_inside_and_a_boundary_outside() {
        assert_eq!(Oklch::max_in_gamut_chroma(0.5, 30.0, 0.01), 0.01);
        let c = Oklch::max_in_gamut_chroma(0.9, 264.0, 0.4);
        assert!(c < 0.4 && c > 0.0);
        assert!(
            Oklch {
                l: 0.9,
                c,
                h: 264.0
            }
            .to_srgb()
            .in_gamut()
        );
        assert!(
            !Oklch {
                l: 0.9,
                c: c + 0.01,
                h: 264.0
            }
            .to_srgb()
            .in_gamut()
        );
    }

    proptest! {
        #[test]
        fn oklab_round_trips_srgb(r in 0.0f64..=1.0, g in 0.0f64..=1.0, b in 0.0f64..=1.0) {
            // Ottosson publishes the matrices to ten digits and the inverse is
            // not the exact inverse of the forward at that precision, so the
            // round trip is good to about a part in ten thousand, well under
            // the 1/255 a hex value can express.
            let c = Srgb { r, g, b };
            let back = Oklab::from_srgb(c).to_srgb();
            prop_assert!(close(back.r, r, 5e-4) && close(back.g, g, 5e-4) && close(back.b, b, 5e-4), "{c:?} -> {back:?}");
        }

        #[test]
        fn oklch_round_trips_oklab(l in 0.0f64..=1.0, a in -0.4f64..=0.4, b in -0.4f64..=0.4) {
            let o = Oklab { l, a, b };
            let back = o.to_oklch().to_oklab();
            prop_assert!(close(back.a, a, 1e-9) && close(back.b, b, 1e-9));
        }
    }
}
