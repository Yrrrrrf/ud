use derive_more::{Display, From, Into};
use serde::Serialize;

/// Coordinate — what uniquely names a thing to be versioned.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Display, From, Into, Serialize)]
pub struct Coordinate(pub String);

/// Constraint — what the manifest asks for.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Display, From, Into, Serialize)]
pub struct Constraint(pub String);

/// Version — a single concrete release.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Display, From, Into, Serialize)]
pub struct Version(pub String);

/// Span — the knowledge of where each dependency physically sits in the manifest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

/// Dependency — one declared requirement.
#[derive(Debug, Clone, Serialize)]
pub struct Dependency {
    pub coordinate: Coordinate,
    pub constraint: Constraint,
    pub span: Option<Span>,
    pub source_hint: Option<String>,
}

/// Availability — what a source returns for a coordinate.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Availability {
    pub versions: Vec<VersionMetadata>,
}

/// VersionMetadata — a single version with its associated metadata.
#[derive(Debug, Clone, Serialize)]
pub struct VersionMetadata {
    pub version: Version,
    pub yanked: bool,
    pub prerelease: bool,
}

/// Verdict — the core's judgment per dependency.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type")]
pub enum Verdict {
    Current {
        latest: Version,
        latest_pre: Option<Version>,
    },
    Outdated {
        target: Version,
        breaking: bool,
        latest_pre: Option<Version>,
    },
    Yanked,
    Unsatisfiable {
        constraint: Constraint,
    },
    Errored(String),
}

/// Report — the collected verdicts.
#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub verdicts: Vec<(Dependency, Verdict)>,
}

impl Default for Report {
    fn default() -> Self {
        Self::new()
    }
}

impl Report {
    pub fn new() -> Self {
        Self {
            verdicts: Vec::new(),
        }
    }

    pub fn push(&mut self, dependency: Dependency, verdict: Verdict) {
        self.verdicts.push((dependency, verdict));
    }
}
