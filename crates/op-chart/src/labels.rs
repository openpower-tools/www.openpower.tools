//! Measuring the chart's label text, placing rows of labels from those
//! measurements, and the markers that let a series be told apart when
//! colour cannot do it alone.

use crate::advances::{
    COUNT, FIRST, LAST, Measured, PER_EM, PLEX_SANS_400, PLEX_SANS_400_FACE, PLEX_SANS_700,
    PLEX_SANS_700_FACE,
};

/// The size the chart's text is drawn at, in CSS px, which is what both
/// consumers' stylesheets set on the svg and the size every measurement
/// here is taken at. A stylesheet that set another size would leave the
/// renderer measuring one thing and drawing another, so both interpolate
/// this constant rather than writing 12 again.
pub const TEXT_PX: f64 = 12.0;

/// How far a line of text reaches above its baseline, as a fraction of the
/// size it is set at.
///
/// With [`DESCENT`] this is the em box, not a measurement of the face:
/// [`crate::advances`] carries advances and nothing else, and the faces'
/// own line heights include leading no row of this drawing leaves. The em
/// box is what the rows are spaced by, so text at [`TEXT_PX`] on rows
/// [`TEXT_PX`] apart meets exactly and never overlaps.
pub const ASCENT: f64 = 0.8;
/// How far a line of text reaches below its baseline, with [`ASCENT`].
pub const DESCENT: f64 = 0.2;

/// Which of the two faces the chart draws with a run of text is set in.
/// The stylesheets set the whole chart in IBM Plex Sans and ask for bold
/// in two places: the series end labels and the playhead readout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Face {
    Regular,
    Bold,
}

impl Face {
    /// Every face the chart draws with, which is every table the generator
    /// emits. A consumer that has to ask a browser about the faces asks
    /// about all of them, and a test in `op-assets` holds this list and the
    /// generator's to the same length.
    pub const ALL: [Face; 2] = [Face::Regular, Face::Bold];

    /// The advance table this face is measured from.
    fn table(self) -> &'static [u16; COUNT] {
        match self {
            Face::Regular => &PLEX_SANS_400,
            Face::Bold => &PLEX_SANS_700,
        }
    }

    /// The face the table beside it was measured from: the family, weight
    /// and style to ask a browser for. Both come out of the generated
    /// source together, so what a consumer asks for cannot drift from what
    /// was measured (decision 14).
    pub fn measured(self) -> Measured {
        match self {
            Face::Regular => PLEX_SANS_400_FACE,
            Face::Bold => PLEX_SANS_700_FACE,
        }
    }
}

/// The width `text` takes at `px` in `face`: the sum of its characters'
/// advances (decision 14). Exact for the digits, which both faces set on
/// one advance, and within a fraction of a pixel for the short words the
/// chart draws: the sweep over all 9025 ordered pairs of the covered
/// block found that both faces do kern, but almost always inwards, so
/// over the strings this chart draws the browser's own layout is never
/// wider than this sum and at worst 0.30 px narrower.
///
/// A character outside the covered block is measured as a full em, which
/// is wider than every advance in either face, so a label the tables
/// cannot measure crowds its neighbours out rather than being drawn over
/// them.
pub fn text_width(text: &str, px: f64, face: Face) -> f64 {
    let table = face.table();
    let per_mille: f64 = text
        .chars()
        .map(|c| {
            if (FIRST..=LAST).contains(&c) {
                f64::from(table[c as usize - FIRST as usize])
            } else {
                PER_EM
            }
        })
        .sum();
    per_mille / PER_EM * px
}

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

/// One label asking for a place on a row.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Wanted {
    /// The place the label would take with nothing in its way: the x of
    /// the cue it points at, or the baseline a series' end label wants.
    pub at: f64,
    /// How far its box reaches back from the place it takes: half its
    /// measured width for a label centred on its cue, nothing for one
    /// drawn from its place, and its ascent for a row placed down the page.
    pub back: f64,
    /// How far its box reaches ahead of the place it takes, with
    /// [`Self::back`]: half its width, its whole width, or its descent.
    pub ahead: f64,
    /// The furthest from `at` the label may be drawn. A label that may not
    /// be moved at all, because moving it would take it off the thing it
    /// names, asks for a reach of zero and is drawn where it stands or not
    /// at all; a mark label reaches half its own width, which leaves its
    /// box over its own rule; an end label reaches the whole column,
    /// because the swatch beside it says which series it names.
    pub reach: f64,
}

/// Place a row of labels from their measured boxes. `None` in, for a cue
/// with nothing to say, is `None` out; `None` out for a label there was no
/// room for. Every box the caller gets back lies inside `lo..=hi` and
/// clears every other by at least `gap`.
///
/// Each label takes the place nearest the one it wants that clears every
/// label already placed, lies inside the row and is within its own reach;
/// a label with no such place is dropped rather than drawn over its
/// neighbour, because a label shifted off the thing it names points at
/// nothing and two labels drawn over each other leave neither readable.
/// That is decision 14's candidate places about each anchor, its lowest
/// overlap first and its removal of the survivors, which collapse into one
/// pass here: a place that overlaps at all is never taken, so the label
/// that would have survived to be removed is the one that finds no place.
/// A label is only ever weighed against the ones already kept, and the
/// places worth trying are its own and the ones flush against a box in its
/// way, since the nearest clearing place on a line is always one of those.
///
/// The row is walked from the low end, by the leading edge of the box each
/// label wants rather than by the place it takes, so a label centred on
/// its cue and one drawn from its cue are ordered by the same thing: left
/// to right along the plot, top to bottom down a column. Three contests
/// are settled by that order and this preference alone. Between two labels
/// whose boxes open at the same place, the one the block lists first keeps
/// it and the other moves or goes, since [`order_by`] leaves ties in input
/// order. Between two that open at different places, the nearer the origin
/// is placed first and keeps its place. And between two free places the
/// same distance from the one a label wants, it takes the one ahead: down
/// the page, or along the axis away from the origin.
///
/// `lo` and `hi` carry whatever clearance the caller wants at the ends of
/// the row: the gap is between neighbours, not against the edge.
///
/// `gap` is clear space between two boxes and holds no allowance for the
/// type inside them. A box here is a sum of advances, and the sweep found
/// the browser's own layout of every string the chart draws to be no
/// wider than that sum and at worst 0.30 px narrower, so a label reserved
/// by its advances is reserved generously and never crowded. Slack for
/// kerning would be room no measurement asks for.
pub fn place(row: &[Option<Wanted>], gap: f64, lo: f64, hi: f64) -> Vec<Option<f64>> {
    // a label with nothing to draw sorts last and is never placed, so it
    // can never stand in the way of one with something to say
    let keys: Vec<f64> = row
        .iter()
        .map(|w| w.map_or(f64::INFINITY, |w| w.at - w.back))
        .collect();
    let mut taken: Vec<(f64, f64)> = Vec::with_capacity(row.len());
    let mut placed: Vec<Option<f64>> = vec![None; row.len()];
    for i in order_by(&keys) {
        let Some(w) = row[i] else { continue };
        // the places inside the row this label's box may sit at; a row
        // narrower than the label itself has none
        let (first, last) = (lo + w.back, hi - w.ahead);
        if first > last {
            continue;
        }
        // The places worth trying: the one it wants, and flush against
        // each box already kept, ahead of it before behind it. A place
        // outside the row is pulled back inside, which is a move like any
        // other and is refused when it is further than the reach allows.
        // The nearest of those that clears every box wins, and a tie goes
        // to the one found first, which is the one ahead.
        let mut best: Option<f64> = None;
        for p in std::iter::once(w.at).chain(
            taken
                .iter()
                .flat_map(|(c, d)| [d + gap + w.back, c - gap - w.ahead]),
        ) {
            let p = p.clamp(first, last);
            let away = (p - w.at).abs();
            if away > w.reach + 1e-9 || best.is_some_and(|q| away >= (q - w.at).abs()) {
                continue;
            }
            let (a, b) = (p - w.back, p + w.ahead);
            if taken
                .iter()
                .all(|(c, d)| b.min(*d) - a.max(*c) + gap <= 0.0)
            {
                best = Some(p);
            }
        }
        if let Some(p) = best {
            placed[i] = Some(p);
            taken.push((p - w.back, p + w.ahead));
        }
    }
    placed
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

    /// A label centred on `at` that will not be moved: the tick labels.
    fn fixed(at: f64, width: f64) -> Option<Wanted> {
        Some(Wanted {
            at,
            back: width / 2.0,
            ahead: width / 2.0,
            reach: 0.0,
        })
    }

    /// A label centred on `at` that may slide by half its own width, which
    /// leaves its box over the cue it names: the mark and band labels.
    fn sliding(at: f64, width: f64) -> Option<Wanted> {
        Some(Wanted {
            at,
            back: width / 2.0,
            ahead: width / 2.0,
            reach: width / 2.0,
        })
    }

    /// A label on a baseline that may be moved anywhere in the row, up or
    /// down: the series end labels down their column.
    fn stacked(at: f64) -> Option<Wanted> {
        Some(Wanted {
            at,
            back: TEXT_PX / 2.0,
            ahead: TEXT_PX / 2.0,
            reach: 200.0,
        })
    }

    #[test]
    fn text_is_measured_from_the_tables_and_the_digits_all_measure_alike() {
        // the sum of the advances, at the size the text is drawn
        let plain = text_width("0", TEXT_PX, Face::Regular);
        assert!((plain - 600.0 / 1000.0 * TEXT_PX).abs() < 1e-9, "{plain}");
        // the bold face is measured from its own table, and the two differ
        assert!(text_width("W", TEXT_PX, Face::Bold) > text_width("W", TEXT_PX, Face::Regular));
        // and the size scales it
        let at_12 = text_width("100s", TEXT_PX, Face::Regular);
        assert!((text_width("100s", 24.0, Face::Regular) - 2.0 * at_12).abs() < 1e-9);
        assert_eq!(text_width("", TEXT_PX, Face::Regular), 0.0);
    }

    #[test]
    fn a_character_the_tables_do_not_cover_is_measured_as_a_full_em() {
        for face in [Face::Regular, Face::Bold] {
            let em = text_width("\u{2014}", TEXT_PX, face);
            assert!((em - TEXT_PX).abs() < 1e-9, "{em}");
            // and it is wider than anything the tables do cover, so a
            // label with one in it takes room rather than stealing it
            let widest = face
                .table()
                .iter()
                .copied()
                .max()
                .expect("the table is not empty");
            assert!(f64::from(widest) < PER_EM, "{widest} is past the em");
        }
    }

    #[test]
    fn a_row_with_room_keeps_every_label_where_it_wants_to_be() {
        let row = [fixed(10.0, 8.0), fixed(40.0, 8.0), fixed(70.0, 8.0)];
        assert_eq!(
            place(&row, 8.0, 0.0, 100.0),
            [Some(10.0), Some(40.0), Some(70.0)]
        );
    }

    #[test]
    fn a_label_that_cannot_clear_its_neighbour_is_dropped_and_not_moved() {
        // two labels 4 px apart, each 20 px wide: the first is placed and
        // the second, which may not be moved, has nowhere to stand
        let row = [fixed(30.0, 20.0), fixed(34.0, 20.0)];
        assert_eq!(place(&row, 8.0, 0.0, 100.0), [Some(30.0), None]);
        // one that may slide keeps its own place while it clears
        let row = [sliding(30.0, 20.0), sliding(58.0, 20.0)];
        assert_eq!(place(&row, 8.0, 0.0, 100.0), [Some(30.0), Some(58.0)]);
        // and slides to the nearest place that does when it does not:
        // 56 leaves 6 px of the 8 the row asks for, and flush against its
        // neighbour with the gap between them is 58
        let row = [sliding(30.0, 20.0), sliding(56.0, 20.0)];
        assert_eq!(place(&row, 8.0, 0.0, 100.0), [Some(30.0), Some(58.0)]);
        // half its own width is as far as it will go, so a label that
        // needs more than that is dropped where it stands
        let row = [sliding(30.0, 20.0), sliding(46.0, 20.0)];
        assert_eq!(place(&row, 8.0, 0.0, 100.0), [Some(30.0), None]);
    }

    /// The tie rule, which is the order the row is walked in: two labels
    /// that want one place are settled by the block's own order, and two
    /// that want different places by which is nearer the origin. Neither
    /// is the order they were written in the markup.
    #[test]
    fn a_tie_goes_to_the_label_the_block_lists_first() {
        // two end labels wanting one baseline: the first keeps it and the
        // second takes the row below, the nearest place that clears
        let row = [stacked(50.0), stacked(50.0)];
        assert_eq!(place(&row, 2.0, 0.0, 100.0), [Some(50.0), Some(64.0)]);
        // no room to move into and the second is dropped, never the first
        assert_eq!(place(&row, 2.0, 44.0, 56.0), [Some(50.0), None]);
        // and the label nearer the origin is placed first whatever its
        // position in the row: here the second listed is the earlier one
        let row = [fixed(34.0, 20.0), fixed(30.0, 20.0)];
        assert_eq!(place(&row, 8.0, 0.0, 100.0), [None, Some(30.0)]);
    }

    /// A row placed down the page: the end labels' column, where a label
    /// may be moved anywhere in the column and so is dropped only when the
    /// column is full. Three series ending within 4 px of each other are
    /// spread over the rows either side of where they end, nearest first,
    /// rather than stacked below them: what the placement keeps is each
    /// label's distance from the line it names.
    #[test]
    fn a_column_moves_a_crowded_label_to_the_nearest_free_row() {
        let row = [stacked(50.0), stacked(52.0), stacked(54.0)];
        assert_eq!(
            place(&row, 2.0, 0.0, 100.0),
            [Some(50.0), Some(64.0), Some(36.0)]
        );
        // the same three in a column with room for two
        assert_eq!(place(&row, 2.0, 44.0, 74.0), [Some(50.0), Some(64.0), None]);
    }

    #[test]
    fn a_label_with_nothing_to_say_neither_draws_nor_stands_in_the_way() {
        let row = [None, fixed(30.0, 20.0), None];
        assert_eq!(place(&row, 8.0, 0.0, 100.0), [None, Some(30.0), None]);
        assert!(place(&[], 8.0, 0.0, 100.0).is_empty());
    }

    #[test]
    fn a_label_the_row_has_no_room_for_at_either_end_is_dropped() {
        // hard against the low end, where its box would run outside, and
        // it may not be moved off the rule it names
        assert_eq!(place(&[fixed(4.0, 20.0)], 8.0, 0.0, 100.0), [None]);
        assert_eq!(place(&[fixed(10.0, 20.0)], 8.0, 0.0, 100.0), [Some(10.0)]);
        // and against the high end
        assert_eq!(place(&[fixed(95.0, 20.0)], 8.0, 0.0, 100.0), [None]);
        // one that may slide is pulled just inside instead, which is
        // nearer its own place than the next rung of its ladder
        assert_eq!(place(&[sliding(95.0, 20.0)], 8.0, 0.0, 100.0), [Some(90.0)]);
        // and a row narrower than the label itself holds nothing
        assert_eq!(place(&[sliding(50.0, 20.0)], 8.0, 45.0, 55.0), [None]);
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

        /// Whatever a row asks for, what comes back is drawable: every box
        /// inside the row, no two closer than the gap, and every place one
        /// the label itself would accept.
        #[test]
        fn a_placed_row_is_inside_its_bounds_and_clear_of_itself(
            wants in proptest::collection::vec(
                (0.0f64..200.0, 4.0f64..40.0, 0.0f64..30.0, prop::bool::ANY), 0..10
            ),
            gap in 0.0f64..12.0
        ) {
            let (lo, hi) = (0.0, 200.0);
            let row: Vec<Option<Wanted>> = wants
                .iter()
                .map(|(at, width, reach, centred)| (*width > 0.0).then_some(Wanted {
                    at: *at,
                    back: if *centred { width / 2.0 } else { 0.0 },
                    ahead: if *centred { width / 2.0 } else { *width },
                    reach: *reach,
                }))
                .collect();
            let placed = place(&row, gap, lo, hi);
            prop_assert_eq!(placed.len(), row.len());
            let mut boxes: Vec<(f64, f64)> = Vec::new();
            for (w, p) in row.iter().zip(&placed) {
                let (Some(w), Some(p)) = (w, p) else { continue };
                prop_assert!((p - w.at).abs() <= w.reach + 1e-9, "{p} is past the reach");
                let (a, b) = (p - w.back, p + w.ahead);
                prop_assert!(a >= lo - 1e-9 && b <= hi + 1e-9, "{a} to {b} is outside");
                for (c, d) in &boxes {
                    prop_assert!(
                        a >= d + gap - 1e-9 || b + gap <= c + 1e-9,
                        "{a} to {b} crowds {c} to {d}"
                    );
                }
                boxes.push((a, b));
            }
        }
    }
}
