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
struct CratesIoResponse {
    versions: Vec<CratesIoVersion>,
}

#[derive(Deserialize)]
struct CratesIoVersion {
    num: String,
    yanked: bool,
}

pub struct CargoEcosystem {
    client: reqwest::Client,
}

impl CargoEcosystem {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .user_agent("ud (https://github.com/filllabs/ud)")
                .build()
                .unwrap(),
        }
    }
}

#[async_trait]
impl Ecosystem for CargoEcosystem {
    fn name(&self) -> &'static str {
        "cargo"
    }

    async fn detect(&self, path: &Path, _content: Option<&str>) -> bool {
        path.file_name().map_or(false, |name| name == "Cargo.toml")
    }

    async fn read(&self, content: &str) -> miette::Result<Vec<Dependency>> {
        let doc: DocumentMut = content
            .parse()
            .map_err(|e| miette::miette!("Failed to parse Cargo.toml: {}", e))?;

        let mut deps = Vec::new();

        for section in &["dependencies", "dev-dependencies", "build-dependencies"] {
            if let Some(table) = doc.get(section).and_then(|v| v.as_table_like()) {
                for (name, value) in table.iter() {
                    let coordinate = Coordinate(name.to_string());

                    let constraint = if let Some(v) = value.as_str() {
                        Constraint(v.to_string())
                    } else if let Some(v) = value.get("version").and_then(|v| v.as_str()) {
                        Constraint(v.to_string())
                    } else {
                        continue;
                    };

                    let span = value
                        .span()
                        .map(|s| Span {
                            start: s.start,
                            end: s.end,
                        })
                        .unwrap_or(Span { start: 0, end: 0 });

                    deps.push(Dependency {
                        coordinate,
                        constraint,
                        span,
                        source_hint: None,
                    });
                }
            }
        }

        Ok(deps)
    }

    async fn source(&self, coordinate: &Coordinate) -> miette::Result<Availability> {
        let url = format!("https://crates.io/api/v1/crates/{}", coordinate.0);
        let res = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| miette::miette!("Network error for {}: {}", coordinate.0, e))?;

        if res.status() == 404 {
            return Ok(Availability::default());
        }

        let json: CratesIoResponse = res
            .json()
            .await
            .map_err(|e| miette::miette!("Failed to parse JSON for {}: {}", coordinate.0, e))?;

        let versions = json
            .versions
            .into_iter()
            .map(|v| {
                let is_prerelease = match semver::Version::parse(&v.num) {
                    Ok(parsed) => !parsed.pre.is_empty(),
                    Err(_) => false,
                };
                VersionMetadata {
                    version: Version(v.num),
                    yanked: v.yanked,
                    prerelease: is_prerelease,
                }
            })
            .collect();

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

        for section in &["dependencies", "dev-dependencies", "build-dependencies"] {
            if let Some(table) = doc.get_mut(section).and_then(|v| v.as_table_like_mut()) {
                if let Some(item) = table.get_mut(&dependency.coordinate.0) {
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
                    } else if let Some(table) = item.as_table_like_mut() {
                        if let Some(v) = table.get_mut("version") {
                            if let Some(val) = v.as_value_mut() {
                                let decor = val.decor().clone();
                                *val = toml_edit::Value::from(new_version.0.clone());
                                *val.decor_mut() = decor;
                            } else {
                                *v = toml_edit::value(new_version.0.clone());
                            }
                        }
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
            span: Span { start: 0, end: 0 },
            source_hint: None,
        };
        let new_version = Version("1.0.219".to_string());
        let new_content = eco.write(content, &dep, &new_version).await.unwrap();
        assert!(new_content.contains("serde = \"1.0.219\""));
        assert!(new_content.contains("# some comment"));
    }
}
