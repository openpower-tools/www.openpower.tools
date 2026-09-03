//! WCAG 2 contrast ratio.

use crate::srgb::Srgb;

/// `(L1 + 0.05) / (L2 + 0.05)` with the lighter luminance on top.
pub fn wcag_contrast(a: Srgb, b: Srgb) -> f64 {
    let la = a.to_linear().luminance();
    let lb = b.to_linear().luminance();
    let (hi, lo) = if la >= lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn black_on_white_is_twenty_one_and_the_ratio_is_symmetric() {
        let w = Srgb::from_hex("#FFFFFF").unwrap();
        let k = Srgb::from_hex("#000000").unwrap();
        assert!((wcag_contrast(w, k) - 21.0).abs() < 1e-9);
        assert_eq!(wcag_contrast(w, k), wcag_contrast(k, w));
        // #767676 on white is the classic 4.54:1 AA boundary example
        let g = Srgb::from_hex("#767676").unwrap();
        assert!((wcag_contrast(g, w) - 4.54).abs() < 0.01);
    }
}
