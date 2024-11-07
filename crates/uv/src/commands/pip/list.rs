use std::cmp::max;
use std::fmt::Write;

use anstream::println;
use anyhow::Result;
use futures::stream::FuturesUnordered;
use futures::TryStreamExt;
use itertools::Itertools;
use owo_colors::OwoColorize;
use rustc_hash::FxHashMap;
use serde::Serialize;
use unicode_width::UnicodeWidthStr;

use uv_cache::{Cache, Refresh};
use uv_cache_info::Timestamp;
use uv_cli::ListFormat;
use uv_client::{Connectivity, RegistryClient, RegistryClientBuilder, VersionFiles};
use uv_configuration::{IndexStrategy, KeyringProviderType, TrustedHost};
use uv_distribution_filename::DistFilename;
use uv_distribution_types::{Diagnostic, IndexCapabilities, IndexLocations, InstalledDist, Name};
use uv_fs::Simplified;
use uv_installer::SitePackages;
use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_platform_tags::Tags;
use uv_python::{EnvironmentPreference, PythonEnvironment};
use uv_python::{Interpreter, PythonRequest};
use uv_resolver::{ExcludeNewer, PrereleaseMode};
use uv_warnings::warn_user_once;

use crate::commands::pip::operations::report_target_environment;
use crate::commands::ExitStatus;
use crate::printer::Printer;

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
    allow_insecure_host: Vec<TrustedHost>,
    connectivity: Connectivity,
    strict: bool,
    exclude_newer: Option<ExcludeNewer>,
    python: Option<&str>,
    system: bool,
    native_tls: bool,
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
    let latest = if outdated {
        let capabilities = IndexCapabilities::default();

        // Initialize the registry client.
        let client =
            RegistryClientBuilder::new(cache.clone().with_refresh(Refresh::All(Timestamp::now())))
                .native_tls(native_tls)
                .connectivity(connectivity)
                .index_urls(index_locations.index_urls())
                .index_strategy(index_strategy)
                .keyring(keyring_provider)
                .allow_insecure_host(allow_insecure_host.clone())
                .markers(environment.interpreter().markers())
                .platform(environment.interpreter().platform())
                .build();

        // Determine the platform tags.
        let interpreter = environment.interpreter();
        let tags = interpreter.tags()?;

        // Initialize the client to fetch the latest version of each package.
        let client = LatestClient {
            client: &client,
            capabilities: &capabilities,
            prerelease,
            exclude_newer,
            tags,
            interpreter,
        };

        // Fetch the latest version for each package.
        results
            .iter()
            .map(|dist| async {
                let latest = client.find_latest(dist.name()).await?;
                Ok::<(&PackageName, Option<DistFilename>), uv_client::Error>((dist.name(), latest))
            })
            .collect::<FuturesUnordered<_>>()
            .try_collect::<FxHashMap<_, _>>()
            .await?
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

/// A client to fetch the latest version of a package from an index.
///
/// The returned distribution is guaranteed to be compatible with the current interpreter.
#[derive(Debug)]
struct LatestClient<'env> {
    client: &'env RegistryClient,
    capabilities: &'env IndexCapabilities,
    prerelease: PrereleaseMode,
    exclude_newer: Option<ExcludeNewer>,
    tags: &'env Tags,
    interpreter: &'env Interpreter,
}

impl<'env> LatestClient<'env> {
    /// Find the latest version of a package from an index.
    async fn find_latest(
        &self,
        package: &PackageName,
    ) -> Result<Option<DistFilename>, uv_client::Error> {
        let mut latest: Option<DistFilename> = None;
        for (_, archive) in self.client.simple(package, None, self.capabilities).await? {
            for datum in archive.iter().rev() {
                // Find the first compatible distribution.
                let files = rkyv::deserialize::<VersionFiles, rkyv::rancor::Error>(&datum.files)
                    .expect("archived version files always deserializes");

                // Determine whether there's a compatible wheel and/or source distribution.
                let mut best = None;

                for (filename, file) in files.all() {
                    // Skip distributions uploaded after the cutoff.
                    if let Some(exclude_newer) = self.exclude_newer {
                        match file.upload_time_utc_ms.as_ref() {
                            Some(&upload_time)
                                if upload_time >= exclude_newer.timestamp_millis() =>
                            {
                                continue;
                            }
                            None => {
                                warn_user_once!(
                                "{} is missing an upload date, but user provided: {exclude_newer}",
                                file.filename,
                            );
                            }
                            _ => {}
                        }
                    }

                    // Skip pre-release distributions.
                    if !filename.version().is_stable() {
                        if !matches!(self.prerelease, PrereleaseMode::Allow) {
                            continue;
                        }
                    }

                    // Skip distributions that are yanked.
                    if file.yanked.is_some_and(|yanked| yanked.is_yanked()) {
                        continue;
                    }

                    // Skip distributions that are incompatible with the current interpreter.
                    if file.requires_python.is_some_and(|requires_python| {
                        !requires_python.contains(self.interpreter.python_full_version())
                    }) {
                        continue;
                    }

                    // Skip distributions that are incompatible with the current platform.
                    if let DistFilename::WheelFilename(filename) = &filename {
                        if !filename.compatibility(self.tags).is_compatible() {
                            continue;
                        }
                    }

                    match filename {
                        DistFilename::WheelFilename(_) => {
                            best = Some(filename);
                            break;
                        }
                        DistFilename::SourceDistFilename(_) => {
                            if best.is_none() {
                                best = Some(filename);
                            }
                        }
                    }
                }

                match (latest.as_ref(), best) {
                    (Some(current), Some(best)) => {
                        if best.version() > current.version() {
                            latest = Some(best);
                        }
                    }
                    (None, Some(best)) => {
                        latest = Some(best);
                    }
                    _ => {}
                }
            }
        }
        Ok(latest)
    }
}
