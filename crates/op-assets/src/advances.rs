//! The advance-width tables `op-chart` measures its label text with
//! (decision 14 of the chart survey).
//!
//! `op-chart` carries no dependencies and no build script, so it can read
//! neither a font file nor a font parser. The widths are measured here
//! instead, where the served faces and `ttf-parser` already live, and
//! committed as a Rust source that crate compiles in. The `emit-advances`
//! bin writes that source; [`tests::the_committed_table_matches_the_faces`]
//! regenerates it and compares byte for byte, so the numbers cannot drift
//! from the faces the site serves.

use std::path::{Path, PathBuf};

use crate::manifest::MANIFEST;

/// Where the generated source is committed, relative to this crate. The
/// regeneration test names the same file as a literal, since `include_str!`
/// takes no constant.
pub const GENERATED: &str = "../op-chart/src/advances.rs";

/// The first character the tables cover.
pub const FIRST: char = ' ';
/// The last character the tables cover.
pub const LAST: char = '~';
/// How many characters that is, and the length of every table.
pub const COUNT: usize = LAST as usize - FIRST as usize + 1;
/// The em, in the units the advances are written in: an advance of
/// [`PER_EM`] is one em wide.
pub const PER_EM: f64 = 1000.0;
/// Advances per row of the emitted table.
const PER_ROW: usize = 12;

/// Where a character's advance sits in a table.
pub fn index(c: char) -> usize {
    c as usize - FIRST as usize
}

/// A face the chart draws text with: the manifest entry to measure, the
/// name its table takes in the generated source, and what it draws.
pub struct Drawn {
    pub family: &'static str,
    pub weight: &'static str,
    pub style: &'static str,
    /// The name of the emitted table.
    pub ident: &'static str,
    /// Which of the chart's text this face sets, for the doc comment.
    pub text: &'static str,
}

/// Every face and weight `op-chart` draws text with.
///
/// The chart's svg sets `font-family: var(--op-font-sans)` at 12 px, which
/// resolves to IBM Plex Sans, and the stylesheet asks for bold in exactly
/// two places: the end labels and the playhead readout. Nothing the
/// renderer emits is italic, and nothing is set in the heading or the mono
/// family, so those faces are not measured.
pub const DRAWN: &[Drawn] = &[
    Drawn {
        family: "IBM Plex Sans",
        weight: "400",
        style: "normal",
        ident: "PLEX_SANS_400",
        text: "the axis labels, the tick labels, and the mark, chapter and band labels",
    },
    Drawn {
        family: "IBM Plex Sans",
        weight: "700",
        style: "normal",
        ident: "PLEX_SANS_700",
        text: "the series end labels and the playhead readout",
    },
];

/// The bytes of a face, decoded from the served woff2 to the TrueType
/// `ttf-parser` reads.
fn face_ttf(assets: &Path, drawn: &Drawn) -> Vec<u8> {
    let entry = MANIFEST
        .iter()
        .find(|e| (e.family, e.weight, e.style) == (drawn.family, drawn.weight, drawn.style))
        .unwrap_or_else(|| panic!("no manifest entry for {} {}", drawn.family, drawn.weight));
    let path = assets.join(entry.path);
    let woff2 =
        std::fs::read(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    woff2_patched::convert_woff2_to_ttf(&mut woff2.as_slice())
        .unwrap_or_else(|e| panic!("cannot decode {}: {e:?}", entry.path))
}

/// The advance of every covered character, in thousandths of the em, read
/// from the face's own `cmap` and `hmtx` tables. Kerning is not applied:
/// the chart sets no prose, and a per-character table is exact for the
/// digits and within a fraction of a pixel for the short words it draws.
pub fn advances(ttf: &[u8]) -> Vec<u16> {
    let face = ttf_parser::Face::parse(ttf, 0).expect("parse font");
    let upem = f64::from(face.units_per_em());
    (FIRST..=LAST)
        .map(|c| {
            let glyph = face
                .glyph_index(c)
                .unwrap_or_else(|| panic!("the face lacks {c:?}"));
            let advance = face
                .glyph_hor_advance(glyph)
                .unwrap_or_else(|| panic!("no advance for {c:?}"));
            let per_em = f64::from(advance) / upem * PER_EM;
            assert!(
                per_em <= f64::from(u16::MAX),
                "{c:?} advance {per_em} overflows"
            );
            per_em.round() as u16
        })
        .collect()
}

/// The face a table was measured from, as the generated source writes it:
/// a `Measured` beside the table it belongs to, so the element that draws
/// the text can ask a browser for the same face before it trusts the
/// widths (decision 14). Written vertically because the one-line form runs
/// past the width `rustfmt` allows, which would have it reformat a
/// generated file the regeneration test compares byte for byte.
fn face(drawn: &Drawn) -> String {
    format!(
        "/// The face [`{ident}`] was measured from.\n\
         pub static {ident}_FACE: Measured = Measured {{\n\
         \x20   family: {:?},\n\
         \x20   weight: {:?},\n\
         \x20   style: {:?},\n\
         }};\n",
        drawn.family,
        drawn.weight,
        drawn.style,
        ident = drawn.ident,
    )
}

/// One table as the generated source writes it: a `rustfmt`-skipped static
/// laid out [`PER_ROW`] advances to the row, each row ending in the
/// characters it measures so a reviewer can read a number against its own
/// glyph. The skip is what keeps `cargo fmt --all --check` and the
/// byte-for-byte regeneration test from pulling in opposite directions.
fn table(drawn: &Drawn, advances: &[u16]) -> String {
    let mut out = format!(
        "/// {} {}: {}.\n#[rustfmt::skip]\npub static {}: [u16; COUNT] = [\n",
        drawn.family, drawn.weight, drawn.text, drawn.ident
    );
    for (row, chunk) in advances.chunks(PER_ROW).enumerate() {
        let numbers: Vec<String> = chunk.iter().map(|a| format!("{a:4},")).collect();
        let from = FIRST as usize + row * PER_ROW;
        let legend: String = (0..chunk.len())
            .map(|k| char::from_u32((from + k) as u32).expect("ascii"))
            .collect();
        out.push_str(&format!("    {} // {legend}\n", numbers.join(" ")));
    }
    out.push_str("];\n");
    out
}

/// The whole generated source, ready to be written to [`GENERATED`].
pub fn generate(assets: &Path) -> String {
    let mut out = format!(
        "//! Advance widths of the faces the chart draws text with, at a unit em.\n\
         //!\n\
         //! Generated from the served woff2 in `crates/op-assets/assets` by\n\
         //! `cargo run -p op-assets --bin emit-advances`; do not edit. The\n\
         //! measurement, the character set and the layout live in\n\
         //! `crates/op-assets/src/advances.rs`, and a test there regenerates this\n\
         //! file and compares it byte for byte, so a font change that is not\n\
         //! regenerated fails the build rather than mismeasuring quietly.\n\
         //!\n\
         //! Each table is indexed by `c as usize - FIRST as usize` and holds the\n\
         //! character's advance in thousandths of the em, so the width of a\n\
         //! string at a given font size is the sum of its advances times the size\n\
         //! over [`PER_EM`]. The block is printable ASCII: the Latin letters in\n\
         //! both cases, the ten digits, the space and every ASCII punctuation\n\
         //! mark, which covers the numbers, units and short names the chart\n\
         //! writes and every label a Latin keyboard can put in one.\n\
         //!\n\
         //! Beside each table is the face it was measured from. A consumer that\n\
         //! draws this text asks a browser for that face, and where the browser\n\
         //! has not got it the widths here describe a face nothing will be set\n\
         //! in, so the drawing has to be corrected from a measurement of what is.\n\
         \n\
         /// The first character the tables cover.\n\
         pub const FIRST: char = {FIRST:?};\n\
         /// The last character the tables cover.\n\
         pub const LAST: char = {LAST:?};\n\
         /// How many characters that is, and the length of every table.\n\
         pub const COUNT: usize = LAST as usize - FIRST as usize + 1;\n\
         /// The em, in the units the advances are written in: an advance of\n\
         /// `PER_EM` is one em wide.\n\
         pub const PER_EM: f64 = {PER_EM:?};\n\
         \n\
         /// The face an advance table was measured from: the family, weight and\n\
         /// style a consumer has to ask a browser for if the widths beside it are\n\
         /// to be the widths it draws.\n\
         #[derive(Clone, Copy, Debug, PartialEq, Eq)]\n\
         pub struct Measured {{\n\
         \x20   /// The CSS family name, unquoted.\n\
         \x20   pub family: &'static str,\n\
         \x20   /// The CSS weight the face was measured at.\n\
         \x20   pub weight: &'static str,\n\
         \x20   /// The CSS style the face was measured at.\n\
         \x20   pub style: &'static str,\n\
         }}\n"
    );
    for drawn in DRAWN {
        out.push('\n');
        out.push_str(&face(drawn));
        out.push('\n');
        out.push_str(&table(drawn, &advances(&face_ttf(assets, drawn))));
    }
    out
}

/// The assets directory the faces are served from.
pub fn assets() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("assets")
}

/// Where [`generate`]'s output belongs.
pub fn generated_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(GENERATED)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The committed table is the faces, measured. Regenerating it from the
    /// woff2 the site serves must reproduce the file byte for byte, so a
    /// font swap, a subsetting change or a hand edit cannot leave `op-chart`
    /// measuring a face that is no longer shipped.
    #[test]
    fn the_committed_table_matches_the_faces() {
        let committed = include_str!("../../op-chart/src/advances.rs");
        let measured = generate(&assets());
        let stale =
            "the advance tables are stale: run `cargo run -p op-assets --bin emit-advances`";
        // line by line first: a whole-file comparison of two identical
        // three-kilobyte sources says only that they differ
        for (n, (a, b)) in measured.lines().zip(committed.lines()).enumerate() {
            assert_eq!(a, b, "{stale} (line {})", n + 1);
        }
        assert_eq!(
            measured.lines().count(),
            committed.lines().count(),
            "{stale}"
        );
        assert!(measured == committed, "{stale} (the line endings differ)");
    }

    /// Generation is a pure function of the faces: two runs, identical bytes.
    #[test]
    fn generation_is_deterministic() {
        assert_eq!(generate(&assets()), generate(&assets()));
    }

    /// What `text` measures when it is set in `ttf` at `px`, straight out
    /// of the face's own tables: the reference every claim about the
    /// committed advances is checked against.
    fn width_from(ttf: &[u8], text: &str, px: f64) -> f64 {
        let face = ttf_parser::Face::parse(ttf, 0).expect("parse font");
        let upem = f64::from(face.units_per_em());
        text.chars()
            .map(|c| {
                let glyph = face
                    .glyph_index(c)
                    .unwrap_or_else(|| panic!("the face lacks {c:?}"));
                let advance = face
                    .glyph_hor_advance(glyph)
                    .unwrap_or_else(|| panic!("no advance for {c:?}"));
                f64::from(advance) / upem * px
            })
            .sum()
    }

    /// Text the chart draws, in the shapes it draws it in: a gridline
    /// value, a time label, a clock reading, two axis names and three
    /// series names, one of them long.
    const DRAWS: &[&str] = &[
        "0",
        "100",
        "-12.5",
        "0.5s",
        "12s",
        "3.30s",
        "progress %",
        "% (opacity, left)",
        "thumb travel",
        "ghost opacity per cent",
        "first frame",
    ];

    /// What the tables are for, checked against what they were read from.
    /// `op-chart` measures a label by adding up the committed advances;
    /// here the same strings are measured a second time, straight out of
    /// the woff2 the site serves through the parser `op-chart` cannot
    /// have, and the two are asked for the same number. A table generated
    /// from one face and indexed as another, a face swapped without
    /// regeneration, or an off-by-one in the indexing shows up here as
    /// pixels.
    ///
    /// A tenth of a pixel at the size the chart is drawn: the tables are
    /// rounded to a thousandth of the em, which is six thousandths of a
    /// pixel a character at 12 px, so nothing this side of a fifteen-word
    /// label can reach the tolerance by rounding alone.
    #[test]
    fn the_renderers_measurement_matches_the_faces_it_was_read_from() {
        let mut widths: Vec<f64> = Vec::new();
        for drawn in DRAWN {
            let ttf = face_ttf(&assets(), drawn);
            let weight = match drawn.weight {
                "700" => op_chart::Face::Bold,
                _ => op_chart::Face::Regular,
            };
            for text in DRAWS {
                let from_the_face = width_from(&ttf, text, op_chart::TEXT_PX);
                let ours = op_chart::text_width(text, op_chart::TEXT_PX, weight);
                assert!(
                    (ours - from_the_face).abs() < 0.1,
                    "{} {} measures {text:?} at {ours} px, the face at {from_the_face}",
                    drawn.family,
                    drawn.weight
                );
                widths.push(ours);
            }
        }
        // the two faces are told apart, so a test that measured one of
        // them twice could not pass this
        let (regular, bold) = widths.split_at(DRAWS.len());
        assert!(
            regular.iter().zip(bold).any(|(a, b)| a != b),
            "both weights measured the same: {widths:?}"
        );
    }

    /// A string set in both faces, wide enough apart in the two that a
    /// measurement cannot mistake one for the other.
    const PROBE: &str = "first frame W";

    /// The face a consumer asks a browser for is the face the advances it
    /// measures with were read from (decision 14's third criterion).
    ///
    /// `op-chart` has no font and no parser: it can name a family and a
    /// weight only because the generator wrote them beside the advances,
    /// and [`the_committed_table_matches_the_faces`] holds what was
    /// written to [`DRAWN`] byte for byte. What that comparison cannot see
    /// is the other end of the join, which is made in `op-chart` and not
    /// in the generated source: whether the [`op_chart::Face`] a run of
    /// text is measured with reports the entry its own table came from.
    /// Which entry that is is settled here by the numbers rather than by a
    /// name: the probe is measured out of each served woff2 in turn, and
    /// the entry whose face agrees with what `op-chart` answers is the one
    /// those advances were read from. Swapping the two idents in [`DRAWN`],
    /// crossing the arms of the accessor's match, or adding a weight to
    /// one of the two lists and not the other all show up here.
    #[test]
    fn the_face_a_weight_is_asked_for_by_is_the_face_its_advances_were_read_from() {
        // as many faces to draw with as tables to draw from, so neither
        // list can gain a face the other has not got
        assert_eq!(op_chart::Face::ALL.len(), DRAWN.len());
        let mut claimed: Vec<&str> = Vec::new();
        for face in op_chart::Face::ALL {
            let ours = op_chart::text_width(PROBE, op_chart::TEXT_PX, face);
            let agrees: Vec<&Drawn> = DRAWN
                .iter()
                .filter(|drawn| {
                    let ttf = face_ttf(&assets(), drawn);
                    (ours - width_from(&ttf, PROBE, op_chart::TEXT_PX)).abs() < 0.1
                })
                .collect();
            let [drawn] = agrees[..] else {
                panic!(
                    "{face:?} measures {PROBE:?} at {ours} px, which {} of the served faces set it in",
                    agrees.len()
                );
            };
            let asked = face.measured();
            assert_eq!(
                (asked.family, asked.weight, asked.style),
                (drawn.family, drawn.weight, drawn.style),
                "{face:?} measures the advances of {} {} and asks a browser for {} {}",
                drawn.family,
                drawn.weight,
                asked.family,
                asked.weight
            );
            claimed.push(drawn.ident);
        }
        // and no two faces measure with one table, which the probe would
        // otherwise let pass if both arms of the accessor named it
        claimed.sort_unstable();
        let mut once = claimed.clone();
        once.dedup();
        assert_eq!(claimed, once, "two faces share one table: {claimed:?}");
    }

    /// Every covered character exists in every measured face and carries a
    /// positive advance, so no visible glyph measures as nothing; and the
    /// space, the one covered character that draws nothing, is present and
    /// positive too, because a zero there would collapse every gap between
    /// the words of a label.
    #[test]
    fn every_covered_character_has_a_positive_advance() {
        for drawn in DRAWN {
            let table = advances(&face_ttf(&assets(), drawn));
            assert_eq!(table.len(), COUNT, "{} {}", drawn.family, drawn.weight);
            for (k, advance) in table.iter().enumerate() {
                let c = char::from_u32((FIRST as usize + k) as u32).expect("ascii");
                assert!(
                    *advance > 0,
                    "{} {} {c:?} is zero",
                    drawn.family,
                    drawn.weight
                );
            }
            let space = table[index(' ')];
            assert!(space > 0, "{} {} space is zero", drawn.family, drawn.weight);
        }
    }
}
