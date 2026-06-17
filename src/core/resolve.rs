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

pub fn resolve(
    dependency: &Dependency,
    availability: &Availability,
    scheme: &dyn Scheme,
) -> Verdict {
    // For now, let's assume the constraint contains a version we can compare against.
    // In a more complex resolver, we might need to know the current 'resolved' version.
    // But for ud v1, we often just look at the manifest's declared version/constraint.

    // Attempt to treat the constraint as a concrete version for comparison purposes.
    // This is a simplification that we might refine later.
    let current_version = Version(dependency.constraint.0.clone());

    let mut candidates: Vec<_> = availability.versions.iter().filter(|v| !v.yanked).collect();

    if candidates.is_empty() {
        return Verdict::Unsatisfiable {
            constraint: dependency.constraint.clone(),
        };
    }

    // Sort candidates using the scheme: newest last.
    candidates.sort_by(|a, b| {
        if scheme.is_newer(&a.version, &b.version) {
            std::cmp::Ordering::Less
        } else if scheme.is_newer(&b.version, &a.version) {
            std::cmp::Ordering::Greater
        } else {
            std::cmp::Ordering::Equal
        }
    });

    let latest = candidates.last().unwrap();

    if scheme.is_newer(&current_version, &latest.version) {
        Verdict::Outdated {
            target: latest.version.clone(),
        }
    } else {
        Verdict::Current {
            latest: latest.version.clone(),
        }
    }
}
