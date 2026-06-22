use crate::core::contract::Ecosystem;
use crate::core::model::{Report, Verdict};
use std::path::Path;

pub struct Pipeline {
    ecosystems: Vec<Box<dyn Ecosystem>>,
    include_prerelease: bool,
}

impl Default for Pipeline {
    fn default() -> Self {
        Self::new()
    }
}

impl Pipeline {
    pub fn new() -> Self {
        Self {
            ecosystems: Vec::new(),
            include_prerelease: false,
        }
    }

    pub fn with_prerelease(mut self, include_prerelease: bool) -> Self {
        self.include_prerelease = include_prerelease;
        self
    }

    pub fn register(&mut self, ecosystem: Box<dyn Ecosystem>) {
        self.ecosystems.push(ecosystem);
    }

    pub async fn run(&self, path: &Path) -> miette::Result<Report> {
        // 1. DETECT
        let content = tokio::fs::read_to_string(path).await.ok();
        let mut matching_ecosystems = Vec::new();
        for eco in &self.ecosystems {
            if eco.detect(path, content.as_deref()).await {
                tracing::debug!(ecosystem = eco.name(), path = %path.display(), "detected ecosystem");
                matching_ecosystems.push(eco);
            }
        }

        if matching_ecosystems.is_empty() {
            return Err(miette::miette!(
                "No ecosystem found for path {}",
                path.display()
            ));
        }

        // For now, take the first one (deterministic resolution §4.1)
        let eco = matching_ecosystems[0];
        let content =
            content.ok_or_else(|| miette::miette!("Could not read file {}", path.display()))?;

        // 2. READ
        let deps = eco.read(&content).await?;
        tracing::debug!(count = deps.len(), "read dependencies from manifest");

        let mut report = Report::new();

        // 3. RESOLVE (in parallel §5)
        let mut futures = Vec::new();
        let include_prerelease = self.include_prerelease;
        for dep in deps {
            futures.push(async move {
                let availability = eco.source(&dep.coordinate).await;
                match availability {
                    Ok(avail) => {
                        let verdict = crate::core::resolve::resolve(
                            &dep,
                            &avail,
                            eco.scheme(),
                            include_prerelease,
                        );
                        tracing::debug!(
                            coordinate = %dep.coordinate.0,
                            verdict = ?verdict,
                            "resolved"
                        );
                        (dep, verdict)
                    }
                    Err(e) => (dep, Verdict::Errored(e.to_string())),
                }
            });
        }

        use futures::StreamExt;
        let results = futures::stream::iter(futures)
            .buffer_unordered(crate::core::runtime::MAX_CONCURRENT_REQUESTS)
            .collect::<Vec<_>>()
            .await;

        for (dep, verdict) in results {
            report.push(dep, verdict);
        }

        Ok(report)
    }
}
