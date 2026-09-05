//! The contact sheet a person reads.
//!
//! [`crate::specimens`] measures a capture; this draws it back out at a
//! size an eye can work at, with the measurement laid over the specimen
//! as a grid rather than left as a column of numbers. A reader should be
//! able to look at one row and see, without arithmetic, whether the
//! advance sum is the width the browser painted and how heavily the
//! label sits on its surface.
//!
//! The discipline is the rest of the project's. Nothing is drawn that
//! does not carry a measurement: hairlines, one colour for the face's own
//! metrics and another for what was measured off the pixels, both from
//! the palette so both work in either theme, a scale in CSS pixels along
//! each row, and the whole thing stated in a key at the top. The grid
//! goes down before the specimen does, so a rule that crosses a glyph
//! passes under it.
//!
//! The lettering is the specimen's own: the page carries a glyph atlas,
//! every character the advance tables cover drawn once in the site's
//! face, and text here is composed by laying those captured glyphs at the
//! advances the table gives. So the sheet is set in the face it is
//! reporting on, with no bitmap font invented for the purpose, and it
//! says so in its key.

use crate::frames::Image;
use crate::specimens::{
    ATLAS_BASE, ATLAS_COLS, ATLAS_H, ATLAS_INK, ATLAS_INSET, ATLAS_PX, ATLAS_SURFACE, ATLAS_W,
    BASELINE_Y, CELL_H, Capture, Case, Field, ORIGIN_X, Page, Row, atlas_count, atlas_rows,
    face_name, field,
};
use op_chart::{Face, TEXT_PX, text_width};
use op_colour::Srgb;
use std::path::Path;

/// How many times the capture is magnified on the sheet. With a capture
/// at two image pixels to the CSS pixel this puts six image pixels on the
/// sheet for every CSS pixel of the page, which is enough to separate the
/// cap height from the x height by a clear band at 12 px text.
pub const ZOOM: u32 = 3;

/// Image pixels of sheet to one CSS pixel of the specimen.
pub fn scale(dpr: u32) -> u32 {
    dpr * ZOOM
}

/// The sheet's outer margin.
const MARGIN: u32 = 24;
/// The column holding each row's name, string and note.
const GUTTER: u32 = 430;
/// How far apart the four letters that name the metric rules are
/// staggered, so rules a couple of pixels apart can each be named
/// without their names touching. They stand in the panel's own left
/// margin, which is clear surface: the text origin is [`ORIGIN_X`] CSS
/// px in, and they are laid before the specimen, so a glyph would win in
/// any case.
const MARK_STEP: u32 = 13;
/// Clear space between the panel and the numbers.
const NUMBERS_GAP: u32 = 16;
/// The width of one column of numbers.
const NUMBER_W: u32 = 82;
/// The height of the scale strip under each row's panel.
const RULER_H: u32 = 22;
/// Clear space under each row.
const ROW_GAP: u32 = 12;
/// A line of the sheet's own text.
const LINE: u32 = 17;

/// The names of the number columns, in the order they are drawn.
const NUMBERS: [&str; 8] = [
    "painted", "advance", "diff", "flat", "shaping", "laid out", "cover", "darkest",
];

/// A sheet being drawn: 8-bit sRGB, three bytes a pixel, rows top to
/// bottom, which is what the png crate writes and what [`Image`] holds.
pub struct Canvas {
    pub width: u32,
    pub height: u32,
    pub rgb: Vec<u8>,
}

impl Canvas {
    /// A sheet of one colour.
    pub fn new(width: u32, height: u32, background: Srgb) -> Self {
        let byte = |c: f64| (c.clamp(0.0, 1.0) * 255.0).round() as u8;
        let fill = [byte(background.r), byte(background.g), byte(background.b)];
        Self {
            width,
            height,
            rgb: fill.repeat((width * height) as usize),
        }
    }

    /// Lay `colour` over one pixel at `alpha`, which is how both the
    /// annotation and the specimen's own glyphs go down: a hairline at a
    /// fraction of its colour stays quiet, and a glyph carries the
    /// coverage it was measured with.
    pub fn blend(&mut self, x: i64, y: i64, colour: Srgb, alpha: f64) {
        if x < 0 || y < 0 || x >= i64::from(self.width) || y >= i64::from(self.height) {
            return;
        }
        let alpha = alpha.clamp(0.0, 1.0);
        if alpha <= 0.0 {
            return;
        }
        let at = ((y as u32 * self.width + x as u32) * 3) as usize;
        for (i, c) in [colour.r, colour.g, colour.b].into_iter().enumerate() {
            let under = f64::from(self.rgb[at + i]) / 255.0;
            let over = under * (1.0 - alpha) + c * alpha;
            self.rgb[at + i] = (over.clamp(0.0, 1.0) * 255.0).round() as u8;
        }
    }

    /// A hairline across, at one image pixel.
    pub fn across(&mut self, x0: i64, x1: i64, y: i64, colour: Srgb, alpha: f64) {
        for x in x0.min(x1)..=x0.max(x1) {
            self.blend(x, y, colour, alpha);
        }
    }

    /// A hairline down, at one image pixel.
    pub fn down(&mut self, x: i64, y0: i64, y1: i64, colour: Srgb, alpha: f64) {
        for y in y0.min(y1)..=y0.max(y1) {
            self.blend(x, y, colour, alpha);
        }
    }

    /// The outline of a box, at one image pixel.
    pub fn outline(&mut self, x: i64, y: i64, width: i64, height: i64, colour: Srgb, alpha: f64) {
        self.across(x, x + width - 1, y, colour, alpha);
        self.across(x, x + width - 1, y + height - 1, colour, alpha);
        self.down(x, y, y + height - 1, colour, alpha);
        self.down(x + width - 1, y, y + height - 1, colour, alpha);
    }

    /// A box laid over what is there at `alpha`, which is a solid fill
    /// at one and a wash below it.
    pub fn wash(&mut self, x: i64, y: i64, width: i64, height: i64, colour: Srgb, alpha: f64) {
        for row in 0..height {
            for column in 0..width {
                self.blend(x + column, y + row, colour, alpha);
            }
        }
    }

    /// A solid box.
    pub fn fill(&mut self, x: i64, y: i64, width: i64, height: i64, colour: Srgb) {
        self.wash(x, y, width, height, colour, 1.0);
    }

    /// Write the sheet as a PNG.
    pub fn write(&self, path: &Path) -> Result<(), String> {
        let fail = |e: &dyn std::fmt::Display| format!("{}: {e}", path.display());
        let file = std::fs::File::create(path).map_err(|e| fail(&e))?;
        let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), self.width, self.height);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .map_err(|e| fail(&e))?
            .write_image_data(&self.rgb)
            .map_err(|e| fail(&e))?;
        Ok(())
    }
}

/// The specimen page's glyph atlas, cut out of the capture: every
/// character the advance tables cover, each drawn once at a known offset
/// into a box of its own, so any string can be composed from them.
pub struct Atlas {
    ink: Field,
    dpr: u32,
}

impl Atlas {
    /// Cut the atlas out of a capture.
    pub fn read(image: &Image, page: &Page, capture: &Capture) -> Result<Self, String> {
        let ink = field(
            image,
            0,
            page.atlas_y * capture.dpr,
            ATLAS_COLS * ATLAS_W * capture.dpr,
            atlas_rows() * ATLAS_H * capture.dpr,
            capture.colour(ATLAS_SURFACE)?,
            capture.colour(ATLAS_INK)?,
        )?;
        Ok(Self {
            ink,
            dpr: capture.dpr,
        })
    }

    /// Where a character's box sits in the atlas, in image pixels.
    fn box_of(&self, c: char) -> Option<(u32, u32)> {
        let first = op_chart::advances::FIRST;
        if !(first..=op_chart::advances::LAST).contains(&c) {
            return None;
        }
        let i = c as u32 - first as u32;
        if i >= atlas_count() {
            return None;
        }
        Some((
            (i % ATLAS_COLS) * ATLAS_W * self.dpr,
            (i / ATLAS_COLS) * ATLAS_H * self.dpr,
        ))
    }

    /// How far the pen moves over one character, in image pixels. The
    /// same advance table the sheet is reporting on, which is stated in
    /// the key: at nine pixels its error is far under one image pixel,
    /// and it is the only measurement of these faces this crate has.
    fn step(&self, c: char) -> f64 {
        text_width(&c.to_string(), ATLAS_PX, Face::Regular) * f64::from(self.dpr)
    }

    /// How wide a string will be drawn, in image pixels.
    pub fn width(&self, text: &str) -> f64 {
        text_width(text, ATLAS_PX, Face::Regular) * f64::from(self.dpr)
    }

    /// Draw a string with its pen starting at `x` and its baseline at
    /// `y`, both in image pixels, laying every glyph over whatever is
    /// already there at the coverage it was captured with.
    pub fn draw(&self, canvas: &mut Canvas, x: f64, y: f64, text: &str, colour: Srgb) {
        let mut pen = x;
        let width = ATLAS_W * self.dpr;
        let height = ATLAS_H * self.dpr;
        for c in text.chars() {
            if let Some((bx, by)) = self.box_of(c) {
                let left = (pen - f64::from(ATLAS_INSET * self.dpr)).round() as i64;
                let top = y.round() as i64 - i64::from(ATLAS_BASE * self.dpr);
                for row in 0..height {
                    for column in 0..width {
                        let alpha = self.ink.coverage(bx + column, by + row);
                        if alpha > 0.0 {
                            canvas.blend(
                                left + i64::from(column),
                                top + i64::from(row),
                                colour,
                                alpha,
                            );
                        }
                    }
                }
            }
            pen += self.step(c);
        }
    }

    /// Draw a string ending at `x` rather than starting there, which is
    /// how a column of numbers is kept aligned on its last digit.
    pub fn draw_to(&self, canvas: &mut Canvas, x: f64, y: f64, text: &str, colour: Srgb) {
        self.draw(canvas, x - self.width(text), y, text, colour);
    }
}

/// The colours the sheet annotates in, all from the palette so they hold
/// in either theme: the face's own metrics in the strong border colour,
/// what was measured off the pixels in the information colour, and the
/// scale and the prose in the muted one.
struct Pens {
    background: Srgb,
    metric: Srgb,
    measured: Srgb,
    quiet: Srgb,
    text: Srgb,
}

impl Pens {
    fn read(capture: &Capture) -> Result<Self, String> {
        Ok(Self {
            background: capture.colour("--op-bg")?,
            metric: capture.colour("--op-border-strong")?,
            measured: capture.colour("--op-status-info")?,
            quiet: capture.colour("--op-muted")?,
            text: capture.colour("--op-text")?,
        })
    }
}

/// How far into a hairline's colour the annotation goes. Quieter than the
/// specimen, which is drawn at the coverage it was captured with.
const HAIRLINE: f64 = 0.85;
/// How far into the measurement's colour the two bands go: the space
/// between where the paint stops and where the advance sum says the pen
/// does. Quiet enough to read as empty room and dark enough to be seen
/// without measuring it.
const BAND: f64 = 0.16;

/// Draw the whole sheet: the key, then a row per case, then whatever the
/// measurement had to say about the set as a whole.
pub fn contact_sheet(
    image: &Image,
    cases: &[Case],
    page: &Page,
    capture: &Capture,
    rows: &[Row],
    summary: &[String],
) -> Result<Canvas, String> {
    let atlas = Atlas::read(image, page, capture)?;
    let pens = Pens::read(capture)?;
    let step = scale(capture.dpr);
    let panel_w = page.cell_w * step;
    let panel_h = CELL_H * step;
    let row_h = panel_h + RULER_H + ROW_GAP;
    let key = key_lines(capture, page, step);
    let head = MARGIN + LINE * (key.len() as u32 + 1) + LINE;
    let width = MARGIN * 2 + GUTTER + panel_w + NUMBERS_GAP + NUMBER_W * NUMBERS.len() as u32;
    let height = head + row_h * rows.len() as u32 + LINE * (summary.len() as u32 + 2) + MARGIN;
    let mut canvas = Canvas::new(width, height, pens.background);

    let panel_x = MARGIN + GUTTER;
    let numbers_x = panel_x + panel_w + NUMBERS_GAP;
    let mut y = MARGIN + LINE;
    atlas.draw(
        &mut canvas,
        f64::from(MARGIN),
        f64::from(y),
        &format!(
            "openpower.tools: what the browser paints where the chart's advance table says it will. {} theme.",
            capture.theme
        ),
        pens.text,
    );
    for line in &key {
        y += LINE;
        atlas.draw(
            &mut canvas,
            f64::from(MARGIN),
            f64::from(y),
            line,
            pens.quiet,
        );
    }
    y += LINE;
    atlas.draw(
        &mut canvas,
        f64::from(MARGIN),
        f64::from(y),
        "label",
        pens.quiet,
    );
    for (i, name) in NUMBERS.iter().enumerate() {
        let right = numbers_x + NUMBER_W * (i as u32 + 1) - 8;
        atlas.draw_to(
            &mut canvas,
            f64::from(right),
            f64::from(y),
            name,
            pens.quiet,
        );
    }
    canvas.across(
        i64::from(MARGIN),
        i64::from(width - MARGIN),
        i64::from(y) + 5,
        pens.quiet,
        HAIRLINE,
    );

    let mut top = head;
    for row in rows {
        let case = &cases[row.case];
        draw_row(
            &mut canvas,
            &atlas,
            &pens,
            capture,
            case,
            row,
            panel_x,
            top,
            step,
            page,
        )?;
        // the row's name, string and note
        let text_y = top + 22;
        atlas.draw(
            &mut canvas,
            f64::from(MARGIN),
            f64::from(text_y),
            case.kind,
            pens.text,
        );
        atlas.draw(
            &mut canvas,
            f64::from(MARGIN),
            f64::from(text_y + LINE),
            &format!("\"{}\"", case.text),
            pens.text,
        );
        atlas.draw(
            &mut canvas,
            f64::from(MARGIN),
            f64::from(text_y + LINE * 2),
            &format!(
                "Plex Sans {}, {} on {}",
                face_name(case.face),
                &case.ink[2..],
                &case.surface[2..]
            ),
            pens.quiet,
        );
        atlas.draw(
            &mut canvas,
            f64::from(MARGIN),
            f64::from(text_y + LINE * 3),
            case.note,
            pens.quiet,
        );
        // the numbers
        let m = &row.shaped;
        let values = [
            format!("{:.2}", m.painted()),
            format!("{:.2}", m.advance),
            format!("{:+.2}", m.difference()),
            format!("{:.2}", row.flat.painted()),
            format!("{:+.2}", row.shaping()),
            format!("{:.2}", m.browser),
            format!("{:.1}%", m.coverage * 100.0),
            format!("{:.1}", m.darkest),
        ];
        for (i, value) in values.iter().enumerate() {
            let right = numbers_x + NUMBER_W * (i as u32 + 1) - 8;
            atlas.draw_to(
                &mut canvas,
                f64::from(right),
                f64::from(text_y + LINE),
                value,
                if i == 4 && row.kerned() {
                    pens.measured
                } else {
                    pens.text
                },
            );
        }
        top += row_h;
    }

    let mut y = top + LINE;
    canvas.across(
        i64::from(MARGIN),
        i64::from(width - MARGIN),
        i64::from(y) - 12,
        pens.quiet,
        HAIRLINE,
    );
    for line in summary {
        atlas.draw(
            &mut canvas,
            f64::from(MARGIN),
            f64::from(y),
            line,
            pens.text,
        );
        y += LINE;
    }
    Ok(canvas)
}

/// One row: the cell's own surface, the grid, the specimen over it, the
/// letters that name the metric rules, and the scale.
#[allow(clippy::too_many_arguments)]
fn draw_row(
    canvas: &mut Canvas,
    atlas: &Atlas,
    pens: &Pens,
    capture: &Capture,
    case: &Case,
    row: &Row,
    panel_x: u32,
    top: u32,
    step: u32,
    page: &Page,
) -> Result<(), String> {
    let surface = capture.colour(case.surface)?;
    let metrics = capture.metrics(case.face)?;
    let panel_w = page.cell_w * step;
    let panel_h = CELL_H * step;
    let at_x = |cx: f64| f64::from(panel_x) + cx * f64::from(step);
    let at_y = |cy: f64| f64::from(top) + cy * f64::from(step);
    canvas.fill(
        i64::from(panel_x),
        i64::from(top),
        i64::from(panel_w),
        i64::from(panel_h),
        surface,
    );

    // the face's own metrics, and the pen's travel over them
    let rules: [(f64, &str); 4] = [
        (f64::from(BASELINE_Y) - metrics.cap, "C"),
        (f64::from(BASELINE_Y) - metrics.x, "x"),
        (f64::from(BASELINE_Y), "B"),
        (f64::from(BASELINE_Y) + metrics.descender, "D"),
    ];
    for (i, (cy, name)) in rules.iter().enumerate() {
        let y = at_y(*cy).round() as i64;
        canvas.across(
            i64::from(panel_x),
            i64::from(panel_x + panel_w - 1),
            y,
            pens.metric,
            HAIRLINE,
        );
        let x = f64::from(panel_x + 4 + MARK_STEP * i as u32);
        atlas.draw(canvas, x, f64::from(y as u32) + 6.0, name, pens.metric);
    }
    for cx in [
        f64::from(ORIGIN_X),
        f64::from(ORIGIN_X) + row.shaped.advance,
    ] {
        canvas.down(
            at_x(cx).round() as i64,
            i64::from(top),
            i64::from(top + panel_h - 1),
            pens.metric,
            HAIRLINE,
        );
    }

    // what the pixels came to, and the two bands where it disagrees
    // with the sum: the room the pen takes before the first mark and
    // after the last, which is what an advance sum counts and an ink
    // box does not
    if let Some(ink) = row.shaped.ink {
        let band_top = at_y(f64::from(BASELINE_Y) - metrics.cap).round() as i64;
        let band_bottom = at_y(f64::from(BASELINE_Y) + metrics.descender).round() as i64;
        for (a, b) in [
            (f64::from(ORIGIN_X), ink.left),
            (ink.right, f64::from(ORIGIN_X) + row.shaped.advance),
        ] {
            let (from, to) = (at_x(a.min(b)).round() as i64, at_x(a.max(b)).round() as i64);
            canvas.wash(
                from,
                band_top,
                to - from,
                band_bottom - band_top,
                pens.measured,
                BAND,
            );
        }
        canvas.outline(
            at_x(ink.left).round() as i64,
            at_y(ink.top).round() as i64,
            (at_x(ink.right) - at_x(ink.left)).round() as i64,
            (at_y(ink.bottom) - at_y(ink.top)).round() as i64,
            pens.measured,
            HAIRLINE,
        );
    }

    // the specimen, over the grid, at the coverage it was captured with
    for y in 0..panel_h {
        for x in 0..panel_w {
            let alpha = row.shaped_field.coverage(x / ZOOM, y / ZOOM);
            if alpha > 0.0 {
                canvas.blend(
                    i64::from(panel_x + x),
                    i64::from(top + y),
                    capture.colour(case.ink)?,
                    alpha,
                );
            }
        }
    }

    // the scale, in CSS pixels from the text origin
    let ruler = top + panel_h + 6;
    canvas.across(
        i64::from(panel_x),
        i64::from(panel_x + panel_w - 1),
        i64::from(ruler),
        pens.quiet,
        HAIRLINE,
    );
    let mut css = -(f64::from(ORIGIN_X) / 10.0).floor() * 10.0;
    while at_x(f64::from(ORIGIN_X) + css) < f64::from(panel_x + panel_w) {
        let x = at_x(f64::from(ORIGIN_X) + css).round() as i64;
        let labelled = (css / 50.0).fract().abs() < 1e-9;
        canvas.down(
            x,
            i64::from(ruler),
            i64::from(ruler) + if labelled { 7 } else { 4 },
            pens.quiet,
            HAIRLINE,
        );
        if labelled {
            let mark = format!("{css:.0}");
            atlas.draw(
                canvas,
                f64::from(x as u32) + 3.0,
                f64::from(ruler + RULER_H - 4),
                &mark,
                pens.quiet,
            );
        }
        css += 10.0;
    }
    Ok(())
}

/// The key: what the sheet is, what its scale is, and what each colour
/// means. Everything a reader needs to take a distance off the picture.
fn key_lines(capture: &Capture, page: &Page, step: u32) -> Vec<String> {
    vec![
        format!(
            "Every string the chart draws, in the served IBM Plex Sans at {TEXT_PX} px over the surface it sits on, \
             captured in {}, {}.",
            capture.browser, capture.positioning
        ),
        format!(
            "Sheet scale: {step} image pixels to the CSS pixel (captured at a device pixel ratio of {}, shown at {ZOOM} times). \
             Each cell is {} by {CELL_H} CSS px.",
            capture.dpr, page.cell_w
        ),
        "The scale under each row is CSS pixels from the text origin, ticked every 10 and labelled every 50."
            .to_owned(),
        "Grey is the face's own metrics, measured from its outline and named in the panel's left margin: \
         C cap height, x x height, B baseline, D descender depth."
            .to_owned(),
        "The grey uprights are the text origin and the end of op-chart's advance sum, so the pen's travel is the band between them."
            .to_owned(),
        "Blue is the measurement: the box the paint covers, taken as every pixel standing more than one CIEDE2000 unit off the surface,"
            .to_owned(),
        "and the two washed bands are where that box falls short of the advance sum, which is the side bearings and any kerning."
            .to_owned(),
        "The grid goes down before the specimen, so a rule that meets a glyph passes under it and the glyph wins."
            .to_owned(),
        "flat is the same string with kerning, ligatures and contextual alternates off, which is the advance table's own layout;"
            .to_owned(),
        "shaping is painted minus flat, and is the kerning alone, since both cells share their first and last glyph."
            .to_owned(),
        "This sheet letters itself from the specimen's own glyph atlas, set at 9 px from the same advance table it reports on."
            .to_owned(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::specimens::{DPR, Variant, cases, layout, measure_capture, summary};

    /// A canvas holds the colour it was made of, lays another over it in
    /// proportion to the coverage asked for, and ignores anything drawn
    /// off its edges rather than wrapping it to the far side.
    #[test]
    fn a_canvas_lays_colour_over_colour_and_stops_at_its_edges() {
        let white = Srgb::from_hex("#FFFFFF").expect("white");
        let black = Srgb::from_hex("#000000").expect("black");
        let mut canvas = Canvas::new(4, 3, white);
        assert_eq!(canvas.rgb.len(), 4 * 3 * 3);
        assert!(canvas.rgb.iter().all(|b| *b == 255));
        canvas.blend(0, 0, black, 1.0);
        assert_eq!(&canvas.rgb[0..3], &[0, 0, 0]);
        canvas.blend(1, 0, black, 0.5);
        assert_eq!(&canvas.rgb[3..6], &[128, 128, 128]);
        canvas.blend(2, 0, black, 0.0);
        assert_eq!(&canvas.rgb[6..9], &[255, 255, 255]);
        // nothing off the edge touches anything on it
        let before = canvas.rgb.clone();
        for (x, y) in [(-1, 0), (0, -1), (4, 0), (0, 3), (-5, -5), (99, 99)] {
            canvas.blend(x, y, black, 1.0);
        }
        assert_eq!(canvas.rgb, before);
        // and a hairline that runs past the edge draws the part inside it
        canvas.across(-3, 99, 2, black, 1.0);
        assert!(canvas.rgb[(2 * 4 * 3)..(3 * 4 * 3)].iter().all(|b| *b == 0));
    }

    /// A sheet written as a PNG reads back as the pixels it was drawn
    /// with: the sheet is the deliverable, so nothing may be lost on the
    /// way to the file.
    #[test]
    fn a_sheet_survives_being_written_and_read() {
        let mut canvas = Canvas::new(9, 5, Srgb::from_hex("#2B415F").expect("a surface"));
        canvas.outline(
            1,
            1,
            7,
            3,
            Srgb::from_hex("#EB6424").expect("an accent"),
            1.0,
        );
        let dir = std::env::temp_dir().join(format!("op-verify-sheet-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a temporary directory");
        let path = dir.join("sheet.png");
        canvas.write(&path).expect("the sheet writes");
        let read = crate::frames::decode(&path).expect("the sheet reads");
        assert_eq!((read.width, read.height), (canvas.width, canvas.height));
        assert_eq!(read.rgb, canvas.rgb);
        std::fs::remove_dir_all(&dir).expect("the temporary directory goes");
    }

    /// The atlas draws the glyph it was asked for, where it says it will,
    /// and nowhere else: a box of solid ink in the atlas becomes a box of
    /// solid colour on the sheet at the pen's own place, and the pen
    /// moves by the advance of what it drew.
    #[test]
    fn the_atlas_draws_where_it_says() {
        let cases = cases();
        let page = layout(&cases);
        let capture = crate::specimens::tests::told(&cases, &page);
        // a capture whose atlas is blank but for one character's box
        let mut image = crate::specimens::tests::painted(&cases, &page, &capture, |_| 0);
        let width = image.width;
        for y in 0..atlas_rows() * ATLAS_H * DPR {
            for x in 0..ATLAS_COLS * ATLAS_W * DPR {
                let at = (((page.atlas_y * DPR + y) * width + x) * 3) as usize;
                image.rgb[at..at + 3].copy_from_slice(&[255, 255, 255]);
            }
        }
        let m = 'M';
        let index = m as u32 - op_chart::advances::FIRST as u32;
        let (bx, by) = (
            (index % ATLAS_COLS) * ATLAS_W * DPR,
            (index / ATLAS_COLS) * ATLAS_H * DPR,
        );
        for y in 0..ATLAS_H * DPR {
            for x in 0..ATLAS_W * DPR {
                let at = (((page.atlas_y * DPR + by + y) * width + bx + x) * 3) as usize;
                image.rgb[at..at + 3].copy_from_slice(&[0, 0, 0]);
            }
        }
        let atlas = Atlas::read(&image, &page, &capture).expect("the atlas reads");
        let white = Srgb::from_hex("#FFFFFF").expect("white");
        let ink = Srgb::from_hex("#000000").expect("black");
        let mut canvas = Canvas::new(200, 100, white);
        atlas.draw(&mut canvas, 60.0, 50.0, "M", ink);
        let solid = |c: &Canvas, x: u32, y: u32| {
            let at = ((y * c.width + x) * 3) as usize;
            c.rgb[at] == 0
        };
        let left = 60 - ATLAS_INSET * DPR;
        let top = 50 - ATLAS_BASE * DPR;
        assert!(
            solid(&canvas, left, top),
            "the box starts where the pen does"
        );
        assert!(solid(
            &canvas,
            left + ATLAS_W * DPR - 1,
            top + ATLAS_H * DPR - 1
        ));
        assert!(
            !solid(&canvas, left - 1, top),
            "the box does not reach back past the pen"
        );
        assert!(
            !solid(&canvas, left + ATLAS_W * DPR, top),
            "nor past its own width"
        );
        // a character with no box in the atlas leaves the sheet alone but
        // still moves the pen, so a string keeps its spacing
        let before = canvas.rgb.clone();
        atlas.draw(&mut canvas, 60.0, 90.0, "\u{2014}", ink);
        assert_eq!(canvas.rgb, before);
        assert!(atlas.width("MM") > atlas.width("M"));
        assert_eq!(atlas.width(""), 0.0);
    }

    /// The sheet is as tall as its rows and as wide as its columns, and
    /// each row's panel stands where the layout says: a row drawn a pixel
    /// out would put its grid over the wrong specimen.
    #[test]
    fn the_sheet_puts_every_row_where_it_says() {
        let cases = cases();
        let page = layout(&cases);
        let capture = crate::specimens::tests::told(&cases, &page);
        let image =
            crate::specimens::tests::painted(&cases, &page, &capture, |i| (i as u32 % 9 + 3) * 2);
        let rows = measure_capture(&image, &cases, &page, &capture).expect("the capture measures");
        let lines = summary(&cases, &rows);
        let canvas =
            contact_sheet(&image, &cases, &page, &capture, &rows, &lines).expect("a sheet");
        let step = scale(capture.dpr);
        let panel_w = page.cell_w * step;
        assert_eq!(
            canvas.width,
            MARGIN * 2 + GUTTER + panel_w + NUMBERS_GAP + NUMBER_W * NUMBERS.len() as u32
        );
        assert!(canvas.height > (CELL_H * step + RULER_H) * rows.len() as u32);
        // every row's panel carries the ink it was painted with, at the
        // magnification the key states
        let panel_x = MARGIN + GUTTER;
        let head = canvas.height
            - (CELL_H * step + RULER_H + ROW_GAP) * rows.len() as u32
            - LINE * (lines.len() as u32 + 2)
            - MARGIN;
        for (n, row) in rows.iter().enumerate() {
            let top = head + (CELL_H * step + RULER_H + ROW_GAP) * n as u32;
            let bar = row.shaped.ink.expect("the bar was painted");
            let inside =
                panel_x + ((f64::from(ORIGIN_X) + bar.width() / 2.0) * f64::from(step)) as u32;
            let baseline = top + (f64::from(BASELINE_Y) - 2.0) as u32 * step;
            let at = ((baseline * canvas.width + inside) * 3) as usize;
            assert!(
                canvas.rgb[at] < 128,
                "row {n} ({}) has no ink over the middle of its bar",
                row.shaped.id
            );
        }
        assert_eq!(rows[0].shaped.variant, Variant::Shaped);
    }
}
