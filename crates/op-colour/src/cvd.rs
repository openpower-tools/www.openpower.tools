//! Colour-vision-deficiency simulation: Machado, Oliveira and Fernandes
//! (2009), the severity 1.0 matrices applied in linear RGB. The palette
//! tests use this as one model and note the few units of slack between
//! models at the margins.

use crate::srgb::{Linear, Srgb};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Deficiency {
    Protanopia,
    Deuteranopia,
    Tritanopia,
}

impl Deficiency {
    pub const ALL: [Self; 3] = [Self::Protanopia, Self::Deuteranopia, Self::Tritanopia];

    pub fn name(self) -> &'static str {
        match self {
            Self::Protanopia => "protanopia",
            Self::Deuteranopia => "deuteranopia",
            Self::Tritanopia => "tritanopia",
        }
    }

    /// Machado et al. 2009, severity 1.0, rows for R', G', B'.
    fn matrix(self) -> [[f64; 3]; 3] {
        match self {
            Self::Protanopia => [
                [0.152_286, 1.052_583, -0.204_868],
                [0.114_503, 0.786_281, 0.099_216],
                [-0.003_882, -0.048_116, 1.051_998],
            ],
            Self::Deuteranopia => [
                [0.367_322, 0.860_646, -0.227_968],
                [0.280_085, 0.672_501, 0.047_413],
                [-0.011_820, 0.042_940, 0.968_881],
            ],
            Self::Tritanopia => [
                [1.255_528, -0.076_749, -0.178_779],
                [-0.078_411, 0.930_809, 0.147_602],
                [0.004_733, 0.691_367, 0.303_900],
            ],
        }
    }
}

/// The colour as a viewer with the deficiency would see it, in sRGB with
/// channels clamped to the gamut.
pub fn simulate(c: Srgb, d: Deficiency) -> Srgb {
    let l = c.to_linear();
    let m = d.matrix();
    let row = |r: [f64; 3]| (r[0] * l.r + r[1] * l.g + r[2] * l.b).clamp(0.0, 1.0);
    Linear {
        r: row(m[0]),
        g: row(m[1]),
        b: row(m[2]),
    }
    .to_srgb()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lab::{Lab, ciede2000};

    fn hex(h: &str) -> Srgb {
        Srgb::from_hex(h).unwrap()
    }

    #[test]
    fn greys_are_unchanged_and_rows_sum_to_one() {
        for d in Deficiency::ALL {
            for row in d.matrix() {
                let s: f64 = row.iter().sum();
                assert!((s - 1.0).abs() < 2e-3, "{d:?} row {row:?} sums to {s}");
            }
            for g in ["#000000", "#808080", "#FFFFFF"] {
                let out = simulate(hex(g), d);
                assert!(
                    (out.r - out.g).abs() < 1e-3 && (out.g - out.b).abs() < 1e-3,
                    "{d:?} {g} -> {out:?}"
                );
            }
        }
    }

    #[test]
    fn red_and_green_collapse_for_protans_and_deutans_but_not_tritans() {
        let (r, g) = (hex("#FF0000"), hex("#00A000"));
        let d = |dfc| {
            ciede2000(
                Lab::from_srgb(simulate(r, dfc)),
                Lab::from_srgb(simulate(g, dfc)),
            )
        };
        let normal = ciede2000(Lab::from_srgb(r), Lab::from_srgb(g));
        assert!(
            d(Deficiency::Protanopia) < normal / 3.0,
            "protan {}",
            d(Deficiency::Protanopia)
        );
        assert!(
            d(Deficiency::Deuteranopia) < normal / 3.0,
            "deutan {}",
            d(Deficiency::Deuteranopia)
        );
        assert!(
            d(Deficiency::Tritanopia) > normal / 2.0,
            "tritan {}",
            d(Deficiency::Tritanopia)
        );
    }

    /// Tritanopia is the blue-yellow deficiency: a blue-yellow pair loses
    /// more of its distance under it than under either red-green
    /// deficiency, and never gains any.
    #[test]
    fn a_blue_yellow_pair_loses_most_under_tritanopia() {
        let (b, y) = (hex("#0060FF"), hex("#E0C000"));
        let d = |dfc| {
            ciede2000(
                Lab::from_srgb(simulate(b, dfc)),
                Lab::from_srgb(simulate(y, dfc)),
            )
        };
        let normal = ciede2000(Lab::from_srgb(b), Lab::from_srgb(y));
        let (p, dt, t) = (
            d(Deficiency::Protanopia),
            d(Deficiency::Deuteranopia),
            d(Deficiency::Tritanopia),
        );
        assert!(
            t < normal && t < p && t < dt,
            "normal {normal} protan {p} deutan {dt} tritan {t}"
        );
    }
}
