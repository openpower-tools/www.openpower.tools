//! APCA (Accessible Perceptual Contrast Algorithm), the W3 0.1.9 / SAPC
//! 0.98G-4g constants, checked against the reference values in the
//! apca-w3 README. Lc is positive for dark text on a light background and
//! negative the other way round; magnitude is what the levels compare.

use crate::srgb::Srgb;

const MAIN_TRC: f64 = 2.4;
const S_RCO: f64 = 0.212_672_9;
const S_GCO: f64 = 0.715_152_2;
const S_BCO: f64 = 0.072_175_0;
const NORM_BG: f64 = 0.56;
const NORM_TXT: f64 = 0.57;
const REV_TXT: f64 = 0.62;
const REV_BG: f64 = 0.65;
const BLK_THRS: f64 = 0.022;
const BLK_CLMP: f64 = 1.414;
const SCALE_BOW: f64 = 1.14;
const SCALE_WOB: f64 = 1.14;
const LO_BOW_OFFSET: f64 = 0.027;
const LO_WOB_OFFSET: f64 = 0.027;
const LO_CLIP: f64 = 0.1;
const DELTA_Y_MIN: f64 = 0.0005;

/// APCA's own luminance estimate (not the sRGB transfer curve).
fn y(c: Srgb) -> f64 {
    S_RCO * c.r.powf(MAIN_TRC) + S_GCO * c.g.powf(MAIN_TRC) + S_BCO * c.b.powf(MAIN_TRC)
}

fn soft_clamp(y: f64) -> f64 {
    if y > BLK_THRS {
        y
    } else {
        y + (BLK_THRS - y).powf(BLK_CLMP)
    }
}

/// Lc for `text` on `background`.
pub fn apca_lc(text: Srgb, background: Srgb) -> f64 {
    let (yt, yb) = (soft_clamp(y(text)), soft_clamp(y(background)));
    if (yb - yt).abs() < DELTA_Y_MIN {
        return 0.0;
    }
    let out = if yb > yt {
        let sapc = (yb.powf(NORM_BG) - yt.powf(NORM_TXT)) * SCALE_BOW;
        if sapc < LO_CLIP {
            0.0
        } else {
            sapc - LO_BOW_OFFSET
        }
    } else {
        let sapc = (yb.powf(REV_BG) - yt.powf(REV_TXT)) * SCALE_WOB;
        if sapc > -LO_CLIP {
            0.0
        } else {
            sapc + LO_WOB_OFFSET
        }
    };
    out * 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(h: &str) -> Srgb {
        Srgb::from_hex(h).unwrap()
    }

    /// The apca-w3 0.1.9 README's reference outputs.
    #[test]
    fn matches_the_apca_w3_reference_values() {
        let cases = [
            ("#888888", "#FFFFFF", 63.056),
            ("#FFFFFF", "#888888", -68.541),
            ("#000000", "#AAAAAA", 58.146),
            ("#AAAAAA", "#000000", -56.241),
            ("#112233", "#DDEEFF", 91.668),
            ("#DDEEFF", "#112233", -93.068),
            ("#112233", "#444444", 8.323),
            ("#444444", "#112233", -7.527),
        ];
        for (t, b, want) in cases {
            let got = apca_lc(hex(t), hex(b));
            assert!(
                (got - want).abs() < 0.01,
                "{t} on {b}: got {got}, want {want}"
            );
        }
    }

    #[test]
    fn black_on_white_is_about_106() {
        let lc = apca_lc(hex("#000000"), hex("#FFFFFF"));
        assert!((lc - 106.04).abs() < 0.05, "{lc}");
        assert_eq!(apca_lc(hex("#808080"), hex("#808080")), 0.0);
    }
}
