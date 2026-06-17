use crate::core::contract::Ecosystem;
use crate::core::model::{Report, Verdict};
use std::path::Path;

pub struct Pipeline {
    ecosystems: Vec<Box<dyn Ecosystem>>,
}

impl Pipeline {
    pub fn new() -> Self {
        Self {
            ecosystems: Vec::new(),
        }
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
        let mut report = Report::new();

        // 3. RESOLVE (in parallel §5)
        let mut futures = Vec::new();
        for dep in deps {
            futures.push(async move {
                let availability = eco.source(&dep.coordinate).await;
                match availability {
                    Ok(avail) => {
                        let verdict = crate::core::resolve::resolve(&dep, &avail, eco.scheme());
                        (dep, verdict)
                    }
                    Err(e) => (dep, Verdict::Errored(e.to_string())),
                }
            });
        }

        let results = futures::future::join_all(futures).await;
        for (dep, verdict) in results {
            report.push(dep, verdict);
        }

        Ok(report)
    }
}
