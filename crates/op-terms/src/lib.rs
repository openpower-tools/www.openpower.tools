//! Controlled vocabularies: the classifications the site talks about,
//! as data rather than as styling hints.
//!
//! A term is a concept in a scheme (`outcome:pass`, `support:patched`,
//! `flight:Toward`). Terms are contained in broader ones - here every
//! scheme's terms sit under one of the five *severity* concepts - and a
//! badge colour, a KPI tint or a table cell are merely projections of
//! that containment onto the UI. Markup names the term
//! (`<opt-term scheme="outcome" value="pass">`); the projection is
//! derived, so it can never disagree with the meaning and a new term
//! cannot ship without a place in the graph (tested).

/// The five severity concepts every other term is contained in; their
/// names are also the palette's status token suffixes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Neutral,
    Info,
    Ok,
    Warning,
    Danger,
}

impl Severity {
    /// The status token suffix (`--op-status-<name>`) and badge variant.
    pub fn name(self) -> &'static str {
        match self {
            Self::Neutral => "neutral",
            Self::Info => "info",
            Self::Ok => "ok",
            Self::Warning => "warning",
            Self::Danger => "danger",
        }
    }
}

/// One term of a scheme, with a human description and its container.
pub struct Term {
    pub scheme: &'static str,
    pub value: &'static str,
    pub description: &'static str,
    pub broader: Severity,
}

/// Every term the site uses, grouped by scheme.
pub const TERMS: &[Term] = &[
    // outcomes of a check in the interaction report
    Term {
        scheme: "outcome",
        value: "pass",
        description: "the check held",
        broader: Severity::Ok,
    },
    Term {
        scheme: "outcome",
        value: "fail",
        description: "the check did not hold",
        broader: Severity::Danger,
    },
    Term {
        scheme: "outcome",
        value: "skipped",
        description: "the check was not run",
        broader: Severity::Neutral,
    },
    // software support on POWER (the can-i-use matrix)
    Term {
        scheme: "support",
        value: "upstream",
        description: "works from the project or distribution itself",
        broader: Severity::Ok,
    },
    Term {
        scheme: "support",
        value: "patched",
        description: "works via maintained downstream patches or packages",
        broader: Severity::Info,
    },
    Term {
        scheme: "support",
        value: "in-progress",
        description: "active effort; usable with caveats or not yet usable",
        broader: Severity::Warning,
    },
    Term {
        scheme: "support",
        value: "broken",
        description: "currently broken",
        broader: Severity::Danger,
    },
    Term {
        scheme: "support",
        value: "unsupported",
        description: "no support and none planned",
        broader: Severity::Danger,
    },
    Term {
        scheme: "support",
        value: "unknown",
        description: "not yet verified - help wanted",
        broader: Severity::Neutral,
    },
    // the flight states of the switch-like interaction machine
    Term {
        scheme: "flight",
        value: "Idle",
        description: "no change in flight",
        broader: Severity::Neutral,
    },
    Term {
        scheme: "flight",
        value: "Toward",
        description: "travelling to a new setting",
        broader: Severity::Info,
    },
    Term {
        scheme: "flight",
        value: "Back",
        description: "returning after an abort",
        broader: Severity::Warning,
    },
    // the severities themselves, so a page can name one directly
    Term {
        scheme: "severity",
        value: "neutral",
        description: "no particular standing",
        broader: Severity::Neutral,
    },
    Term {
        scheme: "severity",
        value: "info",
        description: "worth knowing",
        broader: Severity::Info,
    },
    Term {
        scheme: "severity",
        value: "ok",
        description: "as it should be",
        broader: Severity::Ok,
    },
    Term {
        scheme: "severity",
        value: "warning",
        description: "attention needed",
        broader: Severity::Warning,
    },
    Term {
        scheme: "severity",
        value: "danger",
        description: "something is wrong",
        broader: Severity::Danger,
    },
];

/// Looks a term up; `None` for a value the vocabulary does not have.
pub fn lookup(scheme: &str, value: &str) -> Option<&'static Term> {
    TERMS
        .iter()
        .find(|t| t.scheme == scheme && t.value == value)
}

/// The severity a term projects to, or `Neutral` for the unknown.
pub fn severity_of(scheme: &str, value: &str) -> Severity {
    lookup(scheme, value).map_or(Severity::Neutral, |t| t.broader)
}

/// The terms of one scheme, in declaration order.
pub fn scheme(name: &str) -> impl Iterator<Item = &'static Term> {
    TERMS.iter().filter(move |t| t.scheme == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terms_are_unique_within_their_scheme_and_named_plainly() {
        let mut seen = std::collections::HashSet::new();
        for t in TERMS {
            assert!(
                seen.insert((t.scheme, t.value)),
                "duplicate term {}:{}",
                t.scheme,
                t.value
            );
            assert!(
                !t.description.is_empty(),
                "{}:{} has no description",
                t.scheme,
                t.value
            );
            assert!(
                t.value.chars().all(|c| c.is_alphanumeric() || c == '-'),
                "{}:{} is not a plain token",
                t.scheme,
                t.value
            );
        }
    }

    #[test]
    fn every_severity_names_itself_and_is_its_own_container() {
        for s in [
            Severity::Neutral,
            Severity::Info,
            Severity::Ok,
            Severity::Warning,
            Severity::Danger,
        ] {
            assert_eq!(severity_of("severity", s.name()), s);
        }
    }

    #[test]
    fn unknown_terms_project_to_neutral_and_look_up_as_none() {
        assert!(lookup("outcome", "maybe").is_none());
        assert_eq!(severity_of("outcome", "maybe"), Severity::Neutral);
        assert_eq!(severity_of("nope", "pass"), Severity::Neutral);
    }

    #[test]
    fn the_schemes_the_site_relies_on_are_complete() {
        let outcomes: Vec<&str> = scheme("outcome").map(|t| t.value).collect();
        assert_eq!(outcomes, ["pass", "fail", "skipped"]);
        let support: Vec<&str> = scheme("support").map(|t| t.value).collect();
        assert_eq!(
            support,
            [
                "upstream",
                "patched",
                "in-progress",
                "broken",
                "unsupported",
                "unknown"
            ]
        );
        let flight: Vec<&str> = scheme("flight").map(|t| t.value).collect();
        assert_eq!(flight, ["Idle", "Toward", "Back"]);
    }
}
