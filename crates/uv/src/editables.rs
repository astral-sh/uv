use std::fmt::Write;
use std::ops::Deref;

use anyhow::{anyhow, Context, Result};
use indexmap::IndexMap;
use owo_colors::OwoColorize;

use distribution_types::{
    InstalledDist, LocalEditable, LocalEditables, Name, ParsedUrlError, Requirement, Requirements,
};
use platform_tags::Tags;
use requirements_txt::EditableRequirement;
use uv_cache::{ArchiveTarget, ArchiveTimestamp, Cache};
use uv_client::RegistryClient;
use uv_configuration::{Concurrency, Reinstall};
use uv_dispatch::BuildDispatch;
use uv_distribution::DistributionDatabase;
use uv_installer::{is_dynamic, Downloader, InstalledEditable, ResolvedEditable};
use uv_interpreter::Interpreter;
use uv_resolver::BuiltEditableMetadata;
use uv_types::{HashStrategy, InstalledPackagesProvider};

use crate::commands::elapsed;
use crate::commands::reporters::DownloadReporter;
use crate::printer::Printer;

#[derive(Debug, Default)]
pub(crate) struct ResolvedEditables {
    /// The set of resolved editables, including both those that were already installed and those
    /// that were built.
    pub(crate) editables: Vec<ResolvedEditable>,
    /// The temporary directory in which the built editables were stored.
    #[allow(dead_code)]
    temp_dir: Option<tempfile::TempDir>,
}

impl Deref for ResolvedEditables {
    type Target = [ResolvedEditable];

    fn deref(&self) -> &Self::Target {
        &self.editables
    }
}

impl ResolvedEditables {
    /// Resolve the set of editables that need to be installed.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn resolve(
        editables: Vec<EditableRequirement>,
        installed_packages: &impl InstalledPackagesProvider,
        reinstall: &Reinstall,
        hasher: &HashStrategy,
        interpreter: &Interpreter,
        tags: &Tags,
        cache: &Cache,
        client: &RegistryClient,
        build_dispatch: &BuildDispatch<'_>,
        concurrency: Concurrency,
        printer: Printer,
    ) -> Result<Self> {
        // Partition the editables into those that are already installed, and those that must be built.
        let mut installed = Vec::with_capacity(editables.len());
        let mut builds = Vec::with_capacity(editables.len());
        for editable in editables {
            match reinstall {
                Reinstall::None => {
                    if let [dist] = installed_packages.get_editables(editable.raw()).as_slice() {
                        if let Some(editable) = up_to_date(&editable, dist)? {
                            installed.push(editable);
                        } else {
                            builds.push(editable);
                        }
                    } else {
                        builds.push(editable);
                    }
                }
                Reinstall::All => {
                    builds.push(editable);
                }
                Reinstall::Packages(packages) => {
                    if let [dist] = installed_packages.get_editables(editable.raw()).as_slice() {
                        if packages.contains(dist.name()) {
                            builds.push(editable);
                        } else if let Some(editable) = up_to_date(&editable, dist)? {
                            installed.push(editable);
                        } else {
                            builds.push(editable);
                        }
                    } else {
                        builds.push(editable);
                    }
                }
            }
        }

        // Build any editables.
        let (built_editables, temp_dir) = if builds.is_empty() {
            (Vec::new(), None)
        } else {
            let start = std::time::Instant::now();

            let downloader = Downloader::new(
                cache,
                tags,
                hasher,
                DistributionDatabase::new(client, build_dispatch, concurrency.downloads),
            )
            .with_reporter(DownloadReporter::from(printer).with_length(builds.len() as u64));

            let editables = LocalEditables::from_editables(builds.iter().map(|editable| {
                let EditableRequirement {
                    url,
                    path,
                    extras,
                    marker: _,
                    origin: _,
                } = editable;
                LocalEditable {
                    url: url.clone(),
                    path: path.clone(),
                    extras: extras.clone(),
                }
            }));

            let temp_dir = tempfile::tempdir_in(cache.root())?;

            let editables: Vec<_> = downloader
                .build_editables(editables, temp_dir.path())
                .await
                .context("Failed to build editables")?
                .into_iter()
                .collect();

            // Validate that the editables are compatible with the target Python version.
            for editable in &editables {
                if let Some(python_requires) = editable.metadata.requires_python.as_ref() {
                    if !python_requires.contains(interpreter.python_version()) {
                        return Err(anyhow!(
                            "Editable `{}` requires Python {}, but {} is installed",
                            editable.metadata.name,
                            python_requires,
                            interpreter.python_version()
                        ));
                    }
                }
            }

            let s = if editables.len() == 1 { "" } else { "s" };
            writeln!(
                printer.stderr(),
                "{}",
                format!(
                    "Built {} in {}",
                    format!("{} editable{}", editables.len(), s).bold(),
                    elapsed(start.elapsed())
                )
                .dimmed()
            )?;

            (editables, Some(temp_dir))
        };

        let editables = installed
            .into_iter()
            .map(ResolvedEditable::Installed)
            .chain(built_editables.into_iter().map(ResolvedEditable::Built))
            .collect::<Vec<_>>();

        Ok(Self {
            editables,
            temp_dir,
        })
    }

    pub(crate) fn as_metadata(&self) -> Result<Vec<BuiltEditableMetadata>, Box<ParsedUrlError>> {
        self.iter()
            .map(|editable| {
                let dependencies: Vec<_> = editable
                    .metadata()
                    .requires_dist
                    .iter()
                    .cloned()
                    .map(Requirement::from_pep508)
                    .collect::<Result<_, _>>()?;
                Ok::<_, Box<ParsedUrlError>>(BuiltEditableMetadata {
                    built: editable.local().clone(),
                    metadata: editable.metadata().clone(),
                    requirements: Requirements {
                        dependencies,
                        optional_dependencies: IndexMap::default(),
                    },
                })
            })
            .collect()
    }
}

/// Returns the [`InstalledEditable`] if the installed distribution is up-to-date for the given
/// requirement.
fn up_to_date(
    editable: &EditableRequirement,
    dist: &InstalledDist,
) -> Result<Option<InstalledEditable>> {
    // If the editable isn't up-to-date, don't reuse it.
    if !ArchiveTimestamp::up_to_date_with(&editable.path, ArchiveTarget::Install(dist))? {
        return Ok(None);
    };

    // If the editable is dynamic, don't reuse it.
    if is_dynamic(editable) {
        return Ok(None);
    };

    // If we can't read the metadata from the installed distribution, don't reuse it.
    let Ok(metadata) = dist.metadata() else {
        return Ok(None);
    };

    Ok(Some(InstalledEditable {
        editable: LocalEditable {
            url: editable.url.clone(),
            path: editable.path.clone(),
            extras: editable.extras.clone(),
        },
        wheel: (*dist).clone(),
        metadata,
    }))
}
