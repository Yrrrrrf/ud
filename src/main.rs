use clap::{Parser, Subcommand};
use owo_colors::OwoColorize;
use std::path::PathBuf;
use ud::core::contract::Ecosystem;
use ud::core::model::Verdict;
use ud::core::pipeline::Pipeline;
use ud::ecosystems::cargo::CargoEcosystem;

#[derive(Parser)]
#[command(name = "ud")]
#[command(version, about = "Up to Date — universal dependency updater", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Path to the manifest file or a directory containing one (defaults to current directory)
    path: Option<PathBuf>,

    /// Disable automatic updates (only show changes)
    #[arg(short = 'y', long)]
    preview: bool,
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
async fn main() -> miette::Result<()> {
    let cli = Cli::parse();

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
                "Could not find Cargo.toml in directory {}",
                input_path.display()
            ));
        }
    } else {
        input_path
    };

    let should_update = !cli.preview && !is_tree;

    if cli.preview && !is_tree {
        println!(
            "{}",
            "Preview mode: no changes will be made"
                .bright_black()
                .italic()
        );
    }

    let mut pipeline = Pipeline::new();
    pipeline.register(Box::new(CargoEcosystem::new()));

    let report = pipeline.run(&manifest_path).await?;

    for (dep, verdict) in report.verdicts {
        match verdict {
            Verdict::Current { latest } => {
                if is_tree {
                    let version_str = if is_prerelease(&latest.0) {
                        latest.0.magenta().to_string()
                    } else {
                        latest.0.bright_black().to_string()
                    };
                    println!("  {} {}", dep.coordinate.0, version_str);
                }
            }
            Verdict::Outdated { target } => {
                let target_str = if is_prerelease(&target.0) {
                    target.0.magenta().bold().to_string()
                } else {
                    target.0.green().bold().to_string()
                };

                println!(
                    "  {} {} {} {}",
                    dep.coordinate.0,
                    dep.constraint.0.yellow(),
                    "→".bright_black(),
                    target_str,
                );

                if should_update {
                    let content = tokio::fs::read_to_string(&manifest_path)
                        .await
                        .map_err(|e| miette::miette!("Could not read manifest: {}", e))?;

                    let eco = CargoEcosystem::new();
                    let new_content = eco.write(&content, &dep, &target).await?;
                    tokio::fs::write(&manifest_path, new_content)
                        .await
                        .map_err(|e| miette::miette!("Could not write manifest: {}", e))?;
                    println!("    {}", "Updated!".green().italic());
                }
            }
            Verdict::Yanked => {
                println!("  {} {}", dep.coordinate.0, "! yanked".red().bold());
            }
            Verdict::Unsatisfiable { constraint } => {
                println!(
                    "  {} {} {}",
                    dep.coordinate.0,
                    constraint.0.bright_black(),
                    "✗ no versions found".red()
                );
            }
            Verdict::Errored(e) => {
                println!("  {} {} {}", dep.coordinate.0, "✗ error:".red(), e);
            }
        }
    }

    Ok(())
}

fn is_prerelease(version: &str) -> bool {
    semver::Version::parse(version)
        .map(|v| !v.pre.is_empty())
        .unwrap_or(false)
}
