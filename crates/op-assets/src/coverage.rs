//! What a face's own layout tables can actually reach.
//!
//! Validating a shaper against a browser needs a set of strings to try.
//! The obvious set is every sequence over the alphabet, which is fine for
//! pairs, 9025 of them over printable ASCII, and hopeless past that: an
//! order-5 sweep over 95 characters is 7.7 billion sequences. Iosevka's
//! longest contextual rule is five glyphs, so brute force is exactly the
//! wrong instrument for the faces that need it most.
//!
//! A font already declares what it can do. GPOS pair positioning names
//! the pairs it adjusts, and GSUB ligature substitution names the
//! sequences it replaces. Reading those gives a set that is smaller by
//! orders of magnitude and, unlike a sweep, says which rule each sequence
//! came from.
//!
//! OVER-INCLUSION IS SAFE HERE AND PRECEDENCE IS NOT. OpenType applies
//! the first matching subtable within a lookup, so computing a kern means
//! resolving which subtable wins; getting that wrong mispredicted four
//! pairs when this was first attempted by hand. A test set has no such
//! problem: the union of every subtable's pairs is a superset of what can
//! actually happen, and testing a pair that turns out not to kern costs
//! one measurement. So this deliberately unions rather than resolves, and
//! the shaper decides what the answer is.
//!
//! WHAT IS READ is pair positioning (GPOS type 2) and ligature
//! substitution (GSUB type 4), plain or behind an extension. Plex Sans's
//! pairs are checked against a sweep of every pair read out of Chrome
//! (`fixtures/browser-kerns.txt`), and `fixtures/extension-face.ttf` holds
//! rules behind extension lookups, which no served face has. Contextual
//! rules (GSUB types 5, 6 and 8) are not read yet, and those are where
//! Iosevka keeps its programming ligatures under `calt`; the ligatures
//! reached in Iosevka today are its plain ligature lookups, which join
//! combining marks.

use std::collections::{BTreeMap, BTreeSet};

use read_fonts::{
    FontRef, ReadError, TableProvider,
    tables::{
        gpos::PositionLookup,
        gsub::{self, SubstitutionLookup},
        layout::ClassDef,
    },
    types::GlyphId16,
};

/// What a face can reach, in characters rather than glyphs, with each
/// sequence traceable to the kind of rule that produced it.
#[derive(Clone, Debug, Default)]
pub struct Reachable {
    /// Ordered pairs some GPOS pair-positioning subtable mentions.
    pub kern_pairs: BTreeSet<(char, char)>,
    /// Character sequences some GSUB ligature subtable replaces.
    pub ligatures: BTreeSet<Vec<char>>,
}

impl Reachable {
    /// Every sequence worth measuring, longest first so a caller that
    /// truncates keeps the interesting ones.
    pub fn sequences(&self) -> Vec<Vec<char>> {
        let mut out: Vec<Vec<char>> = self
            .ligatures
            .iter()
            .cloned()
            .chain(self.kern_pairs.iter().map(|(a, b)| vec![*a, *b]))
            .collect();
        out.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
        out.dedup();
        out
    }

    /// How many sequences this is, against the brute-force count for the
    /// same alphabet at the same maximum length. The comparison is the
    /// point of the exercise, so it is computed here rather than left to
    /// a caller to get right.
    pub fn against_brute_force(&self, alphabet: usize) -> (usize, u128) {
        let longest = self
            .sequences()
            .first()
            .map(|s| s.len())
            .unwrap_or(2)
            .max(2);
        let brute: u128 = (alphabet as u128).pow(longest as u32);
        (self.sequences().len(), brute)
    }
}

/// The characters a face can draw, keyed by the glyph each maps to.
///
/// A glyph can have several characters mapping to it; the first by
/// codepoint is used, so the answer does not depend on hash order.
fn characters_by_glyph(font: &FontRef) -> Result<BTreeMap<GlyphId16, char>, ReadError> {
    let cmap = font.cmap()?;
    let mut out: BTreeMap<GlyphId16, char> = BTreeMap::new();
    for codepoint in 0u32..=0x10_FFFF {
        let Some(c) = char::from_u32(codepoint) else {
            continue;
        };
        let Some(gid) = cmap.map_codepoint(c) else {
            continue;
        };
        if let Ok(gid16) = GlyphId16::try_from(gid) {
            out.entry(gid16).or_insert(c);
        }
    }
    Ok(out)
}

/// Every ordered glyph pair a GPOS pair-positioning subtable mentions.
///
/// Format 1 lists its pairs directly. Format 2 is a class matrix, so the
/// pairs are the product of the glyphs in each class, which is where the
/// saving over a sweep comes from: a handful of classes stands for
/// thousands of pairs, and the classes are what the font actually
/// distinguishes.
fn pair_positioning(
    font: &FontRef,
    chars: &BTreeMap<GlyphId16, char>,
) -> Result<BTreeSet<(char, char)>, ReadError> {
    let mut out = BTreeSet::new();
    let Ok(gpos) = font.gpos() else {
        return Ok(out);
    };
    let lookups = gpos.lookup_list()?;
    // A LOOKUP MAY BE WRAPPED. Fonts past a size threshold put their real
    // subtables behind extension lookups, because the 16-bit offsets in a
    // plain lookup cannot reach far enough. None of the faces served here
    // wraps a pair rule: Plex keeps its pair lookups plain, and Iosevka,
    // being monospaced, has no pair positioning at all (the lookups it wraps
    // are mark attachment). fixtures/extension-face.ttf is built to wrap
    // one of each kind, so this path is tested all the same.
    let mut pair_subtables: Vec<read_fonts::tables::gpos::PairPos> = Vec::new();
    for lookup in lookups.lookups().iter() {
        match lookup? {
            PositionLookup::Pair(pair) => {
                for subtable in pair.subtables().iter() {
                    pair_subtables.push(subtable?);
                }
            }
            PositionLookup::Extension(ext) => {
                for subtable in ext.subtables().iter() {
                    if let read_fonts::tables::gpos::ExtensionSubtable::Pair(wrapped) = subtable? {
                        pair_subtables.push(wrapped.extension()?);
                    }
                }
            }
            _ => {}
        }
    }
    {
        for subtable in pair_subtables {
            match subtable {
                read_fonts::tables::gpos::PairPos::Format1(table) => {
                    let coverage: Vec<GlyphId16> = table.coverage()?.iter().collect();
                    for (first, set) in coverage.iter().zip(table.pair_sets().iter()) {
                        let Some(a) = chars.get(first) else { continue };
                        for record in set?.pair_value_records().iter() {
                            let record = record?;
                            if let Some(b) = chars.get(&record.second_glyph()) {
                                out.insert((*a, *b));
                            }
                        }
                    }
                }
                read_fonts::tables::gpos::PairPos::Format2(table) => {
                    let class1 = table.class_def1()?;
                    let class2 = table.class_def2()?;
                    // the glyphs of each class, restricted to ones a
                    // character maps to: a class holds glyphs no keyboard
                    // can produce and those cannot appear in a test string
                    let firsts = classes_of(&class1, table.coverage()?.iter(), chars);
                    let seconds = classes_of(&class2, chars.keys().copied(), chars);
                    // THE MATRIX IS MOSTLY ZEROES AND THE ZEROES ARE NOT
                    // KERNS. Crossing every class with every other, without
                    // reading the value at the intersection, produced 594944
                    // pairs for a face the browser kerns on 1227: worse than
                    // the brute force this exists to avoid. The value record
                    // at (class1, class2) is what says whether the pair moves.
                    let matrix: Vec<_> = table.class1_records().iter().collect();
                    for (i, a_chars) in &firsts {
                        let Some(Ok(row)) = matrix.get(usize::from(*i)) else {
                            continue;
                        };
                        let records = row.class2_records();
                        for (j, b_chars) in &seconds {
                            let Ok(cell) = records.get(usize::from(*j)) else {
                                continue;
                            };
                            if !moves(cell.value_record1()) && !moves(cell.value_record2()) {
                                continue;
                            }
                            for a in a_chars {
                                for b in b_chars {
                                    out.insert((*a, *b));
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(out)
}

/// Whether a value record actually moves anything. A pair whose record is
/// all zeroes is listed by the font only because the class matrix is
/// rectangular, and shaping it proves nothing.
fn moves(v: &read_fonts::tables::gpos::ValueRecord) -> bool {
    v.x_advance().unwrap_or(0) != 0
        || v.y_advance().unwrap_or(0) != 0
        || v.x_placement().unwrap_or(0) != 0
        || v.y_placement().unwrap_or(0) != 0
}

/// The characters of each class, for the glyphs given.
fn classes_of(
    class_def: &ClassDef,
    glyphs: impl Iterator<Item = GlyphId16>,
    chars: &BTreeMap<GlyphId16, char>,
) -> BTreeMap<u16, Vec<char>> {
    let mut out: BTreeMap<u16, Vec<char>> = BTreeMap::new();
    for gid in glyphs {
        if let Some(c) = chars.get(&gid) {
            out.entry(class_def.get(gid)).or_default().push(*c);
        }
    }
    out
}

/// Every character sequence a GSUB ligature subtable replaces.
fn ligature_inputs(
    font: &FontRef,
    chars: &BTreeMap<GlyphId16, char>,
) -> Result<BTreeSet<Vec<char>>, ReadError> {
    let mut out = BTreeSet::new();
    let Ok(gsub) = font.gsub() else {
        return Ok(out);
    };
    // Wrapped the same way as the pair rules: a ligature subtable can sit
    // behind an extension lookup, and reading only plain ones misses it.
    let mut subtables: Vec<gsub::LigatureSubstFormat1> = Vec::new();
    for lookup in gsub.lookup_list()?.lookups().iter() {
        match lookup? {
            SubstitutionLookup::Ligature(lig) => {
                for subtable in lig.subtables().iter() {
                    subtables.push(subtable?);
                }
            }
            SubstitutionLookup::Extension(ext) => {
                for subtable in ext.subtables().iter() {
                    if let gsub::ExtensionSubtable::Ligature(wrapped) = subtable? {
                        subtables.push(wrapped.extension()?);
                    }
                }
            }
            _ => {}
        }
    }
    for subtable in subtables {
        let coverage: Vec<GlyphId16> = subtable.coverage()?.iter().collect();
        for (first, set) in coverage.iter().zip(subtable.ligature_sets().iter()) {
            let Some(a) = chars.get(first) else { continue };
            for ligature in set?.ligatures().iter() {
                let ligature = ligature?;
                let mut seq = vec![*a];
                let mut whole = true;
                for component in ligature.component_glyph_ids() {
                    match chars.get(&component.get()) {
                        Some(c) => seq.push(*c),
                        None => {
                            whole = false;
                            break;
                        }
                    }
                }
                if whole && seq.len() > 1 {
                    out.insert(seq);
                }
            }
        }
    }
    Ok(out)
}

/// Ask a face what its own tables can reach.
pub fn reachable(ttf: &[u8]) -> Result<Reachable, ReadError> {
    let font = FontRef::new(ttf)?;
    let chars = characters_by_glyph(&font)?;
    Ok(Reachable {
        kern_pairs: pair_positioning(&font, &chars)?,
        ligatures: ligature_inputs(&font, &chars)?,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use read_fonts::tables::{gpos::PositionLookup, gsub::SubstitutionLookup};

    use super::*;
    use crate::advances::{self, DRAWN, FIRST, GRID, LAST, TEXT_PX};

    /// A face whose every rule sits behind an extension lookup, built by
    /// fontTools from `fixtures/make_extension_face.py`: `pos A V` (a listed
    /// pair), `pos [V] [A]` (a class pair) and `sub f i by f_i`.
    const EXTENSION_FACE: &[u8] = include_bytes!("../fixtures/extension-face.ttf");

    fn ascii(pair: &(char, char)) -> bool {
        (FIRST..=LAST).contains(&pair.0) && (FIRST..=LAST).contains(&pair.1)
    }

    /// A face the chart draws with, the ASCII pairs its tables reach, and
    /// the pairs the browser measured kerning in it.
    struct Served {
        weight: &'static str,
        ttf: Vec<u8>,
        ours: BTreeSet<(char, char)>,
        theirs: BTreeSet<(char, char)>,
    }

    fn served() -> Vec<Served> {
        let assets = advances::assets();
        let fixture = advances::browser_kerns();
        DRAWN
            .iter()
            .map(|drawn| {
                let ttf = advances::face_ttf(&assets, drawn);
                let ours = reachable(&ttf)
                    .expect("a served face reads")
                    .kern_pairs
                    .into_iter()
                    .filter(ascii)
                    .collect();
                let theirs = fixture
                    .iter()
                    .filter(|(face, ..)| face == drawn.weight)
                    .map(|(_, a, b, _)| (*a, *b))
                    .collect();
                Served {
                    weight: drawn.weight,
                    ttf,
                    ours,
                    theirs,
                }
            })
            .collect()
    }

    /// The contract the module exists for: a test set read from the tables
    /// must contain every pair a browser was seen to kern. Checked against
    /// the sweep read out of Chrome, not against a shaper, so a mistake
    /// this module shares with HarfRust cannot hide.
    #[test]
    fn every_pair_a_browser_kerned_is_reachable() {
        for Served {
            weight,
            ours,
            theirs,
            ..
        } in served()
        {
            assert!(
                theirs.len() > 1000,
                "{weight}: only {} fixture pairs",
                theirs.len()
            );
            let missed: Vec<_> = theirs.difference(&ours).collect();
            assert!(
                missed.is_empty(),
                "{weight}: the tables do not reach {missed:?}"
            );
        }
    }

    /// Over-inclusion is allowed, and this pins how much there is and why.
    /// Unioning every subtable reaches a few pairs the browser did not
    /// report: `La` in the regular kerns by -0.012 px, under the sixty-fourth
    /// of a pixel the fixture keeps, and the `j` pairs are moved by one
    /// subtable and shape to nothing because another wins. Each is shaped
    /// here to show it does not move by a grid step, and the list is exact,
    /// so a regression that floods the set (crossing the class matrix
    /// without reading its values reached 594944 pairs) fails loudly rather
    /// than passing as a superset.
    #[test]
    fn the_pairs_reached_beyond_the_browser_are_known_and_do_not_move() {
        for Served {
            weight,
            ttf,
            ours,
            theirs,
        } in served()
        {
            let extra: Vec<String> = ours
                .difference(&theirs)
                .map(|(a, b)| format!("{a}{b}"))
                .collect();
            let expected: &[&str] = match weight {
                "400" => &["La", "jT", "jV", "jW"],
                "700" => &["jT", "jV", "jW", "jY"],
                other => panic!("no expectation for weight {other}"),
            };
            assert_eq!(extra, expected, "{weight}");
            let handle = advances::shaper_for(&ttf);
            let upem = f64::from(handle.units_per_em());
            for pair in &extra {
                let mut chars = pair.chars();
                let (a, b) = (chars.next().expect("a"), chars.next().expect("b"));
                let kern = (advances::shaped_advance(&handle, pair)
                    - advances::shaped_advance(&handle, &a.to_string())
                    - advances::shaped_advance(&handle, &b.to_string()))
                    / upem
                    * TEXT_PX;
                assert!(kern.abs() < GRID, "{weight} {pair}: shapes to {kern:.5} px");
            }
        }
    }

    /// The fixture is only worth testing against if it really does hide its
    /// rules: every lookup in both tables must be an extension.
    #[test]
    fn the_fixture_hides_every_rule_behind_an_extension() {
        let font = FontRef::new(EXTENSION_FACE).expect("the fixture reads");
        let gpos = font.gpos().expect("GPOS").lookup_list().expect("lookups");
        let gsub = font.gsub().expect("GSUB").lookup_list().expect("lookups");
        let gpos: Vec<_> = gpos
            .lookups()
            .iter()
            .map(|l| l.expect("a lookup"))
            .collect();
        let gsub: Vec<_> = gsub
            .lookups()
            .iter()
            .map(|l| l.expect("a lookup"))
            .collect();
        assert_eq!(gpos.len(), 2);
        assert_eq!(gsub.len(), 1);
        assert!(
            gpos.iter()
                .all(|l| matches!(l, PositionLookup::Extension(_)))
        );
        assert!(
            gsub.iter()
                .all(|l| matches!(l, SubstitutionLookup::Extension(_)))
        );
    }

    /// A rule behind an extension is reached like any other: both pair
    /// formats and the ligature.
    #[test]
    fn rules_behind_extension_lookups_are_reached() {
        let r = reachable(EXTENSION_FACE).expect("the fixture reads");
        assert_eq!(r.kern_pairs, BTreeSet::from([('A', 'V'), ('V', 'A')]));
        assert_eq!(r.ligatures, BTreeSet::from([vec!['f', 'i']]));
    }

    /// Longest first, then in character order, each once, and counted
    /// against the sweep of the same length over the same alphabet.
    #[test]
    fn sequences_are_ordered_and_counted_against_the_sweep() {
        let r = Reachable {
            kern_pairs: BTreeSet::from([('V', 'A'), ('A', 'V')]),
            ligatures: BTreeSet::from([vec!['f', 'f', 'i'], vec!['f', 'i']]),
        };
        let sequences: Vec<String> = r.sequences().iter().map(|s| s.iter().collect()).collect();
        assert_eq!(sequences, ["ffi", "AV", "VA", "fi"]);
        assert_eq!(r.against_brute_force(95), (4, 95u128.pow(3)));
        assert_eq!(
            Reachable::default().against_brute_force(95),
            (0, 95u128.pow(2))
        );
    }
}
