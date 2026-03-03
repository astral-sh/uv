//! Generate sysconfig mappings for supported python-build-standalone *nix platforms.
use anstream::println;
use anyhow::{Result, bail};
use pretty_assertions::StrComparison;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fmt::Write;
use std::path::PathBuf;

use crate::ROOT_DIR;
use crate::generate_all::Mode;

/// Contains current supported targets
const TARGETS_YML_URL: &str = "https://raw.githubusercontent.com/astral-sh/python-build-standalone/refs/tags/20260211/cpython-unix/targets.yml";

#[derive(clap::Args)]
pub(crate) struct Args {
    #[arg(long, default_value_t, value_enum)]
    pub(crate) mode: Mode,
}

#[derive(Debug, Deserialize)]
struct TargetConfig {
    host_cc: Option<String>,
    host_cxx: Option<String>,
    target_cc: Option<String>,
    target_cxx: Option<String>,
}

pub(crate) async fn main(args: &Args) -> Result<()> {
    let reference_string = generate().await?;
    let filename = "generated_mappings.rs";
    let reference_path = PathBuf::from(ROOT_DIR)
        .join("crates")
        .join("uv-python")
        .join("src")
        .join("sysconfig")
        .join(filename);

    match args.mode {
        Mode::DryRun => {
            println!("{reference_string}");
        }
        Mode::Check => match fs_err::read_to_string(reference_path) {
            Ok(current) => {
                if current == reference_string {
                    println!("Up-to-date: {filename}");
                } else {
                    let comparison = StrComparison::new(&current, &reference_string);
                    bail!(
                        "{filename} changed, please run `cargo dev generate-sysconfig-metadata`:\n{comparison}"
                    );
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                bail!("{filename} not found, please run `cargo dev generate-sysconfig-metadata`");
            }
            Err(err) => {
                bail!(
                    "{filename} changed, please run `cargo dev generate-sysconfig-metadata`:\n{err}"
                );
            }
        },
        Mode::Write => match fs_err::read_to_string(&reference_path) {
            Ok(current) => {
                if current == reference_string {
                    println!("Up-to-date: {filename}");
                } else {
                    println!("Updating: {filename}");
                    fs_err::write(reference_path, reference_string.as_bytes())?;
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                println!("Updating: {filename}");
                fs_err::write(reference_path, reference_string.as_bytes())?;
            }
            Err(err) => {
                bail!(
                    "{filename} changed, please run `cargo dev generate-sysconfig-metadata`:\n{err}"
                );
            }
        },
    }

    Ok(())
}

async fn generate() -> Result<String> {
    println!("Downloading python-build-standalone cpython-unix/targets.yml ...");
    let body = reqwest::get(TARGETS_YML_URL).await?.text().await?;

    let parsed: BTreeMap<String, TargetConfig> = serde_yaml::from_str(&body)?;

    let mut replacements: BTreeMap<&str, BTreeMap<String, String>> = BTreeMap::new();

    for targets_config in parsed.values() {
        for sysconfig_cc_entry in ["CC", "LDSHARED", "BLDSHARED", "LINKCC"] {
            if let Some(ref from_cc) = targets_config.host_cc {
                replacements
                    .entry(sysconfig_cc_entry)
                    .or_default()
                    .insert(from_cc.to_owned(), "cc".to_string());
            }
            if let Some(ref from_cc) = targets_config.target_cc {
                replacements
                    .entry(sysconfig_cc_entry)
                    .or_default()
                    .insert(from_cc.to_owned(), "cc".to_string());
            }
        }
        for sysconfig_cxx_entry in ["CXX", "LDCXXSHARED"] {
            if let Some(ref from_cxx) = targets_config.host_cxx {
                replacements
                    .entry(sysconfig_cxx_entry)
                    .or_default()
                    .insert(from_cxx.to_owned(), "c++".to_string());
            }
            if let Some(ref from_cxx) = targets_config.target_cxx {
                replacements
                    .entry(sysconfig_cxx_entry)
                    .or_default()
                    .insert(from_cxx.to_owned(), "c++".to_string());
            }
        }
    }

    let mut output = String::new();

    // Opening statements
    output.push_str("//! DO NOT EDIT\n");
    output.push_str("//!\n");
    output.push_str("//! Generated with `cargo run dev generate-sysconfig-metadata`\n");
    output.push_str("//! Targets from <https://github.com/astral-sh/python-build-standalone/blob/20260211/cpython-unix/targets.yml>\n");
    output.push_str("//!\n");

    // Disable clippy/fmt
    output.push_str("#![allow(clippy::all)]\n");
    output.push_str("#![cfg_attr(any(), rustfmt::skip)]\n\n");

    // Begin main code
    output.push_str("use std::collections::BTreeMap;\n");
    output.push_str("use std::sync::LazyLock;\n\n");
    output.push_str("use crate::sysconfig::replacements::{ReplacementEntry, ReplacementMode};\n\n");

    output.push_str(
        "/// Mapping for sysconfig keys to lookup and replace with the appropriate entry.\n",
    );
    output.push_str("pub(crate) static DEFAULT_VARIABLE_UPDATES: LazyLock<BTreeMap<String, Vec<ReplacementEntry>>> = LazyLock::new(|| {\n");
    output.push_str("    BTreeMap::from_iter([\n");

    // Add Replacement Entries for CC, CXX, etc.
    for (key, entries) in &replacements {
        writeln!(output, "        (\"{key}\".to_string(), vec![")?;
        for (from, to) in entries {
            writeln!(
                output,
                "            ReplacementEntry {{ mode: ReplacementMode::Partial {{ from: \"{from}\".to_string() }}, to: \"{to}\".to_string() }},"
            )?;
        }
        writeln!(output, "        ]),")?;
    }

    // Add AR case last
    output.push_str("        (\"AR\".to_string(), vec![\n");
    output.push_str("            ReplacementEntry {\n");
    output.push_str("                mode: ReplacementMode::Full,\n");
    output.push_str("                to: \"ar\".to_string(),\n");
    output.push_str("            },\n");
    output.push_str("        ]),\n");

    // Closing
    output.push_str("    ])\n});\n");

    Ok(output)
}

#[cfg(test)]
mod tests {
    use std::env;

    use anyhow::Result;

    use uv_static::EnvVars;

    use crate::generate_all::Mode;

    use super::{Args, main};

    #[tokio::test]
    async fn test_generate_sysconfig_mappings() -> Result<()> {
        // Skip this test in CI to avoid redundancy with the dedicated CI job
        if env::var_os(EnvVars::CI).is_some() {
            return Ok(());
        }

        let mode = if env::var(EnvVars::UV_UPDATE_SCHEMA).as_deref() == Ok("1") {
            Mode::Write
        } else {
            Mode::Check
        };
        main(&Args { mode }).await
    }
}
