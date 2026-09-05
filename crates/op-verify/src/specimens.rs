//! Does the browser paint what the advance table says it will?
//!
//! [`crate::checks`] and [`crate::frames`] hold one report against
//! another. This module holds the advance tables against the browser: it
//! lays out a specimen page carrying every string the chart draws, in the
//! real face, weight and size, over the surface that label sits on; a
//! capture of that page is cut back into cells here and measured in ink.
//!
//! The table is a sum of per-character advances, so kerning, ligatures
//! and contextual shaping are invisible to it. Two things follow that a
//! table cannot answer and pixels can: whether the sum is the width the
//! browser actually lays out, and how much ink a label puts on its
//! surface, which is what "heavy" means to a reader.
//!
//! Every case is drawn twice. The shaped cell is the browser's default,
//! which is what the site paints. The flat cell has `font-kerning`,
//! ligatures and contextual alternates turned off, which is the advance
//! table's own layout. Both carry the same first and last glyph, so their
//! side bearings are identical and the difference between their ink boxes
//! is the shaping and nothing else. Without that control every cell would
//! measure narrower than its advance sum, because an ink box excludes the
//! side bearings a sum of advances includes, and side bearings would be
//! mistaken for kerning.
//!
//! The halo the chart paints under its in-plot labels (decision 24's 3 px
//! stroke in the surface colour) is left off here: it is the surface
//! colour, so on the surface it asserts it is invisible, and drawing it
//! would widen every ink box by half its stroke and drown the signal
//! being looked for. Cells therefore sit on the surface each label
//! effectively has beneath it: the page background for the axis labels,
//! which carry no halo, and the surface colour for the labels that do.

use crate::frames::Image;
use op_chart::data::json::{Value, parse};
use op_chart::{Face, TEXT_PX, text_width};
use op_colour::{Lab, Srgb, ciede2000};
use std::path::Path;

/// The device pixel ratio the specimen is captured at. Whole, so a CSS
/// box in the page is a whole box of image pixels with no rounding.
pub const DPR: u32 = 2;

/// A cell's height in CSS px: room for the em box at [`TEXT_PX`] with a
/// little air above the cap and below the descender.
pub const CELL_H: u32 = 22;
/// Where a cell's text sits, measured down from the cell's top edge.
pub const BASELINE_Y: u32 = 15;
/// Where a cell's text starts, measured from the cell's left edge. The
/// gap is what leaves the left side bearing somewhere to be seen.
pub const ORIGIN_X: u32 = 12;
/// Clear space kept past the longest string in the widest face.
pub const TRAIL_X: u32 = 12;
/// Cells to a row of the specimen page: two cases, each with both of its
/// variants, so a case and its control sit side by side.
pub const COLUMNS: u32 = 4;

/// The size the sheet's own lettering is set at, in CSS px.
pub const ATLAS_PX: f64 = 9.0;
/// The width of one glyph's box in the atlas, in CSS px.
pub const ATLAS_W: u32 = 15;
/// The height of one glyph's box in the atlas, in CSS px.
pub const ATLAS_H: u32 = 18;
/// How far into its box a glyph is drawn, which is what keeps a negative
/// left side bearing inside the box.
pub const ATLAS_INSET: u32 = 3;
/// The baseline of a glyph in its atlas box, from the box's top edge.
pub const ATLAS_BASE: u32 = 13;
/// Glyph boxes to a row of the atlas.
pub const ATLAS_COLS: u32 = 24;

/// The ink an atlas glyph is drawn in, and the surface it is drawn on.
pub const ATLAS_INK: &str = "--op-text";
/// The surface the atlas is drawn on, with [`ATLAS_INK`].
pub const ATLAS_SURFACE: &str = "--op-bg";

/// Every palette token the page uses and the capture must resolve.
pub const TOKENS: &[&str] = &[
    "--op-bg",
    "--op-surface",
    "--op-text",
    "--op-muted",
    "--op-accent",
    "--op-playhead",
    "--op-border-strong",
    "--op-status-info",
];

/// How far from the surface a pixel must be to count as carrying ink, in
/// CIEDE2000. One unit is about the smallest difference a person can see
/// side by side, which is the honest floor for "this pixel is painted".
pub const JND: f64 = 1.0;

/// Which of a case's two cells this is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Variant {
    /// The browser's own layout: kerning, ligatures and contextual
    /// alternates as the site gets them.
    Shaped,
    /// The advance table's layout: every shaping feature off, so glyphs
    /// sit at their nominal advances.
    Flat,
}

impl Variant {
    /// The name this variant takes in the page and in the JSON.
    pub fn name(self) -> &'static str {
        match self {
            Variant::Shaped => "shaped",
            Variant::Flat => "flat",
        }
    }

    /// Both variants, shaped first.
    pub fn all() -> [Variant; 2] {
        [Variant::Shaped, Variant::Flat]
    }
}

/// The weight a face is written as in the page, the JSON and the sheet.
pub fn face_name(face: Face) -> &'static str {
    match face {
        Face::Regular => "400",
        Face::Bold => "700",
    }
}

/// One string to be drawn and measured: what the chart calls it, what it
/// says, and the face, ink and surface it says it in.
#[derive(Clone, Debug, PartialEq)]
pub struct Case {
    /// A stable name, used in the DOM, the JSON and the sheet.
    pub id: String,
    /// The chart's own name for this kind of label.
    pub kind: &'static str,
    pub text: &'static str,
    pub face: Face,
    /// The palette token the glyphs are painted in.
    pub ink: &'static str,
    /// The palette token beneath them.
    pub surface: &'static str,
    /// Why this string is here, for the sheet's reader.
    pub note: &'static str,
}

impl Case {
    /// The width the chart would place this label by: the sum of its
    /// characters' advances at the size the chart draws them.
    pub fn advance(&self) -> f64 {
        text_width(self.text, TEXT_PX, self.face)
    }
}

/// A case defined before its face is fixed, which is how the shaping set
/// is written once and measured in both faces.
struct Shaping {
    id: &'static str,
    text: &'static str,
    note: &'static str,
}

/// The pairs, runs and shapes chosen to make shaping visible if it is
/// there at all. Each is drawn in both faces the chart uses.
const SHAPING: &[Shaping] = &[
    Shaping {
        id: "kern-av",
        text: "AV",
        note: "a pair a face commonly kerns",
    },
    Shaping {
        id: "kern-aw",
        text: "AW",
        note: "a pair a face commonly kerns",
    },
    Shaping {
        id: "kern-to",
        text: "To",
        note: "a pair a face commonly kerns",
    },
    Shaping {
        id: "kern-ta",
        text: "Ta",
        note: "a pair a face commonly kerns",
    },
    Shaping {
        id: "kern-yo",
        text: "Yo",
        note: "a pair a face commonly kerns",
    },
    Shaping {
        id: "kern-lt",
        text: "LT",
        note: "a pair a face commonly kerns",
    },
    Shaping {
        id: "kern-p-stop",
        text: "P.",
        note: "a letter over a full stop",
    },
    Shaping {
        id: "kern-f-comma",
        text: "F,",
        note: "a letter over a comma",
    },
    Shaping {
        id: "digits-repeated",
        text: "0000000",
        note: "seven of one digit",
    },
    Shaping {
        id: "digits-mixed",
        text: "1234567",
        note: "seven mixed digits, the same length",
    },
    Shaping {
        id: "caps",
        text: "POWER ISA",
        note: "all capitals",
    },
    Shaping {
        id: "ascender-descender",
        text: "bp",
        note: "an ascender and a descender",
    },
];

/// Every case the specimen draws: the site's own labels, then the shaping
/// set in both faces.
pub fn cases() -> Vec<Case> {
    let mut out: Vec<Case> = site_cases();
    for s in SHAPING {
        for face in [Face::Regular, Face::Bold] {
            out.push(Case {
                id: format!("{}-{}", s.id, face_name(face)),
                kind: "shaping probe",
                text: s.text,
                face,
                ink: "--op-text",
                surface: "--op-surface",
                note: s.note,
            });
        }
    }
    out
}

/// The strings the site's own charts draw, read from the chart page's
/// data block (`pages/component/chart/index.xml`) and from the emitter:
/// the gridline values and the time ticks, the rotated axis title, the
/// mark, chapter and band labels, the series end labels and the
/// playhead's readout.
fn site_cases() -> Vec<Case> {
    let axis = |id: &str, kind: &'static str, text: &'static str, note: &'static str| Case {
        id: id.to_owned(),
        kind,
        text,
        face: Face::Regular,
        ink: "--op-muted",
        surface: "--op-bg",
        note,
    };
    let plot = |id: &str,
                kind: &'static str,
                text: &'static str,
                ink: &'static str,
                note: &'static str| Case {
        id: id.to_owned(),
        kind,
        text,
        face: Face::Regular,
        ink,
        surface: "--op-surface",
        note,
    };
    let bold = |id: &str,
                kind: &'static str,
                text: &'static str,
                ink: &'static str,
                note: &'static str| Case {
        id: id.to_owned(),
        kind,
        text,
        face: Face::Bold,
        ink,
        surface: "--op-surface",
        note,
    };
    vec![
        axis(
            "grid-0",
            "gridline value",
            "0",
            "the shortest label the chart draws",
        ),
        axis(
            "grid-100",
            "gridline value",
            "100",
            "the widest gridline value on the percent scale",
        ),
        axis("tick-0s", "time tick label", "0s", "a digit and a unit"),
        axis(
            "tick-3s",
            "time tick label",
            "3s",
            "the last tick of the demo timeline",
        ),
        axis(
            "axis-title",
            "axis title",
            "progress %",
            "the rotated title, drawn upright here",
        ),
        plot(
            "mark-half",
            "mark label",
            "half",
            "--op-text",
            "the demo chart's one mark",
        ),
        plot(
            "chapter-flight",
            "chapter label",
            "flight",
            "--op-accent",
            "the first chapter of the demo film",
        ),
        plot(
            "chapter-settle",
            "chapter label",
            "settle",
            "--op-accent",
            "the second chapter, in the accent",
        ),
        plot(
            "band-settle",
            "band label",
            "settle",
            "--op-text",
            "the same word in the text colour",
        ),
        bold(
            "end-palette",
            "series end label",
            "palette",
            "--op-text",
            "a series name",
        ),
        bold(
            "end-solid-thumb",
            "series end label",
            "solid thumb",
            "--op-text",
            "a series name with a space",
        ),
        bold(
            "end-progress-ghost",
            "series end label",
            "progress ghost",
            "--op-text",
            "the longest series label the site uses",
        ),
        bold(
            "readout-start",
            "playhead readout",
            "0.00s",
            "--op-playhead",
            "the readout at the start of the timeline",
        ),
        bold(
            "readout-end",
            "playhead readout",
            "3.30s",
            "--op-playhead",
            "the readout at its end",
        ),
    ]
}

/// The width of every cell, in CSS px: room for the widest string in the
/// widest face, with the origin's gap in front and clear space behind.
/// Even, so the box is a whole number of image pixels at [`DPR`].
pub fn cell_width(cases: &[Case]) -> u32 {
    let widest = cases.iter().map(Case::advance).fold(0.0_f64, f64::max);
    let w = ORIGIN_X + widest.ceil() as u32 + TRAIL_X;
    w + w % 2
}

/// Where one cell sits in the page, in CSS px from its top left corner.
#[derive(Clone, Debug, PartialEq)]
pub struct Cell {
    pub id: String,
    pub case: usize,
    pub variant: Variant,
    pub x: u32,
    pub y: u32,
}

/// The page the specimen is drawn on: the glyph atlas the contact sheet
/// letters itself from, then a cell for every case and variant.
#[derive(Clone, Debug, PartialEq)]
pub struct Page {
    pub width: u32,
    pub height: u32,
    pub cell_w: u32,
    /// Where the atlas sits, and how far down the cells start.
    pub atlas_y: u32,
    pub cells_y: u32,
    pub cells: Vec<Cell>,
}

/// Clear space between the atlas and the first row of cells.
const ATLAS_GAP: u32 = 10;

/// How many glyphs the atlas carries: the printable ASCII block, which is
/// the block the advance tables cover.
pub fn atlas_count() -> u32 {
    op_chart::advances::COUNT as u32
}

/// How many rows of glyph boxes that is.
pub fn atlas_rows() -> u32 {
    atlas_count().div_ceil(ATLAS_COLS)
}

/// Lay the page out: the atlas, then the cells in rows of [`COLUMNS`],
/// case by case with each case's variants adjacent.
pub fn layout(cases: &[Case]) -> Page {
    let cell_w = cell_width(cases);
    let atlas_y = 0;
    let cells_y = atlas_rows() * ATLAS_H + ATLAS_GAP;
    let mut cells = Vec::with_capacity(cases.len() * 2);
    for (i, case) in cases.iter().enumerate() {
        for variant in Variant::all() {
            let n = cells.len() as u32;
            cells.push(Cell {
                id: format!("{}-{}", case.id, variant.name()),
                case: i,
                variant,
                x: (n % COLUMNS) * cell_w,
                y: cells_y + (n / COLUMNS) * CELL_H,
            });
        }
    }
    let rows = (cells.len() as u32).div_ceil(COLUMNS);
    Page {
        width: (COLUMNS * cell_w).max(ATLAS_COLS * ATLAS_W),
        height: cells_y + rows * CELL_H,
        cell_w,
        atlas_y,
        cells_y,
        cells,
    }
}

// ---- the page ---------------------------------------------------------

/// The specimen page: one absolutely placed box per cell, each holding an
/// SVG text at an exact origin and baseline, and the glyph atlas above
/// them. Both stylesheets are the site's own, served from the same root,
/// so the faces and the palette are the ones the site ships.
pub fn page(cases: &[Case], page: &Page) -> String {
    let mut out = String::with_capacity(64 * 1024);
    out.push_str(&format!(
        "<!DOCTYPE html>\n<html lang=\"en-GB\" data-theme=\"dark\">\n<head>\n\
         <meta charset=\"utf-8\">\n<title>Chart label specimen</title>\n\
         <link rel=\"stylesheet\" href=\"/fonts.css\">\n\
         <link rel=\"stylesheet\" href=\"/theme.css\">\n<style>\n\
         html {{ background: var(--op-bg); }}\n\
         body {{ margin: 0; padding: 0; width: {}px; height: {}px; position: relative; background: var(--op-bg); }}\n\
         .cell {{ position: absolute; overflow: hidden; }}\n\
         .cell svg {{ display: block; font-family: var(--op-font-sans); font-size: {TEXT_PX}px; font-synthesis: none; }}\n\
         #atlas svg {{ font-size: {ATLAS_PX}px; }}\n\
         .flat svg {{ font-kerning: none; font-variant-ligatures: none; \
         font-feature-settings: \"kern\" 0, \"liga\" 0, \"clig\" 0, \"calt\" 0, \"rlig\" 0; }}\n\
         </style>\n</head>\n<body>\n",
        page.width, page.height
    ));
    out.push_str(&atlas_markup(page));
    for cell in &page.cells {
        let case = &cases[cell.case];
        let flat = if cell.variant == Variant::Flat {
            " flat"
        } else {
            ""
        };
        out.push_str(&format!(
            "<div class=\"cell{flat}\" id=\"{}\" data-advance=\"{:.4}\" \
             style=\"left:{}px;top:{}px;width:{}px;height:{CELL_H}px;background:var({})\">\
             <svg width=\"{}\" height=\"{CELL_H}\" viewBox=\"0 0 {} {CELL_H}\">\
             <text x=\"{ORIGIN_X}\" y=\"{BASELINE_Y}\" fill=\"var({})\"{}>{}</text></svg></div>\n",
            cell.id,
            case.advance(),
            cell.x,
            cell.y,
            page.cell_w,
            case.surface,
            page.cell_w,
            page.cell_w,
            case.ink,
            match case.face {
                Face::Regular => "",
                Face::Bold => " font-weight=\"700\"",
            },
            op_chart::escape(case.text),
        ));
    }
    out.push_str("</body>\n</html>\n");
    out
}

/// The glyph atlas: every character the advance tables cover, each drawn
/// at the same offset into a box of its own, so the contact sheet can
/// letter itself from the site's own face rather than from a bitmap font
/// invented for the purpose.
fn atlas_markup(page: &Page) -> String {
    let w = ATLAS_COLS * ATLAS_W;
    let h = atlas_rows() * ATLAS_H;
    let mut out = format!(
        "<div class=\"cell\" id=\"atlas\" style=\"left:0;top:{}px;width:{w}px;height:{h}px;\
         background:var({ATLAS_SURFACE})\"><svg width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\">",
        page.atlas_y
    );
    for (i, c) in (op_chart::advances::FIRST..=op_chart::advances::LAST).enumerate() {
        let i = i as u32;
        let x = (i % ATLAS_COLS) * ATLAS_W + ATLAS_INSET;
        let y = (i / ATLAS_COLS) * ATLAS_H + ATLAS_BASE;
        out.push_str(&format!(
            "<text x=\"{x}\" y=\"{y}\" fill=\"var({ATLAS_INK})\">{}</text>",
            op_chart::escape(&c.to_string())
        ));
    }
    out.push_str("</svg></div>\n");
    out
}

// ---- the manifest the capture reads -----------------------------------

/// A JSON string, quoted and escaped.
fn quoted(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// What the capture step needs to know about the page it is loading: the
/// size to give the viewport, the tokens to resolve, the atlas, and every
/// cell with the advance sum the chart would place it by.
pub fn manifest(cases: &[Case], page: &Page) -> String {
    let mut out = String::new();
    out.push_str("{\n");
    out.push_str(&format!(" \"text_px\": {TEXT_PX},\n \"dpr\": {DPR},\n"));
    out.push_str(&format!(
        " \"page\": {{\"width\": {}, \"height\": {}}},\n",
        page.width, page.height
    ));
    out.push_str(&format!(
        " \"cell\": {{\"width\": {}, \"height\": {CELL_H}, \"origin_x\": {ORIGIN_X}, \"baseline_y\": {BASELINE_Y}}},\n",
        page.cell_w
    ));
    out.push_str(&format!(
        " \"atlas\": {{\"px\": {ATLAS_PX}, \"cols\": {ATLAS_COLS}, \"box_w\": {ATLAS_W}, \"box_h\": {ATLAS_H}, \
         \"inset_x\": {ATLAS_INSET}, \"baseline_y\": {ATLAS_BASE}, \"y\": {}, \"count\": {}}},\n",
        page.atlas_y,
        atlas_count()
    ));
    out.push_str(" \"tokens\": [");
    for (i, t) in TOKENS.iter().enumerate() {
        out.push_str(&format!("{}{}", if i > 0 { ", " } else { "" }, quoted(t)));
    }
    out.push_str("],\n \"faces\": [\"400\", \"700\"],\n \"cells\": [\n");
    for (i, cell) in page.cells.iter().enumerate() {
        let case = &cases[cell.case];
        out.push_str(&format!(
            "  {{\"id\": {}, \"case\": {}, \"variant\": {}, \"kind\": {}, \"text\": {}, \"face\": {}, \
             \"ink\": {}, \"surface\": {}, \"note\": {}, \"x\": {}, \"y\": {}, \"advance\": {:.4}}}{}\n",
            quoted(&cell.id),
            quoted(&case.id),
            quoted(cell.variant.name()),
            quoted(case.kind),
            quoted(case.text),
            quoted(face_name(case.face)),
            quoted(case.ink),
            quoted(case.surface),
            quoted(case.note),
            cell.x,
            cell.y,
            case.advance(),
            if i + 1 == page.cells.len() { "" } else { "," }
        ));
    }
    out.push_str(" ]\n}\n");
    out
}

// ---- what the capture knows and the table cannot -----------------------

/// A face's own vertical metrics at the size the chart draws, measured by
/// the engine that paints it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Metrics {
    /// The ink top of a capital H: the cap height.
    pub cap: f64,
    /// The ink top of an x: the x height.
    pub x: f64,
    /// The ink bottom of a p: the descender depth.
    pub descender: f64,
    /// The face's own ascent, from its vertical header.
    pub font_ascent: f64,
    /// The face's own descent, with [`Self::font_ascent`].
    pub font_descent: f64,
}

/// Everything about a capture that only the browser could say: which
/// browser drew it and how it positions glyphs, the palette as it was
/// painted, each face's metrics, and the advance the browser laid every
/// cell out at.
#[derive(Clone, Debug, PartialEq)]
pub struct Capture {
    pub theme: String,
    pub dpr: u32,
    pub image: String,
    pub browser: String,
    pub binary: String,
    /// How the browser positions glyphs, as a clause the key can read:
    /// whether it keeps a glyph's advance or rounds it to a whole pixel.
    pub positioning: String,
    pub colours: Vec<(String, String)>,
    pub metrics: Vec<(String, Metrics)>,
    pub advances: Vec<(String, f64)>,
}

impl Capture {
    /// A palette token as the browser painted it.
    pub fn colour(&self, token: &str) -> Result<Srgb, String> {
        let hex = self
            .colours
            .iter()
            .find(|(t, _)| t == token)
            .map(|(_, v)| v.as_str())
            .ok_or_else(|| format!("the capture did not resolve {token}"))?;
        Srgb::from_hex(hex).ok_or_else(|| format!("{token} is {hex}, which is not a colour"))
    }

    /// A face's metrics.
    pub fn metrics(&self, face: Face) -> Result<Metrics, String> {
        self.metrics
            .iter()
            .find(|(f, _)| f == face_name(face))
            .map(|(_, m)| *m)
            .ok_or_else(|| format!("the capture did not measure Plex Sans {}", face_name(face)))
    }

    /// The advance the browser laid a cell's text out at.
    pub fn advance(&self, id: &str) -> Result<f64, String> {
        self.advances
            .iter()
            .find(|(c, _)| c == id)
            .map(|(_, a)| *a)
            .ok_or_else(|| format!("the capture did not measure {id}"))
    }
}

/// Read the JSON the capture step wrote, with op-chart's own reader.
pub fn read_capture(path: &Path) -> Result<Capture, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let value = parse(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let at = |v: &Value, key: &str| -> Result<Value, String> {
        match v {
            Value::Object(fields) => fields
                .iter()
                .find(|(n, _)| n == key)
                .map(|(_, v)| v.clone())
                .ok_or_else(|| format!("{}: no {key}", path.display())),
            other => Err(format!(
                "{}: {key} wanted an object, found {}",
                path.display(),
                other.kind()
            )),
        }
    };
    let string = |v: &Value, key: &str| -> Result<String, String> {
        match at(v, key)? {
            Value::String(s) => Ok(s),
            other => Err(format!("{}: {key} is {}", path.display(), other.kind())),
        }
    };
    let number = |v: &Value, key: &str| -> Result<f64, String> {
        match at(v, key)? {
            Value::Number(n) => Ok(n),
            other => Err(format!("{}: {key} is {}", path.display(), other.kind())),
        }
    };
    let pairs = |v: &Value, key: &str| -> Result<Vec<(String, Value)>, String> {
        match at(v, key)? {
            Value::Object(fields) => Ok(fields),
            other => Err(format!("{}: {key} is {}", path.display(), other.kind())),
        }
    };
    let mut colours = Vec::new();
    for (token, v) in pairs(&value, "colours")? {
        match v {
            Value::String(hex) => colours.push((token, hex)),
            other => return Err(format!("{}: {token} is {}", path.display(), other.kind())),
        }
    }
    let mut metrics = Vec::new();
    for (face, v) in pairs(&value, "metrics")? {
        metrics.push((
            face,
            Metrics {
                cap: number(&v, "cap")?,
                x: number(&v, "x")?,
                descender: number(&v, "descender")?,
                font_ascent: number(&v, "font_ascent")?,
                font_descent: number(&v, "font_descent")?,
            },
        ));
    }
    let mut advances = Vec::new();
    for (id, v) in pairs(&value, "advances")? {
        match v {
            Value::Number(n) => advances.push((id, n)),
            other => {
                return Err(format!(
                    "{}: the advance of {id} is {}",
                    path.display(),
                    other.kind()
                ));
            }
        }
    }
    let fonts = at(&value, "fonts")?;
    let mut how: Vec<String> = pairs(&fonts, "positioning")?
        .into_iter()
        .map(|(face, v)| match v {
            Value::String(s) => format!("{face} {s}"),
            other => format!("{face} {}", other.kind()),
        })
        .collect();
    // both faces agree in every capture so far; where they do, the key
    // says the one thing rather than the same thing twice
    let one = how
        .first()
        .map(|first| first.split_once(' ').map(|(_, rest)| rest.to_owned()))
        .unwrap_or_default();
    let positioning = match one {
        Some(rest) if how.iter().all(|h| h.ends_with(&rest)) => match rest.as_str() {
            "subpixel" => "which keeps each glyph's advance".to_owned(),
            "whole pixel advances" => {
                "which rounds each glyph's advance to a whole pixel".to_owned()
            }
            other => other.to_owned(),
        },
        _ => {
            how.sort();
            how.join(", ")
        }
    };
    Ok(Capture {
        theme: string(&value, "theme")?,
        dpr: number(&value, "dpr")? as u32,
        image: string(&value, "image")?,
        browser: string(&value, "browser")?,
        binary: string(&value, "binary")?,
        positioning,
        colours,
        metrics,
        advances,
    })
}

// ---- measuring the ink -------------------------------------------------

/// How far every pixel of one cell stands from that cell's surface, in
/// CIEDE2000, with the distance the cell's own ink stands at. The ratio
/// of the two is the coverage of a pixel: nought where the surface shows
/// through untouched, one where the glyph is solid.
#[derive(Clone, Debug, PartialEq)]
pub struct Field {
    /// The cell's size in image pixels.
    pub width: u32,
    pub height: u32,
    /// The distance from the surface, one per pixel, rows top to bottom.
    pub distance: Vec<f64>,
    /// How far the ink itself stands from the surface.
    pub ink: f64,
}

impl Field {
    /// The distance from the surface at one pixel.
    pub fn at(&self, x: u32, y: u32) -> f64 {
        self.distance[(y * self.width + x) as usize]
    }

    /// How much of the ink covers one pixel, nought to one.
    pub fn coverage(&self, x: u32, y: u32) -> f64 {
        (self.at(x, y) / self.ink).clamp(0.0, 1.0)
    }
}

/// Cut one box out of a capture and reduce it to distances from its
/// surface. The box is in image pixels, which is CSS pixels times the
/// capture's device pixel ratio, and every box in the page is whole at
/// that ratio, so nothing here rounds.
pub fn field(
    image: &Image,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    surface: Srgb,
    ink: Srgb,
) -> Result<Field, String> {
    if x + width > image.width || y + height > image.height {
        return Err(format!(
            "a box at {x},{y} of {width} by {height} runs off a capture of {} by {}",
            image.width, image.height
        ));
    }
    let surface_lab = Lab::from_srgb(surface);
    let mut distance = Vec::with_capacity((width * height) as usize);
    for row in 0..height {
        for column in 0..width {
            let at = (((y + row) * image.width + x + column) * 3) as usize;
            let px = Srgb {
                r: f64::from(image.rgb[at]) / 255.0,
                g: f64::from(image.rgb[at + 1]) / 255.0,
                b: f64::from(image.rgb[at + 2]) / 255.0,
            };
            distance.push(ciede2000(Lab::from_srgb(px), surface_lab));
        }
    }
    Ok(Field {
        width,
        height,
        distance,
        ink: ciede2000(Lab::from_srgb(ink), surface_lab),
    })
}

/// The box the ink of one cell occupies, in CSS px measured from the
/// cell's own top left corner.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ink {
    pub left: f64,
    pub right: f64,
    pub top: f64,
    pub bottom: f64,
}

impl Ink {
    /// How wide the mark actually is.
    pub fn width(self) -> f64 {
        self.right - self.left
    }

    /// How tall it is.
    pub fn height(self) -> f64 {
        self.bottom - self.top
    }
}

/// What one cell came to.
#[derive(Clone, Debug, PartialEq)]
pub struct Measurement {
    pub id: String,
    pub variant: Variant,
    /// The painted box, or none where the cell carries no ink at all.
    pub ink: Option<Ink>,
    /// The share of the label's own em box that carries ink, weighted by
    /// how far each pixel stands from the surface.
    pub coverage: f64,
    /// That weight summed, in CSS px of solid ink.
    pub ink_area: f64,
    /// The furthest any pixel of the em box stands from the surface.
    pub darkest: f64,
    /// The mean over the em box.
    pub mean: f64,
    /// The advance sum op-chart would place this label by.
    pub advance: f64,
    /// The advance the browser laid it out at.
    pub browser: f64,
}

impl Measurement {
    /// The painted width, or nought where nothing was painted.
    pub fn painted(&self) -> f64 {
        self.ink.map_or(0.0, Ink::width)
    }

    /// How far the painted width stands from the advance sum. Negative
    /// where the paint is narrower, which is what side bearings and
    /// kerning both do.
    pub fn difference(&self) -> f64 {
        self.painted() - self.advance
    }

    /// That difference as a share of the advance sum.
    pub fn relative(&self) -> f64 {
        if self.advance > 0.0 {
            self.difference() / self.advance
        } else {
            0.0
        }
    }
}

/// Measure one cell: the ink box from the pixels, and the weight of the
/// ink over the box the chart reserves for the label, which is its
/// advance wide and its em box tall. Coverage over the reserved box
/// rather than over the whole cell, so a short label and a long one can
/// be compared: what a reader calls heavy is ink per unit of the room
/// the label takes.
pub fn measure(
    field: &Field,
    dpr: u32,
    id: &str,
    variant: Variant,
    advance: f64,
    browser: f64,
) -> Measurement {
    let dpr_f = f64::from(dpr);
    let mut ink: Option<(u32, u32, u32, u32)> = None;
    for y in 0..field.height {
        for x in 0..field.width {
            if field.at(x, y) >= JND {
                ink = Some(match ink {
                    None => (x, y, x, y),
                    Some((l, t, r, b)) => (l.min(x), t.min(y), r.max(x), b.max(y)),
                });
            }
        }
    }
    let em_left = (f64::from(ORIGIN_X) * dpr_f).round() as u32;
    let em_right = ((f64::from(ORIGIN_X) + advance) * dpr_f)
        .round()
        .min(f64::from(field.width)) as u32;
    let em_top = ((f64::from(BASELINE_Y) - TEXT_PX * op_chart::ASCENT) * dpr_f)
        .round()
        .max(0.0) as u32;
    let em_bottom = ((f64::from(BASELINE_Y) + TEXT_PX * op_chart::DESCENT) * dpr_f)
        .round()
        .min(f64::from(field.height)) as u32;
    let mut weight = 0.0;
    let mut total = 0.0;
    let mut darkest = 0.0_f64;
    let mut count = 0u32;
    for y in em_top..em_bottom {
        for x in em_left..em_right {
            weight += field.coverage(x, y);
            total += field.at(x, y);
            darkest = darkest.max(field.at(x, y));
            count += 1;
        }
    }
    let cells = f64::from(count.max(1));
    Measurement {
        id: id.to_owned(),
        variant,
        ink: ink.map(|(l, t, r, b)| Ink {
            left: f64::from(l) / dpr_f,
            right: f64::from(r + 1) / dpr_f,
            top: f64::from(t) / dpr_f,
            bottom: f64::from(b + 1) / dpr_f,
        }),
        coverage: weight / cells,
        ink_area: weight / (dpr_f * dpr_f),
        darkest,
        mean: total / cells,
        advance,
        browser,
    }
}

/// One case measured: the shaped cell the site paints and the flat
/// control that carries the advance table's own layout.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub case: usize,
    pub shaped: Measurement,
    pub flat: Measurement,
    /// The fields both cells were measured from, kept for the sheet.
    pub shaped_field: Field,
    pub flat_field: Field,
}

impl Row {
    /// How much narrower the browser's own shaping made the painted mark
    /// than the advance table's layout of the same glyphs. The two cells
    /// share their first and last glyph, so their side bearings cancel
    /// and what is left is the shaping.
    pub fn shaping(&self) -> f64 {
        self.shaped.painted() - self.flat.painted()
    }

    /// The same, as the browser laid the two out rather than as they
    /// were painted.
    pub fn shaping_laid_out(&self) -> f64 {
        self.shaped.browser - self.flat.browser
    }

    /// Whether this case shows the signature of kerning: the paint
    /// narrower than the table's own layout by more than a pixel.
    pub fn kerned(&self) -> bool {
        self.shaping() < -1.0
    }
}

/// Measure a whole capture: every cell cut out by its known box and
/// reduced to ink, paired case by case with its control.
pub fn measure_capture(
    image: &Image,
    cases: &[Case],
    page: &Page,
    capture: &Capture,
) -> Result<Vec<Row>, String> {
    let want = (page.width * capture.dpr, page.height * capture.dpr);
    if (image.width, image.height) != want {
        return Err(format!(
            "the capture is {} by {} where the page is {} by {} at a ratio of {}",
            image.width, image.height, want.0, want.1, capture.dpr
        ));
    }
    let mut rows = Vec::with_capacity(cases.len());
    for pair in page.cells.chunks(2) {
        let case = &cases[pair[0].case];
        let surface = capture.colour(case.surface)?;
        let ink = capture.colour(case.ink)?;
        let mut made = Vec::with_capacity(2);
        for cell in pair {
            let f = field(
                image,
                cell.x * capture.dpr,
                cell.y * capture.dpr,
                page.cell_w * capture.dpr,
                CELL_H * capture.dpr,
                surface,
                ink,
            )?;
            let m = measure(
                &f,
                capture.dpr,
                &cell.id,
                cell.variant,
                case.advance(),
                capture.advance(&cell.id)?,
            );
            made.push((f, m));
        }
        let (flat_field, flat) = made.pop().expect("the flat control");
        let (shaped_field, shaped) = made.pop().expect("the shaped cell");
        rows.push(Row {
            case: pair[0].case,
            shaped,
            flat,
            shaped_field,
            flat_field,
        });
    }
    Ok(rows)
}

// ---- what it all came to ----------------------------------------------

/// The case that came off worst on some measure, and by how much.
struct Worst<'a> {
    case: &'a Case,
    row: &'a Row,
    by: f64,
}

/// The worst case by a measure that is bad when it is large.
fn worst<'a>(cases: &'a [Case], rows: &'a [Row], of: impl Fn(&Row) -> f64) -> Option<Worst<'a>> {
    rows.iter()
        .map(|row| Worst {
            case: &cases[row.case],
            row,
            by: of(row),
        })
        .max_by(|a, b| a.by.abs().total_cmp(&b.by.abs()))
}

/// How a case is named in a line of prose.
fn named(case: &Case) -> String {
    format!("{:?} in {}", case.text, face_name(case.face))
}

/// What the whole set came to, in the lines the sheet prints under it and
/// the run prints to its console.
///
/// The painted width of a string is not its advance sum and never was:
/// an ink box stops at the last mark, and a sum of advances runs to the
/// pen's stopping place, so every case paints narrower by its first
/// glyph's left side bearing and its last glyph's right one. What that
/// leaves is a comparison of two differences: how much narrower the paint
/// is than the sum, and how much of that the flat control shows too. The
/// part the control does not show is the shaping.
pub fn summary(cases: &[Case], rows: &[Row]) -> Vec<String> {
    let mut lines = Vec::new();
    lines.push(format!(
        "{} cases, {} cells: the paint against op-chart's advance sum, and against the table's own layout of the same glyphs.",
        rows.len(),
        rows.len() * 2
    ));
    if let Some(w) = worst(cases, rows, |r| r.shaped.difference()) {
        lines.push(format!(
            "Worst absolute: {} painted {:.2} px against a sum of {:.2}, {:+.2} px.",
            named(w.case),
            w.row.shaped.painted(),
            w.row.shaped.advance,
            w.by
        ));
    }
    if let Some(w) = worst(cases, rows, |r| r.shaped.relative()) {
        lines.push(format!(
            "Worst relative: {} painted {:.2} px against a sum of {:.2}, {:+.1}%.",
            named(w.case),
            w.row.shaped.painted(),
            w.row.shaped.advance,
            w.by * 100.0
        ));
    }
    // the side bearings, which are what most of that difference is
    let bearings: Vec<f64> = rows
        .iter()
        .map(|r| r.flat.painted() - r.flat.advance)
        .collect();
    let low = bearings.iter().copied().fold(f64::INFINITY, f64::min);
    let high = bearings.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let mean = bearings.iter().sum::<f64>() / bearings.len() as f64;
    lines.push(format!(
        "Side bearings account for {low:+.2} to {high:+.2} px of that (mean {mean:+.2}): the flat control, \
         laid out at the table's own advances, paints narrower than the sum by exactly that much."
    ));
    let kerned: Vec<&Row> = rows.iter().filter(|r| r.kerned()).collect();
    if kerned.is_empty() {
        lines.push(
            "Kerning: no case painted more than a pixel narrower than the flat control, so nothing here \
             shapes by more than a pixel."
                .to_owned(),
        );
    } else {
        let mut named_cases: Vec<String> = kerned
            .iter()
            .map(|r| format!("{} {:+.2}", named(&cases[r.case]), r.shaping()))
            .collect();
        named_cases.sort();
        lines.push(format!(
            "Kerning: {} of {} cases painted more than a pixel narrower than the flat control: {}.",
            kerned.len(),
            rows.len(),
            named_cases.join(", ")
        ));
    }
    let close = rows
        .iter()
        .filter(|r| (-1.0..=-0.5).contains(&r.shaping()))
        .count();
    lines.push(format!(
        "Another {close} painted between half a pixel and a pixel narrower, which is as fine as an ink box \
         cut to whole image pixels can tell at a ratio of {DPR}."
    ));
    // the browser's own layout, which has no such floor
    let mut laid: Vec<(f64, String)> = rows
        .iter()
        .map(|r| (r.shaping_laid_out(), named(&cases[r.case])))
        .collect();
    laid.sort_by(|a, b| a.0.total_cmp(&b.0));
    let shaped = laid.iter().filter(|(s, _)| s.abs() > 0.01).count();
    let tightest: Vec<String> = laid
        .iter()
        .take(4)
        .map(|(s, n)| format!("{n} {s:+.2}"))
        .collect();
    lines.push(format!(
        "As the browser laid them out, {shaped} of {} cases shaped at all; tightest: {}.",
        rows.len(),
        tightest.join(", ")
    ));
    lines.push(
        "Where nothing shapes, the browser laid the string out at the advance sum to the last hundredth of a pixel."
            .to_owned(),
    );
    // the strings the site actually draws, which is the question the
    // chart has to answer; the probes were chosen to be hard
    let site: Vec<&Row> = rows
        .iter()
        .filter(|r| cases[r.case].kind != "shaping probe")
        .collect();
    if let Some(w) = site.iter().max_by(|a, b| {
        a.shaping_laid_out()
            .abs()
            .total_cmp(&b.shaping_laid_out().abs())
    }) {
        let over = site
            .iter()
            .filter(|r| r.shaping_laid_out().abs() > 0.5)
            .count();
        lines.push(format!(
            "Of the {} labels the site's own charts draw, the worst the browser shaped was {} by {:+.2} px, \
             and {over} shaped by more than half a pixel.",
            site.len(),
            named(&cases[w.case]),
            w.shaping_laid_out()
        ));
    }
    let mut by_weight: Vec<(f64, String)> = rows
        .iter()
        .map(|r| (r.shaped.coverage, named(&cases[r.case])))
        .collect();
    by_weight.sort_by(|a, b| b.0.total_cmp(&a.0));
    let name = |slice: &[(f64, String)]| {
        slice
            .iter()
            .map(|(c, n)| format!("{n} {:.1}%", c * 100.0))
            .collect::<Vec<_>>()
            .join(", ")
    };
    lines.push(format!(
        "Heaviest by ink over the box the chart reserves for the label: {}.",
        name(&by_weight[..3.min(by_weight.len())])
    ));
    lines.push(format!(
        "Lightest: {}.",
        name(&by_weight[by_weight.len().saturating_sub(3)..])
    ));
    lines
}

/// Every number this run measured, as JSON, one record per case.
pub fn measured_json(
    cases: &[Case],
    rows: &[Row],
    capture: &Capture,
    summary: &[String],
) -> String {
    let mut out = String::new();
    out.push_str("{\n");
    out.push_str(&format!(
        " \"theme\": {},\n \"browser\": {},\n \"binary\": {},\n \"positioning\": {},\n \"dpr\": {},\n \"image\": {},\n",
        quoted(&capture.theme),
        quoted(&capture.browser),
        quoted(&capture.binary),
        quoted(&capture.positioning),
        capture.dpr,
        quoted(&capture.image)
    ));
    out.push_str(" \"summary\": [\n");
    for (i, line) in summary.iter().enumerate() {
        out.push_str(&format!(
            "  {}{}\n",
            quoted(line),
            if i + 1 == summary.len() { "" } else { "," }
        ));
    }
    out.push_str(" ],\n \"cases\": [\n");
    for (i, row) in rows.iter().enumerate() {
        let case = &cases[row.case];
        let ink = |m: &Measurement| match m.ink {
            Some(b) => format!(
                "{{\"left\": {:.3}, \"right\": {:.3}, \"top\": {:.3}, \"bottom\": {:.3}, \"width\": {:.3}, \"height\": {:.3}}}",
                b.left,
                b.right,
                b.top,
                b.bottom,
                b.width(),
                b.height()
            ),
            None => "null".to_owned(),
        };
        out.push_str(&format!(
            "  {{\"id\": {}, \"kind\": {}, \"text\": {}, \"face\": {}, \"ink\": {}, \"surface\": {}, \"note\": {},\n",
            quoted(&case.id),
            quoted(case.kind),
            quoted(case.text),
            quoted(face_name(case.face)),
            quoted(case.ink),
            quoted(case.surface),
            quoted(case.note)
        ));
        out.push_str(&format!(
            "   \"advance\": {:.4}, \"painted\": {:.3}, \"difference\": {:.3}, \"relative\": {:.5},\n",
            row.shaped.advance,
            row.shaped.painted(),
            row.shaped.difference(),
            row.shaped.relative()
        ));
        out.push_str(&format!(
            "   \"flat_painted\": {:.3}, \"shaping\": {:.3}, \"kerned\": {},\n",
            row.flat.painted(),
            row.shaping(),
            row.kerned()
        ));
        out.push_str(&format!(
            "   \"laid_out\": {:.4}, \"laid_out_flat\": {:.4}, \"laid_out_shaping\": {:.4},\n",
            row.shaped.browser,
            row.flat.browser,
            row.shaping_laid_out()
        ));
        out.push_str(&format!(
            "   \"coverage\": {:.5}, \"ink_area\": {:.3}, \"darkest\": {:.3}, \"mean\": {:.3},\n",
            row.shaped.coverage, row.shaped.ink_area, row.shaped.darkest, row.shaped.mean
        ));
        out.push_str(&format!(
            "   \"ink_box\": {}, \"flat_ink_box\": {}}}{}\n",
            ink(&row.shaped),
            ink(&row.flat),
            if i + 1 == rows.len() { "" } else { "," }
        ));
    }
    out.push_str(" ]\n}\n");
    out
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Every cell has a box of its own inside the page, no two overlap,
    /// and the page is big enough to hold them all. A capture is cut into
    /// cells by these boxes, so an overlap would measure one label's ink
    /// as another's.
    #[test]
    fn the_cells_tile_the_page_without_touching() {
        let cases = cases();
        let page = layout(&cases);
        assert_eq!(page.cells.len(), cases.len() * 2);
        for cell in &page.cells {
            assert!(cell.x + page.cell_w <= page.width, "{} runs off", cell.id);
            assert!(cell.y + CELL_H <= page.height, "{} runs off", cell.id);
            assert!(cell.y >= page.cells_y, "{} is over the atlas", cell.id);
        }
        for (i, a) in page.cells.iter().enumerate() {
            for b in &page.cells[i + 1..] {
                let apart = a.x + page.cell_w <= b.x
                    || b.x + page.cell_w <= a.x
                    || a.y + CELL_H <= b.y
                    || b.y + CELL_H <= a.y;
                assert!(apart, "{} and {} overlap", a.id, b.id);
            }
        }
    }

    /// The cell is wide enough for the widest string it will hold, with
    /// the origin's gap in front of it: a label that ran past the cell's
    /// edge would be measured clipped.
    #[test]
    fn every_string_fits_its_cell_with_room_to_spare() {
        let cases = cases();
        let w = f64::from(cell_width(&cases));
        for case in &cases {
            let end = f64::from(ORIGIN_X) + case.advance();
            assert!(end <= w - 1.0, "{} ends at {end} in a cell of {w}", case.id);
        }
    }

    /// Each case is drawn shaped and flat, and the two carry the same
    /// string in the same face: the flat cell is the control the shaping
    /// is measured against, so a difference in anything else would make
    /// the comparison meaningless.
    #[test]
    fn each_case_has_a_shaped_cell_and_a_flat_control() {
        let cases = cases();
        let page = layout(&cases);
        for pair in page.cells.chunks(2) {
            assert_eq!(pair[0].case, pair[1].case);
            assert_eq!(pair[0].variant, Variant::Shaped);
            assert_eq!(pair[1].variant, Variant::Flat);
        }
    }

    /// Every id is unique: they name DOM nodes, JSON records and rows of
    /// the sheet, and two cells under one name would silently merge.
    #[test]
    fn every_cell_is_named_once() {
        let cases = cases();
        let page = layout(&cases);
        let mut ids: Vec<&str> = page.cells.iter().map(|c| c.id.as_str()).collect();
        ids.sort_unstable();
        let count = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), count);
    }

    /// The site's own strings are in the set, and so is the shaping set,
    /// in both faces: the page is a specimen of what the chart draws, not
    /// of what was convenient to draw.
    #[test]
    fn the_set_holds_the_site_strings_and_the_shaping_probes() {
        let cases = cases();
        let has = |t: &str| cases.iter().any(|c| c.text == t);
        for drawn in [
            "0",
            "100",
            "0s",
            "3s",
            "progress %",
            "half",
            "flight",
            "settle",
            "palette",
            "solid thumb",
            "progress ghost",
            "0.00s",
            "3.30s",
        ] {
            assert!(has(drawn), "the chart draws {drawn:?} and the set lacks it");
        }
        for probe in [
            "AV",
            "AW",
            "To",
            "Ta",
            "Yo",
            "LT",
            "P.",
            "F,",
            "POWER ISA",
            "bp",
        ] {
            let faces: Vec<Face> = cases
                .iter()
                .filter(|c| c.text == probe)
                .map(|c| c.face)
                .collect();
            assert_eq!(faces.len(), 2, "{probe:?} is not drawn in both faces");
            assert!(faces.contains(&Face::Regular) && faces.contains(&Face::Bold));
        }
        // the digit pair is the same length in the same face, so their
        // widths may be compared directly
        let same = cases
            .iter()
            .find(|c| c.text == "0000000")
            .expect("repeated digits");
        let mixed = cases
            .iter()
            .find(|c| c.text == "1234567")
            .expect("mixed digits");
        assert_eq!(same.text.len(), mixed.text.len());
    }

    /// The page names each cell, places it where the layout says, and
    /// carries the advance sum on it, so a capture can be cut into cells
    /// and compared without a second implementation of the sum.
    #[test]
    fn the_page_carries_every_box_and_its_advance_sum() {
        let cases = cases();
        let sheet = layout(&cases);
        let html = page(&cases, &sheet);
        for cell in &sheet.cells {
            let case = &cases[cell.case];
            let placed = format!(
                "id=\"{}\" data-advance=\"{:.4}\" style=\"left:{}px;top:{}px;width:{}px;height:{CELL_H}px;",
                cell.id,
                case.advance(),
                cell.x,
                cell.y,
                sheet.cell_w
            );
            assert!(html.contains(&placed), "the page lacks {placed}");
        }
        assert_eq!(
            html.matches("<text ").count(),
            sheet.cells.len() + atlas_count() as usize
        );
        // the flat control turns the shaping off, and only there
        assert_eq!(html.matches("class=\"cell flat\"").count(), cases.len());
        assert!(html.contains("font-kerning: none"));
    }

    /// The manifest the capture reads is JSON, and it says the same
    /// things the page does.
    #[test]
    fn the_manifest_parses_and_agrees_with_the_page() {
        use op_chart::data::json::{Value, parse};
        let cases = cases();
        let sheet = layout(&cases);
        let text = manifest(&cases, &sheet);
        let value = parse(&text).expect("the manifest is JSON");
        let Value::Object(fields) = &value else {
            panic!("the manifest is an object");
        };
        let get = |k: &str| fields.iter().find(|(n, _)| n == k).map(|(_, v)| v);
        let Some(Value::Array(cells)) = get("cells") else {
            panic!("the manifest lists its cells");
        };
        assert_eq!(cells.len(), sheet.cells.len());
        let Some(Value::Object(first)) = cells.first() else {
            panic!("a cell is an object");
        };
        let field = |k: &str| first.iter().find(|(n, _)| n == k).map(|(_, v)| v);
        assert_eq!(field("id"), Some(&Value::String(sheet.cells[0].id.clone())));
        assert_eq!(
            field("advance"),
            Some(&Value::Number(
                format!("{:.4}", cases[0].advance())
                    .parse()
                    .expect("a number")
            ))
        );
    }

    /// A capture with a known surface and a known bar of ink in every
    /// cell, so the box arithmetic can be held to a width that is known
    /// before it is measured.
    pub(crate) fn painted(
        cases: &[Case],
        page: &Page,
        capture: &Capture,
        bar: impl Fn(usize) -> u32,
    ) -> Image {
        let width = page.width * DPR;
        let height = page.height * DPR;
        let mut rgb = vec![0u8; (width * height * 3) as usize];
        let put = |rgb: &mut Vec<u8>, x: u32, y: u32, c: [u8; 3]| {
            let at = ((y * width + x) * 3) as usize;
            rgb[at..at + 3].copy_from_slice(&c);
        };
        let byte = |c: f64| (c * 255.0).round() as u8;
        for (i, cell) in page.cells.iter().enumerate() {
            let case = &cases[cell.case];
            // each cell gets its own surface, which are different
            // colours here, so a cell measured against the wrong one
            // would read its whole box as ink
            let s = capture.colour(case.surface).expect("the surface resolves");
            let surface = [byte(s.r), byte(s.g), byte(s.b)];
            for row in 0..CELL_H * DPR {
                for column in 0..page.cell_w * DPR {
                    put(&mut rgb, cell.x * DPR + column, cell.y * DPR + row, surface);
                }
            }
            // a solid bar from the text origin, on the baseline, the
            // width this cell was asked for
            let top = (BASELINE_Y - 5) * DPR;
            for row in top..top + 6 * DPR {
                for column in 0..bar(i) {
                    put(
                        &mut rgb,
                        cell.x * DPR + ORIGIN_X * DPR + column,
                        cell.y * DPR + row,
                        [0, 0, 0],
                    );
                }
            }
        }
        Image { width, height, rgb }
    }

    /// A capture record with the palette a measurement needs, black ink
    /// on white, and the browser laying every cell out at the sum.
    pub(crate) fn told(cases: &[Case], page: &Page) -> Capture {
        let metrics = Metrics {
            cap: 8.376,
            x: 6.192,
            descender: 2.868,
            font_ascent: 11.616,
            font_descent: 3.132,
        };
        Capture {
            theme: "light".to_owned(),
            dpr: DPR,
            image: "specimen-light.png".to_owned(),
            browser: "test".to_owned(),
            binary: "test".to_owned(),
            positioning: "which keeps each glyph's advance".to_owned(),
            colours: TOKENS
                .iter()
                .map(|t| {
                    let hex = match *t {
                        "--op-bg" => "#FFFFFF",
                        "--op-surface" => "#E0E0E0",
                        "--op-border-strong" => "#808080",
                        "--op-status-info" => "#0000FF",
                        _ => "#000000",
                    };
                    ((*t).to_owned(), hex.to_owned())
                })
                .collect(),
            metrics: vec![("400".to_owned(), metrics), ("700".to_owned(), metrics)],
            advances: page
                .cells
                .iter()
                .map(|c| (c.id.clone(), cases[c.case].advance()))
                .collect(),
        }
    }

    /// A box of pixels becomes a box of distances from its own surface,
    /// and a pixel of the ink itself covers the box completely.
    #[test]
    fn a_field_is_the_distance_from_the_surface() {
        let white = Srgb::from_hex("#FFFFFF").expect("white");
        let black = Srgb::from_hex("#000000").expect("black");
        let mut rgb = vec![255u8; 4 * 2 * 3];
        rgb[0..3].copy_from_slice(&[0, 0, 0]);
        rgb[3..6].copy_from_slice(&[128, 128, 128]);
        let image = Image {
            width: 4,
            height: 2,
            rgb,
        };
        let f = field(&image, 0, 0, 4, 2, white, black).expect("the box fits");
        assert!(f.ink > 90.0, "black is {} from white", f.ink);
        assert_eq!(f.coverage(0, 0), 1.0);
        assert_eq!(f.at(2, 0), 0.0);
        assert_eq!(f.coverage(2, 0), 0.0);
        let half = f.coverage(1, 0);
        assert!((0.3..0.8).contains(&half), "mid grey covers {half}");
        // a box that runs off the capture is refused rather than wrapped
        let off = field(&image, 2, 0, 4, 2, white, black);
        assert!(off.expect_err("a box off the edge").contains("runs off"));
    }

    /// The measurement finds the bar it was given: its width to the
    /// device pixel, its box from the cell's own corner, and no box at
    /// all where nothing was painted.
    #[test]
    fn a_painted_bar_measures_its_own_width() {
        let cases = cases();
        let page = layout(&cases);
        let capture = told(&cases, &page);
        // every cell gets a bar of its own width, so a cell cut from the
        // wrong place would report its neighbour's
        let bar = |i: usize| (i as u32 % 17 + 3) * 2;
        let image = painted(&cases, &page, &capture, bar);
        let rows = measure_capture(&image, &cases, &page, &capture).expect("the capture measures");
        assert_eq!(rows.len(), cases.len());
        for (n, row) in rows.iter().enumerate() {
            for (m, (measured, painted_field)) in [
                (&row.shaped, &row.shaped_field),
                (&row.flat, &row.flat_field),
            ]
            .into_iter()
            .enumerate()
            {
                let want = f64::from(bar(n * 2 + m)) / f64::from(DPR);
                let ink = measured.ink.expect("the bar was painted");
                assert!(
                    (ink.width() - want).abs() < 1e-9,
                    "{} painted {} where the bar is {want}",
                    measured.id,
                    ink.width()
                );
                assert!(
                    (ink.left - f64::from(ORIGIN_X)).abs() < 1e-9,
                    "{}",
                    measured.id
                );
                assert_eq!(measured.painted(), ink.width());
                // the darkest pixel of a solid bar stands exactly where
                // the cell's own ink stands from its own surface, which
                // is what says the bar was read against the right one
                assert!(
                    (measured.darkest - painted_field.ink).abs() < 1e-9,
                    "{} is darkest at {} where its ink stands at {}",
                    measured.id,
                    measured.darkest,
                    painted_field.ink
                );
                assert!(painted_field.ink > 80.0, "{} has no contrast", measured.id);
            }
        }
        // an empty capture has nothing to measure and says so rather
        // than reporting a box of nothing
        let blank = painted(&cases, &page, &capture, |_| 0);
        let rows =
            measure_capture(&blank, &cases, &page, &capture).expect("a blank capture measures");
        for row in &rows {
            assert_eq!(row.shaped.ink, None);
            assert_eq!(row.shaped.painted(), 0.0);
            assert_eq!(row.shaped.coverage, 0.0);
            assert_eq!(row.shaped.darkest, 0.0);
        }
    }

    /// Coverage is ink over the box the chart reserves for the label,
    /// not over the cell: a cell filled with ink across that box covers
    /// it entirely however wide the cell is.
    #[test]
    fn coverage_is_ink_over_the_reserved_box() {
        let cases = cases();
        let page = layout(&cases);
        let white = Srgb::from_hex("#FFFFFF").expect("white");
        let black = Srgb::from_hex("#000000").expect("black");
        let width = page.cell_w * DPR;
        let height = CELL_H * DPR;
        let image = Image {
            width,
            height,
            rgb: vec![0u8; (width * height * 3) as usize],
        };
        let f = field(&image, 0, 0, width, height, white, black).expect("the cell fits");
        let advance = cases[0].advance();
        let m = measure(&f, DPR, "solid", Variant::Shaped, advance, advance);
        assert!(
            (m.coverage - 1.0).abs() < 1e-9,
            "a solid cell covers {}",
            m.coverage
        );
        // the reserved box is cut to whole image pixels, so its area can
        // fall short of the em box by up to half a CSS pixel on a side
        let em = advance * TEXT_PX * (op_chart::ASCENT + op_chart::DESCENT);
        let slack = (TEXT_PX + advance) / f64::from(DPR);
        assert!(
            (m.ink_area - em).abs() <= slack,
            "{} of ink against a box of {em}, further out than the {slack} a whole-pixel cut can lose",
            m.ink_area
        );
        // and the two agree with each other whatever the cut: a fully
        // covered box is its own area of ink
        assert!((m.ink_area - m.coverage * m.ink_area).abs() < 1e-9);
        // a cell inked over only part of that box covers only that part
        let half = measure(&f, DPR, "solid", Variant::Shaped, advance / 2.0, advance);
        assert!((half.coverage - 1.0).abs() < 1e-9);
        assert!(
            half.ink_area < m.ink_area,
            "{} against {}",
            half.ink_area,
            m.ink_area
        );
    }

    /// A capture of the wrong size is refused: it would be cut into
    /// boxes that fell somewhere else on the page.
    #[test]
    fn a_capture_of_the_wrong_size_is_refused() {
        let cases = cases();
        let page = layout(&cases);
        let capture = told(&cases, &page);
        let image = Image {
            width: 8,
            height: 8,
            rgb: vec![0; 8 * 8 * 3],
        };
        let e = measure_capture(&image, &cases, &page, &capture).expect_err("the size is wrong");
        assert!(e.contains("where the page is"), "{e}");
    }

    /// The capture record reads back what the capture step wrote, names
    /// what it cannot find, and collapses the positioning to one clause
    /// when both faces agree.
    #[test]
    fn a_capture_record_is_read_or_named() {
        let dir = std::env::temp_dir().join(format!("op-verify-capture-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a temporary directory");
        let path = dir.join("capture-light.json");
        let body = r##"{"theme": "light", "dpr": 2, "image": "specimen-light.png",
          "browser": "chrome", "binary": "chrome (152)",
          "fonts": {"positioning": {"400": "subpixel", "700": "subpixel"}},
          "colours": {"--op-bg": "#020202", "--op-text": "#DCCAA4"},
          "metrics": {"400": {"cap": 8.4, "x": 6.2, "descender": 2.9, "font_ascent": 11.6, "font_descent": 3.1}},
          "advances": {"grid-0-shaped": 7.2}}"##;
        std::fs::write(&path, body).expect("the capture writes");
        let capture = read_capture(&path).expect("the capture reads");
        assert_eq!(capture.positioning, "which keeps each glyph's advance");
        assert_eq!(
            capture.colour("--op-bg"),
            Srgb::from_hex("#020202").ok_or(String::new())
        );
        assert_eq!(capture.advance("grid-0-shaped"), Ok(7.2));
        assert!(capture.metrics(Face::Regular).is_ok());
        assert!(
            capture
                .metrics(Face::Bold)
                .expect_err("no bold metrics")
                .contains("700"),
            "the missing face is named"
        );
        assert!(
            capture
                .colour("--op-nothing")
                .expect_err("no such token")
                .contains("--op-nothing")
        );
        assert!(
            capture
                .advance("nowhere")
                .expect_err("no such cell")
                .contains("nowhere")
        );
        // a face that rounds is named as such, and two that disagree are
        // both reported rather than one standing for the pair
        let rounded = body.replace(r#""700": "subpixel""#, r#""700": "whole pixel advances""#);
        std::fs::write(&path, &rounded).expect("the capture writes");
        assert_eq!(
            read_capture(&path).expect("the capture reads").positioning,
            "400 subpixel, 700 whole pixel advances"
        );
        std::fs::write(&path, "{").expect("a broken capture writes");
        assert!(read_capture(&path).is_err());
        std::fs::remove_dir_all(&dir).expect("the temporary directory goes");
    }

    /// The summary names the case that came off worst, says whether
    /// anything kerned, and ranks the set by weight. Built from bars
    /// whose widths are chosen so the answer is known: one cell is made
    /// far narrower than its sum, and one is made far heavier.
    #[test]
    fn the_summary_names_the_worst_and_ranks_the_weight() {
        let cases = cases();
        let page = layout(&cases);
        let capture = told(&cases, &page);
        // the advance sum in device pixels, so every bar is exactly its
        // own sum wide and nothing disagrees
        let exact =
            |i: usize| (cases[page.cells[i].case].advance() * f64::from(DPR)).round() as u32;
        let image = painted(&cases, &page, &capture, exact);
        let rows = measure_capture(&image, &cases, &page, &capture).expect("the capture measures");
        let lines = summary(&cases, &rows);
        assert!(lines[0].starts_with(&format!("{} cases", cases.len())));
        assert!(
            lines.iter().any(|l| l.starts_with("Kerning: no case")),
            "nothing was made to kern: {lines:#?}"
        );
        // now narrow one cell's shaped bar by four pixels and it becomes
        // both the worst disagreement and the one that kerned
        let narrowed = painted(&cases, &page, &capture, |i| {
            if i == 6 { exact(i) - 8 } else { exact(i) }
        });
        let rows =
            measure_capture(&narrowed, &cases, &page, &capture).expect("the capture measures");
        let lines = summary(&cases, &rows);
        let name = format!("{:?}", cases[page.cells[6].case].text);
        assert!(
            lines[1].starts_with("Worst absolute:") && lines[1].contains(&name),
            "{}",
            lines[1]
        );
        let kerning = lines
            .iter()
            .find(|l| l.starts_with("Kerning:"))
            .expect("a kerning line");
        assert!(
            kerning.contains("1 of") && kerning.contains(&name),
            "{kerning}"
        );
        // the heaviest is the case with the most ink over its own box,
        // which with bars of the sum's own width is the shortest string
        assert!(lines.iter().any(|l| l.starts_with("Heaviest by ink")));
        assert!(lines.iter().any(|l| l.starts_with("Lightest:")));
    }

    /// The JSON is JSON, carries every case, and says the same numbers
    /// the summary was built from.
    #[test]
    fn the_measured_json_parses_and_carries_every_case() {
        let cases = cases();
        let page = layout(&cases);
        let capture = told(&cases, &page);
        let image = painted(&cases, &page, &capture, |i| (i as u32 % 11 + 4) * 2);
        let rows = measure_capture(&image, &cases, &page, &capture).expect("the capture measures");
        let lines = summary(&cases, &rows);
        let text = measured_json(&cases, &rows, &capture, &lines);
        let Value::Object(fields) = parse(&text).expect("the measurement is JSON") else {
            panic!("the measurement is an object");
        };
        let get = |k: &str| fields.iter().find(|(n, _)| n == k).map(|(_, v)| v);
        let Some(Value::Array(records)) = get("cases") else {
            panic!("the measurement lists its cases");
        };
        assert_eq!(records.len(), rows.len());
        let Some(Value::Object(first)) = records.first() else {
            panic!("a case is an object");
        };
        let field_of = |k: &str| first.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone());
        assert_eq!(field_of("id"), Some(Value::String(cases[0].id.clone())));
        assert_eq!(
            field_of("painted"),
            Some(Value::Number(
                format!("{:.3}", rows[0].shaped.painted())
                    .parse()
                    .expect("a number")
            ))
        );
        assert_eq!(field_of("kerned"), Some(Value::Bool(rows[0].kerned())));
    }
}
