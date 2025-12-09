use std::fmt::Write;
use std::path::PathBuf;

use anyhow::Result;
use itertools::Itertools;
use owo_colors::OwoColorize;
use tracing::debug;

use uv_cache::Cache;
use uv_distribution_types::{Diagnostic, InstalledDistKind, Name};
use uv_fs::Simplified;
use uv_installer::SitePackages;
use uv_preview::Preview;
use uv_python::PythonPreference;
use uv_python::{EnvironmentPreference, Prefix, PythonEnvironment, PythonRequest, Target};

use crate::commands::ExitStatus;
use crate::commands::pip::operations::report_target_environment;
use crate::printer::Printer;

/// Enumerate the installed packages in the current environment.
pub(crate) fn pip_freeze(
    exclude_editable: bool,
    strict: bool,
    python: Option<&str>,
    system: bool,
    target: Option<Target>,
    prefix: Option<Prefix>,
    paths: Option<Vec<PathBuf>>,
    cache: &Cache,
    printer: Printer,
    preview: Preview,
) -> Result<ExitStatus> {
    // Detect the current Python interpreter.
    let environment = PythonEnvironment::find(
        &python.map(PythonRequest::parse).unwrap_or_default(),
        EnvironmentPreference::from_system_flag(system, false),
        PythonPreference::default().with_system_flag(system),
        cache,
        preview,
    )?;

    // Apply any `--target` or `--prefix` directories.
    let environment = if let Some(target) = target {
        debug!(
            "Using `--target` directory at {}",
            target.root().user_display()
        );
        environment.with_target(target)?
    } else if let Some(prefix) = prefix {
        debug!(
            "Using `--prefix` directory at {}",
            prefix.root().user_display()
        );
        environment.with_prefix(prefix)?
    } else {
        environment
    };

    report_target_environment(&environment, cache, printer)?;

    // Collect all the `site-packages` directories.
    let site_packages = match paths {
        Some(paths) => {
            paths
                .into_iter()
                .filter_map(|path| {
                    environment
                        .clone()
                        .with_target(uv_python::Target::from(path))
                        // Drop invalid paths as per `pip freeze`.
                        .ok()
                })
                .map(|environment| SitePackages::from_environment(&environment))
                .collect::<Result<Vec<_>>>()?
        }
        None => vec![SitePackages::from_environment(&environment)?],
    };

    site_packages
        .iter()
        .flat_map(uv_installer::SitePackages::iter)
        .filter(|dist| !(exclude_editable && dist.is_editable()))
        .sorted_unstable_by(|a, b| a.name().cmp(b.name()).then(a.version().cmp(b.version())))
        .map(|dist| match &dist.kind {
            InstalledDistKind::Registry(dist) => {
                format!("{}=={}", dist.name().bold(), dist.version)
            }
            InstalledDistKind::Url(dist) => {
                if dist.editable {
                    format!("-e {}", dist.url)
                } else {
                    format!("{} @ {}", dist.name().bold(), dist.url)
                }
            }
            InstalledDistKind::EggInfoFile(dist) => {
                format!("{}=={}", dist.name().bold(), dist.version)
            }
            InstalledDistKind::EggInfoDirectory(dist) => {
                format!("{}=={}", dist.name().bold(), dist.version)
            }
            InstalledDistKind::LegacyEditable(dist) => {
                format!("-e {}", dist.target.display())
            }
        })
        .dedup()
        .try_for_each(|dist| writeln!(printer.stdout_important(), "{dist}"))?;

    // Validate that the environment is consistent.
    if strict {
        // Determine the markers and tags to use for resolution.
        let markers = environment.interpreter().resolver_marker_environment();
        let tags = environment.interpreter().tags()?;

        for entry in site_packages {
            for diagnostic in entry.diagnostics(&markers, tags)? {
                writeln!(
                    printer.stderr(),
                    "{}{} {}",
                    "warning".yellow().bold(),
                    ":".bold(),
                    diagnostic.message().bold()
                )?;
            }
        }
    }

    Ok(ExitStatus::Success)
}
