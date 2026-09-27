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
//! WHAT IS READ is pair positioning (GPOS type 2), and in GSUB ligature
//! substitution (type 4) and the contextual rules (types 5, 6 and 8:
//! contextual, chaining contextual and reverse chaining, in every format),
//! each plain or behind an extension. The contextual rules read are those
//! of the features a browser applies without being asked
//! (`DEFAULT_FEATURES`). They name glyphs rather than characters, often
//! variants an earlier rule produced, so each glyph is traced back to text
//! that makes it, with the text around it that its own rule needed, and
//! each rule yields a few sequences that put every glyph it names where it
//! names it (`contextual_inputs`).
//!
//! Everything is held to independent references. Plex Sans's pairs are
//! checked against a sweep of every pair read out of Chrome
//! (`fixtures/browser-kerns.txt`). Iosevka's contextual rules, 131 subtables
//! under `calt`, are checked against HarfBuzz (`fixtures/calt-glyphs.txt`):
//! the 2074 sequences read from the rules, none longer than eight
//! characters, bring in every glyph a sweep of all 82 million strings of up
//! to four characters finds, 146 of them, and 13 more that need five or six
//! characters, where a sweep would be 7.7 billion strings. A face built to
//! hold one rule of every kind (`fixtures/context-face.ttf`) checks each
//! format and case the served faces do not use. GPOS contextual positioning
//! (types 7 and 8) is not read; no face served here has any.

use std::collections::{BTreeMap, BTreeSet};

use read_fonts::{
    FontRef, ReadError, TableProvider,
    tables::{
        gpos::PositionLookup,
        gsub::{self, SubstitutionLookup},
        layout::{
            ChainedSequenceContext, ClassDef, CoverageTable, SequenceContext, SequenceLookupRecord,
        },
    },
    types::{BigEndian, GlyphId16, Tag},
};

/// What a face can reach, in characters rather than glyphs, with each
/// sequence traceable to the kind of rule that produced it.
#[derive(Clone, Debug, Default)]
pub struct Reachable {
    /// Ordered pairs some GPOS pair-positioning subtable mentions.
    pub kern_pairs: BTreeSet<(char, char)>,
    /// Character sequences some GSUB ligature subtable replaces.
    pub ligatures: BTreeSet<Vec<char>>,
    /// Character sequences that put every glyph a contextual, chaining
    /// contextual or reverse chaining rule names in the place it names it.
    pub contextual: BTreeSet<Vec<char>>,
}

impl Reachable {
    /// Every sequence worth measuring, longest first so a caller that
    /// truncates keeps the interesting ones.
    pub fn sequences(&self) -> Vec<Vec<char>> {
        let mut out: Vec<Vec<char>> = self
            .ligatures
            .iter()
            .chain(&self.contextual)
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
                            // a listed pair can list zeroes too (a feature
                            // file's `<0 0 0 0>`), and those move nothing
                            if !moves(record.value_record1()) && !moves(record.value_record2()) {
                                continue;
                            }
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

/// A GSUB subtable with any extension wrapper taken off, so every reader
/// below sees the rules a face keeps behind extensions (see the comment in
/// `pair_positioning`) exactly as it sees plain ones.
enum Subst<'a> {
    Single(gsub::SingleSubst<'a>),
    Multiple(gsub::MultipleSubstFormat1<'a>),
    Alternate(gsub::AlternateSubstFormat1<'a>),
    Ligature(gsub::LigatureSubstFormat1<'a>),
    Context(SequenceContext<'a>),
    Chain(ChainedSequenceContext<'a>),
    Reverse(gsub::ReverseChainSingleSubstFormat1<'a>),
}

/// Every GSUB subtable of the face, unwrapped, in lookup order, with the
/// index of the lookup it belongs to.
fn gsub_subtables<'a>(font: &FontRef<'a>) -> Result<Vec<(u16, Subst<'a>)>, ReadError> {
    let mut out = Vec::new();
    let Ok(gsub) = font.gsub() else {
        return Ok(out);
    };
    for (index, lookup) in gsub.lookup_list()?.lookups().iter().enumerate() {
        let index = index as u16;
        match lookup? {
            SubstitutionLookup::Single(l) => {
                for t in l.subtables().iter() {
                    out.push((index, Subst::Single(t?)));
                }
            }
            SubstitutionLookup::Multiple(l) => {
                for t in l.subtables().iter() {
                    out.push((index, Subst::Multiple(t?)));
                }
            }
            SubstitutionLookup::Alternate(l) => {
                for t in l.subtables().iter() {
                    out.push((index, Subst::Alternate(t?)));
                }
            }
            SubstitutionLookup::Ligature(l) => {
                for t in l.subtables().iter() {
                    out.push((index, Subst::Ligature(t?)));
                }
            }
            SubstitutionLookup::Contextual(l) => {
                for t in l.subtables().iter() {
                    out.push((index, Subst::Context(t?)));
                }
            }
            SubstitutionLookup::ChainContextual(l) => {
                for t in l.subtables().iter() {
                    out.push((index, Subst::Chain(t?)));
                }
            }
            SubstitutionLookup::Reverse(l) => {
                for t in l.subtables().iter() {
                    out.push((index, Subst::Reverse(t?)));
                }
            }
            SubstitutionLookup::Extension(l) => {
                for t in l.subtables().iter() {
                    out.push((
                        index,
                        match t? {
                            gsub::ExtensionSubtable::Single(w) => Subst::Single(w.extension()?),
                            gsub::ExtensionSubtable::Multiple(w) => Subst::Multiple(w.extension()?),
                            gsub::ExtensionSubtable::Alternate(w) => {
                                Subst::Alternate(w.extension()?)
                            }
                            gsub::ExtensionSubtable::Ligature(w) => Subst::Ligature(w.extension()?),
                            gsub::ExtensionSubtable::Contextual(w) => {
                                Subst::Context(w.extension()?)
                            }
                            gsub::ExtensionSubtable::ChainContextual(w) => {
                                Subst::Chain(w.extension()?)
                            }
                            gsub::ExtensionSubtable::Reverse(w) => Subst::Reverse(w.extension()?),
                        },
                    ));
                }
            }
        }
    }
    Ok(out)
}

/// Every character sequence a GSUB ligature subtable replaces.
fn ligature_inputs(
    subtables: &[(u16, Subst)],
    chars: &BTreeMap<GlyphId16, char>,
) -> Result<BTreeSet<Vec<char>>, ReadError> {
    let mut out = BTreeSet::new();
    for (_, subtable) in subtables {
        let Subst::Ligature(subtable) = subtable else {
            continue;
        };
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

/// How many contexts each glyph keeps: the best few by length, then in
/// character order. One is not enough (the shortest context of a glyph is
/// often the one a rule cannot use), and keeping every context a glyph
/// appears in grows without bound.
const CONTEXTS_PER_GLYPH: usize = 4;

/// Where a glyph comes from: text that produces it, and the span of that
/// text it is drawn from. The text either side of the span is context the
/// rule that writes the glyph needed: in Iosevka the arrow body glyph
/// behind `<-->` is only written when a hyphen follows, so its context
/// carries that hyphen and the `>` after it.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Context {
    chars: Vec<char>,
    start: usize,
    end: usize,
}

impl Context {
    fn key(&self) -> (usize, &[char], usize, usize) {
        (self.chars.len(), &self.chars, self.start, self.end)
    }
}

/// Where in a text a position's glyph was drawn from: start and end.
type Span = (usize, usize);
/// A text and the span each position of a rule took in it.
type Joined = (Vec<char>, Vec<Span>);
/// A rule's sequence: its text, and the glyph and span at each position.
type Placed = (Vec<char>, Vec<(GlyphId16, Span)>);

/// The contexts known for each glyph, the best `CONTEXTS_PER_GLYPH` of each.
#[derive(Default)]
struct Contexts(BTreeMap<GlyphId16, Vec<Context>>);

impl Contexts {
    /// Offer a context for `glyph`, which keeps it if it is among the best;
    /// whether it was kept.
    fn offer(&mut self, glyph: GlyphId16, context: Context) -> bool {
        let known = self.0.entry(glyph).or_default();
        if known.contains(&context) {
            return false;
        }
        known.push(context.clone());
        known.sort_by(|a, b| a.key().cmp(&b.key()));
        known.truncate(CONTEXTS_PER_GLYPH);
        known.contains(&context)
    }
}

/// Join contexts position by position into one text, each glyph's core
/// placed after the last and the text either side of it required to agree
/// with what is already there. Returns the text and each position's span.
///
/// Strict, it returns `None` where two contexts disagree. Otherwise a later
/// position wins: an earlier glyph's right context is cut back where it
/// disagrees with what follows, and a left context that disagrees with what
/// precedes is dropped. That can lose what made the earlier glyph, but the
/// rule's own positions are kept, which is the point of the sequence; the
/// strict join is tried first.
fn splice(parts: &[&Context], strict: bool) -> Option<Joined> {
    let first = parts.first()?;
    let mut out = first.chars.clone();
    let mut spans = vec![(first.start, first.end)];
    let mut at = first.end;
    for part in &parts[1..] {
        let mut left = &part.chars[..part.start];
        let rest = &part.chars[part.start..];
        // the left context must agree with the text before `at`, and any of
        // it that reaches further back is prepended
        let overlap = left.len().min(at);
        if out[at - overlap..at] != left[left.len() - overlap..] {
            if strict {
                return None;
            }
            left = &[];
        }
        let overlap = left.len().min(at);
        let extra = left.len() - overlap;
        if extra > 0 {
            let mut grown = left[..extra].to_vec();
            grown.extend(&out);
            out = grown;
            at += extra;
            for span in &mut spans {
                span.0 += extra;
                span.1 += extra;
            }
        }
        // the core and right context must agree with any text already past
        // `at` (an earlier right context), and extend it
        for (i, c) in rest.iter().enumerate() {
            match out.get(at + i) {
                Some(existing) if existing != c => {
                    if strict {
                        return None;
                    }
                    out.truncate(at + i);
                    out.push(*c);
                }
                Some(_) => {}
                None => out.push(*c),
            }
        }
        let width = part.end - part.start;
        spans.push((at, at + width));
        at += width;
    }
    Some((out, spans))
}

/// The cores alone, joined: the text a rule's positions make with no
/// context either side, for when their contexts disagree.
fn cores(parts: &[&Context]) -> Joined {
    let mut out = Vec::new();
    let mut spans = Vec::new();
    for part in parts {
        let start = out.len();
        out.extend(&part.chars[part.start..part.end]);
        spans.push((start, out.len()));
    }
    (out, spans)
}

/// One position of a contextual rule: the glyphs that can stand there.
enum Position<'a> {
    /// These glyphs exactly.
    Glyphs(Vec<GlyphId16>),
    /// The glyphs a class definition puts in this class. Class 0 holds
    /// every glyph no other class claims, which is most of the face, so a
    /// rule that asks for it is asking for "anything else" and one
    /// representative stands for it.
    Class(ClassDef<'a>, u16),
}

/// What a rule writes at one of its positions.
enum Write {
    /// A nested lookup, by index, applied there.
    Lookup(u16),
    /// A reverse chaining rule's own substitution.
    Map(BTreeMap<GlyphId16, GlyphId16>),
}

/// A contextual, chaining contextual or reverse chaining rule: the lookup
/// it belongs to, its positions in reading order (a backtrack is stored
/// closest glyph first, and is reversed here), and what it writes at which
/// position.
struct Rule<'a> {
    lookup: u16,
    positions: Vec<Position<'a>>,
    writes: Vec<(usize, Write)>,
}

impl<'a> Rule<'a> {
    fn new(
        lookup: u16,
        positions: Vec<Position<'a>>,
        backtrack: usize,
        records: &[SequenceLookupRecord],
    ) -> Self {
        let writes = records
            .iter()
            .map(|r| {
                (
                    backtrack + usize::from(r.sequence_index()),
                    Write::Lookup(r.lookup_list_index()),
                )
            })
            .collect();
        Rule {
            lookup,
            positions,
            writes,
        }
    }
}

/// Every contextual rule in the lookups `applied`.
fn contextual_rules<'a>(
    subtables: &[(u16, Subst<'a>)],
    applied: &BTreeSet<u16>,
) -> Result<Vec<Rule<'a>>, ReadError> {
    let glyphs = |cov: CoverageTable| Position::Glyphs(cov.iter().collect());
    let listed = |seq: &[BigEndian<GlyphId16>]| -> Vec<Position<'a>> {
        seq.iter()
            .map(|g| Position::Glyphs(vec![g.get()]))
            .collect()
    };
    let mut rules = Vec::new();
    for (index, subtable) in subtables {
        if !applied.contains(index) {
            continue;
        }
        match subtable {
            Subst::Context(SequenceContext::Format1(t)) => {
                for (first, set) in t.coverage()?.iter().zip(t.seq_rule_sets().iter()) {
                    let Some(set) = set.transpose()? else {
                        continue;
                    };
                    for rule in set.seq_rules().iter() {
                        let rule = rule?;
                        let mut positions = vec![Position::Glyphs(vec![first])];
                        positions.extend(listed(rule.input_sequence()));
                        rules.push(Rule::new(*index, positions, 0, rule.seq_lookup_records()));
                    }
                }
            }
            Subst::Context(SequenceContext::Format2(t)) => {
                let classes = t.class_def()?;
                let firsts: Vec<GlyphId16> = t.coverage()?.iter().collect();
                for (class, set) in t.class_seq_rule_sets().iter().enumerate() {
                    let Some(set) = set.transpose()? else {
                        continue;
                    };
                    let class = class as u16;
                    let first: Vec<GlyphId16> = firsts
                        .iter()
                        .copied()
                        .filter(|g| classes.get(*g) == class)
                        .collect();
                    for rule in set.class_seq_rules().iter() {
                        let rule = rule?;
                        let mut positions = vec![Position::Glyphs(first.clone())];
                        positions.extend(
                            rule.input_sequence()
                                .iter()
                                .map(|c| Position::Class(classes.clone(), c.get())),
                        );
                        rules.push(Rule::new(*index, positions, 0, rule.seq_lookup_records()));
                    }
                }
            }
            Subst::Context(SequenceContext::Format3(t)) => {
                let mut positions = Vec::new();
                for cov in t.coverages().iter() {
                    positions.push(glyphs(cov?));
                }
                rules.push(Rule::new(*index, positions, 0, t.seq_lookup_records()));
            }
            Subst::Chain(ChainedSequenceContext::Format1(t)) => {
                for (first, set) in t.coverage()?.iter().zip(t.chained_seq_rule_sets().iter()) {
                    let Some(set) = set.transpose()? else {
                        continue;
                    };
                    for rule in set.chained_seq_rules().iter() {
                        let rule = rule?;
                        let back = rule.backtrack_sequence();
                        let mut positions: Vec<Position> = listed(back).into_iter().rev().collect();
                        positions.push(Position::Glyphs(vec![first]));
                        positions.extend(listed(rule.input_sequence()));
                        positions.extend(listed(rule.lookahead_sequence()));
                        rules.push(Rule::new(
                            *index,
                            positions,
                            back.len(),
                            rule.seq_lookup_records(),
                        ));
                    }
                }
            }
            Subst::Chain(ChainedSequenceContext::Format2(t)) => {
                let (back, input, ahead) = (
                    t.backtrack_class_def()?,
                    t.input_class_def()?,
                    t.lookahead_class_def()?,
                );
                let firsts: Vec<GlyphId16> = t.coverage()?.iter().collect();
                for (class, set) in t.chained_class_seq_rule_sets().iter().enumerate() {
                    let Some(set) = set.transpose()? else {
                        continue;
                    };
                    let class = class as u16;
                    let first: Vec<GlyphId16> = firsts
                        .iter()
                        .copied()
                        .filter(|g| input.get(*g) == class)
                        .collect();
                    for rule in set.chained_class_seq_rules().iter() {
                        let rule = rule?;
                        let behind = rule.backtrack_sequence();
                        let mut positions: Vec<Position> = behind
                            .iter()
                            .rev()
                            .map(|c| Position::Class(back.clone(), c.get()))
                            .collect();
                        positions.push(Position::Glyphs(first.clone()));
                        positions.extend(
                            rule.input_sequence()
                                .iter()
                                .map(|c| Position::Class(input.clone(), c.get())),
                        );
                        positions.extend(
                            rule.lookahead_sequence()
                                .iter()
                                .map(|c| Position::Class(ahead.clone(), c.get())),
                        );
                        rules.push(Rule::new(
                            *index,
                            positions,
                            behind.len(),
                            rule.seq_lookup_records(),
                        ));
                    }
                }
            }
            Subst::Chain(ChainedSequenceContext::Format3(t)) => {
                let mut positions = Vec::new();
                let back: Vec<_> = t.backtrack_coverages().iter().collect();
                let behind = back.len();
                for cov in back.into_iter().rev() {
                    positions.push(glyphs(cov?));
                }
                for cov in t
                    .input_coverages()
                    .iter()
                    .chain(t.lookahead_coverages().iter())
                {
                    positions.push(glyphs(cov?));
                }
                rules.push(Rule::new(*index, positions, behind, t.seq_lookup_records()));
            }
            Subst::Reverse(t) => {
                let mut positions = Vec::new();
                let back: Vec<_> = t.backtrack_coverages().iter().collect();
                let behind = back.len();
                for cov in back.into_iter().rev() {
                    positions.push(glyphs(cov?));
                }
                let coverage = t.coverage()?;
                let map = coverage
                    .iter()
                    .zip(t.substitute_glyph_ids().iter().map(|g| g.get()))
                    .collect();
                positions.push(glyphs(coverage));
                for cov in t.lookahead_coverages().iter() {
                    positions.push(glyphs(cov?));
                }
                rules.push(Rule {
                    lookup: *index,
                    positions,
                    writes: vec![(behind, Write::Map(map))],
                });
            }
            _ => {}
        }
    }
    Ok(rules)
}

/// The GSUB features a browser applies to horizontal text without being
/// asked: HarfBuzz's defaults, which Chrome and Firefox use as they are.
/// Everything else (discretionary ligatures, stylistic sets, the language
/// systems Iosevka offers as features) runs only when a page asks for it,
/// and a rule that never runs is not a rule a test string should chase.
const DEFAULT_FEATURES: [&[u8; 4]; 7] = [
    b"ccmp", b"locl", b"rlig", b"rclt", b"calt", b"clig", b"liga",
];

/// The lookups the default features apply directly, as opposed to those
/// only a contextual rule calls.
fn default_lookups(font: &FontRef) -> Result<BTreeSet<u16>, ReadError> {
    let mut out = BTreeSet::new();
    let Ok(gsub) = font.gsub() else {
        return Ok(out);
    };
    let features = gsub.feature_list()?;
    for record in features.feature_records() {
        if !DEFAULT_FEATURES
            .iter()
            .any(|tag| record.feature_tag() == Tag::new(tag))
        {
            continue;
        }
        let feature = record.feature(features.offset_data())?;
        out.extend(feature.lookup_list_indices().iter().map(|i| i.get()));
    }
    Ok(out)
}

/// What each lookup's one-to-one substitutions write, glyph by glyph: the
/// single, multiple and alternate substitutions a rule can call.
fn lookup_writes(
    subtables: &[(u16, Subst)],
) -> Result<BTreeMap<u16, BTreeMap<GlyphId16, Vec<GlyphId16>>>, ReadError> {
    let mut out: BTreeMap<u16, BTreeMap<GlyphId16, Vec<GlyphId16>>> = BTreeMap::new();
    for (index, subtable) in subtables {
        let map = out.entry(*index).or_default();
        let mut add = |from: GlyphId16, to: GlyphId16| map.entry(from).or_default().push(to);
        match subtable {
            Subst::Single(gsub::SingleSubst::Format1(t)) => {
                let delta = i32::from(t.delta_glyph_id());
                for from in t.coverage()?.iter() {
                    let to = (i32::from(from.to_u16()) + delta).rem_euclid(0x1_0000) as u16;
                    add(from, GlyphId16::new(to));
                }
            }
            Subst::Single(gsub::SingleSubst::Format2(t)) => {
                for (from, to) in t.coverage()?.iter().zip(t.substitute_glyph_ids()) {
                    add(from, to.get());
                }
            }
            Subst::Multiple(t) => {
                for (from, seq) in t.coverage()?.iter().zip(t.sequences().iter()) {
                    for to in seq?.substitute_glyph_ids() {
                        add(from, to.get());
                    }
                }
            }
            Subst::Alternate(t) => {
                for (from, set) in t.coverage()?.iter().zip(t.alternate_sets().iter()) {
                    for to in set?.alternate_glyph_ids() {
                        add(from, to.get());
                    }
                }
            }
            _ => {}
        }
    }
    Ok(out)
}

/// A rule's positions resolved to glyph lists once, over every glyph that
/// can occur (`anything` marks a class 0 position, where one member stands
/// for the rest).
struct Resolved<'r> {
    rule: &'r Rule<'r>,
    positions: Vec<(Vec<GlyphId16>, bool)>,
}

fn resolve<'r>(rule: &'r Rule<'r>, universe: &BTreeSet<GlyphId16>) -> Resolved<'r> {
    let positions = rule
        .positions
        .iter()
        .map(|position| match position {
            Position::Glyphs(glyphs) => (glyphs.clone(), false),
            Position::Class(def, class) => (
                universe
                    .iter()
                    .copied()
                    .filter(|g| def.get(*g) == *class)
                    .collect(),
                *class == 0,
            ),
        })
        .collect();
    Resolved { rule, positions }
}

/// The members of each position of a rule: every context of every glyph
/// that can stand there, best first. None if some position has none, since
/// then no text reaches the rule.
fn members<'c>(
    rule: &Resolved,
    contexts: &'c Contexts,
) -> Option<Vec<Vec<(GlyphId16, &'c Context)>>> {
    let mut out = Vec::with_capacity(rule.positions.len());
    for (glyphs, anything) in &rule.positions {
        let mut here: Vec<(GlyphId16, &Context)> = glyphs
            .iter()
            .filter_map(|g| contexts.0.get(g).map(|cs| (g, cs)))
            .flat_map(|(g, cs)| cs.iter().map(move |c| (*g, c)))
            .collect();
        here.sort_by(|a, b| a.1.key().cmp(&b.1.key()).then(a.0.cmp(&b.0)));
        if *anything {
            here.truncate(1);
        }
        if here.is_empty() {
            return None;
        }
        out.push(here);
    }
    Some(out)
}

/// The text of each of a rule's sequences, and the glyph and span at each
/// position of it.
///
/// A rule's positions are sets, and the product of the sets is what a
/// sweep would try; a rule with five positions of four glyphs each is a
/// thousand strings. Covering each member once needs only as many strings
/// as the largest position holds: the k-th string takes the k-th member of
/// every position, wrapping round the shorter ones. That exercises every
/// glyph a rule names, in every context kept for it, in the place the rule
/// names it; it does not try every combination, and does not need to,
/// because the shaper decides what a string turns into.
fn rule_sequences(members: &[Vec<(GlyphId16, &Context)>]) -> Vec<Placed> {
    let widest = members.iter().map(Vec::len).max().unwrap_or(0);
    (0..widest)
        .map(|k| {
            let chosen: Vec<&(GlyphId16, &Context)> =
                members.iter().map(|m| &m[k % m.len()]).collect();
            let parts: Vec<&Context> = chosen.iter().map(|(_, c)| *c).collect();
            let (text, spans) = splice(&parts, true)
                .or_else(|| splice(&parts, false))
                .unwrap_or_else(|| cores(&parts));
            let placed = chosen.iter().map(|(g, _)| *g).zip(spans).collect();
            (text, placed)
        })
        .collect()
}

/// Every glyph's contexts, and the sequences that exercise every contextual
/// rule, found together.
///
/// A CONTEXTUAL RULE NAMES GLYPHS, NOT CHARACTERS, and many of the glyphs it
/// names are variants an earlier rule put there: in Iosevka, 74 of the 210
/// positions in the chaining rules under `calt` name no glyph a character
/// maps to. Each such glyph is traced back to text that produces it, and
/// the text carries its context. A substitution a feature applies directly
/// gives its output the contexts of its input (a ligature, its components'
/// contexts spliced). A rule gives the glyph it writes the rule's whole
/// sequence as context, so a later rule that names the glyph gets the text
/// around it that made it, not just the characters under it. A rule offers
/// two contexts for what it writes: the window its own positions cover, and
/// that window with the context its positions' glyphs brought along.
///
/// Each step was measured against the four-character HarfBuzz sweep's 146
/// glyphs. The shortest standalone text for each glyph reached 125: the
/// arrow bodies in `<-->` and `<==>` are written only when what follows is
/// right, and a standalone source forgets what follows. Carrying each
/// rule's whole sequence reached 138, and 136 keeping sixteen contexts a
/// glyph rather than four, because contexts grew along chains of rules
/// until other rules took over; offering the rule's own window as well
/// reached 140. Reading only the rules of the default features reached 141
/// (Iosevka's `dlig` and language features were filling each glyph's few
/// contexts with rules a browser never runs), and joining disagreeing
/// contexts by letting the later position win, rather than falling back to
/// the bare cores, reached all 146.
///
/// The lookups are read in index order, which is the order a shaper applies
/// them in, so a rule meets only variants earlier lookups wrote, and within
/// a lookup its rules repeat until they write nothing new, since a shaper
/// walks a lookup across the text and a rule can read what another rule of
/// the same lookup wrote a position earlier. The repeats end: a glyph keeps
/// at most `CONTEXTS_PER_GLYPH`, and a full set only admits a context that
/// is shorter, or at the same length earlier in character order, than one
/// it holds. Repeating every lookup until nothing changed anywhere was
/// tried first: it let a rule see variants of later lookups, which no
/// shaper does, and the mutation run showed no test depended on it; one
/// pass with no repeats at all reached 125 of the 146.
fn contextual_inputs(
    subtables: &[(u16, Subst)],
    rules: &[Rule],
    direct: &BTreeSet<u16>,
    chars: &BTreeMap<GlyphId16, char>,
) -> Result<BTreeSet<Vec<char>>, ReadError> {
    let writes = lookup_writes(subtables)?;
    // every glyph that can occur: those characters map to, and every glyph
    // a substitution writes
    let mut universe: BTreeSet<GlyphId16> = chars.keys().copied().collect();
    universe.extend(writes.values().flat_map(|m| m.values().flatten().copied()));
    for rule in rules {
        for (_, write) in &rule.writes {
            if let Write::Map(map) = write {
                universe.extend(map.values().copied());
            }
        }
    }
    for (_, subtable) in subtables {
        if let Subst::Ligature(t) = subtable {
            for set in t.ligature_sets().iter() {
                for ligature in set?.ligatures().iter() {
                    universe.insert(ligature?.ligature_glyph());
                }
            }
        }
    }
    let rules: Vec<Resolved> = rules.iter().map(|r| resolve(r, &universe)).collect();
    let mut contexts = Contexts::default();
    for (gid, c) in chars {
        contexts.offer(
            *gid,
            Context {
                chars: vec![*c],
                start: 0,
                end: 1,
            },
        );
    }
    // One pass in lookup order, as a shaper applies lookups: a rule sees
    // the variants earlier lookups wrote, and its sequences are read off the
    // contexts known when its turn comes.
    let mut out = BTreeSet::new();
    for index in direct {
        for (_, subtable) in subtables.iter().filter(|(i, _)| i == index) {
            let Subst::Ligature(t) = subtable else {
                continue;
            };
            for (first, set) in t.coverage()?.iter().zip(t.ligature_sets().iter()) {
                for ligature in set?.ligatures().iter() {
                    let ligature = ligature?;
                    let mut glyphs = vec![first];
                    glyphs.extend(ligature.component_glyph_ids().iter().map(|g| g.get()));
                    let Some(parts) = glyphs
                        .iter()
                        .map(|g| contexts.0.get(g).and_then(|cs| cs.first()))
                        .collect::<Option<Vec<&Context>>>()
                    else {
                        continue;
                    };
                    let (text, spans) = splice(&parts, true)
                        .or_else(|| splice(&parts, false))
                        .unwrap_or_else(|| cores(&parts));
                    let span = (spans[0].0, spans[spans.len() - 1].1);
                    let context = Context {
                        chars: text,
                        start: span.0,
                        end: span.1,
                    };
                    contexts.offer(ligature.ligature_glyph(), context);
                }
            }
        }
        for (from, tos) in writes.get(index).into_iter().flatten() {
            let known = contexts.0.get(from).cloned().unwrap_or_default();
            for to in tos {
                for context in &known {
                    contexts.offer(*to, context.clone());
                }
            }
        }
        // What this lookup's rules write, in the context of the rule. A
        // shaper walks a lookup across the text, so a rule can read what
        // another rule of the same lookup wrote a position earlier: the
        // lookup's rules repeat until they write nothing new.
        let own: Vec<&Resolved> = rules.iter().filter(|r| r.rule.lookup == *index).collect();
        loop {
            let mut changed = false;
            for rule in &own {
                let Some(members) = members(rule, &contexts) else {
                    continue;
                };
                let mut offers = Vec::new();
                for (text, placed) in rule_sequences(&members) {
                    for (at, write) in &rule.rule.writes {
                        let Some((glyph, span)) = placed.get(*at) else {
                            continue;
                        };
                        let outputs: Vec<GlyphId16> = match write {
                            Write::Lookup(index) => writes
                                .get(index)
                                .and_then(|m| m.get(glyph))
                                .cloned()
                                .unwrap_or_default(),
                            Write::Map(map) => map.get(glyph).into_iter().copied().collect(),
                        };
                        // the window the rule's own positions cover, and the
                        // whole text around it
                        let first = placed.first().map_or(0, |(_, (start, _))| *start);
                        let last = placed.last().map_or(text.len(), |(_, (_, end))| *end);
                        for output in outputs {
                            offers.push((
                                output,
                                Context {
                                    chars: text[first..last].to_vec(),
                                    start: span.0 - first,
                                    end: span.1 - first,
                                },
                            ));
                            offers.push((
                                output,
                                Context {
                                    chars: text.clone(),
                                    start: span.0,
                                    end: span.1,
                                },
                            ));
                        }
                    }
                    out.insert(text);
                }
                for (glyph, context) in offers {
                    changed |= contexts.offer(glyph, context);
                }
            }
            if !changed {
                break;
            }
        }
    }
    Ok(out)
}

/// Ask a face what its own tables can reach.
pub fn reachable(ttf: &[u8]) -> Result<Reachable, ReadError> {
    let font = FontRef::new(ttf)?;
    let chars = characters_by_glyph(&font)?;
    let subtables = gsub_subtables(&font)?;
    let applied = default_lookups(&font)?;
    let rules = contextual_rules(&subtables, &applied)?;
    Ok(Reachable {
        kern_pairs: pair_positioning(&font, &chars)?,
        ligatures: ligature_inputs(&subtables, &chars)?,
        contextual: contextual_inputs(&subtables, &rules, &applied, &chars)?,
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
            // one the ligatures already hold, which is listed once
            contextual: BTreeSet::from([vec!['-', '-', '>', '>'], vec!['f', 'i']]),
        };
        let sequences: Vec<String> = r.sequences().iter().map(|s| s.iter().collect()).collect();
        assert_eq!(sequences, ["-->>", "ffi", "AV", "VA", "fi"]);
        assert_eq!(r.against_brute_force(95), (5, 95u128.pow(4)));
        assert_eq!(
            Reachable::default().against_brute_force(95),
            (0, 95u128.pow(2))
        );
    }

    /// Iosevka SS08 regular, the face `fixtures/calt-glyphs.txt` was swept in.
    fn iosevka() -> Vec<u8> {
        let drawn = advances::Drawn {
            family: "Iosevka SS08",
            weight: "400",
            style: "normal",
            ident: "",
            text: "",
        };
        advances::face_ttf(&advances::assets(), &drawn)
    }

    /// The fixture: every glyph HarfBuzz saw `calt` bring in over every
    /// string of one to three printable ASCII characters, by glyph id, with
    /// the shortest string that shows it.
    fn calt_glyphs() -> Vec<(u16, String)> {
        include_str!("../fixtures/calt-glyphs.txt")
            .lines()
            .filter(|l| !l.starts_with('#') && !l.is_empty())
            .map(|line| {
                let mut fields = line.splitn(3, '\t');
                let gid = fields
                    .next()
                    .expect("a glyph id")
                    .parse()
                    .expect("a number");
                let _name = fields.next().expect("a glyph name");
                let witness = fields.next().expect("a witness");
                let witness = witness
                    .strip_prefix('|')
                    .and_then(|w| w.strip_suffix('|'))
                    .expect("a witness between bars");
                (gid, witness.to_owned())
            })
            .collect()
    }

    /// The glyphs `calt` brings into `text`: present when shaped with the
    /// default features and absent when shaped with `calt` off.
    fn brought_by_calt(shaper: &harfrust::Shaper<'_>, text: &str) -> BTreeSet<u16> {
        let shape = |features: &[harfrust::Feature]| -> BTreeSet<u16> {
            let mut buffer = harfrust::UnicodeBuffer::new();
            buffer.push_str(text);
            buffer.guess_segment_properties();
            let options = harfrust::ShapeOptions::new().features(features);
            shaper
                .shape(buffer, options)
                .glyph_infos()
                .iter()
                .map(|g| g.glyph_id as u16)
                .collect()
        };
        let off: harfrust::Feature = "-calt".parse().expect("a feature");
        let on = shape(&[]);
        let without = shape(&[off]);
        on.difference(&without).copied().collect()
    }

    /// The shaper this crate uses agrees with HarfBuzz, the one browsers
    /// use, on every glyph in the fixture: each witness brings in its glyph
    /// here too. The test below leans on that agreement.
    #[test]
    fn harfrust_brings_in_every_glyph_harfbuzz_did_for_the_same_text() {
        let ttf = iosevka();
        let handle = advances::shaper_for(&ttf);
        let shaper = handle.data.shaper(&handle.font).build();
        let fixture = calt_glyphs();
        assert!(fixture.len() > 100, "only {} glyphs", fixture.len());
        for (gid, witness) in fixture {
            let brought = brought_by_calt(&shaper, &witness);
            assert!(
                brought.contains(&gid),
                "{witness:?} brings in {brought:?}, not {gid}"
            );
        }
    }

    /// The contract for contextual rules, against an independent sweep:
    /// shaping the sequences read from Iosevka's rules brings in every glyph
    /// that HarfBuzz saw `calt` bring in over all 82 million strings of up to
    /// four characters, and the 13 the fixture lists beyond that, each of
    /// which needs five or six. And it does so with a set at least a
    /// thousand times smaller than that sweep, which is what the module is
    /// for.
    #[test]
    fn contextual_sequences_bring_in_every_glyph_the_sweep_found() {
        let ttf = iosevka();
        let r = reachable(&ttf).expect("the face reads");
        let handle = advances::shaper_for(&ttf);
        let shaper = handle.data.shaper(&handle.font).build();
        let mut brought = BTreeSet::new();
        for sequence in &r.contextual {
            let text: String = sequence.iter().collect();
            brought.extend(brought_by_calt(&shaper, &text));
        }
        let swept: BTreeSet<u16> = calt_glyphs().into_iter().map(|(gid, _)| gid).collect();
        let missed: Vec<_> = swept.difference(&brought).collect();
        assert!(missed.is_empty(), "the sequences never bring in {missed:?}");
        // every string of one to four printable ASCII characters
        let sweep: usize = (1..=4).map(|n| 95usize.pow(n)).sum();
        assert!(
            r.contextual.len() * 1000 < sweep,
            "{} sequences is not a thousandth of the {sweep} the sweep shapes",
            r.contextual.len()
        );
    }

    /// A face with one rule of every kind this module reads, built by
    /// fontTools from `fixtures/make_context_face.py`, and for each rule the
    /// text that shows it, confirmed with HarfBuzz.
    const CONTEXT_FACE: &[u8] = include_bytes!("../fixtures/context-face.ttf");

    /// (rule, witness, glyph id, whether the default features fire it)
    fn context_witnesses() -> Vec<(String, String, u16, bool)> {
        include_str!("../fixtures/context-face.txt")
            .lines()
            .filter(|l| !l.starts_with('#') && !l.is_empty())
            .map(|line| {
                let f: Vec<&str> = line.split('\t').collect();
                (
                    f[0].to_owned(),
                    f[1].to_owned(),
                    f[2].parse().expect("a glyph id"),
                    f[4] == "fires",
                )
            })
            .collect()
    }

    /// The fixture is only worth testing against if it holds what it says:
    /// every contextual format, reverse chaining, and single substitution
    /// in both of its formats.
    #[test]
    fn the_context_fixture_holds_every_rule_kind() {
        let font = FontRef::new(CONTEXT_FACE).expect("the fixture reads");
        let mut kinds = BTreeSet::new();
        for (_, subtable) in gsub_subtables(&font).expect("GSUB") {
            kinds.insert(match subtable {
                Subst::Single(gsub::SingleSubst::Format1(_)) => "single 1",
                Subst::Single(gsub::SingleSubst::Format2(_)) => "single 2",
                Subst::Ligature(_) => "ligature",
                Subst::Context(SequenceContext::Format1(_)) => "context 1",
                Subst::Context(SequenceContext::Format2(_)) => "context 2",
                Subst::Context(SequenceContext::Format3(_)) => "context 3",
                Subst::Chain(ChainedSequenceContext::Format1(_)) => "chain 1",
                Subst::Chain(ChainedSequenceContext::Format2(_)) => "chain 2",
                Subst::Chain(ChainedSequenceContext::Format3(_)) => "chain 3",
                Subst::Reverse(_) => "reverse",
                Subst::Multiple(_) => "multiple",
                Subst::Alternate(_) => "alternate",
            });
        }
        let expected = BTreeSet::from([
            "single 1",
            "single 2",
            "ligature",
            "context 1",
            "context 2",
            "context 3",
            "chain 1",
            "chain 2",
            "chain 3",
            "reverse",
        ]);
        assert_eq!(kinds, expected);
    }

    /// Every rule of every kind is reached: shaping the sequences read from
    /// the face brings in each rule's glyph. A rule that never fires
    /// contributes no sequence: one under a feature no browser applies by
    /// default (`dlig`), and two that read a variant and a ligature only
    /// later lookups write, which a shaper applying lookups in order never
    /// lets them see.
    #[test]
    fn every_rule_kind_is_reached() {
        let r = reachable(CONTEXT_FACE).expect("the fixture reads");
        let handle = advances::shaper_for(CONTEXT_FACE);
        let shaper = handle.data.shaper(&handle.font).build();
        let shaped = |text: &str| -> BTreeSet<u16> {
            let mut buffer = harfrust::UnicodeBuffer::new();
            buffer.push_str(text);
            buffer.guess_segment_properties();
            shaper
                .shape(buffer, harfrust::ShapeOptions::new())
                .glyph_infos()
                .iter()
                .map(|g| g.glyph_id as u16)
                .collect()
        };
        let brought: BTreeSet<u16> = r
            .contextual
            .iter()
            .flat_map(|s| shaped(&s.iter().collect::<String>()))
            .collect();
        for (rule, witness, gid, fires) in context_witnesses() {
            if fires {
                assert!(
                    shaped(&witness).contains(&gid),
                    "{rule}: HarfRust disagrees on {witness:?}"
                );
                assert!(
                    brought.contains(&gid),
                    "{rule}: no sequence brings in glyph {gid}"
                );
            } else {
                let texts: Vec<String> = r.contextual.iter().map(|s| s.iter().collect()).collect();
                assert!(
                    !texts.iter().any(|t| t.contains(&witness)),
                    "{rule} is not applied by default, yet {texts:?} chases it"
                );
            }
        }
    }

    /// A pair kerns when any one of its value fields moves, and not when
    /// none does: the fixture has a pair moving each field alone and one
    /// moving nothing.
    #[test]
    fn a_pair_moving_any_one_field_kerns_and_one_moving_none_does_not() {
        let r = reachable(CONTEXT_FACE).expect("the fixture reads");
        assert_eq!(
            r.kern_pairs,
            BTreeSet::from([('A', 'B'), ('C', 'D'), ('E', 'F'), ('G', 'H')])
        );
    }

    /// A ligature is a sequence text can type only if it has two or more
    /// components and a character maps to each: `uv` and `OP` are, the
    /// one-glyph ligature of `K` and the ligature of `l` with an unmapped
    /// glyph are not.
    #[test]
    fn only_typeable_ligatures_of_two_or_more_are_listed() {
        let r = reachable(CONTEXT_FACE).expect("the fixture reads");
        assert_eq!(
            r.ligatures,
            BTreeSet::from([vec!['u', 'v'], vec!['O', 'P']])
        );
    }

    fn ctx(text: &str, start: usize, end: usize) -> Context {
        Context {
            chars: text.chars().collect(),
            start,
            end,
        }
    }

    /// Contexts that agree are overlapped; the spans say where each glyph's
    /// core landed. `<-` with its core on `-` and a right context `->`, then
    /// `-` needing `<` two back, then `>`, make `<-->`.
    #[test]
    fn agreeing_contexts_overlap() {
        let a = ctx("<-->", 1, 2);
        let b = ctx("<--", 2, 3);
        let c = ctx(">", 0, 1);
        let (text, spans) = splice(&[&a, &b, &c], true).expect("they agree");
        assert_eq!(text.iter().collect::<String>(), "<-->");
        assert_eq!(spans, [(1, 2), (2, 3), (3, 4)]);
        // a left context reaching past the start is prepended, and moves the
        // spans already placed
        let (text, spans) = splice(&[&ctx("x", 0, 1), &ctx("abxy", 3, 4)], true).expect("agree");
        assert_eq!(text.iter().collect::<String>(), "abxy");
        assert_eq!(spans, [(2, 3), (3, 4)]);
    }

    /// Disagreeing contexts are refused when strict; otherwise the later
    /// position wins, cutting back a right context and dropping a left one.
    #[test]
    fn disagreeing_contexts_are_refused_strictly_and_resolved_otherwise() {
        let a = ctx("ab!", 0, 1);
        let b = ctx("c", 0, 1);
        assert_eq!(splice(&[&a, &b], true), None);
        let (text, spans) = splice(&[&a, &b], false).expect("resolved");
        assert_eq!(text.iter().collect::<String>(), "ac");
        assert_eq!(spans, [(0, 1), (1, 2)]);
        let left = ctx("zq", 1, 2);
        assert_eq!(splice(&[&a, &left], true), None);
        let (text, _) = splice(&[&a, &left], false).expect("resolved");
        assert_eq!(text.iter().collect::<String>(), "aq");
        // the soft join keeps what agrees, where the bare cores keep nothing:
        // `x` before `a` survives a right context cut back after it
        let kept = ctx("xab!", 1, 2);
        assert_eq!(splice(&[&kept, &b], true), None);
        let (text, spans) = splice(&[&kept, &b], false).expect("resolved");
        assert_eq!(text.iter().collect::<String>(), "xac");
        assert_eq!(spans, [(1, 2), (2, 3)]);
        assert_eq!(cores(&[&kept, &b]).0.iter().collect::<String>(), "ac");
    }

    /// A glyph keeps the best `CONTEXTS_PER_GLYPH` contexts, shortest first
    /// and then in character order, each once, and an offer says whether it
    /// was kept.
    #[test]
    fn a_glyph_keeps_its_best_few_contexts() {
        let mut contexts = Contexts::default();
        let g = GlyphId16::new(7);
        let offered = ["eeeee", "dddd", "ccc", "bb", "a", "zz"];
        for text in offered {
            assert!(
                contexts.offer(g, ctx(text, 0, 1)),
                "{text} is among the best so far"
            );
        }
        assert!(
            !contexts.offer(g, ctx("a", 0, 1)),
            "a repeat changes nothing"
        );
        assert!(
            !contexts.offer(g, ctx("yyyyyy", 0, 1)),
            "worse than every kept context"
        );
        let kept: Vec<String> = contexts.0[&g]
            .iter()
            .map(|c| c.chars.iter().collect())
            .collect();
        assert_eq!(kept, ["a", "bb", "zz", "ccc"]);
    }
}
