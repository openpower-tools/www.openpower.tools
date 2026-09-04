//! Direct end labels and markers: the non-colour cues that let a series
//! be told apart when colour cannot do it alone.

/// The indices of `keys` in ascending `total_cmp` order (so negative zero
/// sorts before zero and the order is total), ties in input order: an
/// insertion sort, because a chart has at most a handful of labels and the
/// standard library's generic stable sort costs several kilobytes of wasm
/// for the one call that would use it.
pub fn order_by(keys: &[f64]) -> Vec<usize> {
    let mut order: Vec<usize> = Vec::with_capacity(keys.len());
    for i in 0..keys.len() {
        let mut at = order.len();
        while at > 0 && keys[order[at - 1]].total_cmp(&keys[i]) == std::cmp::Ordering::Greater {
            at -= 1;
        }
        order.insert(at, i);
    }
    order
}

/// Move a set of preferred vertical positions so that no two are closer
/// than `gap` and all stay inside `lo..=hi` when there is room. Order is
/// preserved: a label wanted higher stays higher. Returns the placed
/// position for each input, in input order.
pub fn spread(preferred: &[f64], gap: f64, lo: f64, hi: f64) -> Vec<f64> {
    let n = preferred.len();
    if n == 0 {
        return Vec::new();
    }
    let order = order_by(preferred);
    let mut placed: Vec<f64> = order.iter().map(|i| preferred[*i]).collect();
    // sweep down: push each label below the one above it
    placed[0] = placed[0].max(lo);
    for k in 1..n {
        placed[k] = placed[k].max(placed[k - 1] + gap);
    }
    // sweep up from the bottom edge: pull overflow back inside
    if placed[n - 1] > hi {
        placed[n - 1] = hi;
        for k in (0..n - 1).rev() {
            placed[k] = placed[k].min(placed[k + 1] - gap);
        }
    }
    let mut out = vec![0.0; n];
    for (k, i) in order.into_iter().enumerate() {
        out[i] = placed[k];
    }
    out
}

/// The marker shape for a series, as an SVG path centred on the origin,
/// 7 px across. One distinct shape per palette series.
pub fn marker_path(index: usize) -> &'static str {
    match index {
        1 => "M-3.5 0a3.5 3.5 0 1 0 7 0a3.5 3.5 0 1 0 -7 0", // circle
        2 => "M-3 -3h6v6h-6z",                               // square
        3 => "M0 -4L4 3.5h-8z",                              // triangle
        4 => "M0 -4.2L4.2 0L0 4.2L-4.2 0z",                  // diamond
        5 => "M-3.2 -3.2L3.2 3.2M-3.2 3.2L3.2 -3.2",         // cross
        _ => "M0 -4v8M-4 0h8",                               // plus
    }
}

/// Which sample indices carry a marker: at most `max` of them, evenly
/// spaced, always including the last sample.
pub fn marker_samples(count: usize, max: usize) -> Vec<usize> {
    if count == 0 || max == 0 {
        return Vec::new();
    }
    if count <= max {
        return (0..count).collect();
    }
    let step = (count - 1) as f64 / (max - 1) as f64;
    let mut out: Vec<usize> = (0..max)
        .map(|k| (k as f64 * step).round() as usize)
        .collect();
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn labels_that_do_not_collide_are_left_alone() {
        assert_eq!(
            spread(&[10.0, 40.0, 70.0], 12.0, 0.0, 100.0),
            vec![10.0, 40.0, 70.0]
        );
    }

    #[test]
    fn colliding_labels_are_separated_and_kept_inside_the_band() {
        let placed = spread(&[50.0, 52.0, 51.0], 12.0, 0.0, 100.0);
        assert_eq!(placed, vec![50.0, 74.0, 62.0]);
        // at the bottom edge the run is pulled back up
        let placed = spread(&[95.0, 97.0], 12.0, 0.0, 100.0);
        assert_eq!(placed, vec![88.0, 100.0]);
        // at the top edge
        let placed = spread(&[-5.0, 2.0], 12.0, 0.0, 100.0);
        assert_eq!(placed, vec![0.0, 12.0]);
    }

    #[test]
    fn every_series_has_a_distinct_marker_and_samples_are_bounded() {
        let shapes: std::collections::BTreeSet<&str> = (1..=6).map(marker_path).collect();
        assert_eq!(shapes.len(), 6);
        assert_eq!(marker_samples(5, 8), vec![0, 1, 2, 3, 4]);
        let s = marker_samples(38, 8);
        assert_eq!(s.len(), 8);
        assert_eq!((s[0], *s.last().unwrap()), (0, 37));
        assert!(s.windows(2).all(|w| w[1] > w[0]));
        assert!(marker_samples(0, 8).is_empty());
    }

    proptest! {
        #[test]
        fn order_by_matches_the_standard_stable_sort(
            keys in proptest::collection::vec(-5.0f64..5.0, 0..12)
        ) {
            // a few repeated keys so ties are exercised: they keep input order
            let keys: Vec<f64> = keys.iter().map(|k| (k * 2.0).round() / 2.0).collect();
            let mut reference: Vec<usize> = (0..keys.len()).collect();
            reference.sort_by(|a, b| keys[*a].total_cmp(&keys[*b]));
            prop_assert_eq!(order_by(&keys), reference);
        }

        #[test]
        fn placement_keeps_order_gap_and_band(
            prefs in proptest::collection::vec(0.0f64..200.0, 1..8), gap in 4.0f64..20.0
        ) {
            let (lo, hi) = (0.0, 200.0);
            let placed = spread(&prefs, gap, lo, hi);
            prop_assert_eq!(placed.len(), prefs.len());
            let mut idx: Vec<usize> = (0..prefs.len()).collect();
            idx.sort_by(|a, b| prefs[*a].total_cmp(&prefs[*b]));
            for w in idx.windows(2) {
                let (a, b) = (placed[w[0]], placed[w[1]]);
                prop_assert!(b - a >= gap - 1e-9, "gap {} between {a} and {b}", b - a);
            }
            if (prefs.len() as f64 - 1.0) * gap <= hi - lo {
                for y in &placed {
                    prop_assert!(*y >= lo - 1e-9 && *y <= hi + 1e-9, "{y} outside the band");
                }
            }
        }
    }
}
