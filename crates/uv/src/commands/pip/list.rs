use std::cmp::max;
use std::fmt::Write;

use anstream::println;
use anyhow::Result;
use futures::StreamExt;
use itertools::Itertools;
use owo_colors::OwoColorize;
use rustc_hash::FxHashMap;
use serde::Serialize;
use tokio::sync::Semaphore;
use unicode_width::UnicodeWidthStr;

use uv_cache::{Cache, Refresh};
use uv_cache_info::Timestamp;
use uv_cli::ListFormat;
use uv_client::RegistryClientBuilder;
use uv_configuration::{Concurrency, IndexStrategy, KeyringProviderType};
use uv_distribution_filename::DistFilename;
use uv_distribution_types::{Diagnostic, IndexCapabilities, IndexLocations, InstalledDist, Name};
use uv_fs::Simplified;
use uv_installer::SitePackages;
use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_python::PythonRequest;
use uv_python::{EnvironmentPreference, PythonEnvironment};
use uv_resolver::{ExcludeNewer, PrereleaseMode, RequiresPython};

use crate::commands::pip::latest::LatestClient;
use crate::commands::pip::operations::report_target_environment;
use crate::commands::reporters::LatestVersionReporter;
use crate::commands::ExitStatus;
use crate::printer::Printer;
use crate::settings::NetworkSettings;

/// Enumerate the installed packages in the current environment.
#[allow(clippy::fn_params_excessive_bools)]
pub(crate) async fn pip_list(
    editable: Option<bool>,
    exclude: &[PackageName],
    format: &ListFormat,
    outdated: bool,
    prerelease: PrereleaseMode,
    index_locations: IndexLocations,
    index_strategy: IndexStrategy,
    keyring_provider: KeyringProviderType,
    network_settings: &NetworkSettings,
    concurrency: Concurrency,
    strict: bool,
    exclude_newer: Option<ExcludeNewer>,
    python: Option<&str>,
    system: bool,
    cache: &Cache,
    printer: Printer,
) -> Result<ExitStatus> {
    // Disallow `--outdated` with `--format freeze`.
    if outdated && matches!(format, ListFormat::Freeze) {
        anyhow::bail!("`--outdated` cannot be used with `--format freeze`");
    }

    // Detect the current Python interpreter.
    let environment = PythonEnvironment::find(
        &python.map(PythonRequest::parse).unwrap_or_default(),
        EnvironmentPreference::from_system_flag(system, false),
        cache,
    )?;

    report_target_environment(&environment, cache, printer)?;

    // Build the installed index.
    let site_packages = SitePackages::from_environment(&environment)?;

    // Filter if `--editable` is specified; always sort by name.
    let results = site_packages
        .iter()
        .filter(|dist| editable.is_none() || editable == Some(dist.is_editable()))
        .filter(|dist| !exclude.contains(dist.name()))
        .sorted_unstable_by(|a, b| a.name().cmp(b.name()).then(a.version().cmp(b.version())))
        .collect_vec();

    // Determine the latest version for each package.
    let latest = if outdated && !results.is_empty() {
        let capabilities = IndexCapabilities::default();

        // Initialize the registry client.
        let client =
            RegistryClientBuilder::new(cache.clone().with_refresh(Refresh::All(Timestamp::now())))
                .native_tls(network_settings.native_tls)
                .connectivity(network_settings.connectivity)
                .allow_insecure_host(network_settings.allow_insecure_host.clone())
                .index_urls(index_locations.index_urls())
                .index_strategy(index_strategy)
                .keyring(keyring_provider)
                .markers(environment.interpreter().markers())
                .platform(environment.interpreter().platform())
                .build();
        let download_concurrency = Semaphore::new(concurrency.downloads);

        // Determine the platform tags.
        let interpreter = environment.interpreter();
        let tags = interpreter.tags()?;
        let requires_python =
            RequiresPython::greater_than_equal_version(interpreter.python_full_version());

        // Initialize the client to fetch the latest version of each package.
        let client = LatestClient {
            client: &client,
            capabilities: &capabilities,
            prerelease,
            exclude_newer,
            tags: Some(tags),
            requires_python: &requires_python,
        };

        let reporter = LatestVersionReporter::from(printer).with_length(results.len() as u64);

        // Fetch the latest version for each package.
        let mut fetches = futures::stream::iter(&results)
            .map(|dist| async {
                let latest = client
                    .find_latest(dist.name(), None, &download_concurrency)
                    .await?;
                Ok::<(&PackageName, Option<DistFilename>), uv_client::Error>((dist.name(), latest))
            })
            .buffer_unordered(concurrency.downloads);

        let mut map = FxHashMap::default();
        while let Some((package, version)) = fetches.next().await.transpose()? {
            if let Some(version) = version.as_ref() {
                reporter.on_fetch_version(package, version.version());
            } else {
                reporter.on_fetch_progress();
            }
            map.insert(package, version);
        }
        reporter.on_fetch_complete();
        map
    } else {
        FxHashMap::default()
    };

    // Remove any up-to-date packages from the results.
    let results = if outdated {
        results
            .into_iter()
            .filter(|dist| {
                latest[dist.name()]
                    .as_ref()
                    .is_some_and(|filename| filename.version() > dist.version())
            })
            .collect_vec()
    } else {
        results
    };

    match format {
        ListFormat::Json => {
            let rows = results
                .iter()
                .copied()
                .map(|dist| Entry {
                    name: dist.name().clone(),
                    version: dist.version().clone(),
                    latest_version: latest
                        .get(dist.name())
                        .and_then(|filename| filename.as_ref())
                        .map(DistFilename::version)
                        .cloned(),
                    latest_filetype: latest
                        .get(dist.name())
                        .and_then(|filename| filename.as_ref())
                        .map(FileType::from),
                    editable_project_location: dist
                        .as_editable()
                        .map(|url| url.to_file_path().unwrap().simplified_display().to_string()),
                })
                .collect_vec();
            let output = serde_json::to_string(&rows)?;
            println!("{output}");
        }
        ListFormat::Columns if results.is_empty() => {}
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

            // The latest version and type are only displayed if outdated.
            if outdated {
                columns.push(Column {
                    header: String::from("Latest"),
                    rows: results
                        .iter()
                        .map(|dist| {
                            latest
                                .get(dist.name())
                                .and_then(|filename| filename.as_ref())
                                .map(DistFilename::version)
                                .map(ToString::to_string)
                                .unwrap_or_default()
                        })
                        .collect_vec(),
                });
                columns.push(Column {
                    header: String::from("Type"),
                    rows: results
                        .iter()
                        .map(|dist| {
                            latest
                                .get(dist.name())
                                .and_then(|filename| filename.as_ref())
                                .map(FileType::from)
                                .as_ref()
                                .map(ToString::to_string)
                                .unwrap_or_default()
                        })
                        .collect_vec(),
                });
            }

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
                println!("{}", elems.join(" ").trim_end());
            }
        }
        ListFormat::Freeze if results.is_empty() => {}
        ListFormat::Freeze => {
            for dist in &results {
                println!("{}=={}", dist.name().bold(), dist.version());
            }
        }
    }

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

#[derive(Debug)]
enum FileType {
    /// A wheel distribution (i.e., a `.whl` file).
    Wheel,
    /// A source distribution (e.g., a `.tar.gz` file).
    SourceDistribution,
}

impl std::fmt::Display for FileType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Wheel => write!(f, "wheel"),
            Self::SourceDistribution => write!(f, "sdist"),
        }
    }
}

impl Serialize for FileType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Self::Wheel => serializer.serialize_str("wheel"),
            Self::SourceDistribution => serializer.serialize_str("sdist"),
        }
    }
}

impl From<&DistFilename> for FileType {
    fn from(filename: &DistFilename) -> Self {
        match filename {
            DistFilename::WheelFilename(_) => Self::Wheel,
            DistFilename::SourceDistFilename(_) => Self::SourceDistribution,
        }
    }
}

/// An entry in a JSON list of installed packages.
#[derive(Debug, Serialize)]
struct Entry {
    name: PackageName,
    version: Version,
    #[serde(skip_serializing_if = "Option::is_none")]
    latest_version: Option<Version>,
    #[serde(skip_serializing_if = "Option::is_none")]
    latest_filetype: Option<FileType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    editable_project_location: Option<String>,
}

/// A column in a table.
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
///
/// A combination of [`itertools::multizip`] and [`itertools::izip`].
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
