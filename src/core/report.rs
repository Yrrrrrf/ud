use crate::core::model::{Report, Verdict};
use owo_colors::OwoColorize;

pub struct HumanReporter;

impl HumanReporter {
    pub fn render(report: &Report, is_tree: bool, should_update: bool) -> String {
        let mut buf = String::new();
        for (dep, verdict) in &report.verdicts {
            match verdict {
                Verdict::Current { latest, latest_pre } => {
                    if is_tree {
                        let version_str = latest.0.bright_black().to_string();
                        let pre_suffix = if let Some(pre) = latest_pre {
                            format!(" ({})", pre.0)
                                .magenta()
                                .dimmed()
                                .italic()
                                .to_string()
                        } else {
                            String::new()
                        };
                        buf.push_str(&format!(
                            "  {} {}{}\n",
                            dep.coordinate.0, version_str, pre_suffix
                        ));
                    }
                }
                Verdict::Outdated {
                    target,
                    breaking: _,
                    latest_pre,
                } => {
                    let old_str = format_old_constraint(&dep.constraint.0, &target.0);
                    let target_str = if is_prerelease(&target.0) {
                        target.0.magenta().to_string()
                    } else {
                        target.0.green().to_string()
                    };

                    let pre_suffix = if let Some(pre) = latest_pre {
                        format!(" ({})", pre.0)
                            .magenta()
                            .dimmed()
                            .italic()
                            .to_string()
                    } else {
                        String::new()
                    };

                    buf.push_str(&format!(
                        "  {} {} {} {}{}\n",
                        dep.coordinate.0,
                        old_str,
                        "→".bright_black(),
                        target_str,
                        pre_suffix,
                    ));

                    if should_update {
                        buf.push_str(&format!("    {}\n", "Updated!".green().italic()));
                    }
                }
                Verdict::Yanked => {
                    buf.push_str(&format!(
                        "  {} {}\n",
                        dep.coordinate.0,
                        "! yanked".red().bold()
                    ));
                }
                Verdict::Unsatisfiable { constraint } => {
                    buf.push_str(&format!(
                        "  {} {} {}\n",
                        dep.coordinate.0,
                        constraint.0.bright_black(),
                        "✗ no versions found".red()
                    ));
                }
                Verdict::Errored(e) => {
                    buf.push_str(&format!(
                        "  {} {} {}\n",
                        dep.coordinate.0,
                        "✗ error:".red(),
                        e
                    ));
                }
            }
        }
        buf
    }
}

fn format_old_constraint(constraint: &str, target: &str) -> String {
    let version_start = constraint.find(|c: char| c.is_ascii_digit()).unwrap_or(0);
    let prefix_symbols = &constraint[..version_start];
    let version_part = &constraint[version_start..];

    let mut common_len = 0;
    for (c1, c2) in version_part.chars().zip(target.chars()) {
        if c1 == c2 {
            common_len += c1.len_utf8();
        } else {
            break;
        }
    }

    let (same_part, diff_part) = version_part.split_at(common_len);

    format!(
        "{}{}{}",
        prefix_symbols.bright_black(),
        same_part.bright_black(),
        diff_part.yellow(),
    )
}

pub struct JsonReporter;

impl JsonReporter {
    pub fn render(report: &Report) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(report)
    }
}

fn is_prerelease(version: &str) -> bool {
    semver::Version::parse(version)
        .map(|v| !v.pre.is_empty())
        .unwrap_or(false)
}
