//! Every ordered pair of characters the chart can draw, kerned once.
//!
//! [`crate::specimens`] measures eight pairs a face is known to kern.
//! This measures all of them. A De Bruijn sequence of order two over the
//! block op-chart's advance tables cover holds every ordered pair exactly
//! once, so one string of 9026 characters, laid out once per face,
//! carries the kern of all 9025 pairs.
//!
//! The reading is the pen's own arithmetic. The browser puts glyph `i` at
//! `x[i]` and glyph `i + 1` at `x[i] + advance(s[i]) + kern(s[i],
//! s[i+1])`, so the step between two reported positions, less the
//! advance the table gives for the first character, is the kern of that
//! ordered pair and nothing else. `getStartPositionOfChar` gives every
//! position from one layout, which is what makes an exhaustive sweep
//! cost one layout rather than nine thousand.
//!
//! Each face is laid out three times over, which costs three layouts and
//! settles three different questions.
//!
//! * `flat` turns every shaping feature off, so the browser lays each
//!   glyph at its nominal advance. Its step must equal the advance table
//!   for every one of the 9026 characters, which is an exhaustive check
//!   of the table itself rather than of the kerning.
//! * `kern` turns ligatures and contextual alternates off and leaves
//!   kerning on, so its step less the advance is the pairwise kern,
//!   uncontaminated by anything a neighbour did.
//! * `shaped` is what the site gets. Where its step differs from `kern`,
//!   something other than a pair decided the layout: a ligature, or a
//!   contextual alternate reading further than two characters. Those are
//!   counted and named rather than averaged into the kerns, because an
//!   order-two sweep cannot attribute them to a pair.

use op_chart::advances::{COUNT, FIRST, LAST};
use op_chart::{Face, TEXT_PX, text_width};

/// The order of the sweep: every ordered pair, which is order two.
pub const ORDER: usize = 2;

/// The characters the sweep runs over, which are exactly the characters
/// op-chart's advance tables cover. Taken from those tables rather than
/// written out, so the two cannot drift apart.
pub fn alphabet() -> Vec<char> {
    (FIRST..=LAST).collect()
}

/// A De Bruijn sequence B(k, n) as indices into an alphabet of `k`, by
/// the Lyndon word construction of Fredricksen, Kessler and Maiorana:
/// the concatenation, in lexicographic order, of the Lyndon words over
/// the alphabet whose length divides `n`.
///
/// The result has `k` to the power `n` elements and every ordered
/// `n`-tuple appears exactly once when it is read as a cycle.
pub fn de_bruijn(k: usize, n: usize) -> Vec<usize> {
    assert!(k > 0 && n > 0, "an alphabet of {k} and an order of {n}");
    let mut state = Lyndon {
        k,
        n,
        a: vec![0; k * n + 1],
        out: Vec::with_capacity(k.pow(n as u32)),
    };
    state.walk(1, 1);
    state.out
}

/// The recursion the construction is written as, with the working array
/// and the output it fills.
struct Lyndon {
    k: usize,
    n: usize,
    a: Vec<usize>,
    out: Vec<usize>,
}

impl Lyndon {
    /// Extend the necklace at position `t` whose longest proper prefix
    /// repeats every `p`, emitting the Lyndon words as they close.
    fn walk(&mut self, t: usize, p: usize) {
        if t > self.n {
            if self.n.is_multiple_of(p) {
                self.out.extend_from_slice(&self.a[1..=p]);
            }
            return;
        }
        self.a[t] = self.a[t - p];
        self.walk(t + 1, p);
        for j in self.a[t - p] + 1..self.k {
            self.a[t] = j;
            self.walk(t + 1, t);
        }
    }
}

/// The sweep string: the cyclic sequence written out with its first
/// `ORDER - 1` characters repeated at the end, so every ordered pair
/// appears once as a contiguous substring of a plain string rather than
/// of a cycle.
pub fn text() -> String {
    let letters = alphabet();
    let sequence = de_bruijn(letters.len(), ORDER);
    sequence
        .iter()
        .chain(sequence.iter().take(ORDER - 1))
        .map(|i| letters[*i])
        .collect()
}

/// Where a character sits in a table of pairs.
pub fn index(c: char) -> Option<usize> {
    (FIRST..=LAST)
        .contains(&c)
        .then(|| c as usize - FIRST as usize)
}

/// How the browser was asked to lay one run out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shaping {
    /// The browser's default: what the site gets.
    Shaped,
    /// Kerning on, ligatures and contextual alternates off.
    Kern,
    /// Every shaping feature off: the advance table's own layout.
    Flat,
}

impl Shaping {
    /// The name this run takes in the page and in the JSON.
    pub fn name(self) -> &'static str {
        match self {
            Shaping::Shaped => "shaped",
            Shaping::Kern => "kern",
            Shaping::Flat => "flat",
        }
    }

    /// The CSS that puts a run in this state.
    fn css(self) -> &'static str {
        match self {
            Shaping::Shaped => "",
            Shaping::Kern => {
                "font-kerning: normal; font-variant-ligatures: none; \
                              font-feature-settings: \"liga\" 0, \"clig\" 0, \"calt\" 0, \"rlig\" 0;"
            }
            Shaping::Flat => {
                "font-kerning: none; font-variant-ligatures: none; \
                              font-feature-settings: \"kern\" 0, \"liga\" 0, \"clig\" 0, \"calt\" 0, \"rlig\" 0;"
            }
        }
    }

    /// All three, as the page draws them.
    pub fn all() -> [Shaping; 3] {
        [Shaping::Shaped, Shaping::Kern, Shaping::Flat]
    }
}

/// What one run of the sweep is called in the DOM and in the JSON.
pub fn run_id(face: Face, shaping: Shaping) -> String {
    format!(
        "sweep-{}-{}",
        crate::specimens::face_name(face),
        shaping.name()
    )
}

/// Both faces the chart draws with, in the order the JSON lists them.
pub fn faces() -> [Face; 2] {
    [Face::Regular, Face::Bold]
}

/// The sweep page: one SVG text node per face and shaping, each holding
/// the whole sequence at the size the chart draws.
///
/// Whitespace is held twice over. The space is a character of the
/// alphabet like any other, and a run that collapsed its spaces would
/// shift every position after the first one, so the text carries
/// `xml:space="preserve"` and the stylesheet sets `white-space: pre`.
/// The flat control catches it either way: a collapsed space would put
/// its step at nought where the table says 2.832 px.
pub fn page(sweep: &str) -> String {
    let mut out = String::with_capacity(sweep.len() * 8);
    out.push_str(&format!(
        "<!DOCTYPE html>\n<html lang=\"en-GB\">\n<head>\n\
         <meta charset=\"utf-8\">\n<title>Kerning sweep</title>\n\
         <link rel=\"stylesheet\" href=\"/fonts.css\">\n\
         <link rel=\"stylesheet\" href=\"/theme.css\">\n<style>\n\
         body {{ margin: 0; background: var(--op-bg); color: var(--op-text); }}\n\
         svg {{ display: block; overflow: visible; }}\n\
         svg text {{ font-family: var(--op-font-sans); font-size: {TEXT_PX}px; \
         font-synthesis: none; white-space: pre; fill: var(--op-text); }}\n\
         </style>\n</head>\n<body>\n"
    ));
    let escaped = op_chart::escape(sweep);
    for face in faces() {
        for shaping in Shaping::all() {
            let weight = match face {
                Face::Regular => "400",
                Face::Bold => "700",
            };
            out.push_str(&format!(
                "<svg width=\"400\" height=\"20\"><text id=\"{}\" x=\"0\" y=\"15\" \
                 xml:space=\"preserve\" style=\"font-weight: {weight}; {}\">{escaped}</text></svg>\n",
                run_id(face, shaping),
                shaping.css()
            ));
        }
    }
    out.push_str("</body>\n</html>\n");
    out
}

/// What the capture step needs: the sequence, the runs to read, and the
/// advance the table gives every character of the sequence, so a run
/// that came back wrong can be named before anything is measured.
pub fn manifest(sweep: &str) -> String {
    let mut out = String::new();
    out.push_str("{\n");
    out.push_str(&format!(
        " \"order\": {ORDER},\n \"alphabet\": {},\n \"pairs\": {},\n \"length\": {},\n \"text_px\": {TEXT_PX},\n",
        COUNT,
        COUNT * COUNT,
        sweep.chars().count()
    ));
    out.push_str(" \"runs\": [");
    let mut first = true;
    for face in faces() {
        for shaping in Shaping::all() {
            out.push_str(&format!(
                "{}\n  {{\"id\": {:?}, \"face\": {:?}, \"shaping\": {:?}}}",
                if first { "" } else { "," },
                run_id(face, shaping),
                crate::specimens::face_name(face),
                shaping.name()
            ));
            first = false;
        }
    }
    out.push_str("\n ],\n");
    out.push_str(&format!(" \"sequence\": {:?}\n}}\n", sweep));
    out
}

/// The advance the table gives one character at the size the chart
/// draws, in CSS px.
pub fn advance(c: char, face: Face) -> f64 {
    text_width(&c.to_string(), TEXT_PX, face)
}

/// The full em at the size the chart draws, which is what op-chart's
/// measurement charges a character its tables do not cover. Nothing in
/// the sweep is outside that block, so a step this wide would mean the
/// sequence and the tables had drifted apart.
pub fn em() -> f64 {
    TEXT_PX
}

// ---- reading the sweep back ------------------------------------------

/// The finest difference the measurement can tell from nothing.
///
/// Chrome reports a character's position on a grid of a sixty-fourth of a
/// CSS pixel, so a step between two reported positions carries up to a
/// hundred and twenty-eighth either side of the truth. A sixty-fourth is
/// the first magnitude that cannot be that rounding, and the flat control
/// bears it out: over all 9025 steps of both faces no residual against
/// the advance table exceeds 0.00775 px, which is a hundred and twenty
/// eighth to the last digit.
pub const FLOOR: f64 = 1.0 / 64.0;

/// The characters the chart can put in a label without an author's help,
/// and the ones an author is most likely to use: the digits, the full
/// stop, the colon, the percent sign, the space, and the Latin letters.
/// The emitter itself writes only digits, the full stop, the minus sign
/// and the letter s; everything else here is what a label can hold.
pub const DRAWABLE: &str = " .:%0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

/// One run as the browser laid it out.
#[derive(Clone, Debug, PartialEq)]
pub struct Run {
    pub id: String,
    /// Where every character starts, in CSS px from the text's origin.
    pub x: Vec<f64>,
    /// The extent of every character, for telling a character that is
    /// its own glyph from one that shares a glyph with its neighbour.
    pub width: Vec<f64>,
    pub total: f64,
}

/// A whole capture of the sweep.
#[derive(Clone, Debug, PartialEq)]
pub struct Capture {
    pub browser: String,
    pub binary: String,
    pub length: usize,
    pub runs: Vec<Run>,
}

impl Capture {
    /// One run by the name the page gave it.
    pub fn run(&self, face: Face, shaping: Shaping) -> Result<&Run, String> {
        let id = run_id(face, shaping);
        self.runs
            .iter()
            .find(|r| r.id == id)
            .ok_or_else(|| format!("the capture has no run called {id}"))
    }
}

/// Read the JSON the sweep capture wrote, with op-chart's own reader.
pub fn read_capture(path: &std::path::Path) -> Result<Capture, String> {
    use op_chart::data::json::{Value, parse};
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
    let numbers = |v: &Value, key: &str| -> Result<Vec<f64>, String> {
        match at(v, key)? {
            Value::Array(items) => items
                .iter()
                .map(|i| match i {
                    Value::Number(n) => Ok(*n),
                    other => Err(format!("{}: {key} holds {}", path.display(), other.kind())),
                })
                .collect(),
            other => Err(format!("{}: {key} is {}", path.display(), other.kind())),
        }
    };
    let Value::Object(runs) = at(&value, "runs")? else {
        return Err(format!("{}: runs is not an object", path.display()));
    };
    let mut read = Vec::with_capacity(runs.len());
    for (id, run) in &runs {
        read.push(Run {
            id: id.clone(),
            x: numbers(run, "x")?,
            width: numbers(run, "width")?,
            total: number(run, "total")?,
        });
    }
    Ok(Capture {
        browser: string(&value, "browser")?,
        binary: string(&value, "binary")?,
        length: number(&value, "length")? as usize,
        runs: read,
    })
}

// ---- what the sweep came to -------------------------------------------

/// One face, swept.
#[derive(Clone, Debug, PartialEq)]
pub struct Swept {
    pub face: Face,
    /// The kern of every ordered pair, indexed `a * COUNT + b`, in CSS
    /// px, from the run with kerning on and everything else off.
    pub kern: Vec<f64>,
    /// What the default layout did that pairwise kerning does not
    /// explain, same indexing: a ligature, or an alternate reading
    /// further than a pair.
    pub context: Vec<f64>,
    /// The largest the flat control strayed from the advance table over
    /// all 9025 steps, which is the table's own exhaustive check.
    pub table_worst: f64,
    /// How many of those steps strayed past [`FLOOR`].
    pub table_off: usize,
    /// How many characters reported an extent that did not match the
    /// step to the next one, which is a character that is not a glyph of
    /// its own.
    pub extent_off: usize,
}

impl Swept {
    /// The kern of one ordered pair, or nought for a pair off the block.
    pub fn of(&self, a: char, b: char) -> f64 {
        match (index(a), index(b)) {
            (Some(a), Some(b)) => self.kern[a * COUNT + b],
            _ => 0.0,
        }
    }

    /// What the default layout added to that pair beyond its kern.
    pub fn context_of(&self, a: char, b: char) -> f64 {
        match (index(a), index(b)) {
            (Some(a), Some(b)) => self.context[a * COUNT + b],
            _ => 0.0,
        }
    }

    /// Every pair that moved past [`FLOOR`], largest first.
    pub fn kerned(&self, over: &str) -> Vec<(char, char, f64)> {
        let mut out = Vec::new();
        for a in over.chars() {
            for b in over.chars() {
                let k = self.of(a, b);
                if k.abs() > FLOOR {
                    out.push((a, b, k));
                }
            }
        }
        out.sort_by(|p, q| q.2.abs().total_cmp(&p.2.abs()));
        out
    }

    /// Every step the default layout laid out differently from pairwise
    /// kerning: a ligature or a contextual alternate.
    pub fn contextual(&self) -> Vec<(char, char, f64)> {
        let mut out = Vec::new();
        for a in alphabet() {
            for b in alphabet() {
                let c = self.context_of(a, b);
                if c.abs() > FLOOR {
                    out.push((a, b, c));
                }
            }
        }
        out.sort_by(|p, q| q.2.abs().total_cmp(&p.2.abs()));
        out
    }

    /// The error the advance table makes on one string: the sum of the
    /// kerns of its consecutive pairs, which is exactly how much wider
    /// the sum is than the browser's own layout.
    pub fn error(&self, text: &str) -> f64 {
        let chars: Vec<char> = text.chars().collect();
        chars.windows(2).map(|p| self.of(p[0], p[1])).sum()
    }

    /// The same with whatever the default layout does on top, which is
    /// what the site actually gets.
    pub fn error_shaped(&self, text: &str) -> f64 {
        let chars: Vec<char> = text.chars().collect();
        chars
            .windows(2)
            .map(|p| self.of(p[0], p[1]) + self.context_of(p[0], p[1]))
            .sum()
    }

    /// The worst error per character a string over `over` can accumulate,
    /// and the pair that does it: the tightest two-cycle, since a string
    /// that alternates two characters pays each of the pair's two kerns
    /// once per two characters, and a repeated character pays its own
    /// kern every time.
    pub fn worst_rate(&self, over: &str) -> Option<(char, char, f64)> {
        let mut worst: Option<(char, char, f64)> = None;
        for a in over.chars() {
            for b in over.chars() {
                let rate = (self.of(a, b) + self.of(b, a)) / 2.0;
                if worst.is_none_or(|(_, _, w)| rate < w) {
                    worst = Some((a, b, rate));
                }
            }
        }
        worst
    }
}

/// Turn a capture into the kerns of both faces.
pub fn analyse(sweep: &str, capture: &Capture) -> Result<Vec<Swept>, String> {
    let chars: Vec<char> = sweep.chars().collect();
    if chars.len() != capture.length {
        return Err(format!(
            "the capture holds {} characters where the sweep is {}",
            capture.length,
            chars.len()
        ));
    }
    let mut out = Vec::with_capacity(faces().len());
    for face in faces() {
        let flat = capture.run(face, Shaping::Flat)?;
        let kern_run = capture.run(face, Shaping::Kern)?;
        let shaped = capture.run(face, Shaping::Shaped)?;
        for run in [flat, kern_run, shaped] {
            if run.x.len() != chars.len() || run.width.len() != chars.len() {
                return Err(format!(
                    "{} laid out {} characters where the sweep is {}",
                    run.id,
                    run.x.len(),
                    chars.len()
                ));
            }
        }
        let mut kern = vec![0.0; COUNT * COUNT];
        let mut context = vec![0.0; COUNT * COUNT];
        let mut table_worst = 0.0_f64;
        let mut table_off = 0;
        let mut extent_off = 0;
        for i in 0..chars.len() - 1 {
            let (a, b) = (chars[i], chars[i + 1]);
            let (ai, bi) = match (index(a), index(b)) {
                (Some(ai), Some(bi)) => (ai, bi),
                _ => {
                    return Err(format!(
                        "{a:?}{b:?} at {i} is off the block the tables cover"
                    ));
                }
            };
            let nominal = advance(a, face);
            let residual = (flat.x[i + 1] - flat.x[i]) - nominal;
            table_worst = table_worst.max(residual.abs());
            if residual.abs() > FLOOR {
                table_off += 1;
            }
            let step = kern_run.x[i + 1] - kern_run.x[i];
            kern[ai * COUNT + bi] = step - nominal;
            context[ai * COUNT + bi] = (shaped.x[i + 1] - shaped.x[i]) - step;
            if (shaped.width[i] - (shaped.x[i + 1] - shaped.x[i])).abs() > FLOOR {
                extent_off += 1;
            }
        }
        out.push(Swept {
            face,
            kern,
            context,
            table_worst,
            table_off,
            extent_off,
        });
    }
    Ok(out)
}

// ---- the verdict ------------------------------------------------------

/// The buckets the kerns are counted into, in CSS px.
const BUCKETS: [f64; 5] = [0.25, 0.5, 0.75, 1.0, 1.5];

/// A pair as it is written in a line of prose: the two characters in
/// one pair of quotes, escaped, so a comma or a quote reads as itself
/// rather than as two quoted characters run together.
fn pair(a: char, b: char) -> String {
    format!("{:?}", [a, b].iter().collect::<String>())
}

/// What the sweep found, in the lines the run prints and the JSON keeps.
///
/// Read in order the lines say: the advance table is exact for every
/// character in both faces; kerning moves a minority of pairs but moves
/// some of them by more than a pixel; only one pair in the whole block
/// ligates; and the strings the chart actually draws land where the sum
/// says to within a fraction of a pixel.
pub fn verdict(sweep: &str, swept: &[Swept], drawn: &[(String, String)]) -> Vec<String> {
    let mut lines = Vec::new();
    lines.push(format!(
        "A De Bruijn sweep of order {ORDER} over the {COUNT} characters op-chart's tables cover: \
         {} ordered pairs, every one of them once, in {} characters laid out {} times per face.",
        COUNT * COUNT,
        sweep.chars().count(),
        Shaping::all().len()
    ));
    for face in swept {
        let name = crate::specimens::face_name(face.face);
        lines.push(format!("Plex Sans {name}:"));
        lines.push(format!(
            "  The advance table is exact. Over all {} steps the flat run strayed at most {:.5} px from \
             the table, which is the {:.5} px the engine's own sixty-fourth-pixel grid can round by, and \
             {} steps strayed past a sixty-fourth.",
            COUNT * COUNT,
            face.table_worst,
            1.0 / 128.0,
            face.table_off
        ));
        let kerned = face.kerned(&alphabet().iter().collect::<String>());
        lines.push(format!(
            "  {} of {} pairs kern past a sixty-fourth of a pixel ({:.1}%); {} do not move at all.",
            kerned.len(),
            COUNT * COUNT,
            kerned.len() as f64 / (COUNT * COUNT) as f64 * 100.0,
            COUNT * COUNT - kerned.len()
        ));
        let tighter = kerned.iter().filter(|(_, _, k)| *k < 0.0).count();
        let mut counts = [0usize; BUCKETS.len() + 1];
        for (_, _, k) in &kerned {
            let at = BUCKETS
                .iter()
                .position(|b| k.abs() <= *b)
                .unwrap_or(BUCKETS.len());
            counts[at] += 1;
        }
        let mut spread = Vec::new();
        let mut low = FLOOR;
        for (i, b) in BUCKETS.iter().enumerate() {
            spread.push(format!("{low:.2} to {b:.2}: {}", counts[i]));
            low = *b;
        }
        spread.push(format!(
            "past {:.2}: {}",
            BUCKETS[BUCKETS.len() - 1],
            counts[BUCKETS.len()]
        ));
        lines.push(format!(
            "  {tighter} of them tighten and {} loosen. By size in px, {}.",
            kerned.len() - tighter,
            spread.join("; ")
        ));
        let ten: Vec<String> = kerned
            .iter()
            .take(10)
            .map(|(a, b, k)| format!("{} {k:+.3}", pair(*a, *b)))
            .collect();
        lines.push(format!("  Ten largest: {}.", ten.join(", ")));
        let drawable = face.kerned(DRAWABLE);
        if let Some((a, b, k)) = drawable.first() {
            lines.push(format!(
                "  Over the {} characters a label is likely to hold (digits, full stop, colon, percent, \
                 space, Latin letters), {} of {} pairs kern and the largest is {} at {k:+.3} px.",
                DRAWABLE.chars().count(),
                drawable.len(),
                DRAWABLE.chars().count() * DRAWABLE.chars().count(),
                pair(*a, *b)
            ));
        }
        // the minus sign is the one character outside that set the
        // emitter can write by itself, on a negative gridline value
        let minus = face.kerned(&format!("{DRAWABLE}-"));
        if let Some((a, b, k)) = minus.iter().find(|(a, b, _)| *a == '-' || *b == '-') {
            lines.push(format!(
                "  The emitter can also write a minus sign, on a negative gridline value; its largest kern \
                 there is {} at {k:+.3} px.",
                pair(*a, *b)
            ));
        }
        if let Some((a, b, rate)) = face.worst_rate(&alphabet().iter().collect::<String>()) {
            lines.push(format!(
                "  Worst accumulation: {} repeats at {rate:.3} px a character, so a string of n characters \
                 can fall short of its advance sum by {:.2} px at n = 4, {:.2} at n = 8 and {:.2} at n = 16.",
                pair(a, b),
                rate.abs() * 3.0,
                rate.abs() * 7.0,
                rate.abs() * 15.0
            ));
        }
        if let Some((a, b, rate)) = face.worst_rate(DRAWABLE) {
            lines.push(format!(
                "  Over the likely characters the worst is {} at {rate:.3} px a character: {:.2} px at n = 4, \
                 {:.2} at n = 8, {:.2} at n = 16.",
                pair(a, b),
                rate.abs() * 3.0,
                rate.abs() * 7.0,
                rate.abs() * 15.0
            ));
        }
        let contextual = face.contextual();
        if contextual.is_empty() {
            lines.push(
                "  Nothing in the block ligates or takes a contextual alternate: the default layout is \
                 pairwise kerning and nothing else."
                    .to_owned(),
            );
        } else {
            let named: Vec<String> = contextual
                .iter()
                .map(|(a, b, c)| format!("{} {c:+.3}", pair(*a, *b)))
                .collect();
            let net: f64 = contextual.iter().map(|(_, _, c)| c).sum();
            lines.push(format!(
                "  {} came out differently under the default layout than under pairwise kerning: {}. That is one ligature, whose advance the engine splits across its two characters, netting {net:+.3} px.",
                if contextual.len() == 1 { "One step".to_owned() } else { format!("{} steps", contextual.len()) },
                named.join(", ")
            ));
        }
        lines.push(format!(
            "  {} characters reported an extent that did not match the step to the next one, so every \
             character of the sweep is a glyph of its own and no position was averaged over a run.",
            face.extent_off
        ));
    }
    if !drawn.is_empty() {
        lines.push("Against the strings the site's own charts draw:".to_owned());
        let mut worst: Option<(String, &'static str, f64)> = None;
        for (name, text) in drawn {
            for face in swept {
                let e = face.error_shaped(text);
                let further = worst.as_ref().is_none_or(|(_, _, w)| e.abs() > w.abs());
                if further {
                    worst = Some((name.clone(), crate::specimens::face_name(face.face), e));
                }
            }
        }
        if let Some((name, face, e)) = worst {
            lines.push(format!(
                "  The advance table's worst error over all {} of them in either face is {e:+.3} px, on \
                 {name:?} in {face}. Every other string is nearer than that.",
                drawn.len()
            ));
        }
        let over = drawn
            .iter()
            .filter(|(_, t)| swept.iter().any(|f| f.error_shaped(t).abs() > 1.0))
            .count();
        lines.push(if over == 0 {
            format!(
                "  None of the {} is more than a pixel out in either face: the sum of advances is the layout to within a pixel for every string the chart draws today.",
                drawn.len()
            )
        } else {
            format!(
                "  {over} of the {} are more than a pixel out in one face or the other, so the sum is not the layout to within a pixel for everything the chart draws.",
                drawn.len()
            )
        });
    }
    // what the sweep does not settle, said plainly
    let tightest = swept
        .iter()
        .filter_map(|f| f.worst_rate(DRAWABLE))
        .map(|(_, _, r)| r)
        .fold(0.0_f64, f64::min);
    lines.push(format!(
        "It does not follow for every string the chart could draw. Over the likely characters a string loses up to {:.3} px a character, so four characters can be {:.2} px short of their sum and eight can be {:.2}. The sum is a bound, never a shortfall: it is always the wider number, so a label placed by it is reserved generously rather than crowded.",
        tightest.abs(),
        tightest.abs() * 3.0,
        tightest.abs() * 7.0
    ));
    lines.push(format!(
        "Order {ORDER} settles pairs and nothing else. Order 3 would be {} characters, still one layout per face, and would reach the three-character substitutions this cannot: it covers all {} ordered triples.",
        COUNT * COUNT * COUNT + 2,
        COUNT * COUNT * COUNT
    ));
    let ligatures: usize = swept.iter().map(|f| f.contextual().len()).sum();
    lines.push(if ligatures == 0 {
        "Nothing found here argues for it: no pair in either face ligated or took an alternate, so there is no evidence of context being read at all.".to_owned()
    } else {
        "Worth doing once, on this evidence but not urgently: the only context either face read over a pair is the fi ligature, and Plex Sans also carries ffi and ffl, which need three characters and so are invisible to an order-two sweep. Neither can appear in any string the chart draws today, and both would tighten rather than loosen, so the sum would stay the safe bound it is.".to_owned()
    });
    lines
}

/// Every kern the sweep found, as JSON. Only the pairs that moved are
/// listed: the rest are nought, and a table of 9025 zeroes is not
/// evidence, it is noise.
pub fn measured_json(sweep: &str, capture: &Capture, swept: &[Swept], lines: &[String]) -> String {
    let quoted = |s: &str| format!("{s:?}");
    let mut out = String::new();
    out.push_str("{\n");
    out.push_str(&format!(
        " \"order\": {ORDER},\n \"alphabet\": {COUNT},\n \"pairs\": {},\n \"length\": {},\n \"text_px\": {TEXT_PX},\n \"floor\": {FLOOR},\n",
        COUNT * COUNT,
        sweep.chars().count()
    ));
    out.push_str(&format!(
        " \"browser\": {},\n \"binary\": {},\n",
        quoted(&capture.browser),
        quoted(&capture.binary)
    ));
    out.push_str(" \"verdict\": [\n");
    for (i, line) in lines.iter().enumerate() {
        out.push_str(&format!(
            "  {}{}\n",
            quoted(line),
            if i + 1 == lines.len() { "" } else { "," }
        ));
    }
    out.push_str(" ],\n \"faces\": [\n");
    for (n, face) in swept.iter().enumerate() {
        let kerned = face.kerned(&alphabet().iter().collect::<String>());
        let contextual = face.contextual();
        out.push_str(&format!(
            "  {{\"face\": {}, \"table_worst\": {:.6}, \"table_off\": {}, \"extent_off\": {},\n",
            quoted(crate::specimens::face_name(face.face)),
            face.table_worst,
            face.table_off,
            face.extent_off
        ));
        out.push_str(&format!("   \"kerned\": {}, \"kerns\": [\n", kerned.len()));
        for (i, (a, b, k)) in kerned.iter().enumerate() {
            out.push_str(&format!(
                "    [{}, {}, {k:.5}]{}\n",
                quoted(&a.to_string()),
                quoted(&b.to_string()),
                if i + 1 == kerned.len() { "" } else { "," }
            ));
        }
        out.push_str("   ],\n   \"contextual\": [\n");
        for (i, (a, b, c)) in contextual.iter().enumerate() {
            out.push_str(&format!(
                "    [{}, {}, {c:.5}]{}\n",
                quoted(&a.to_string()),
                quoted(&b.to_string()),
                if i + 1 == contextual.len() { "" } else { "," }
            ));
        }
        out.push_str(&format!(
            "   ]}}{}\n",
            if n + 1 == swept.len() { "" } else { "," }
        ));
    }
    out.push_str(" ]\n}\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The construction reproduces the sequence the literature gives for
    /// the smallest interesting case: B(2, 3) is 00010111, the
    /// concatenation of the binary Lyndon words whose length divides 3
    /// in lexicographic order (0, 001, 011, 1).
    #[test]
    fn the_construction_gives_the_published_sequence() {
        assert_eq!(de_bruijn(2, 3), vec![0, 0, 0, 1, 0, 1, 1, 1]);
        assert_eq!(de_bruijn(2, 2), vec![0, 0, 1, 1]);
        assert_eq!(de_bruijn(2, 1), vec![0, 1]);
        assert_eq!(de_bruijn(3, 1), vec![0, 1, 2]);
        // an alphabet of one has one sequence of one element whatever
        // the order, since 1 to the power n is 1 and that one element,
        // read as a cycle, is every n-tuple there is
        assert_eq!(de_bruijn(1, 4), vec![0]);
    }

    /// Whatever the alphabet and the order, the sequence has k to the n
    /// elements and every ordered n-tuple appears exactly once when it
    /// is read as a cycle. Checked over enough shapes that a
    /// construction that happened to work for pairs would not pass.
    #[test]
    fn every_tuple_appears_exactly_once() {
        for (k, n) in [
            (2, 2),
            (2, 3),
            (2, 5),
            (3, 2),
            (3, 3),
            (4, 2),
            (5, 3),
            (7, 2),
        ] {
            let sequence = de_bruijn(k, n);
            assert_eq!(
                sequence.len(),
                k.pow(n as u32),
                "B({k}, {n}) is the wrong length"
            );
            let mut seen = vec![0usize; k.pow(n as u32)];
            for start in 0..sequence.len() {
                let mut at = 0;
                for step in 0..n {
                    at = at * k + sequence[(start + step) % sequence.len()];
                }
                seen[at] += 1;
            }
            assert!(
                seen.iter().all(|c| *c == 1),
                "B({k}, {n}) covers {} tuples of {}",
                seen.iter().filter(|c| **c == 1).count(),
                seen.len()
            );
        }
    }

    /// The sweep string runs over exactly the block op-chart's tables
    /// cover, is one character longer than the cycle, and holds every
    /// ordered pair once as a contiguous substring. If op-chart ever
    /// covered a different block this would fail rather than sweep a
    /// different alphabet than the one being tested.
    #[test]
    fn the_sweep_covers_every_ordered_pair_of_the_tables_own_block() {
        let letters = alphabet();
        assert_eq!(letters.len(), COUNT);
        assert_eq!(letters.first(), Some(&FIRST));
        assert_eq!(letters.last(), Some(&LAST));
        assert_eq!(COUNT, 95, "the printable ASCII block");
        let sweep = text();
        let chars: Vec<char> = sweep.chars().collect();
        assert_eq!(chars.len(), COUNT * COUNT + ORDER - 1);
        assert_eq!(chars.len(), 9026);
        let mut seen = vec![0usize; COUNT * COUNT];
        for pair in chars.windows(2) {
            let a = index(pair[0]).unwrap_or_else(|| panic!("{:?} is off the block", pair[0]));
            let b = index(pair[1]).unwrap_or_else(|| panic!("{:?} is off the block", pair[1]));
            seen[a * COUNT + b] += 1;
        }
        let missing: Vec<usize> = seen
            .iter()
            .enumerate()
            .filter(|(_, c)| **c != 1)
            .map(|(i, _)| i)
            .collect();
        assert!(
            missing.is_empty(),
            "{} of {} pairs are not covered exactly once",
            missing.len(),
            seen.len()
        );
        // the space is a character of the alphabet like any other, and
        // the sequence opens on a run of them, which is the case a page
        // that collapsed its whitespace would lose first
        assert!(
            sweep.starts_with("  "),
            "the sweep opens on {:?}",
            &sweep[..4]
        );
    }

    /// Every character of the sweep has an advance in both faces, so no
    /// step of the measurement is charged the full em of a character the
    /// tables cannot measure.
    #[test]
    fn every_character_of_the_sweep_is_measurable() {
        for face in faces() {
            for c in alphabet() {
                let a = advance(c, face);
                assert!(a > 0.0 && a < em(), "{c:?} advances {a} in {face:?}");
            }
        }
    }

    /// The page carries one run per face and shaping, each holding the
    /// whole sequence with its whitespace preserved, and the flat run
    /// turns off the kerning the sweep is looking for.
    #[test]
    fn the_page_lays_the_sweep_out_once_per_face_and_shaping() {
        let sweep = text();
        let html = page(&sweep);
        assert_eq!(
            html.matches("<text ").count(),
            faces().len() * Shaping::all().len()
        );
        assert_eq!(html.matches("xml:space=\"preserve\"").count(), 6);
        assert_eq!(html.matches("white-space: pre").count(), 1);
        for face in faces() {
            for shaping in Shaping::all() {
                assert!(html.contains(&format!("id=\"{}\"", run_id(face, shaping))));
            }
        }
        assert_eq!(
            html.matches("\"kern\" 0").count(),
            2,
            "one flat run per face"
        );
        assert_eq!(html.matches("font-weight: 700").count(), 3);
        // the sequence goes in once per run, escaped for markup
        assert_eq!(
            html.matches("&amp;").count(),
            6 * sweep.matches('&').count()
        );
        assert!(!html.contains("&&"));
    }

    /// The manifest is JSON and says what the page holds.
    #[test]
    fn the_manifest_parses_and_carries_the_sequence() {
        use op_chart::data::json::{Value, parse};
        let sweep = text();
        let text_json = manifest(&sweep);
        let Value::Object(fields) = parse(&text_json).expect("the manifest is JSON") else {
            panic!("the manifest is an object");
        };
        let get = |k: &str| fields.iter().find(|(n, _)| n == k).map(|(_, v)| v);
        assert_eq!(get("pairs"), Some(&Value::Number((COUNT * COUNT) as f64)));
        assert_eq!(get("length"), Some(&Value::Number(9026.0)));
        assert_eq!(get("sequence"), Some(&Value::String(sweep.clone())));
        let Some(Value::Array(runs)) = get("runs") else {
            panic!("the manifest lists its runs");
        };
        assert_eq!(runs.len(), 6);
    }

    /// A capture built from a kern table chosen here, so what comes back
    /// out of [`analyse`] can be held to what went in.
    fn told(
        sweep: &str,
        kern: impl Fn(char, char, Face) -> f64,
        context: impl Fn(char, char) -> f64,
    ) -> Capture {
        let chars: Vec<char> = sweep.chars().collect();
        let mut runs = Vec::new();
        for face in faces() {
            for shaping in Shaping::all() {
                let mut x = Vec::with_capacity(chars.len());
                let mut pen = 0.0;
                for i in 0..chars.len() {
                    x.push(pen);
                    if i + 1 < chars.len() {
                        pen += advance(chars[i], face)
                            + match shaping {
                                Shaping::Flat => 0.0,
                                Shaping::Kern => kern(chars[i], chars[i + 1], face),
                                Shaping::Shaped => {
                                    kern(chars[i], chars[i + 1], face)
                                        + context(chars[i], chars[i + 1])
                                }
                            };
                    }
                }
                let width: Vec<f64> = (0..chars.len())
                    .map(|i| {
                        if i + 1 < chars.len() {
                            x[i + 1] - x[i]
                        } else {
                            0.0
                        }
                    })
                    .collect();
                runs.push(Run {
                    id: run_id(face, shaping),
                    total: *x.last().expect("a position"),
                    x,
                    width,
                });
            }
        }
        Capture {
            browser: "test".to_owned(),
            binary: "test".to_owned(),
            length: chars.len(),
            runs,
        }
    }

    /// Every kern that went in comes back out, for all 9025 pairs and
    /// both faces, and the flat run reads as an exact advance table. A
    /// sweep that recovered its pairs off by one, or that read the wrong
    /// run for a face, would fail here rather than report a face's
    /// kerning as another's.
    #[test]
    fn the_sweep_recovers_every_kern_it_was_given() {
        let sweep = text();
        // a kern that depends on both characters and the face, so a swap
        // of either would show, and that is not a round number of the
        // engine's grid
        let kern = |a: char, b: char, face: Face| {
            let n =
                (index(a).expect("on the block") * 31 + index(b).expect("on the block") * 7) % 23;
            let sign = if n.is_multiple_of(2) { -1.0 } else { 1.0 };
            sign * (n as f64) / 32.0 * if face == Face::Bold { 1.5 } else { 1.0 }
        };
        let context = |a: char, b: char| if (a, b) == ('f', 'i') { -0.5 } else { 0.0 };
        let capture = told(&sweep, kern, context);
        let swept = analyse(&sweep, &capture).expect("the sweep analyses");
        assert_eq!(swept.len(), 2);
        for face in &swept {
            // the positions are accumulated in floating point over nine
            // thousand steps, so the residual is that arithmetic and not
            // a disagreement with the table
            assert!(
                face.table_worst < 1e-9,
                "the flat run is the table, out by {}",
                face.table_worst
            );
            assert_eq!(face.table_off, 0);
            assert_eq!(face.extent_off, 0);
            for a in alphabet() {
                for b in alphabet() {
                    let want = kern(a, b, face.face);
                    assert!(
                        (face.of(a, b) - want).abs() < 1e-9,
                        "{a:?}{b:?} in {:?} came back {} where it went in {want}",
                        face.face,
                        face.of(a, b)
                    );
                }
            }
            // the one contextual pair is kept apart from the kerns
            assert_eq!(face.contextual().len(), 1);
            let (a, b, c) = face.contextual()[0];
            assert_eq!((a, b), ('f', 'i'));
            assert!((c + 0.5).abs() < 1e-9);
            assert!((face.context_of('f', 'i') + 0.5).abs() < 1e-9);
            assert_eq!(face.context_of('f', 'l'), 0.0);
        }
        // the two faces differ, so neither was read for the other
        assert!(swept[0].of('A', 'V') != swept[1].of('A', 'V'));
    }

    /// A string's error is the sum of the kerns of its own pairs, and
    /// nothing else: the sweep's whole claim is that placing a label by
    /// the advance sum is wrong by exactly that much.
    #[test]
    fn a_string_is_wrong_by_the_sum_of_its_own_kerns() {
        let sweep = text();
        let kern = |a: char, _b: char, _f: Face| if a == 'A' { -0.75 } else { 0.0 };
        let capture = told(&sweep, kern, |_, _| 0.0);
        let swept = analyse(&sweep, &capture).expect("the sweep analyses");
        let face = &swept[0];
        assert!((face.error("AAAA") + 2.25).abs() < 1e-9, "three pairs of A");
        assert!(
            (face.error("A") - 0.0).abs() < 1e-9,
            "one character has no pair"
        );
        assert_eq!(face.error(""), 0.0);
        assert!(
            (face.error("BAB") + 0.75).abs() < 1e-9,
            "only the pair starting in A"
        );
        // a character off the block costs nothing rather than panicking,
        // since the tables cannot speak for it either way
        assert_eq!(face.of('\u{2014}', 'A'), 0.0);
        assert_eq!(face.error("\u{2014}\u{2014}"), 0.0);
        // the worst rate is the tightest two-cycle, which here is A with
        // itself since only a pair starting in A moves
        let (a, b, rate) = face.worst_rate(DRAWABLE).expect("a worst pair");
        assert_eq!((a, b), ('A', 'A'));
        assert!((rate + 0.75).abs() < 1e-9);
    }

    /// A capture that does not match the sweep is refused rather than
    /// measured against the wrong characters, and a missing run is named.
    #[test]
    fn a_capture_that_does_not_match_the_sweep_is_refused() {
        let sweep = text();
        let mut capture = told(&sweep, |_, _, _| 0.0, |_, _| 0.0);
        capture.length = 10;
        let e = analyse(&sweep, &capture).expect_err("the length is wrong");
        assert!(e.contains("10 characters where the sweep is 9026"), "{e}");
        let mut short = told(&sweep, |_, _, _| 0.0, |_, _| 0.0);
        short.runs[0].x.truncate(50);
        let e = analyse(&sweep, &short).expect_err("a run is short");
        assert!(e.contains("laid out 50 characters"), "{e}");
        let mut missing = told(&sweep, |_, _, _| 0.0, |_, _| 0.0);
        missing
            .runs
            .retain(|r| r.id != run_id(Face::Bold, Shaping::Kern));
        let e = analyse(&sweep, &missing).expect_err("a run is missing");
        assert!(e.contains("sweep-700-kern"), "{e}");
    }

    /// A character whose extent does not match its step is counted
    /// rather than folded into the kerns, which is the guard against an
    /// engine that reports positions for a run it did not lay out one
    /// glyph to a character.
    #[test]
    fn a_character_that_is_not_its_own_glyph_is_counted() {
        let sweep = text();
        let mut capture = told(&sweep, |_, _, _| 0.0, |_, _| 0.0);
        let run = capture
            .runs
            .iter_mut()
            .find(|r| r.id == run_id(Face::Regular, Shaping::Shaped))
            .expect("the shaped run");
        run.width[100] = 0.0;
        run.width[101] = 99.0;
        let swept = analyse(&sweep, &capture).expect("the sweep analyses");
        assert_eq!(swept[0].extent_off, 2);
        assert_eq!(swept[1].extent_off, 0);
    }

    /// The capture record reads back what the capture step wrote, and
    /// names what it cannot find.
    #[test]
    fn a_sweep_capture_is_read_or_named() {
        let dir = std::env::temp_dir().join(format!("op-verify-sweep-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a temporary directory");
        let path = dir.join("sweep-capture.json");
        let body = r#"{"length": 3, "browser": "chrome", "binary": "chrome (152)",
          "runs": {"sweep-400-flat": {"chars": 3, "x": [0, 1.5, 3], "width": [1.5, 1.5, 0], "total": 4.5}}}"#;
        std::fs::write(&path, body).expect("the capture writes");
        let capture = read_capture(&path).expect("the capture reads");
        assert_eq!(capture.length, 3);
        assert_eq!(capture.browser, "chrome");
        assert_eq!(
            capture
                .run(Face::Regular, Shaping::Flat)
                .expect("the run")
                .x,
            vec![0.0, 1.5, 3.0]
        );
        assert!(
            capture
                .run(Face::Bold, Shaping::Flat)
                .expect_err("no bold run")
                .contains("sweep-700-flat")
        );
        std::fs::write(&path, "{\"length\": 3}").expect("a capture with no runs");
        assert!(
            read_capture(&path)
                .expect_err("no runs")
                .contains("no runs")
        );
        std::fs::write(&path, "{").expect("a broken capture");
        assert!(read_capture(&path).is_err());
        std::fs::remove_dir_all(&dir).expect("the temporary directory goes");
    }

    /// The verdict says the things it must: that the table was checked
    /// exhaustively, how many pairs kern, the largest of them, what the
    /// context ran to, and what order three would cost.
    #[test]
    fn the_verdict_says_what_was_found() {
        let sweep = text();
        let kern = |a: char, b: char, _f: Face| {
            if (a, b) == ('P', '.') {
                -1.25
            } else if (a, b) == ('A', 'V') {
                -0.5
            } else {
                0.0
            }
        };
        let capture = told(
            &sweep,
            kern,
            |a, b| if (a, b) == ('f', 'i') { -0.5 } else { 0.0 },
        );
        let swept = analyse(&sweep, &capture).expect("the sweep analyses");
        let drawn = vec![
            ("half".to_owned(), "half".to_owned()),
            ("P.".to_owned(), "P.".to_owned()),
        ];
        let lines = verdict(&sweep, &swept, &drawn);
        let all = lines.join("\n");
        assert!(all.contains("9025 ordered pairs"), "{all}");
        assert!(all.contains("The advance table is exact"), "{all}");
        assert!(all.contains("2 of 9025 pairs kern"), "{all}");
        assert!(all.contains("\"P.\" -1.250"), "{all}");
        assert!(all.contains("\"fi\""), "{all}");
        assert!(all.contains("857375 ordered triples"), "{all}");
        // the one string that is more than a pixel out is counted as such
        assert!(
            all.contains("1 of the 2 are more than a pixel out"),
            "{all}"
        );
        assert!(!all.contains("to within a pixel for every string"), "{all}");
        let json = measured_json(&sweep, &capture, &swept, &lines);
        let value = op_chart::data::json::parse(&json).expect("the measurement is JSON");
        let op_chart::data::json::Value::Object(fields) = value else {
            panic!("the measurement is an object");
        };
        let get = |k: &str| fields.iter().find(|(n, _)| n == k).map(|(_, v)| v);
        assert_eq!(
            get("pairs"),
            Some(&op_chart::data::json::Value::Number(9025.0))
        );
        let Some(op_chart::data::json::Value::Array(faces)) = get("faces") else {
            panic!("the measurement lists its faces");
        };
        assert_eq!(faces.len(), 2);
    }
}
