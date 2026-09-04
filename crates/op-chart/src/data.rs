//! The data block a chart carries: a dependency-free JSON reader, the
//! schema it reads (decision 1 of docs/research/chart-element-2026-09.md),
//! the hash that tells a live pre-render from a stale one, and the escape
//! that lets the block sit inside a `<script>` element.

use crate::{Chapter, Series, Spec};
use json::Value;

/// A rejected data block. The message names the field at fault, so a build
/// can point at the offending line rather than at the block as a whole.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    pub message: String,
}

impl Error {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Error {}

pub mod json {
    //! Just enough JSON for a data block: values, nesting, whitespace,
    //! string escapes including surrogate pairs, and numbers in the
    //! grammar json.org states. No dependency, and nothing beyond the
    //! grammar: a trailing comma or a leading zero is a rejection, not a
    //! guess at what was meant.

    use super::Error;

    /// A parsed JSON value. Objects keep their keys in document order, so
    /// a reader can name the offending field in the order the author
    /// wrote it.
    #[derive(Clone, Debug, PartialEq)]
    pub enum Value {
        Null,
        Bool(bool),
        Number(f64),
        String(String),
        Array(Vec<Value>),
        Object(Vec<(String, Value)>),
    }

    impl Value {
        /// The value's type, for a message that says what was found.
        pub fn kind(&self) -> &'static str {
            match self {
                Value::Null => "null",
                Value::Bool(_) => "a boolean",
                Value::Number(_) => "a number",
                Value::String(_) => "a string",
                Value::Array(_) => "an array",
                Value::Object(_) => "an object",
            }
        }
    }

    /// Read one JSON value and reject anything after it.
    pub fn parse(text: &str) -> Result<Value, Error> {
        let mut p = Reader {
            text,
            b: text.as_bytes(),
            i: 0,
        };
        p.ws();
        let v = p.value()?;
        p.ws();
        if p.i < p.b.len() {
            return Err(p.fail("text after the value"));
        }
        Ok(v)
    }

    struct Reader<'a> {
        text: &'a str,
        b: &'a [u8],
        i: usize,
    }

    impl Reader<'_> {
        fn fail(&self, what: &str) -> Error {
            Error::new(format!(
                "the data is not valid JSON: {what} at byte {}",
                self.i
            ))
        }

        fn ws(&mut self) {
            while matches!(self.b.get(self.i), Some(b' ' | b'\t' | b'\n' | b'\r')) {
                self.i += 1;
            }
        }

        fn eat(&mut self, c: u8) -> bool {
            if self.b.get(self.i) == Some(&c) {
                self.i += 1;
                return true;
            }
            false
        }

        fn word(&mut self, word: &str) -> bool {
            if self.text[self.i..].starts_with(word) {
                self.i += word.len();
                return true;
            }
            false
        }

        fn value(&mut self) -> Result<Value, Error> {
            match self.b.get(self.i) {
                Some(b'{') => self.object(),
                Some(b'[') => self.array(),
                Some(b'"') => Ok(Value::String(self.string()?)),
                Some(b't') if self.word("true") => Ok(Value::Bool(true)),
                Some(b'f') if self.word("false") => Ok(Value::Bool(false)),
                Some(b'n') if self.word("null") => Ok(Value::Null),
                Some(_) => Ok(Value::Number(self.number()?)),
                None => Err(self.fail("nothing where a value belongs")),
            }
        }

        fn object(&mut self) -> Result<Value, Error> {
            self.i += 1; // the brace
            let mut fields = Vec::new();
            self.ws();
            if self.eat(b'}') {
                return Ok(Value::Object(fields));
            }
            loop {
                self.ws();
                if self.b.get(self.i) != Some(&b'"') {
                    return Err(self.fail("a key that is not a string"));
                }
                let key = self.string()?;
                self.ws();
                if !self.eat(b':') {
                    return Err(self.fail("a key with no colon after it"));
                }
                self.ws();
                fields.push((key, self.value()?));
                self.ws();
                if self.eat(b',') {
                    continue;
                }
                if self.eat(b'}') {
                    return Ok(Value::Object(fields));
                }
                return Err(self.fail("an object that is never closed"));
            }
        }

        fn array(&mut self) -> Result<Value, Error> {
            self.i += 1; // the bracket
            let mut items = Vec::new();
            self.ws();
            if self.eat(b']') {
                return Ok(Value::Array(items));
            }
            loop {
                self.ws();
                items.push(self.value()?);
                self.ws();
                if self.eat(b',') {
                    continue;
                }
                if self.eat(b']') {
                    return Ok(Value::Array(items));
                }
                return Err(self.fail("an array that is never closed"));
            }
        }

        /// Four hex digits of a `\u` escape.
        fn hex4(&mut self) -> Result<u32, Error> {
            let mut n = 0u32;
            for _ in 0..4 {
                let d = self
                    .b
                    .get(self.i)
                    .and_then(|c| char::from(*c).to_digit(16))
                    .ok_or_else(|| self.fail("a \\u escape without four hex digits"))?;
                n = n * 16 + d;
                self.i += 1;
            }
            Ok(n)
        }

        fn string(&mut self) -> Result<String, Error> {
            self.i += 1; // the opening quote
            let mut out = String::new();
            // the run of bytes since the last escape, copied whole: a quote
            // or a backslash never appears inside a multi-byte character,
            // so every slice below falls on a character boundary
            let mut run = self.i;
            loop {
                let Some(&c) = self.b.get(self.i) else {
                    return Err(self.fail("a string that is never closed"));
                };
                match c {
                    b'"' => {
                        out.push_str(&self.text[run..self.i]);
                        self.i += 1;
                        return Ok(out);
                    }
                    b'\\' => {
                        out.push_str(&self.text[run..self.i]);
                        self.i += 1;
                        out.push(self.escape()?);
                        run = self.i;
                    }
                    0x00..=0x1f => return Err(self.fail("a control character in a string")),
                    _ => self.i += 1,
                }
            }
        }

        /// The character one escape stands for, the backslash already eaten.
        fn escape(&mut self) -> Result<char, Error> {
            let Some(&e) = self.b.get(self.i) else {
                return Err(self.fail("a backslash at the end of the data"));
            };
            self.i += 1;
            Ok(match e {
                b'"' => '"',
                b'\\' => '\\',
                b'/' => '/',
                b'b' => '\u{8}',
                b'f' => '\u{c}',
                b'n' => '\n',
                b'r' => '\r',
                b't' => '\t',
                b'u' => {
                    let hi = self.hex4()?;
                    // a high surrogate is half a character: the low half
                    // must follow as its own escape
                    let c = if (0xd800..0xdc00).contains(&hi) {
                        if !(self.b.get(self.i) == Some(&b'\\')
                            && self.b.get(self.i + 1) == Some(&b'u'))
                        {
                            return Err(self.fail("a high surrogate with no low half after it"));
                        }
                        self.i += 2;
                        let lo = self.hex4()?;
                        if !(0xdc00..0xe000).contains(&lo) {
                            return Err(self.fail("a high surrogate followed by a non-surrogate"));
                        }
                        char::from_u32(0x1_0000 + ((hi - 0xd800) << 10) + (lo - 0xdc00))
                    } else {
                        char::from_u32(hi).filter(|_| !(0xdc00..0xe000).contains(&hi))
                    };
                    c.ok_or_else(|| self.fail("a lone low surrogate"))?
                }
                _ => return Err(self.fail("an escape that stands for nothing")),
            })
        }

        fn number(&mut self) -> Result<f64, Error> {
            let start = self.i;
            self.eat(b'-');
            match self.b.get(self.i) {
                Some(b'0') => {
                    self.i += 1;
                    if self.b.get(self.i).is_some_and(u8::is_ascii_digit) {
                        return Err(self.fail("a number with a leading zero"));
                    }
                }
                Some(d) if d.is_ascii_digit() => self.digits(),
                _ => return Err(self.fail("a value that is not JSON")),
            }
            if self.eat(b'.') {
                if !self.b.get(self.i).is_some_and(u8::is_ascii_digit) {
                    return Err(self.fail("a decimal point with no digit after it"));
                }
                self.digits();
            }
            if matches!(self.b.get(self.i), Some(b'e' | b'E')) {
                self.i += 1;
                if !self.eat(b'+') {
                    self.eat(b'-');
                }
                if !self.b.get(self.i).is_some_and(u8::is_ascii_digit) {
                    return Err(self.fail("an exponent with no digit after it"));
                }
                self.digits();
            }
            self.text[start..self.i]
                .parse::<f64>()
                .map_err(|_| self.fail("a number that cannot be read"))
        }

        fn digits(&mut self) {
            while self.b.get(self.i).is_some_and(u8::is_ascii_digit) {
                self.i += 1;
            }
        }
    }
}

/// The keys a data block may carry, in the order the schema states them.
const KEYS: [&str; 7] = [
    "series", "rows", "marks", "band", "chapters", "duration", "y",
];

/// The stroke width a parsed series draws with; 2 is the palette's design
/// width and the block does not carry one.
const SERIES_WIDTH: f64 = 2.0;

/// The room left above and below a computed value domain, as a share of the
/// rows' own range, so that a line at either extreme is drawn clear of the
/// axis rather than along it.
const Y_PAD: f64 = 0.04;

/// One column of the block: what names it and which non-colour cues it
/// takes. `dash` and `marker` are indices into the site's dash and marker
/// tables and default to the column's own position.
#[derive(Clone, Debug, PartialEq)]
pub struct SeriesSpec {
    pub id: String,
    pub label: String,
    /// The unit the values are in; empty for none.
    pub unit: String,
    pub dash: usize,
    pub marker: usize,
}

/// One sample row: a time and one value per series, `None` for a gap.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub t: f64,
    pub values: Vec<Option<f64>>,
}

/// A labelled instant on the time axis.
#[derive(Clone, Debug, PartialEq)]
pub struct Mark {
    pub t: f64,
    pub label: String,
}

/// A labelled span of the time axis.
#[derive(Clone, Debug, PartialEq)]
pub struct Band {
    pub t0: f64,
    pub t1: f64,
    pub label: String,
}

/// A block that passed the schema: rows of values on one shared time axis,
/// with the marks, band and chapters that annotate it.
#[derive(Clone, Debug, PartialEq)]
pub struct Data {
    pub series: Vec<SeriesSpec>,
    pub rows: Vec<Row>,
    pub marks: Vec<Mark>,
    pub band: Option<Band>,
    pub chapters: Vec<Chapter>,
    pub duration: f64,
    /// The value domain the author asked for, `(lo, hi)` with lo below hi.
    pub y: Option<(f64, f64)>,
}

impl Data {
    /// Where the time axis ends: the last row's time, or the whole
    /// duration when there are no rows to bound it.
    pub fn end(&self) -> f64 {
        self.rows.last().map_or(self.duration, |r| r.t)
    }

    /// The value domain the chart is drawn on: what the block asked for;
    /// else, when every series is in percent, the film's percent scale
    /// ([`crate::layout::PERCENT`], so a chart agrees with the film it
    /// follows); else the rows' own range with a little room above and
    /// below so no line runs along an edge. A flat series is given a whole
    /// unit of room, and a block with nothing sampled keeps the percent
    /// scale.
    pub fn y_domain(&self) -> (f64, f64) {
        if let Some(y) = self.y {
            return y;
        }
        // percent data takes the film's percent scale, quarters with room at
        // both ends, so a chart and the film it follows agree on their axes
        if !self.series.is_empty() && self.series.iter().all(|s| s.unit == "%") {
            return crate::layout::PERCENT;
        }
        let mut lo = f64::INFINITY;
        let mut hi = f64::NEG_INFINITY;
        for v in self.rows.iter().flat_map(|r| r.values.iter().flatten()) {
            lo = lo.min(*v);
            hi = hi.max(*v);
        }
        if lo > hi {
            return crate::layout::PERCENT;
        }
        let pad = if hi > lo { (hi - lo) * Y_PAD } else { 0.5 };
        (lo - pad, hi + pad)
    }

    /// One [`Series`] per column, gaps kept as gaps. The palette class is
    /// the dash index one higher, so the first column is `series-1`; the
    /// axis label is the first unit any column names.
    pub fn to_spec(&self) -> Spec {
        let end = self.end();
        Spec {
            end,
            duration: self.duration.max(end),
            y: self.y_domain(),
            ylabel: self
                .series
                .iter()
                .map(|s| s.unit.as_str())
                .find(|u| !u.is_empty())
                .unwrap_or_default()
                .to_owned(),
            chapters: self.chapters.clone(),
            series: self
                .series
                .iter()
                .enumerate()
                .map(|(j, s)| Series {
                    label: s.label.clone(),
                    index: s.dash + 1,
                    points: self
                        .rows
                        .iter()
                        .map(|r| r.values.get(j).copied().flatten().map(|v| (r.t, v)))
                        .collect(),
                    width: SERIES_WIDTH,
                })
                .collect(),
        }
    }
}

/// FNV-1a 64 over the block's bytes, ignoring the whitespace around it, so
/// that re-indenting a block in the page does not invalidate the
/// pre-render drawn from it.
pub fn hash(text: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in text.trim_matches(|c: char| c.is_ascii_whitespace()).bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// [`hash`] as the 16 lowercase hex digits a `data-hash` attribute carries.
pub fn hash_hex(text: &str) -> String {
    format!("{:016x}", hash(text))
}

/// Rewrite every `</script`, in any case, as `<\/script`. The backslash
/// before a solidus is a JSON string escape and decodes to the same text,
/// so a block that says `</script>` inside a string still parses and no
/// HTML parser can find the end of the element early. Apply it to a whole
/// block only: the sequence is expected inside a string.
pub fn escape_script(text: &str) -> String {
    const TAIL: &str = "/script";
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find('<') {
        let (head, tail) = rest.split_at(i);
        out.push_str(head);
        let after = &tail[1..];
        if after.is_char_boundary(TAIL.len()) && after[..TAIL.len()].eq_ignore_ascii_case(TAIL) {
            // the backslash goes in, the author's own spelling stays
            out.push_str("<\\");
            out.push_str(&after[..TAIL.len()]);
            rest = &after[TAIL.len()..];
        } else {
            out.push('<');
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

/// The first value under `key`.
fn find<'a>(fields: &'a [(String, Value)], key: &str) -> Option<&'a Value> {
    fields.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

/// The value under `key`, which the schema requires.
fn need<'a>(fields: &'a [(String, Value)], at: &str, key: &str) -> Result<&'a Value, Error> {
    find(fields, key).ok_or_else(|| Error::new(format!("{at} has no {key}")))
}

fn array<'a>(v: &'a Value, at: &str) -> Result<&'a [Value], Error> {
    match v {
        Value::Array(items) => Ok(items),
        other => Err(Error::new(format!(
            "{at} must be an array, not {}",
            other.kind()
        ))),
    }
}

fn object<'a>(v: &'a Value, at: &str) -> Result<&'a [(String, Value)], Error> {
    match v {
        Value::Object(fields) => Ok(fields),
        other => Err(Error::new(format!(
            "{at} must be an object, not {}",
            other.kind()
        ))),
    }
}

fn string(v: &Value, at: &str) -> Result<String, Error> {
    match v {
        Value::String(s) => Ok(s.clone()),
        other => Err(Error::new(format!(
            "{at} must be a string, not {}",
            other.kind()
        ))),
    }
}

/// A number the geometry can use: nothing infinite, nothing not a number.
fn finite(v: &Value, at: &str) -> Result<f64, Error> {
    match v {
        Value::Number(n) if n.is_finite() => Ok(*n),
        Value::Number(n) => Err(Error::new(format!("{at} is {n}, which is not finite"))),
        other => Err(Error::new(format!(
            "{at} must be a number, not {}",
            other.kind()
        ))),
    }
}

/// A whole number at or above zero: an index into one of the site's tables.
fn index(v: &Value, at: &str) -> Result<usize, Error> {
    let n = finite(v, at)?;
    if n < 0.0 || n.fract() != 0.0 {
        return Err(Error::new(format!(
            "{at} is {n}: it must be a whole number at or above zero"
        )));
    }
    Ok(n as usize)
}

/// The optional string at `key`, empty when the block leaves it out.
fn string_or_empty(fields: &[(String, Value)], at: &str, key: &str) -> Result<String, Error> {
    match find(fields, key) {
        Some(v) => string(v, &format!("{at}.{key}")),
        None => Ok(String::new()),
    }
}

/// Read a data block. Every rejection names the field at fault.
pub fn parse(text: &str) -> Result<Data, Error> {
    let root = json::parse(text)?;
    let fields = object(&root, "the data")?;
    for (key, _) in fields {
        if !KEYS.contains(&key.as_str()) {
            return Err(Error::new(format!(
                "unknown key \"{key}\": the allowed keys are {}",
                KEYS.join(", ")
            )));
        }
    }

    let mut series = Vec::new();
    for (i, item) in array(need(fields, "the data", "series")?, "series")?
        .iter()
        .enumerate()
    {
        let at = format!("series[{i}]");
        let f = object(item, &at)?;
        series.push(SeriesSpec {
            id: string(need(f, &at, "id")?, &format!("{at}.id"))?,
            label: string(need(f, &at, "label")?, &format!("{at}.label"))?,
            unit: string_or_empty(f, &at, "unit")?,
            dash: match find(f, "dash") {
                Some(v) => index(v, &format!("{at}.dash"))?,
                None => i,
            },
            marker: match find(f, "marker") {
                Some(v) => index(v, &format!("{at}.marker"))?,
                None => i,
            },
        });
    }

    let mut rows: Vec<Row> = Vec::new();
    for (i, item) in array(need(fields, "the data", "rows")?, "rows")?
        .iter()
        .enumerate()
    {
        let at = format!("rows[{i}]");
        let cells = array(item, &at)?;
        if cells.len() != 1 + series.len() {
            return Err(Error::new(format!(
                "{at} has {} values but the block declares {} series: a row is [t, v1, ..., vN]",
                cells.len(),
                series.len()
            )));
        }
        let t = finite(&cells[0], &format!("{at}.t"))?;
        if let Some(prev) = rows.last().map(|r| r.t)
            && t < prev
        {
            return Err(Error::new(format!(
                "{at}.t is {t}, below the previous row's {prev}: rows must be in non-decreasing t"
            )));
        }
        let mut values = Vec::with_capacity(series.len());
        for (j, cell) in cells[1..].iter().enumerate() {
            values.push(match cell {
                Value::Null => None,
                _ => Some(finite(
                    cell,
                    &format!("{at} value for series \"{}\"", series[j].id),
                )?),
            });
        }
        rows.push(Row { t, values });
    }

    let mut marks = Vec::new();
    if let Some(v) = find(fields, "marks") {
        for (i, item) in array(v, "marks")?.iter().enumerate() {
            let at = format!("marks[{i}]");
            let f = object(item, &at)?;
            marks.push(Mark {
                t: finite(need(f, &at, "t")?, &format!("{at}.t"))?,
                label: string(need(f, &at, "label")?, &format!("{at}.label"))?,
            });
        }
    }

    let band = match find(fields, "band") {
        Some(v) => {
            let f = object(v, "band")?;
            Some(Band {
                t0: finite(need(f, "band", "t0")?, "band.t0")?,
                t1: finite(need(f, "band", "t1")?, "band.t1")?,
                label: string(need(f, "band", "label")?, "band.label")?,
            })
        }
        None => None,
    };

    let mut chapters = Vec::new();
    if let Some(v) = find(fields, "chapters") {
        for (i, item) in array(v, "chapters")?.iter().enumerate() {
            let at = format!("chapters[{i}]");
            let f = object(item, &at)?;
            chapters.push(Chapter {
                t: finite(need(f, &at, "t")?, &format!("{at}.t"))?,
                label: string(need(f, &at, "title")?, &format!("{at}.title"))?,
            });
        }
    }

    let duration = finite(need(fields, "the data", "duration")?, "duration")?;
    if duration <= 0.0 {
        return Err(Error::new(format!(
            "duration is {duration}: it must be above zero"
        )));
    }

    let y = match find(fields, "y") {
        Some(v) => {
            let pair = array(v, "y")?;
            if pair.len() != 2 {
                return Err(Error::new(format!(
                    "y has {} values: it must be [lo, hi]",
                    pair.len()
                )));
            }
            let lo = finite(&pair[0], "y lo")?;
            let hi = finite(&pair[1], "y hi")?;
            if lo >= hi {
                return Err(Error::new(format!(
                    "y is [{lo}, {hi}]: lo must be below hi"
                )));
            }
            Some((lo, hi))
        }
        None => None,
    };

    Ok(Data {
        series,
        rows,
        marks,
        band,
        chapters,
        duration,
        y,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A block with every key the schema allows.
    const FULL: &str = r#"{
        "series": [
            {"id": "ghost", "label": "ghost left %", "unit": "%", "dash": 2, "marker": 2},
            {"id": "blend", "label": "palette blend %", "unit": "s"}
        ],
        "rows": [[0, 0, 0], [0.5, 40, null], [1, 100, 25]],
        "marks": [{"t": 0.5, "label": "abort"}],
        "band": {"t0": 0.2, "t1": 0.8, "label": "flight"},
        "chapters": [{"t": 0, "title": "start"}, {"t": 0.5, "title": "settle"}],
        "duration": 1.5,
        "y": [-4, 106]
    }"#;

    fn err(text: &str) -> String {
        parse(text)
            .expect_err("this block must be rejected")
            .message
    }

    #[test]
    fn a_full_block_reads_every_field() {
        let d = parse(FULL).expect("the block is valid");
        assert_eq!(d.series.len(), 2);
        assert_eq!(d.series[0].id, "ghost");
        assert_eq!(d.series[0].unit, "%");
        assert_eq!((d.series[0].dash, d.series[0].marker), (2, 2));
        // a column that names neither takes its own position
        assert_eq!((d.series[1].dash, d.series[1].marker), (1, 1));
        assert_eq!(d.series[1].unit, "s");
        assert_eq!(d.rows.len(), 3);
        assert_eq!(d.rows[1].t, 0.5);
        assert_eq!(d.rows[1].values, vec![Some(40.0), None]);
        assert_eq!(
            d.marks,
            vec![Mark {
                t: 0.5,
                label: "abort".to_owned()
            }]
        );
        assert_eq!(
            d.band,
            Some(Band {
                t0: 0.2,
                t1: 0.8,
                label: "flight".to_owned()
            })
        );
        assert_eq!(d.chapters.len(), 2);
        assert_eq!(d.chapters[1].label, "settle");
        assert_eq!(d.duration, 1.5);
        assert_eq!(d.y, Some((-4.0, 106.0)));
        assert_eq!(d.end(), 1.0);
    }

    #[test]
    fn the_optional_keys_may_all_be_absent() {
        let d = parse(r#"{"series": [], "rows": [], "duration": 2}"#).expect("valid");
        assert!(d.series.is_empty() && d.rows.is_empty() && d.marks.is_empty());
        assert_eq!((d.band.clone(), d.y), (None, None));
        assert!(d.chapters.is_empty());
        // with no row to bound it the axis runs the whole duration
        assert_eq!(d.end(), 2.0);
    }

    #[test]
    fn every_rejection_names_the_field_at_fault() {
        // malformed JSON
        assert!(err("{").contains("not valid JSON"));
        assert!(err(r#"{"series": [], "rows": [],}"#).contains("not valid JSON"));
        assert!(err(r#"{"series": [], "rows": [], "duration": 01}"#).contains("leading zero"));
        assert!(err("[1, 2]").contains("the data must be an object"));
        assert!(err(r#"{"series": [], "rows": [], "duration": 1} 7"#).contains("text after"));
        // a row whose arity is not 1 + the series count
        let e = err(
            r#"{"series": [{"id": "a", "label": "A"}], "rows": [[0, 1], [1, 2, 3]], "duration": 1}"#,
        );
        assert!(
            e.contains("rows[1] has 3 values but the block declares 1 series"),
            "{e}"
        );
        // a duration that is not finite, and one that is not positive
        let e = err(r#"{"series": [], "rows": [], "duration": 1e400}"#);
        assert!(
            e.contains("duration is inf") && e.contains("not finite"),
            "{e}"
        );
        let e = err(r#"{"series": [], "rows": [], "duration": 0}"#);
        assert!(
            e.contains("duration is 0") && e.contains("above zero"),
            "{e}"
        );
        let e = err(r#"{"series": [], "rows": [], "duration": -2}"#);
        assert!(e.contains("duration is -2"), "{e}");
        // a row that goes back in time
        let e = err(r#"{"series": [], "rows": [[0], [2], [1]], "duration": 3}"#);
        assert!(
            e.contains("rows[2].t is 1, below the previous row's 2")
                && e.contains("non-decreasing"),
            "{e}"
        );
        // an unknown top-level key, with the allowed keys named
        let e = err(r#"{"series": [], "rows": [], "duration": 1, "colour": "red"}"#);
        assert!(e.contains("unknown key \"colour\""), "{e}");
        for key in KEYS {
            assert!(e.contains(key), "{key} is missing from {e}");
        }
        // a value domain that is not an interval
        let e = err(r#"{"series": [], "rows": [], "duration": 1, "y": [5, 5]}"#);
        assert!(e.contains("y is [5, 5]: lo must be below hi"), "{e}");
        let e = err(r#"{"series": [], "rows": [], "duration": 1, "y": [9, 2]}"#);
        assert!(e.contains("lo must be below hi"), "{e}");
        let e = err(r#"{"series": [], "rows": [], "duration": 1, "y": [1]}"#);
        assert!(e.contains("y has 1 values"), "{e}");
    }

    #[test]
    fn the_fields_a_block_must_carry_are_named_when_they_are_missing() {
        assert!(err(r#"{"rows": [], "duration": 1}"#).contains("the data has no series"));
        assert!(err(r#"{"series": [], "duration": 1}"#).contains("the data has no rows"));
        assert!(err(r#"{"series": [], "rows": []}"#).contains("the data has no duration"));
        assert!(
            err(r#"{"series": [{"label": "A"}], "rows": [], "duration": 1}"#)
                .contains("series[0] has no id")
        );
        assert!(
            err(r#"{"series": [{"id": "a"}], "rows": [], "duration": 1}"#)
                .contains("series[0] has no label")
        );
        // and a field of the wrong type says what it found
        let e = err(r#"{"series": {}, "rows": [], "duration": 1}"#);
        assert!(e.contains("series must be an array, not an object"), "{e}");
        let e = err(r#"{"series": [], "rows": [], "duration": "1"}"#);
        assert!(e.contains("duration must be a number, not a string"), "{e}");
        let e = err(
            r#"{"series": [{"id": "a", "label": "A", "dash": 1.5}], "rows": [], "duration": 1}"#,
        );
        assert!(e.contains("series[0].dash is 1.5"), "{e}");
        let e =
            err(r#"{"series": [{"id": "a", "label": "A"}], "rows": [[0, "x"]], "duration": 1}"#);
        assert!(
            e.contains("rows[0] value for series \"a\" must be a number"),
            "{e}"
        );
    }

    #[test]
    fn every_escape_form_decodes_and_a_raw_character_survives() {
        let v = json::parse(r#""\" \\ \/ \b \f \n \r \t""#).expect("valid");
        assert_eq!(v, Value::String("\" \\ / \u{8} \u{c} \n \r \t".to_owned()));
        // \uXXXX, including a surrogate pair for a character above the
        // basic plane, and the same characters written raw
        let v = json::parse(r#""\u00e9 A\u00fc \ud834\udd1e""#).expect("valid");
        assert_eq!(v, Value::String("é Aü 𝄞".to_owned()));
        let raw = json::parse("\"é 𝄞 ünïcode\"").expect("valid");
        assert_eq!(raw, Value::String("é 𝄞 ünïcode".to_owned()));
        // a round trip through the schema: the escaped spelling and the raw
        // one read as the same block
        let escaped = r#"{"series": [{"id": "a", "label": "caf\u00e9 \ud834\udd1e"}], "rows": [], "duration": 1}"#;
        let plain =
            "{\"series\": [{\"id\": \"a\", \"label\": \"café 𝄞\"}], \"rows\": [], \"duration\": 1}";
        assert_eq!(parse(escaped), parse(plain));
        assert_eq!(parse(escaped).expect("valid").series[0].label, "café 𝄞");
        // half a pair, a stray escape and a short one are all rejected
        assert!(json::parse(r#""\ud834""#).is_err());
        assert!(json::parse(r#""\udd1e""#).is_err());
        assert!(json::parse(r#""\ud834A""#).is_err());
        assert!(json::parse(r#""\q""#).is_err());
        assert!(json::parse(r#""\u12""#).is_err());
        assert!(json::parse("\"a\nb\"").is_err());
    }

    #[test]
    fn numbers_follow_the_json_grammar() {
        assert_eq!(json::parse("-0.5e2"), Ok(Value::Number(-50.0)));
        assert_eq!(
            json::parse("[1E+2, 0, -0]"),
            Ok(Value::Array(vec![
                Value::Number(100.0),
                Value::Number(0.0),
                Value::Number(0.0),
            ]))
        );
        for bad in ["1.", ".5", "+1", "1e", "1e+", "--1", "0x10", "nul", "tru"] {
            assert!(json::parse(bad).is_err(), "{bad} must be rejected");
        }
    }

    #[test]
    fn the_hash_is_fnv_1a_and_ignores_the_whitespace_around_the_block() {
        assert_eq!(hash(""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(hash("a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(hash(FULL), hash(&format!("\n  \t{FULL}\r\n ")));
        assert_ne!(hash(FULL), hash(&FULL.replace("1.5", "1.6")));
        // the inner whitespace is part of the block, the outer is not
        assert_ne!(hash("{\"a\": 1}"), hash("{\"a\":1}"));
        assert_eq!(hash_hex(""), "cbf29ce484222325");
        assert_eq!(hash_hex("a"), "af63dc4c8601ec8c");
        assert_eq!(hash_hex(FULL).len(), 16);
        // a hash whose top byte is zero still fills sixteen digits
        assert_eq!(hash_hex("baa"), "0039231913392937");
    }

    #[test]
    fn the_script_escape_hides_every_case_and_keeps_the_data() {
        let block = r#"{"series": [{"id": "a", "label": "ends with </SCRIPT> and </script>"}], "rows": [], "duration": 1}"#;
        let escaped = escape_script(block);
        assert!(!escaped.to_ascii_lowercase().contains("</script"));
        assert!(escaped.contains("<\\/SCRIPT>") && escaped.contains("<\\/script>"));
        assert_eq!(parse(&escaped), parse(block));
        assert_eq!(
            parse(&escaped).expect("valid").series[0].label,
            "ends with </SCRIPT> and </script>"
        );
        // a lone bracket, a short tail and a multi-byte character after one
        // are all left alone
        assert_eq!(escape_script("a < b </scrip <"), "a < b </scrip <");
        assert_eq!(escape_script("<é>"), "<é>");
        assert_eq!(escape_script(""), "");
    }

    #[test]
    fn a_block_becomes_one_series_per_column_with_its_gaps() {
        let spec = parse(FULL).expect("valid").to_spec();
        assert_eq!(spec.end, 1.0);
        assert_eq!(spec.duration, 1.5);
        // the axis label is the first unit any column names
        assert_eq!(spec.ylabel, "%");
        assert_eq!(spec.chapters.len(), 2);
        assert_eq!(spec.series.len(), 2);
        // the dash index one higher is the palette class
        assert_eq!((spec.series[0].index, spec.series[1].index), (3, 2));
        assert_eq!(spec.series[0].label, "ghost left %");
        assert_eq!(
            spec.series[0].points,
            vec![Some((0.0, 0.0)), Some((0.5, 40.0)), Some((1.0, 100.0))]
        );
        // the null in the second column is a gap, not a zero
        assert_eq!(
            spec.series[1].points,
            vec![Some((0.0, 0.0)), None, Some((1.0, 25.0))]
        );
        assert_eq!(spec.series[1].width, SERIES_WIDTH);
        // a block whose rows run past its stated duration still promises the
        // slider a maximum at least as large as the axis
        let d = parse(r#"{"series": [], "rows": [[9]], "duration": 1}"#).expect("valid");
        assert_eq!((d.to_spec().end, d.to_spec().duration), (9.0, 9.0));
    }

    #[test]
    fn the_value_domain_is_the_blocks_own_or_the_rows_range_with_room_around_it() {
        // every series in percent: the film's scale, whatever the values span
        let pct = parse(r#"{"duration": 1, "series": [{"id": "a", "label": "a", "unit": "%"}, {"id": "b", "label": "b", "unit": "%"}], "rows": [[0, 10, 20], [1, 30, 40]]}"#).unwrap();
        assert_eq!(pct.y_domain(), crate::layout::PERCENT);
        // one series in another unit: the data's own padded range
        let mixed = parse(r#"{"duration": 1, "series": [{"id": "a", "label": "a", "unit": "%"}, {"id": "b", "label": "b", "unit": "ms"}], "rows": [[0, 10, 20], [1, 30, 40]]}"#).unwrap();
        assert_ne!(mixed.y_domain(), crate::layout::PERCENT);
        // what the block asks for, exactly
        assert_eq!(parse(FULL).expect("valid").to_spec().y, (-4.0, 106.0));
        let asked = r#"{"series": [{"id": "a", "label": "A"}], "rows": [[0, 5]], "duration": 1, "y": [0, 1000]}"#;
        assert_eq!(parse(asked).expect("valid").to_spec().y, (0.0, 1000.0));
        // otherwise the rows' own range, with four percent of it above and
        // below, so no line is drawn along an edge
        let ms = r#"{"series": [{"id": "a", "label": "A"}, {"id": "b", "label": "B"}],
            "rows": [[0, 200, null], [1, 700, 1200]], "duration": 1}"#;
        let y = parse(ms).expect("valid").to_spec().y;
        assert_eq!(y, (200.0 - 40.0, 1200.0 + 40.0));
        // gaps are not values: a null never drags the domain to zero
        assert!(y.0 > 0.0);
        // percentages are padded like anything else: the film's own scale
        // leaves more room above than below, and a block that wants it says
        // so rather than having it guessed from the numbers
        let pc =
            r#"{"series": [{"id": "a", "label": "A"}], "rows": [[0, 0], [1, 100]], "duration": 1}"#;
        assert_eq!(parse(pc).expect("valid").to_spec().y, (-4.0, 104.0));
        // a flat series is given a whole unit rather than no room at all
        let flat =
            r#"{"series": [{"id": "a", "label": "A"}], "rows": [[0, 7], [1, 7]], "duration": 1}"#;
        assert_eq!(parse(flat).expect("valid").to_spec().y, (6.5, 7.5));
        // and a block with nothing sampled keeps the percent scale too
        let bare = r#"{"series": [], "rows": [], "duration": 1}"#;
        assert_eq!(
            parse(bare).expect("valid").to_spec().y,
            crate::layout::PERCENT
        );
    }
}
