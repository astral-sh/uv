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
use uv_fs::Normalized;
use uv_installer::SitePackages;
use uv_interpreter::Virtualenv;

use crate::commands::ExitStatus;
use crate::printer::Printer;

/// Enumerate the installed packages in the current environment.
pub(crate) fn pip_list(
    cache: &Cache,
    strict: bool,
    editable: bool,
    mut printer: Printer,
) -> Result<ExitStatus> {
    // Detect the current Python interpreter.
    let platform = Platform::current()?;
    let venv = Virtualenv::from_env(platform, cache)?;

    debug!(
        "Using Python {} environment at {}",
        venv.interpreter().python_version(),
        venv.python_executable().normalized_display().cyan()
    );

    // Build the installed index.
    let site_packages = SitePackages::from_executable(&venv)?;

    // Filter if `--editable` is specified; always sort by name.
    let results = site_packages
        .iter()
        .filter(|f| !editable || f.is_editable())
        .sorted_unstable_by(|a, b| a.name().cmp(b.name()).then(a.version().cmp(b.version())))
        .collect_vec();
    if results.is_empty() {
        return Ok(ExitStatus::Success);
    }

    // The package name and version are always present.
    let mut columns = vec![
        Column {
            header: String::from("Package"),
            rows: results.iter().map(|f| f.name().to_string()).collect_vec(),
        },
        Column {
            header: String::from("Version"),
            rows: results
                .iter()
                .map(|f| f.version().to_string())
                .collect_vec(),
        },
    ];

    // Editable column is only displayed if at least one editable package is found.
    if site_packages
        .iter()
        .any(distribution_types::InstalledDist::is_editable)
    {
        columns.push(Column {
            header: String::from("Editable project location"),
            rows: results
                .iter()
                .map(|f| f.as_editable())
                .map(|e| {
                    if let Some(url) = e {
                        url.to_file_path()
                            .unwrap()
                            .into_os_string()
                            .into_string()
                            .unwrap()
                    } else {
                        String::new()
                    }
                })
                .collect_vec(),
        });
    }

    for elems in Multizip(columns.iter().map(Column::fmt_padded).collect_vec()) {
        println!("{0}", elems.join(" "));
    }

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

#[derive(Debug)]
struct Column {
    /// The header of the column.
    header: String,
    /// The rows of the column.
    rows: Vec<String>,
}

impl<'a> Column {
    /// Return the width of the column.
    fn max_width(&self) -> usize {
        max(
            self.header.width(),
            self.rows.iter().map(|f| f.width()).max().unwrap_or(0),
        )
    }

    /// Return an iterator of the column, with the header and rows formatted to the maximum width.
    fn fmt_padded(&'a self) -> impl Iterator<Item = String> + 'a {
        let max_width = self.max_width();
        let header = vec![
            format!("{0:width$}", self.header, width = max_width),
            format!("{:-^width$}", "", width = max_width),
        ];

        header
            .into_iter()
            .chain(self.rows.iter().map(move |f| format!("{f:max_width$}")))
    }
}

/// Zip an unknown number of iterators.
/// Combination of [`itertools::multizip`] and [`itertools::izip`].
#[derive(Debug)]
struct Multizip<T>(Vec<T>);

impl<T> Iterator for Multizip<T>
where
    T: Iterator,
{
    type Item = Vec<T::Item>;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.iter_mut().map(Iterator::next).collect()
    }
}
