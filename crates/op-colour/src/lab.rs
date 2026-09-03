//! CIELAB (D65) and the CIEDE2000 colour difference (Sharma, Wu and
//! Dalal, 2005), checked against that paper's published test pairs.

use crate::srgb::Srgb;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lab {
    pub l: f64,
    pub a: f64,
    pub b: f64,
}

const D65: [f64; 3] = [0.950_47, 1.0, 1.088_83];

fn f(t: f64) -> f64 {
    const DELTA: f64 = 6.0 / 29.0;
    if t > DELTA * DELTA * DELTA {
        t.cbrt()
    } else {
        t / (3.0 * DELTA * DELTA) + 4.0 / 29.0
    }
}

impl Lab {
    pub fn from_srgb(c: Srgb) -> Self {
        let [x, y, z] = c.to_linear().to_xyz();
        let (fx, fy, fz) = (f(x / D65[0]), f(y / D65[1]), f(z / D65[2]));
        Self {
            l: 116.0 * fy - 16.0,
            a: 500.0 * (fx - fy),
            b: 200.0 * (fy - fz),
        }
    }
}

/// CIEDE2000 with the standard weights kL = kC = kH = 1.
pub fn ciede2000(p: Lab, q: Lab) -> f64 {
    use std::f64::consts::PI;
    let deg = |r: f64| r.to_degrees();
    let rad = |d: f64| d.to_radians();
    let c1 = p.a.hypot(p.b);
    let c2 = q.a.hypot(q.b);
    let c_bar = (c1 + c2) / 2.0;
    let c7 = c_bar.powi(7);
    let g = 0.5 * (1.0 - (c7 / (c7 + 25f64.powi(7))).sqrt());
    let a1p = (1.0 + g) * p.a;
    let a2p = (1.0 + g) * q.a;
    let c1p = a1p.hypot(p.b);
    let c2p = a2p.hypot(q.b);
    let hp = |a: f64, b: f64| {
        if a == 0.0 && b == 0.0 {
            0.0
        } else {
            deg(b.atan2(a)).rem_euclid(360.0)
        }
    };
    let h1p = hp(a1p, p.b);
    let h2p = hp(a2p, q.b);
    let dlp = q.l - p.l;
    let dcp = c2p - c1p;
    let dhp = if c1p * c2p == 0.0 {
        0.0
    } else {
        let d = h2p - h1p;
        if d.abs() <= 180.0 {
            d
        } else if d > 180.0 {
            d - 360.0
        } else {
            d + 360.0
        }
    };
    let d_hp = 2.0 * (c1p * c2p).sqrt() * rad(dhp / 2.0).sin();
    let lbp = (p.l + q.l) / 2.0;
    let cbp = (c1p + c2p) / 2.0;
    let hbp = if c1p * c2p == 0.0 {
        h1p + h2p
    } else {
        let s = h1p + h2p;
        if (h1p - h2p).abs() <= 180.0 {
            s / 2.0
        } else if s < 360.0 {
            (s + 360.0) / 2.0
        } else {
            (s - 360.0) / 2.0
        }
    };
    let t = 1.0 - 0.17 * rad(hbp - 30.0).cos()
        + 0.24 * rad(2.0 * hbp).cos()
        + 0.32 * rad(3.0 * hbp + 6.0).cos()
        - 0.20 * rad(4.0 * hbp - 63.0).cos();
    let d_theta = 30.0 * (-((hbp - 275.0) / 25.0).powi(2)).exp();
    let cbp7 = cbp.powi(7);
    let rc = 2.0 * (cbp7 / (cbp7 + 25f64.powi(7))).sqrt();
    let sl = 1.0 + 0.015 * (lbp - 50.0).powi(2) / (20.0 + (lbp - 50.0).powi(2)).sqrt();
    let sc = 1.0 + 0.045 * cbp;
    let sh = 1.0 + 0.015 * cbp * t;
    let rt = -(2.0 * d_theta * PI / 180.0).sin() * rc;
    ((dlp / sl).powi(2) + (dcp / sc).powi(2) + (d_hp / sh).powi(2) + rt * (dcp / sc) * (d_hp / sh))
        .sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lab(l: f64, a: f64, b: f64) -> Lab {
        Lab { l, a, b }
    }

    /// Sharma, Wu and Dalal (2005), table 1: the pairs that exercise the
    /// hue-difference branches, the neutral case, the rotation term and
    /// the near-black cases.
    #[test]
    fn matches_the_sharma_test_pairs() {
        let cases = [
            ((50.0, 2.6772, -79.7751), (50.0, 0.0, -82.7485), 2.0425),
            ((50.0, 3.1571, -77.2803), (50.0, 0.0, -82.7485), 2.8615),
            ((50.0, 2.8361, -74.0200), (50.0, 0.0, -82.7485), 3.4412),
            ((50.0, -1.3802, -84.2814), (50.0, 0.0, -82.7485), 1.0),
            ((50.0, 0.0, 0.0), (50.0, -1.0, 2.0), 2.3669),
            ((50.0, -1.0, 2.0), (50.0, 0.0, 0.0), 2.3669),
            ((50.0, 2.49, -0.001), (50.0, -2.49, 0.0009), 7.1792),
            ((50.0, 2.49, -0.001), (50.0, -2.49, 0.0011), 7.2195),
            ((50.0, -0.001, 2.49), (50.0, 0.0009, -2.49), 4.8045),
            ((50.0, -0.001, 2.49), (50.0, 0.0011, -2.49), 4.7461),
            ((50.0, 2.5, 0.0), (50.0, 0.0, -2.5), 4.3065),
            ((50.0, 2.5, 0.0), (73.0, 25.0, -18.0), 27.1492),
            ((50.0, 2.5, 0.0), (61.0, -5.0, 29.0), 22.8977),
            ((50.0, 2.5, 0.0), (56.0, -27.0, -3.0), 31.9030),
            ((50.0, 2.5, 0.0), (58.0, 24.0, 15.0), 19.4535),
            ((50.0, 2.5, 0.0), (50.0, 3.1736, 0.5854), 1.0),
            ((50.0, 2.5, 0.0), (50.0, 3.2972, 0.0), 1.0),
            (
                (60.2574, -34.0099, 36.2677),
                (60.4626, -34.1751, 39.4387),
                1.2644,
            ),
            (
                (63.0109, -31.0961, -5.8663),
                (62.8187, -29.7946, -4.0864),
                1.2630,
            ),
            (
                (61.2901, 3.7196, -5.3901),
                (61.4292, 2.2480, -4.9620),
                1.8731,
            ),
            (
                (35.0831, -44.1164, 3.7933),
                (35.0232, -40.0716, 1.5901),
                1.8645,
            ),
            (
                (22.7233, 20.0904, -46.6940),
                (23.0331, 14.9730, -42.5619),
                2.0373,
            ),
            (
                (36.4612, 47.8580, 18.3852),
                (36.2715, 50.5065, 21.2231),
                1.4146,
            ),
            (
                (90.8027, -2.0831, 1.4410),
                (91.1528, -1.6435, 0.0447),
                1.4441,
            ),
            (
                (90.9257, -0.5406, -0.9208),
                (88.6381, -0.8985, -0.7239),
                1.5381,
            ),
            (
                (6.7747, -0.2908, -2.4247),
                (5.8714, -0.0985, -2.2286),
                0.6377,
            ),
            (
                (2.0776, 0.0795, -1.1350),
                (0.9033, -0.0636, -0.5514),
                0.9082,
            ),
        ];
        for ((l1, a1, b1), (l2, a2, b2), want) in cases {
            let got = ciede2000(lab(l1, a1, b1), lab(l2, a2, b2));
            assert!(
                (got - want).abs() < 1e-4,
                "({l1},{a1},{b1}) vs ({l2},{a2},{b2}): got {got}, want {want}"
            );
        }
    }

    #[test]
    fn lab_of_white_and_black_is_the_l_axis() {
        let w = Lab::from_srgb(Srgb::from_hex("#FFFFFF").unwrap());
        let k = Lab::from_srgb(Srgb::from_hex("#000000").unwrap());
        assert!(
            (w.l - 100.0).abs() < 1e-3 && w.a.abs() < 1e-2 && w.b.abs() < 1e-2,
            "{w:?}"
        );
        assert!(k.l.abs() < 1e-9 && k.a.abs() < 1e-9 && k.b.abs() < 1e-9);
    }

    #[test]
    fn difference_is_symmetric_and_zero_for_identity() {
        let p = Lab::from_srgb(Srgb::from_hex("#2B415F").unwrap());
        let q = Lab::from_srgb(Srgb::from_hex("#E69F00").unwrap());
        assert_eq!(ciede2000(p, p), 0.0);
        assert!((ciede2000(p, q) - ciede2000(q, p)).abs() < 1e-9);
    }
}
