use clap::{Parser, Subcommand};
use owo_colors::OwoColorize;
use std::path::PathBuf;
use ud::core::contract::Ecosystem;
use ud::core::model::Verdict;
use ud::core::pipeline::Pipeline;
use ud::core::report::{HumanReporter, JsonReporter};
use ud::ecosystems::cargo::CargoEcosystem;

#[derive(Parser)]
#[command(name = "ud")]
#[command(version, about = "Up to Date — universal dependency updater")]
#[command(
    long_about = "ud checks your manifest for outdated dependencies and optionally updates them.\n\n\
    Exit codes:\n  \
    0  All dependencies are current (or successfully updated)\n  \
    1  Outdated dependencies detected (check mode)\n  \
    2  Hard error (missing file, parse failure, network error)"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Path to the manifest file or a directory containing one (defaults to current directory)
    path: Option<PathBuf>,

    /// Update the manifest file losslessly with compatible versions
    #[arg(short = 'u', long)]
    update: bool,

    /// Also apply breaking version bumps when updating (requires --update)
    #[arg(long = "allow-breaking")]
    allow_breaking: bool,

    /// Include prerelease versions
    #[arg(long = "pre")]
    pre: bool,

    /// Output the report as JSON
    #[arg(long)]
    json: bool,

    /// Enable verbose logging
    #[arg(short = 'v', long)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// List all dependencies with their status
    Tree {
        /// Path to the manifest file or directory (defaults to current directory)
        path: Option<PathBuf>,
    },
}

#[tokio::main]
async fn main() {
    match run().await {
        Ok(has_outdated) => {
            if has_outdated {
                std::process::exit(1);
            }
        }
        Err(e) => {
            // Friendly single-line error messages
            eprintln!("{} {}", "error:".red().bold(), e);
            std::process::exit(2);
        }
    }
}

async fn run() -> miette::Result<bool> {
    let cli = Cli::parse();
    ud::core::runtime::init_tracing(cli.verbose);

    let (input_path, is_tree) = match cli.command {
        Some(Commands::Tree { path }) => (path.unwrap_or_else(|| PathBuf::from(".")), true),
        None => (cli.path.unwrap_or_else(|| PathBuf::from(".")), false),
    };

    // Resolve directory to Cargo.toml
    let manifest_path = if input_path.is_dir() {
        let cargo_toml = input_path.join("Cargo.toml");
        if cargo_toml.exists() {
            cargo_toml
        } else {
            return Err(miette::miette!(
                "file not found: {}/Cargo.toml",
                input_path.display()
            ));
        }
    } else if !input_path.exists() {
        return Err(miette::miette!("file not found: {}", input_path.display()));
    } else {
        input_path
    };

    let should_update = cli.update && !is_tree;

    if !cli.update && !is_tree && !cli.json {
        eprintln!(
            "{}",
            "Check mode: no changes will be made"
                .bright_black()
                .italic()
        );
    }

    let mut pipeline = Pipeline::new().with_prerelease(cli.pre);
    pipeline.register(Box::new(CargoEcosystem::new()));

    let report = pipeline.run(&manifest_path).await?;

    let mut has_outdated = false;

    if should_update {
        // Single-pass update: collect all edits, apply once
        let mut edits = Vec::new();

        for (dep, verdict) in &report.verdicts {
            if let Verdict::Outdated {
                compatible, latest, ..
            } = verdict
            {
                has_outdated = true;

                // Determine the target version based on --allow-breaking
                let target = if cli.allow_breaking {
                    Some(latest)
                } else {
                    compatible.as_ref()
                };

                if let Some(version) = target
                    && ud::core::is_actual_upgrade(&dep.constraint.0, &version.0)
                {
                    edits.push((dep, version));
                }
            }
        }

        if !edits.is_empty() {
            let content = tokio::fs::read_to_string(&manifest_path)
                .await
                .map_err(|e| miette::miette!("could not read manifest: {}", e))?;

            let eco = CargoEcosystem::new();
            let new_content = eco.write_batch(&content, &edits).await?;
            tokio::fs::write(&manifest_path, new_content)
                .await
                .map_err(|e| miette::miette!("could not write manifest: {}", e))?;
        }
    } else {
        // Check mode: just detect outdated
        for (_, verdict) in &report.verdicts {
            if matches!(verdict, Verdict::Outdated { .. }) {
                has_outdated = true;
                break;
            }
        }
    }

    if cli.json {
        let output = JsonReporter::render(&report)
            .map_err(|e| miette::miette!("JSON serialization error: {}", e))?;
        println!("{}", output);
    } else {
        let output = HumanReporter::render(&report, is_tree, should_update);
        print!("{}", output);
    }

    Ok(has_outdated)
}
