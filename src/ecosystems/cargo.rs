use crate::core::contract::{Ecosystem, Scheme};
use crate::core::model::{
    Availability, Constraint, Coordinate, Dependency, Span, Version, VersionMetadata,
};
use crate::core::resolve::SemverScheme;
use async_trait::async_trait;
use std::path::Path;
use toml_edit::DocumentMut;

use serde::Deserialize;

#[derive(Deserialize)]
struct SparseIndexEntry {
    vers: String,
    yanked: bool,
}

fn sparse_index_path(name: &str) -> String {
    let name_lower = name.to_lowercase();
    let chars: Vec<char> = name_lower.chars().collect();
    let len = chars.len();
    if len == 1 {
        format!("1/{}", name_lower)
    } else if len == 2 {
        format!("2/{}", name_lower)
    } else if len == 3 {
        format!("3/{}/{}", chars[0], name_lower)
    } else {
        let first_two: String = chars[0..2].iter().collect();
        let next_two: String = chars[2..4].iter().collect();
        format!("{}/{}/{}", first_two, next_two, name_lower)
    }
}

pub struct CargoEcosystem {
    client: reqwest::Client,
}

impl Default for CargoEcosystem {
    fn default() -> Self {
        Self::new()
    }
}

impl CargoEcosystem {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .user_agent("ud (https://github.com/Yrrrrrf/ud)")
                .build()
                .unwrap(),
        }
    }
}

fn parse_table(table: &dyn toml_edit::TableLike, deps: &mut Vec<Dependency>) {
    for (name, value) in table.iter() {
        let coordinate = Coordinate(name.to_string());

        let constraint = if let Some(v) = value.as_str() {
            Constraint(v.to_string())
        } else if let Some(v) = value.get("version").and_then(|v| v.as_str()) {
            Constraint(v.to_string())
        } else {
            continue;
        };

        let span = value.span().map(|s| Span {
            start: s.start,
            end: s.end,
        });

        deps.push(Dependency {
            coordinate,
            constraint,
            span,
            source_hint: None,
        });
    }
}

fn update_table_item(item: &mut toml_edit::Item, new_version: &Version) {
    if let Some(inline) = item.as_inline_table_mut() {
        if let Some(v) = inline.get_mut("version") {
            let decor = v.decor().clone();
            *v = toml_edit::Value::from(new_version.0.clone());
            *v.decor_mut() = decor;
        }
    } else if let Some(v) = item.as_value_mut() {
        let decor = v.decor().clone();
        *v = toml_edit::Value::from(new_version.0.clone());
        *v.decor_mut() = decor;
    } else if let Some(table) = item.as_table_like_mut()
        && let Some(v) = table.get_mut("version")
    {
        if let Some(val) = v.as_value_mut() {
            let decor = val.decor().clone();
            *val = toml_edit::Value::from(new_version.0.clone());
            *val.decor_mut() = decor;
        } else {
            *v = toml_edit::value(new_version.0.clone());
        }
    }
}

#[async_trait]
impl Ecosystem for CargoEcosystem {
    fn name(&self) -> &'static str {
        "cargo"
    }

    async fn detect(&self, path: &Path, _content: Option<&str>) -> bool {
        path.file_name().is_some_and(|name| name == "Cargo.toml")
    }

    async fn read(&self, content: &str) -> miette::Result<Vec<Dependency>> {
        let doc: DocumentMut = content
            .parse()
            .map_err(|e| miette::miette!("Failed to parse Cargo.toml: {}", e))?;

        let mut deps = Vec::new();

        // 1. Root dependencies
        for section in &["dependencies", "dev-dependencies", "build-dependencies"] {
            if let Some(table) = doc.get(section).and_then(|v| v.as_table_like()) {
                parse_table(table, &mut deps);
            }
        }

        // 2. Workspace dependencies
        if let Some(table) = doc
            .get("workspace")
            .and_then(|w| w.get("dependencies"))
            .and_then(|v| v.as_table_like())
        {
            parse_table(table, &mut deps);
        }

        // 3. Target-specific dependencies
        if let Some(target_table) = doc.get("target").and_then(|t| t.as_table_like()) {
            for (_target_name, target_val) in target_table.iter() {
                for section in &["dependencies", "dev-dependencies", "build-dependencies"] {
                    if let Some(table) = target_val.get(section).and_then(|v| v.as_table_like()) {
                        parse_table(table, &mut deps);
                    }
                }
            }
        }

        Ok(deps)
    }

    async fn source(&self, coordinate: &Coordinate) -> miette::Result<Availability> {
        let path = sparse_index_path(&coordinate.0);
        let url = format!("https://index.crates.io/{}", path);
        let res = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| miette::miette!("Network error for {}: {}", coordinate.0, e))?;

        if res.status() == 404 {
            return Ok(Availability::default());
        }

        if !res.status().is_success() {
            return Err(miette::miette!(
                "HTTP error {} for {}",
                res.status(),
                coordinate.0
            ));
        }

        let body = res.text().await.map_err(|e| {
            miette::miette!("Failed to read response body for {}: {}", coordinate.0, e)
        })?;

        let mut versions = Vec::new();
        for line in body.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let entry: SparseIndexEntry = serde_json::from_str(line).map_err(|e| {
                miette::miette!(
                    "Failed to parse index JSON for {}: {} (line: {})",
                    coordinate.0,
                    e,
                    line
                )
            })?;

            let is_prerelease = match semver::Version::parse(&entry.vers) {
                Ok(parsed) => !parsed.pre.is_empty(),
                Err(_) => false,
            };

            versions.push(VersionMetadata {
                version: Version(entry.vers),
                yanked: entry.yanked,
                prerelease: is_prerelease,
            });
        }

        Ok(Availability { versions })
    }

    fn scheme(&self) -> &dyn Scheme {
        &SemverScheme
    }

    async fn write(
        &self,
        content: &str,
        dependency: &Dependency,
        new_version: &Version,
    ) -> miette::Result<String> {
        let mut doc: DocumentMut = content
            .parse()
            .map_err(|e| miette::miette!("Failed to parse Cargo.toml: {}", e))?;

        // 1. Root dependencies
        for section in &["dependencies", "dev-dependencies", "build-dependencies"] {
            if let Some(table) = doc.get_mut(section).and_then(|v| v.as_table_like_mut())
                && let Some(item) = table.get_mut(&dependency.coordinate.0)
            {
                update_table_item(item, new_version);
            }
        }

        // 2. Workspace dependencies
        if let Some(table) = doc
            .get_mut("workspace")
            .and_then(|w| w.get_mut("dependencies"))
            .and_then(|v| v.as_table_like_mut())
            && let Some(item) = table.get_mut(&dependency.coordinate.0)
        {
            update_table_item(item, new_version);
        }

        // 3. Target dependencies
        if let Some(target_table) = doc.get_mut("target").and_then(|t| t.as_table_like_mut()) {
            for (_target_name, target_val) in target_table.iter_mut() {
                for section in &["dependencies", "dev-dependencies", "build-dependencies"] {
                    if let Some(table) = target_val
                        .get_mut(section)
                        .and_then(|v| v.as_table_like_mut())
                        && let Some(item) = table.get_mut(&dependency.coordinate.0)
                    {
                        update_table_item(item, new_version);
                    }
                }
            }
        }

        Ok(doc.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_cargo_read() {
        let content = r#"
[package]
name = "test"
version = "0.1.0"

[dependencies]
serde = "1.0.0"
tokio = { version = "1.0", features = ["full"] }
"#;
        let eco = CargoEcosystem::new();
        let deps = eco.read(content).await.unwrap();
        assert_eq!(deps.len(), 2);
        assert_eq!(deps[0].coordinate.0, "serde");
        assert_eq!(deps[0].constraint.0, "1.0.0");
        assert_eq!(deps[1].coordinate.0, "tokio");
        assert_eq!(deps[1].constraint.0, "1.0");
    }

    #[tokio::test]
    async fn test_cargo_write() {
        let content = r#"
[dependencies]
serde = "1.0.0" # some comment
"#;
        let eco = CargoEcosystem::new();
        let dep = Dependency {
            coordinate: Coordinate("serde".to_string()),
            constraint: Constraint("1.0.0".to_string()),
            span: Some(Span { start: 0, end: 0 }),
            source_hint: None,
        };
        let new_version = Version("1.0.219".to_string());
        let new_content = eco.write(content, &dep, &new_version).await.unwrap();
        assert!(new_content.contains("serde = \"1.0.219\""));
        assert!(new_content.contains("# some comment"));
    }

    #[test]
    fn test_sparse_index_path() {
        assert_eq!(sparse_index_path("a"), "1/a");
        assert_eq!(sparse_index_path("ab"), "2/ab");
        assert_eq!(sparse_index_path("abc"), "3/a/abc");
        assert_eq!(sparse_index_path("abcd"), "ab/cd/abcd");
        assert_eq!(sparse_index_path("serde"), "se/rd/serde");
        // Check case insensitivity
        assert_eq!(sparse_index_path("SerDe"), "se/rd/serde");
    }

    #[test]
    fn test_sparse_index_parse() {
        let ndjson = r#"
{"name":"serde","vers":"1.0.0","deps":[],"cksum":"123","features":{},"yanked":false}
{"name":"serde","vers":"1.0.1-alpha.1","deps":[],"cksum":"456","features":{},"yanked":true}
"#;
        let mut entries = Vec::new();
        for line in ndjson.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let entry: SparseIndexEntry = serde_json::from_str(line).unwrap();
            entries.push(entry);
        }
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].vers, "1.0.0");
        assert!(!entries[0].yanked);
        assert_eq!(entries[1].vers, "1.0.1-alpha.1");
        assert!(entries[1].yanked);
    }

    #[tokio::test]
    async fn test_workspace_and_target_dependencies() {
        let content = r#"
[workspace.dependencies]
workspace-dep = "1.2.3"

[target.'cfg(target_os = "linux")'.dependencies]
linux-dep = "0.4.5"
"#;
        let eco = CargoEcosystem::new();
        let deps = eco.read(content).await.unwrap();
        assert_eq!(deps.len(), 2);

        let ws_dep = deps
            .iter()
            .find(|d| d.coordinate.0 == "workspace-dep")
            .unwrap();
        assert_eq!(ws_dep.constraint.0, "1.2.3");

        let target_dep = deps.iter().find(|d| d.coordinate.0 == "linux-dep").unwrap();
        assert_eq!(target_dep.constraint.0, "0.4.5");

        // Test writing back to workspace
        let ws_version = Version("1.2.4".to_string());
        let updated_content = eco.write(content, ws_dep, &ws_version).await.unwrap();
        assert!(updated_content.contains("workspace-dep = \"1.2.4\""));

        // Test writing back to target
        let target_version = Version("0.4.6".to_string());
        let updated_content2 = eco
            .write(content, target_dep, &target_version)
            .await
            .unwrap();
        assert!(updated_content2.contains("linux-dep = \"0.4.6\""));
    }
}
