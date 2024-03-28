use std::cmp::max;
use std::fmt::Write;

use anyhow::Result;
use chrono::{DateTime, Utc};
use itertools::Itertools;
use owo_colors::OwoColorize;
use serde::Serialize;
use tracing::debug;
use unicode_width::UnicodeWidthStr;
use uv_dispatch::BuildDispatch;

use crate::commands::pip_install::{build_editables, read_requirements, resolve, Error};
use crate::commands::{ExitStatus, Upgrade};
use crate::printer::Printer;
use crate::requirements::{ExtrasSpecification, NamedRequirements, RequirementsSource};
use distribution_types::{IndexLocations, InstalledDist, Name, Resolution};
use tempfile::tempdir_in;
use uv_auth::{KeyringProvider, GLOBAL_AUTH_STORE};
use uv_cache::Cache;
use uv_client::{Connectivity, FlatIndex, FlatIndexClient, RegistryClientBuilder};
use uv_fs::Simplified;
use uv_installer::{Reinstall, SitePackages};
use uv_interpreter::PythonEnvironment;
use uv_normalize::PackageName;
use uv_resolver::{DependencyMode, InMemoryIndex, OptionsBuilder, PreReleaseMode, ResolutionMode};
use uv_traits::{BuildIsolation, ConfigSettings, InFlight, NoBinary, NoBuild, SetupPyStrategy};

use super::ListFormat;

/// Enumerate the installed packages in the current environment.
#[allow(clippy::too_many_arguments, clippy::fn_params_excessive_bools)]
pub(crate) async fn pip_list(
    strict: bool,
    outdated: bool,
    editable: bool,
    exclude_editable: bool,
    exclude: &[PackageName],
    format: &ListFormat,
    python: Option<&str>,
    system: bool,
    cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    // Detect the current Python interpreter.
    let venv = if let Some(python) = python {
        PythonEnvironment::from_requested_python(python, cache)?
    } else if system {
        PythonEnvironment::from_default_python(cache)?
    } else {
        match PythonEnvironment::from_virtualenv(cache) {
            Ok(venv) => venv,
            Err(uv_interpreter::Error::VenvNotFound) => {
                PythonEnvironment::from_default_python(cache)?
            }
            Err(err) => return Err(err.into()),
        }
    };

    debug!(
        "Using Python {} environment at {}",
        venv.interpreter().python_version(),
        venv.python_executable().user_display().cyan()
    );

    // Build the installed index.
    let site_packages = SitePackages::from_executable(&venv)?;

    // Filter if `--editable` is specified; always sort by name.
    let mut results = site_packages
        .iter()
        .filter(|dist| {
            (!dist.is_editable() && !editable) || (dist.is_editable() && !exclude_editable)
        })
        .filter(|dist| !exclude.contains(dist.name()))
        .sorted_unstable_by(|a, b| a.name().cmp(b.name()).then(a.version().cmp(b.version())))
        .collect_vec();
    if results.is_empty() {
        return Ok(ExitStatus::Success);
    }

    if outdated {
        let constraints: &[RequirementsSource] = Default::default();
        let overrides: &[RequirementsSource] = Default::default();
        let extras: ExtrasSpecification<'_> = ExtrasSpecification::default();
        let connectivity: Connectivity = Connectivity::Online;

        let resolution_mode: ResolutionMode = ResolutionMode::Highest;
        let prerelease_mode: PreReleaseMode = PreReleaseMode::default();
        let dependency_mode: DependencyMode = DependencyMode::default();
        let upgrade: Upgrade = Upgrade::All;
        let index_locations: IndexLocations = IndexLocations::default();
        let keyring_provider: KeyringProvider = KeyringProvider::default();
        let reinstall: Reinstall = Reinstall::All;
        let setup_py: SetupPyStrategy = SetupPyStrategy::default();
        let config_settings: ConfigSettings = ConfigSettings::default();
        let no_build: NoBuild = NoBuild::None;
        let no_binary: NoBinary = NoBinary::None;
        let exclude_newer: Option<DateTime<Utc>> = None;
        let break_system_packages: bool = false;
        let native_tls: bool = false;

        let _lock = venv.lock()?;

        // Determine the set of installed packages.
        let site_packages = SitePackages::from_executable(&venv)?;

        let requirements = &site_packages
            .by_name
            .keys()
            .map(|p| RequirementsSource::Package(p.as_dist_info_name().to_string()))
            .collect::<Vec<_>>();

        // Read all requirements from the provided sources.
        let spec =
            read_requirements(requirements, constraints, overrides, &extras, connectivity).await?;

        // If the environment is externally managed, abort.
        if let Some(externally_managed) = venv.interpreter().is_externally_managed() {
            if break_system_packages {
                debug!("Ignoring externally managed environment due to `--break-system-packages`");
            } else {
                return if let Some(error) = externally_managed.into_error() {
                    Err(anyhow::anyhow!(
                        "The interpreter at {} is externally managed, and indicates the following:\n\n{}\n\nConsider creating a virtual environment with `uv venv`.",
                        venv.root().simplified_display().cyan(),
                        textwrap::indent(&error, "  ").green(),
                    ))
                } else {
                    Err(anyhow::anyhow!(
                        "The interpreter at {} is externally managed. Instead, create a virtual environment with `uv venv`.",
                        venv.root().simplified_display().cyan()
                    ))
                };
            }
        }

        // Convert from unnamed to named requirements.
        let NamedRequirements {
            project,
            requirements,
            constraints,
            overrides,
            editables,
            index_url,
            extra_index_urls,
            no_index,
            find_links,
        } = NamedRequirements::from_spec(spec)?;

        // Determine the tags, markers, and interpreter to use for resolution.
        let interpreter = venv.interpreter().clone();
        let tags = venv.interpreter().tags()?;
        let markers = venv.interpreter().markers();

        // Incorporate any index locations from the provided sources.
        let index_locations =
            index_locations.combine(index_url, extra_index_urls, find_links, no_index);

        // Add all authenticated sources to the store.
        for url in index_locations.urls() {
            GLOBAL_AUTH_STORE.save_from_url(url);
        }

        // Initialize the registry client.
        let client = RegistryClientBuilder::new(cache.clone())
            .native_tls(native_tls)
            .connectivity(connectivity)
            .index_urls(index_locations.index_urls())
            .keyring_provider(keyring_provider)
            .markers(markers)
            .platform(interpreter.platform())
            .build();

        // Resolve the flat indexes from `--find-links`.
        let flat_index = {
            let client = FlatIndexClient::new(&client, cache);
            let entries = client.fetch(index_locations.flat_index()).await?;
            FlatIndex::from_entries(entries, tags)
        };

        let build_isolation = BuildIsolation::Isolated;

        // Create a shared in-memory index.
        let index = InMemoryIndex::default();

        // Track in-flight downloads, builds, etc., across resolutions.
        let in_flight = InFlight::default();

        let resolve_dispatch = BuildDispatch::new(
            &client,
            cache,
            &interpreter,
            &index_locations,
            &flat_index,
            &index,
            &in_flight,
            setup_py,
            &config_settings,
            build_isolation,
            &no_build,
            &no_binary,
        )
        .with_options(OptionsBuilder::new().exclude_newer(exclude_newer).build());

        // Build all editable distributions. The editables are shared between resolution and
        // installation, and should live for the duration of the command. If an editable is already
        // installed in the environment, we'll still re-build it here.
        let editable_wheel_dir;
        let editables = if editables.is_empty() {
            vec![]
        } else {
            editable_wheel_dir = tempdir_in(venv.root())?;
            build_editables(
                &editables,
                editable_wheel_dir.path(),
                cache,
                &interpreter,
                tags,
                &client,
                &resolve_dispatch,
                printer,
            )
            .await?
        };

        let options = OptionsBuilder::new()
            .resolution_mode(resolution_mode)
            .prerelease_mode(prerelease_mode)
            .dependency_mode(dependency_mode)
            .exclude_newer(exclude_newer)
            .build();

        // Resolve the requirements.
        let resolution = match resolve(
            requirements,
            constraints,
            overrides,
            project,
            &editables,
            &site_packages,
            &reinstall,
            &upgrade,
            &interpreter,
            tags,
            markers,
            &client,
            &flat_index,
            &index,
            &resolve_dispatch,
            options,
            printer,
        )
        .await
        {
            Ok(resolution) => Resolution::from(resolution),
            Err(Error::Resolve(uv_resolver::ResolveError::NoSolution(err))) => {
                let report = miette::Report::msg(format!("{err}"))
                    .context("No solution found when resolving dependencies:");
                eprint!("{report:?}");
                return Ok(ExitStatus::Failure);
            }
            Err(err) => return Err(err.into()),
        };

        results = results
            .into_iter()
            .filter(|result| {
                let current_version = result.version();
                match resolution.get(result.name()) {
                    Some(dist) => {
                        let available_version = dist.version().unwrap_or(current_version);
                        available_version > current_version
                    }
                    _ => false,
                }
            })
            .collect::<Vec<_>>();
    }

    match format {
        ListFormat::Columns => {
            // The package name and version are always present.
            let mut columns = vec![
                Column {
                    header: String::from("Package"),
                    rows: results
                        .iter()
                        .copied()
                        .map(|dist| dist.name().to_string())
                        .collect_vec(),
                },
                Column {
                    header: String::from("Version"),
                    rows: results
                        .iter()
                        .map(|dist| dist.version().to_string())
                        .collect_vec(),
                },
            ];

            // Editable column is only displayed if at least one editable package is found.
            if results.iter().copied().any(InstalledDist::is_editable) {
                columns.push(Column {
                    header: String::from("Editable project location"),
                    rows: results
                        .iter()
                        .map(|dist| dist.as_editable())
                        .map(|url| {
                            url.map(|url| {
                                url.to_file_path().unwrap().simplified_display().to_string()
                            })
                            .unwrap_or_default()
                        })
                        .collect_vec(),
                });
            }

            for elems in MultiZip(columns.iter().map(Column::fmt).collect_vec()) {
                writeln!(printer.stdout(), "{}", elems.join(" ").trim_end())?;
            }
        }
        ListFormat::Json => {
            let rows = results.iter().copied().map(Entry::from).collect_vec();
            let output = serde_json::to_string(&rows)?;
            writeln!(printer.stdout(), "{output}")?;
        }
        ListFormat::Freeze => {
            for dist in &results {
                writeln!(
                    printer.stdout(),
                    "{}=={}",
                    dist.name().bold(),
                    dist.version()
                )?;
            }
        }
    }

    // Validate that the environment is consistent.
    if strict {
        for diagnostic in site_packages.diagnostics()? {
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

/// An entry in a JSON list of installed packages.
#[derive(Debug, Serialize)]
struct Entry {
    name: String,
    version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    editable_project_location: Option<String>,
}

impl From<&InstalledDist> for Entry {
    fn from(dist: &InstalledDist) -> Self {
        Self {
            name: dist.name().to_string(),
            version: dist.version().to_string(),
            editable_project_location: dist
                .as_editable()
                .map(|url| url.to_file_path().unwrap().simplified_display().to_string()),
        }
    }
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
    fn fmt(&'a self) -> impl Iterator<Item = String> + 'a {
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
struct MultiZip<T>(Vec<T>);

impl<T> Iterator for MultiZip<T>
where
    T: Iterator,
{
    type Item = Vec<T::Item>;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.iter_mut().map(Iterator::next).collect()
    }
}
