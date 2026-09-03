//! A port of d3-array 3.2's `ticks`, `tickIncrement` and `tickStep`
//! (BSD/ISC, Mike Bostock). The reference cases in the tests are the
//! values d3's own test suite asserts, so the port is checked against the
//! original rather than against itself.

const E10: f64 = 7.071_067_811_865_475_5; // sqrt(50)
const E5: f64 = 3.162_277_660_168_379_5; // sqrt(10)
const E2: f64 = std::f64::consts::SQRT_2;

/// JavaScript's `Math.round`: halves go toward positive infinity, so
/// `-0.5` rounds to `0` where Rust's `round` gives `-1`.
fn js_round(x: f64) -> f64 {
    (x + 0.5).floor()
}

/// `(i1, i2, inc)`: the integer tick range and the increment. A negative
/// `inc` means "divide by -inc" (the sub-unit case), exactly as in d3, so
/// that tick values are computed as exact quotients rather than repeated
/// sums of a rounded step.
fn tick_spec(start: f64, stop: f64, count: f64) -> (f64, f64, f64) {
    let step = (stop - start) / count.max(0.0);
    let power = step.log10().floor();
    let error = step / 10f64.powf(power);
    let factor = if error >= E10 {
        10.0
    } else if error >= E5 {
        5.0
    } else if error >= E2 {
        2.0
    } else {
        1.0
    };
    let (mut i1, mut i2, inc);
    if power < 0.0 {
        let scale = 10f64.powf(-power) / factor;
        i1 = js_round(start * scale);
        i2 = js_round(stop * scale);
        if i1 / scale < start {
            i1 += 1.0;
        }
        if i2 / scale > stop {
            i2 -= 1.0;
        }
        inc = -scale;
    } else {
        let scale = 10f64.powf(power) * factor;
        i1 = js_round(start / scale);
        i2 = js_round(stop / scale);
        if i1 * scale < start {
            i1 += 1.0;
        }
        if i2 * scale > stop {
            i2 -= 1.0;
        }
        inc = scale;
    }
    if i2 < i1 && (0.5..2.0).contains(&count) {
        return tick_spec(start, stop, count * 2.0);
    }
    (i1, i2, inc)
}

/// Roughly `count` nicely rounded values between `start` and `stop`
/// inclusive; multiples of 1, 2 or 5 times a power of ten. An empty
/// vector when `count` is not positive; `[start]` when the domain is a
/// point; descending when `stop < start`.
pub fn ticks(start: f64, stop: f64, count: f64) -> Vec<f64> {
    // a NaN count yields nothing, as in d3
    if count.is_nan() || count <= 0.0 {
        return Vec::new();
    }
    if start == stop {
        return vec![start];
    }
    let reverse = stop < start;
    let (i1, i2, inc) = if reverse {
        tick_spec(stop, start, count)
    } else {
        tick_spec(start, stop, count)
    };
    if i1.is_nan() || i2.is_nan() || i2 < i1 {
        return Vec::new();
    }
    let n = (i2 - i1) as usize + 1;
    (0..n)
        .map(|i| {
            let k = if reverse {
                i2 - i as f64
            } else {
                i1 + i as f64
            };
            if inc < 0.0 { k / -inc } else { k * inc }
        })
        .collect()
}

/// d3's `tickIncrement`: the raw increment, negative meaning a divisor.
pub fn tick_increment(start: f64, stop: f64, count: f64) -> f64 {
    tick_spec(start, stop, count).2
}

/// d3's `tickStep`: the positive step between ticks, negated for a
/// descending domain.
pub fn tick_step(start: f64, stop: f64, count: f64) -> f64 {
    let reverse = stop < start;
    let inc = if reverse {
        tick_increment(stop, start, count)
    } else {
        tick_increment(start, stop, count)
    };
    let sign = if reverse { -1.0 } else { 1.0 };
    sign * if inc < 0.0 { 1.0 / -inc } else { inc }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    // d3-array/test/ticks-test.js expectations, verbatim.
    #[test]
    fn matches_d3s_test_suite() {
        assert_eq!(
            ticks(0.0, 1.0, 10.0),
            vec![0.0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0]
        );
        assert_eq!(
            ticks(0.0, 1.0, 9.0),
            vec![0.0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0]
        );
        assert_eq!(
            ticks(0.0, 1.0, 8.0),
            vec![0.0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0]
        );
        assert_eq!(ticks(0.0, 1.0, 7.0), vec![0.0, 0.2, 0.4, 0.6, 0.8, 1.0]);
        assert_eq!(ticks(0.0, 1.0, 5.0), vec![0.0, 0.2, 0.4, 0.6, 0.8, 1.0]);
        assert_eq!(ticks(0.0, 1.0, 4.0), vec![0.0, 0.2, 0.4, 0.6, 0.8, 1.0]);
        assert_eq!(ticks(0.0, 1.0, 3.0), vec![0.0, 0.5, 1.0]);
        assert_eq!(ticks(0.0, 1.0, 2.0), vec![0.0, 0.5, 1.0]);
        assert_eq!(ticks(0.0, 1.0, 1.0), vec![0.0, 1.0]);
        assert_eq!(
            ticks(0.0, 10.0, 10.0),
            vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0]
        );
        assert_eq!(ticks(0.0, 10.0, 5.0), vec![0.0, 2.0, 4.0, 6.0, 8.0, 10.0]);
        assert_eq!(ticks(0.0, 10.0, 2.0), vec![0.0, 5.0, 10.0]);
        assert_eq!(ticks(0.0, 10.0, 1.0), vec![0.0, 10.0]);
        assert_eq!(
            ticks(-10.0, 10.0, 10.0),
            vec![-10.0, -8.0, -6.0, -4.0, -2.0, 0.0, 2.0, 4.0, 6.0, 8.0, 10.0]
        );
        assert_eq!(ticks(-10.0, 10.0, 5.0), vec![-10.0, -5.0, 0.0, 5.0, 10.0]);
        assert_eq!(ticks(-10.0, 10.0, 2.0), vec![-10.0, 0.0, 10.0]);
        assert_eq!(ticks(-10.0, 10.0, 1.0), vec![0.0]);
        assert_eq!(
            ticks(-1.0, 1.0, 10.0),
            vec![-1.0, -0.8, -0.6, -0.4, -0.2, 0.0, 0.2, 0.4, 0.6, 0.8, 1.0]
        );
        // descending domains reverse the ticks
        assert_eq!(ticks(1.0, 0.0, 5.0), vec![1.0, 0.8, 0.6, 0.4, 0.2, 0.0]);
        // a point domain is its own tick; a non-positive count is empty
        assert_eq!(ticks(1.0, 1.0, 5.0), vec![1.0]);
        assert_eq!(ticks(0.0, 1.0, 0.0), Vec::<f64>::new());
        // d3 doubles a count below 2 when the first pass yields no tick inside
        // the domain: without the doubling this would be empty
        assert_eq!(ticks(0.51, 0.59, 1.0), vec![0.55]);
        assert_eq!(ticks(0.51, 0.59, 0.4), Vec::<f64>::new());
        // a domain with no tick at all
        assert_eq!(ticks(0.0, 0.8, 1.0), vec![0.0]);
    }

    #[test]
    fn our_film_axis_at_eight_ticks_is_half_seconds() {
        assert_eq!(
            ticks(0.0, 3.7, 8.0),
            vec![0.0, 0.5, 1.0, 1.5, 2.0, 2.5, 3.0, 3.5]
        );
    }

    #[test]
    fn rounding_follows_javascript_on_negative_halves() {
        assert_eq!(js_round(-0.5), 0.0);
        assert_eq!(js_round(-1.5), -1.0);
        assert_eq!(js_round(2.5), 3.0);
        assert_eq!(ticks(-10.0, 10.0, 1.0), vec![0.0]);
    }

    #[test]
    fn increment_and_step_agree_with_d3() {
        assert_eq!(tick_increment(0.0, 1.0, 10.0), -10.0);
        assert_eq!(tick_step(0.0, 1.0, 10.0), 0.1);
        assert_eq!(tick_step(0.0, 10.0, 5.0), 2.0);
        assert_eq!(tick_step(10.0, 0.0, 5.0), -2.0);
        assert_eq!(tick_increment(0.0, 10.0, 5.0), 2.0);
    }

    proptest! {
        #[test]
        fn ticks_lie_in_the_domain_sorted_and_evenly_spaced(
            a in -1.0e6f64..1.0e6, span in 1.0e-6f64..1.0e6, count in 1.0f64..50.0
        ) {
            let b = a + span;
            let v = ticks(a, b, count);
            prop_assert!(v.len() <= (count as usize) * 3 + 2, "too many ticks: {}", v.len());
            for w in v.windows(2) {
                prop_assert!(w[0] < w[1]);
            }
            let tol = span * 1e-9;
            for x in &v {
                prop_assert!(*x >= a - tol && *x <= b + tol, "{x} outside [{a}, {b}]");
            }
            if v.len() >= 3 {
                let step = v[1] - v[0];
                for w in v.windows(2) {
                    prop_assert!(((w[1] - w[0]) - step).abs() <= step * 1e-9);
                }
                // the step is 1, 2 or 5 times a power of ten
                let mant = step / 10f64.powf(step.log10().floor());
                prop_assert!([1.0, 2.0, 5.0].iter().any(|m| (mant - m).abs() < 1e-6), "step {step}");
            }
            // reversing the domain reverses the ticks exactly
            let mut r = ticks(b, a, count);
            r.reverse();
            prop_assert_eq!(r, v);
        }
    }
}
