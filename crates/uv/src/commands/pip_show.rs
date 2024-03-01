use std::cmp::max;
use std::fmt::Write;

use anstream::println;
use anyhow::Result;
use itertools::Itertools;
use owo_colors::OwoColorize;
use tracing::debug;
use unicode_width::UnicodeWidthStr;

use distribution_types::Name;
use platform_host::Platform;
use uv_cache::Cache;
use uv_fs::Simplified;
use uv_installer::SitePackages;
use uv_interpreter::PythonEnvironment;
use uv_normalize::PackageName;

use crate::commands::ExitStatus;
use crate::printer::Printer;
use crate::requirements::{RequirementsSource, RequirementsSpecification};

/// Show information about one or more installed packages.
#[allow(clippy::too_many_arguments, clippy::fn_params_excessive_bools)]
pub(crate) fn pip_show(
    sources: &[RequirementsSource],
    strict: bool,
    exclude_editable: bool,
    exclude: &[PackageName],
    python: Option<&str>,
    system: bool,
    cache: &Cache,
    mut printer: Printer,
) -> Result<ExitStatus> {
    // Read all requirements from the provided sources.
    let RequirementsSpecification {
        project: _project,
        requirements,
        constraints: _constraints,
        overrides: _overrides,
        editables,
        index_url: _index_url,
        extra_index_urls: _extra_index_urls,
        no_index: _no_index,
        find_links: _find_links,
        extras: _extras,
    } = RequirementsSpecification::from_simple_sources(sources)?;

    // Detect the current Python interpreter.
    let platform = Platform::current()?;
    let venv = if let Some(python) = python {
        PythonEnvironment::from_requested_python(python, &platform, cache)?
    } else if system {
        PythonEnvironment::from_default_python(&platform, cache)?
    } else {
        match PythonEnvironment::from_virtualenv(platform.clone(), cache) {
            Ok(venv) => venv,
            Err(uv_interpreter::Error::VenvNotFound) => {
                PythonEnvironment::from_default_python(&platform, cache)?
            }
            Err(err) => return Err(err.into()),
        }
    };

    debug!(
        "Using Python {} environment at {}",
        venv.interpreter().python_version(),
        venv.python_executable().simplified_display().cyan()
    );

    // Build the installed index.
    let site_packages = SitePackages::from_executable(&venv)?;

    // Filter if `--editable` is specified; always sort by name.
    let results = site_packages
        .iter()
        // .filter(|f| (requirements.map(|r| r.name).contains(f.name())))
        .filter(|f| (!f.is_editable()) || (f.is_editable() && !exclude_editable))
        .filter(|f| !exclude.contains(f.name()))
        .sorted_unstable_by(|a, b| a.name().cmp(b.name()).then(a.version().cmp(b.version())))
        .collect_vec();
    if results.is_empty() {
        return Ok(ExitStatus::Success);
    }
    println!();
    println!("{:?}", results[0]);

    // Validate that the environment is consistent.
    if strict {
        for diagnostic in site_packages.diagnostics()? {
            writeln!(
                printer,
                "{}{} {}",
                "warning".yellow().bold(),
                ":".bold(),
                diagnostic.message().bold()
            )?;
        }
    }

    Ok(ExitStatus::Success)
}
