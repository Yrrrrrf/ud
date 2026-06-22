use crate::core::contract::Scheme;
use crate::core::model::{Availability, Dependency, Verdict, Version};
use semver::{Version as SemVersion, VersionReq};

pub struct SemverScheme;

impl Scheme for SemverScheme {
    fn is_newer(&self, a: &Version, b: &Version) -> bool {
        let a_v = SemVersion::parse(&a.0);
        let b_v = SemVersion::parse(&b.0);

        match (a_v, b_v) {
            (Ok(a), Ok(b)) => b > a,
            _ => false,
        }
    }

    fn satisfies(&self, version: &Version, constraint: &crate::core::model::Constraint) -> bool {
        let v = SemVersion::parse(&version.0);
        let req = VersionReq::parse(&constraint.0);

        match (v, req) {
            (Ok(v), Ok(req)) => req.matches(&v),
            _ => false,
        }
    }
}

fn declared_version(constraint: &str) -> Option<SemVersion> {
    let req = VersionReq::parse(constraint).ok()?;
    let comp = req
        .comparators
        .iter()
        .find(|c| !matches!(c.op, semver::Op::Less | semver::Op::LessEq));

    let version = match comp {
        Some(c) => SemVersion {
            major: c.major,
            minor: c.minor.unwrap_or(0),
            patch: c.patch.unwrap_or(0),
            pre: c.pre.clone(),
            build: semver::BuildMetadata::EMPTY,
        },
        None => SemVersion {
            major: 0,
            minor: 0,
            patch: 0,
            pre: semver::Prerelease::EMPTY,
            build: semver::BuildMetadata::EMPTY,
        },
    };
    Some(version)
}

pub fn is_actual_upgrade(constraint: &str, target: &str) -> bool {
    let parsed_target = SemVersion::parse(target).ok();
    let base_version = declared_version(constraint);

    match (parsed_target, base_version) {
        (Some(t), Some(b)) => t > b,
        _ => target != constraint,
    }
}

pub fn resolve(
    dependency: &Dependency,
    availability: &Availability,
    scheme: &dyn Scheme,
    include_prerelease: bool,
) -> Verdict {
    let mut parsed_candidates: Vec<(&crate::core::model::VersionMetadata, Option<SemVersion>)> =
        availability
            .versions
            .iter()
            .filter(|v| !v.yanked)
            .filter(|v| include_prerelease || !v.prerelease)
            .map(|v| {
                let parsed = SemVersion::parse(&v.version.0).ok();
                (v, parsed)
            })
            .collect();

    if parsed_candidates.is_empty() {
        return Verdict::Unsatisfiable {
            constraint: dependency.constraint.clone(),
        };
    }

    // Sort parsed_candidates: newest last.
    parsed_candidates.sort_by(|a, b| match (&a.1, &b.1) {
        (Some(av), Some(bv)) => av.cmp(bv),
        (Some(_), None) => std::cmp::Ordering::Greater,
        (None, Some(_)) => std::cmp::Ordering::Less,
        (None, None) => a.0.version.0.cmp(&b.0.version.0),
    });

    let (latest_meta, _) = parsed_candidates.last().unwrap();
    let latest_version = latest_meta.version.clone();

    // Compute the latest compatible version (satisfies the declared constraint)
    let latest_compatible = parsed_candidates
        .iter()
        .rev()
        .find(|(meta, _)| scheme.satisfies(&meta.version, &dependency.constraint))
        .map(|(meta, _)| meta.version.clone());

    // Compute the latest prerelease (if newer than latest stable)
    let latest_pre = availability
        .versions
        .iter()
        .filter(|v| !v.yanked && v.prerelease)
        .filter_map(|v| {
            let parsed = SemVersion::parse(&v.version.0).ok()?;
            Some((v, parsed))
        })
        .max_by(|a, b| a.1.cmp(&b.1))
        .map(|(v, _)| v.version.clone())
        .filter(|lp| {
            let lp_v = SemVersion::parse(&lp.0).ok();
            let target_v = SemVersion::parse(&latest_version.0).ok();
            match (lp_v, target_v) {
                (Some(lp_v), Some(target_v)) => lp_v > target_v,
                _ => false,
            }
        });

    let declared_semver = declared_version(&dependency.constraint.0);
    let declared_version = match &declared_semver {
        Some(sv) => Version(sv.to_string()),
        None => Version(dependency.constraint.0.clone()),
    };

    if scheme.is_newer(&declared_version, &latest_version) {
        Verdict::Outdated {
            compatible: latest_compatible,
            latest: latest_version,
            latest_pre,
        }
    } else {
        Verdict::Current {
            latest: latest_version,
            latest_pre,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::{Coordinate, Span, VersionMetadata};

    #[test]
    fn test_declared_version() {
        assert_eq!(
            declared_version("1.36").unwrap(),
            SemVersion::parse("1.36.0").unwrap()
        );
        assert_eq!(
            declared_version("^1.2").unwrap(),
            SemVersion::parse("1.2.0").unwrap()
        );
        assert_eq!(
            declared_version(">=1, <2").unwrap(),
            SemVersion::parse("1.0.0").unwrap()
        );
        assert_eq!(
            declared_version("=2.3.4").unwrap(),
            SemVersion::parse("2.3.4").unwrap()
        );
        assert_eq!(
            declared_version("<2").unwrap(),
            SemVersion::parse("0.0.0").unwrap()
        );
    }

    #[test]
    fn test_resolve_partial_version() {
        let dep = Dependency {
            coordinate: Coordinate("tokio".into()),
            constraint: crate::core::model::Constraint("1.36".into()),
            span: Some(Span { start: 0, end: 0 }),
            source_hint: None,
            section: None,
        };
        let avail = Availability {
            versions: vec![
                VersionMetadata {
                    version: Version("1.36.0".into()),
                    yanked: false,
                    prerelease: false,
                },
                VersionMetadata {
                    version: Version("1.45.0".into()),
                    yanked: false,
                    prerelease: false,
                },
            ],
        };
        let verdict = resolve(&dep, &avail, &SemverScheme, false);
        assert_eq!(
            verdict,
            Verdict::Outdated {
                compatible: Some(Version("1.45.0".into())),
                latest: Version("1.45.0".into()),
                latest_pre: None,
            }
        );
    }

    #[test]
    fn test_resolve_compatible_and_breaking() {
        let dep = Dependency {
            coordinate: Coordinate("serde".into()),
            constraint: crate::core::model::Constraint("^1.2".into()),
            span: Some(Span { start: 0, end: 0 }),
            source_hint: None,
            section: None,
        };
        let avail = Availability {
            versions: vec![
                VersionMetadata {
                    version: Version("1.2.0".into()),
                    yanked: false,
                    prerelease: false,
                },
                VersionMetadata {
                    version: Version("1.9.0".into()),
                    yanked: false,
                    prerelease: false,
                },
                VersionMetadata {
                    version: Version("2.0.0".into()),
                    yanked: false,
                    prerelease: false,
                },
            ],
        };
        // latest is 2.0.0, but compatible (satisfying ^1.2) is 1.9.0
        let verdict = resolve(&dep, &avail, &SemverScheme, false);
        assert_eq!(
            verdict,
            Verdict::Outdated {
                compatible: Some(Version("1.9.0".into())),
                latest: Version("2.0.0".into()),
                latest_pre: None,
            }
        );
    }

    #[test]
    fn test_resolve_prerelease() {
        let dep = Dependency {
            coordinate: Coordinate("foo".into()),
            constraint: crate::core::model::Constraint("^1.9".into()),
            span: Some(Span { start: 0, end: 0 }),
            source_hint: None,
            section: None,
        };
        let avail = Availability {
            versions: vec![
                VersionMetadata {
                    version: Version("1.9.0".into()),
                    yanked: false,
                    prerelease: false,
                },
                VersionMetadata {
                    version: Version("2.0.0-rc.1".into()),
                    yanked: false,
                    prerelease: true,
                },
            ],
        };

        // Excludes prerelease by default
        assert_eq!(
            resolve(&dep, &avail, &SemverScheme, false),
            Verdict::Current {
                latest: Version("1.9.0".into()),
                latest_pre: Some(Version("2.0.0-rc.1".into())),
            }
        );

        // Includes prerelease when opt-in — 2.0.0-rc.1 is the latest overall,
        // but 1.9.0 still satisfies ^1.9 so it's the compatible target.
        assert_eq!(
            resolve(&dep, &avail, &SemverScheme, true),
            Verdict::Outdated {
                compatible: Some(Version("1.9.0".into())),
                latest: Version("2.0.0-rc.1".into()),
                latest_pre: None,
            }
        );
    }

    #[test]
    fn test_resolve_0x_breaking() {
        // For 0.x crates, minor bumps are breaking under semver caret rules
        let dep = Dependency {
            coordinate: Coordinate("tower".into()),
            constraint: crate::core::model::Constraint("^0.4.13".into()),
            span: None,
            source_hint: None,
            section: None,
        };
        let avail = Availability {
            versions: vec![
                VersionMetadata {
                    version: Version("0.4.13".into()),
                    yanked: false,
                    prerelease: false,
                },
                VersionMetadata {
                    version: Version("0.4.15".into()),
                    yanked: false,
                    prerelease: false,
                },
                VersionMetadata {
                    version: Version("0.5.2".into()),
                    yanked: false,
                    prerelease: false,
                },
            ],
        };
        let verdict = resolve(&dep, &avail, &SemverScheme, false);
        assert_eq!(
            verdict,
            Verdict::Outdated {
                compatible: Some(Version("0.4.15".into())),
                latest: Version("0.5.2".into()),
                latest_pre: None,
            }
        );
    }
}
