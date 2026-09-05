//! Advance widths of the faces the chart draws text with, at a unit em.
//!
//! Generated from the served woff2 in `crates/op-assets/assets` by
//! `cargo run -p op-assets --bin emit-advances`; do not edit. The
//! measurement, the character set and the layout live in
//! `crates/op-assets/src/advances.rs`, and a test there regenerates this
//! file and compares it byte for byte, so a font change that is not
//! regenerated fails the build rather than mismeasuring quietly.
//!
//! Each table is indexed by `c as usize - FIRST as usize` and holds the
//! character's advance in thousandths of the em, so the width of a
//! string at a given font size is the sum of its advances times the size
//! over [`PER_EM`]. The block is printable ASCII: the Latin letters in
//! both cases, the ten digits, the space and every ASCII punctuation
//! mark, which covers the numbers, units and short names the chart
//! writes and every label a Latin keyboard can put in one.
//!
//! Beside each table is the face it was measured from. A consumer that
//! draws this text asks a browser for that face, and where the browser
//! has not got it the widths here describe a face nothing will be set
//! in, so the drawing has to be corrected from a measurement of what is.

/// The first character the tables cover.
pub const FIRST: char = ' ';
/// The last character the tables cover.
pub const LAST: char = '~';
/// How many characters that is, and the length of every table.
pub const COUNT: usize = LAST as usize - FIRST as usize + 1;
/// The em, in the units the advances are written in: an advance of
/// `PER_EM` is one em wide.
pub const PER_EM: f64 = 1000.0;

/// The face an advance table was measured from: the family, weight and
/// style a consumer has to ask a browser for if the widths beside it are
/// to be the widths it draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Measured {
    /// The CSS family name, unquoted.
    pub family: &'static str,
    /// The CSS weight the face was measured at.
    pub weight: &'static str,
    /// The CSS style the face was measured at.
    pub style: &'static str,
}

/// The face [`PLEX_SANS_400`] was measured from.
pub static PLEX_SANS_400_FACE: Measured = Measured {
    family: "IBM Plex Sans",
    weight: "400",
    style: "normal",
};

/// IBM Plex Sans 400: the axis labels, the tick labels, and the mark, chapter and band labels.
#[rustfmt::skip]
pub static PLEX_SANS_400: [u16; COUNT] = [
     236,  284,  419,  713,  598,  927,  694,  242,  335,  335,  450,  600, //  !"#$%&'()*+
     272,  399,  272,  383,  600,  600,  600,  600,  600,  600,  600,  600, // ,-./01234567
     600,  600,  292,  292,  600,  600,  600,  477,  891,  641,  653,  621, // 89:;<=>?@ABC
     671,  583,  559,  695,  707,  400,  510,  634,  501,  812,  707,  708, // DEFGHIJKLMNO
     606,  708,  640,  581,  572,  678,  609,  891,  613,  593,  580,  317, // PQRSTUVWXYZ[
     383,  317,  600,  565,  600,  534,  580,  503,  580,  549,  324,  528, // \]^_`abcdefg
     568,  250,  250,  527,  272,  873,  568,  560,  580,  580,  367,  487, // hijklmnopqrs
     351,  568,  492,  768,  507,  499,  464,  343,  314,  343,  600, // tuvwxyz{|}~
];

/// The face [`PLEX_SANS_700`] was measured from.
pub static PLEX_SANS_700_FACE: Measured = Measured {
    family: "IBM Plex Sans",
    weight: "700",
    style: "normal",
};

/// IBM Plex Sans 700: the series end labels and the playhead readout.
#[rustfmt::skip]
pub static PLEX_SANS_700: [u16; COUNT] = [
     236,  320,  493,  632,  601,  974,  721,  268,  338,  338,  601,  600, //  !"#$%&'()*+
     310,  403,  310,  460,  600,  600,  600,  600,  600,  600,  600,  600, // ,-./01234567
     600,  600,  330,  330,  600,  600,  600,  500,  903,  685,  667,  651, // 89:;<=>?@ABC
     697,  607,  585,  719,  724,  432,  559,  696,  530,  819,  724,  714, // DEFGHIJKLMNO
     656,  714,  674,  624,  584,  694,  650,  973,  673,  649,  607,  334, // PQRSTUVWXYZ[
     460,  334,  600,  556,  600,  569,  608,  517,  608,  562,  361,  552, // \]^_`abcdefg
     596,  286,  286,  577,  303,  894,  596,  564,  608,  608,  404,  504, // hijklmnopqrs
     383,  596,  538,  841,  560,  534,  518,  372,  402,  372,  600, // tuvwxyz{|}~
];
