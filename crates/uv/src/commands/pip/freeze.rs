use std::fmt::Write;
use std::path::PathBuf;

use anyhow::Result;
use itertools::{Either, Itertools};
use owo_colors::OwoColorize;

use uv_cache::Cache;
use uv_distribution_types::{Diagnostic, InstalledDist, Name};
use uv_installer::SitePackages;
use uv_python::{EnvironmentPreference, PythonEnvironment, PythonRequest};

use crate::commands::pip::operations::report_target_environment;
use crate::commands::ExitStatus;
use crate::printer::Printer;

/// Enumerate the installed packages in the current environment.
pub(crate) fn pip_freeze(
    exclude_editable: bool,
    strict: bool,
    python: Option<&str>,
    system: bool,
    paths: Option<Vec<PathBuf>>,
    cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    // Detect the current Python interpreter.
    let environment = PythonEnvironment::find(
        &python.map(PythonRequest::parse).unwrap_or_default(),
        EnvironmentPreference::from_system_flag(system, false),
        cache,
    )?;

    report_target_environment(&environment, cache, printer)?;

    let mut site_packagess = match paths {
        Some(paths) => {
            assert_ne!(paths.len(), 0);
            Either::Left(paths.into_iter().filter_map(|path| {
                environment
                    .clone()
                    .with_target(uv_python::Target::from(path))
                    .ok() // Drop invalid paths as per `pip freeze`'s behavior
            }))
        }
        None => Either::Right(std::iter::once(environment.clone())),
    }
    .map(|env| SitePackages::from_environment(&env));

    // Build the installed index.
    // TODO: use [`try_reduce`](https://github.com/rust-lang/rust/issues/87053) when stabilized
    let mut site_packages = site_packagess
        .next()
        .expect("iterator must have at least one element")?;

    site_packagess.try_for_each(|other_site_packages: Result<SitePackages>| -> Result<()> {
        site_packages.extend(other_site_packages?);
        Ok(())
    })?;

    site_packages
        .iter()
        .filter(|dist| !(exclude_editable && dist.is_editable()))
        .sorted_unstable_by(|a, b| a.name().cmp(b.name()).then(a.version().cmp(b.version())))
        .map(|dist| match dist {
            InstalledDist::Registry(dist) => {
                format!("{}=={}", dist.name().bold(), dist.version)
            }
            InstalledDist::Url(dist) => {
                if dist.editable {
                    format!("-e {}", dist.url)
                } else {
                    format!("{} @ {}", dist.name().bold(), dist.url)
                }
            }
            InstalledDist::EggInfoFile(dist) => {
                format!("{}=={}", dist.name().bold(), dist.version)
            }
            InstalledDist::EggInfoDirectory(dist) => {
                format!("{}=={}", dist.name().bold(), dist.version)
            }
            InstalledDist::LegacyEditable(dist) => {
                format!("-e {}", dist.target.display())
            }
        })
        .dedup()
        .try_for_each(|dist| writeln!(printer.stdout(), "{dist}"))?;

    // Validate that the environment is consistent.
    if strict {
        // Determine the markers to use for resolution.
        let markers = environment.interpreter().resolver_marker_environment();

        for diagnostic in site_packages.diagnostics(&markers)? {
            writeln!(
                printer.stderr(),
                "{}{} {}",
                "warning".yellow().bold(),
                ":".bold(),
                diagnostic.message().bold()
            )?;
        }
    }

    Ok(ExitStatus::Success)
}
