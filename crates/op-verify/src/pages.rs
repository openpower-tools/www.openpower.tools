//! Does the page build emit the same bytes twice?
//!
//! The pre-rendered chart is the artefact a reader without scripts gets,
//! and it is produced by the page build rather than by the element. Every
//! step from the data block to the markup is a pure function, so a second
//! emission should be byte identical; if it is not, something in the path
//! is reading a clock, a hash map's order, or the filesystem's.
//!
//! This compares two directories of emitted pages rather than re-running
//! anything itself, so the gate can stage the second emission however it
//! likes and this stays a pure comparison with tests of its own.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// How two emissions of one page differ.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Difference {
    /// The second emission has no such page.
    Missing(String),
    /// The second emission has a page the first does not.
    Extra(String),
    /// Both have it and the bytes differ, at this line and this character
    /// of it, with what each side said around that point.
    Differs {
        page: String,
        line: usize,
        column: usize,
        first: String,
        second: String,
    },
    /// Both have it, the lines all match, and the byte counts do not:
    /// a trailing newline, or a line ending, changed.
    Length {
        page: String,
        first: usize,
        second: usize,
    },
}

impl std::fmt::Display for Difference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Missing(page) => write!(f, "{page} was emitted once and not the second time"),
            Self::Extra(page) => write!(f, "{page} was emitted the second time and not the first"),
            Self::Differs {
                page,
                line,
                column,
                first,
                second,
            } => write!(
                f,
                "{page} line {line} character {column} differs:\n    first:  {}\n    second: {}",
                around(first, *column),
                around(second, *column)
            ),
            Self::Length {
                page,
                first,
                second,
            } => write!(
                f,
                "{page} has the same lines and a different length: {first} bytes then {second}"
            ),
        }
    }
}

/// The neighbourhood of `column` in `line`, since a page is one line of
/// markup tens of thousands of characters long and the first 160 of them
/// are the same on both sides by definition: what a reader needs is the
/// text either side of the point where the two parted.
fn around(line: &str, column: usize) -> String {
    const BEFORE: usize = 60;
    const AFTER: usize = 100;
    let chars: Vec<char> = line.chars().collect();
    if chars.len() <= BEFORE + AFTER {
        return line.to_owned();
    }
    let start = column.saturating_sub(BEFORE);
    let end = (column + AFTER).min(chars.len());
    let window: String = chars[start..end].iter().collect();
    format!(
        "{}{window}{}",
        if start > 0 { "..." } else { "" },
        if end < chars.len() { "..." } else { "" }
    )
}

/// Where two lines first part, counted in characters from one.
fn first_divergence(a: &str, b: &str) -> usize {
    a.chars()
        .zip(b.chars())
        .position(|(p, q)| p != q)
        .unwrap_or_else(|| a.chars().count().min(b.chars().count()))
        + 1
}

/// Every `<slug>/index.html` under `root`, keyed by the slug, so two
/// directories can be compared by what they emitted rather than by
/// walking them in step.
pub fn pages(root: &Path) -> Result<BTreeMap<String, PathBuf>, String> {
    let mut out = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = std::fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        for entry in entries {
            let entry = entry.map_err(|e| format!("{}: {e}", dir.display()))?;
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.file_name().is_some_and(|n| n == "index.html") {
                let slug = path
                    .parent()
                    .and_then(|p| p.strip_prefix(root).ok())
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_default();
                out.insert(
                    if slug.is_empty() {
                        ".".to_owned()
                    } else {
                        slug
                    },
                    path,
                );
            }
        }
    }
    Ok(out)
}

/// Compare what two directories emitted. An empty result is two identical
/// emissions.
pub fn compare(first: &Path, second: &Path) -> Result<Vec<Difference>, String> {
    let a = pages(first)?;
    let b = pages(second)?;
    let mut out = Vec::new();
    for (slug, path) in &a {
        let Some(other) = b.get(slug) else {
            out.push(Difference::Missing(slug.clone()));
            continue;
        };
        let x = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let y = std::fs::read_to_string(other).map_err(|e| format!("{}: {e}", other.display()))?;
        if x == y {
            continue;
        }
        match x
            .lines()
            .zip(y.lines())
            .enumerate()
            .find(|(_, (p, q))| p != q)
        {
            Some((i, (p, q))) => out.push(Difference::Differs {
                page: slug.clone(),
                line: i + 1,
                column: first_divergence(p, q),
                first: p.to_owned(),
                second: q.to_owned(),
            }),
            None => out.push(Difference::Length {
                page: slug.clone(),
                first: x.len(),
                second: y.len(),
            }),
        }
    }
    for slug in b.keys() {
        if !a.contains_key(slug) {
            out.push(Difference::Extra(slug.clone()));
        }
    }
    Ok(out)
}

/// What the gate prints: the count that was compared and every
/// difference, or the count and that they matched.
pub fn verdict(first: &Path, second: &Path) -> Result<(Vec<String>, bool), String> {
    let counted = pages(first)?.len();
    if counted == 0 {
        return Err(format!(
            "{}: no <slug>/index.html to compare, so nothing was checked",
            first.display()
        ));
    }
    let differences = compare(first, second)?;
    let mut lines = vec![format!(
        "{counted} pages emitted twice, from {} and {}.",
        first.display(),
        second.display()
    )];
    if differences.is_empty() {
        lines.push("  every page is byte identical across the two emissions.".to_owned());
        return Ok((lines, true));
    }
    for d in &differences {
        lines.push(format!("  {d}"));
    }
    Ok((lines, false))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A directory of pages, written under a fresh temporary root.
    fn emitted(name: &str, pages: &[(&str, &str)]) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("op-verify-pages-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for (slug, body) in pages {
            let dir = root.join(slug);
            std::fs::create_dir_all(&dir).expect("page dir");
            std::fs::write(dir.join("index.html"), body).expect("write page");
        }
        root
    }

    /// Two emissions of the same pages are no differences at all, and the
    /// verdict says how many it actually compared, so a run that compared
    /// nothing cannot read as a pass.
    #[test]
    fn identical_emissions_differ_nowhere_and_say_what_they_compared() {
        let body = "<!doctype html><title>a</title><svg class=\"chart\"><text>0s</text></svg>";
        let a = emitted(
            "same-a",
            &[("component/chart", body), ("about", "<p>x</p>")],
        );
        let b = emitted(
            "same-b",
            &[("component/chart", body), ("about", "<p>x</p>")],
        );
        assert_eq!(compare(&a, &b).expect("compare"), vec![]);
        let (lines, ok) = verdict(&a, &b).expect("verdict");
        assert!(ok);
        assert!(lines[0].starts_with("2 pages emitted twice"), "{lines:?}");
        let _ = std::fs::remove_dir_all(&a);
        let _ = std::fs::remove_dir_all(&b);
    }

    /// One character of markup moving is found, on the right page and the
    /// right line, and the report shows both sides.
    #[test]
    fn a_single_changed_character_is_found_and_located() {
        let a = emitted(
            "one-a",
            &[(
                "component/chart",
                "<title>a</title>\n<text x=\"46.0\">0s</text>",
            )],
        );
        let b = emitted(
            "one-b",
            &[(
                "component/chart",
                "<title>a</title>\n<text x=\"46.1\">0s</text>",
            )],
        );
        let found = compare(&a, &b).expect("compare");
        assert_eq!(found.len(), 1, "{found:?}");
        match &found[0] {
            Difference::Differs {
                page,
                line,
                column,
                first,
                second,
            } => {
                assert_eq!(page, "component/chart");
                assert_eq!(*line, 2);
                // the digit that moved, not the start of the line
                assert_eq!(*column, 13, "{first}");
                assert!(
                    first.contains("46.0") && second.contains("46.1"),
                    "{first} {second}"
                );
            }
            other => panic!("{other:?}"),
        }
        let _ = std::fs::remove_dir_all(&a);
        let _ = std::fs::remove_dir_all(&b);
    }

    /// A page is one line of markup tens of thousands of characters long,
    /// so the report has to show the neighbourhood of the change rather
    /// than the start of the line, which is identical on both sides by
    /// definition. This is the failure the first version had.
    #[test]
    fn a_change_late_in_a_long_line_is_shown_and_not_just_its_beginning() {
        let head = "<svg>".to_owned() + &"<line class=\"grid\"/>".repeat(400);
        let a = emitted(
            "long-a",
            &[(
                "component/chart",
                &(head.clone() + "<text x=\"46.0\">0s</text>"),
            )],
        );
        let b = emitted(
            "long-b",
            &[(
                "component/chart",
                &(head.clone() + "<text x=\"46.1\">0s</text>"),
            )],
        );
        let found = compare(&a, &b).expect("compare");
        assert_eq!(found.len(), 1, "{found:?}");
        let said = found[0].to_string();
        assert!(said.contains("46.0") && said.contains("46.1"), "{said}");
        assert!(
            said.contains("..."),
            "the window should say it is a window: {said}"
        );
        let _ = std::fs::remove_dir_all(&a);
        let _ = std::fs::remove_dir_all(&b);
    }

    /// A page that appears in one emission and not the other is reported
    /// in whichever direction it went, so neither a lost page nor a
    /// surprise one passes quietly.
    #[test]
    fn a_page_emitted_only_once_is_reported_either_way() {
        let a = emitted("miss-a", &[("about", "<p>x</p>"), ("gone", "<p>y</p>")]);
        let b = emitted("miss-b", &[("about", "<p>x</p>"), ("new", "<p>z</p>")]);
        let found = compare(&a, &b).expect("compare");
        assert!(
            found.contains(&Difference::Missing("gone".to_owned())),
            "{found:?}"
        );
        assert!(
            found.contains(&Difference::Extra("new".to_owned())),
            "{found:?}"
        );
        let _ = std::fs::remove_dir_all(&a);
        let _ = std::fs::remove_dir_all(&b);
    }

    /// A trailing newline is a difference the line walk cannot see, so it
    /// is reported by length rather than passed over.
    #[test]
    fn a_change_the_lines_agree_on_is_still_a_difference() {
        let a = emitted("len-a", &[("about", "<p>x</p>")]);
        let b = emitted("len-b", &[("about", "<p>x</p>\n")]);
        let found = compare(&a, &b).expect("compare");
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(matches!(found[0], Difference::Length { .. }), "{found:?}");
        let _ = std::fs::remove_dir_all(&a);
        let _ = std::fs::remove_dir_all(&b);
    }

    /// Comparing a directory with no pages in it is an error and not a
    /// pass: a gate that quietly compared nothing is worse than no gate.
    #[test]
    fn comparing_nothing_is_an_error() {
        let a = emitted("empty-a", &[]);
        std::fs::create_dir_all(&a).expect("root");
        let b = emitted("empty-b", &[]);
        std::fs::create_dir_all(&b).expect("root");
        assert!(verdict(&a, &b).is_err());
        let _ = std::fs::remove_dir_all(&a);
        let _ = std::fs::remove_dir_all(&b);
    }
}
