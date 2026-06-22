use crate::core::model::*;
use async_trait::async_trait;
use std::path::Path;

/// The Ecosystem trait is the heart of the plugin system.
/// Every plugin translates its world into the universal model.
#[async_trait]
pub trait Ecosystem: Send + Sync {
    /// The human-readable name of the ecosystem (e.g., "cargo", "npm").
    fn name(&self) -> &'static str;

    /// 1. Detect — "Is this manifest mine?"
    /// Given a path and optionally a peek at content, claim it or pass.
    async fn detect(&self, path: &Path, content: Option<&str>) -> bool;

    /// 2. Read — "What does this manifest declare?"
    /// Produce Dependency[] with spans.
    async fn read(&self, content: &str) -> miette::Result<Vec<Dependency>>;

    /// 3. Source — "What versions exist for this coordinate?"
    /// Produce Availability.
    async fn source(&self, coordinate: &Coordinate) -> miette::Result<Availability>;

    /// 4. Scheme — "How do versions order and satisfy constraints?"
    fn scheme(&self) -> &dyn Scheme;

    /// Optional: Write — "Set this dependency's version in the manifest, losslessly."
    async fn write(
        &self,
        _content: &str,
        _dependency: &Dependency,
        _new_version: &Version,
    ) -> miette::Result<String> {
        Err(miette::miette!(
            "Ecosystem {} does not support writing",
            self.name()
        ))
    }

    /// Optional: Write a batch of edits in a single pass.
    /// Default implementation calls `write()` sequentially (override for efficiency).
    async fn write_batch(
        &self,
        content: &str,
        edits: &[(&Dependency, &Version)],
    ) -> miette::Result<String> {
        let mut result = content.to_string();
        for (dep, version) in edits {
            result = self.write(&result, dep, version).await?;
        }
        Ok(result)
    }
}

/// A Version Scheme provides the rules for ordering versions and testing constraints.
pub trait Scheme: Send + Sync {
    /// Returns true if `b` is strictly newer than `a`.
    fn is_newer(&self, a: &Version, b: &Version) -> bool;

    /// Returns true if `version` satisfies `constraint`.
    fn satisfies(&self, version: &Version, constraint: &Constraint) -> bool;
}
