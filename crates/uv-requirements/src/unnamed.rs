use std::borrow::Cow;
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;

use configparser::ini::Ini;
use futures::{stream::FuturesOrdered, TryStreamExt};
use serde::Deserialize;
use tracing::debug;
use url::Host;

use distribution_filename::{DistExtension, SourceDistFilename, WheelFilename};
use distribution_types::{
    BuildableSource, DirectSourceUrl, DirectorySourceUrl, GitSourceUrl, PathSourceUrl,
    RemoteSource, SourceUrl, VersionId,
};
use pep508_rs::{UnnamedRequirement, VersionOrUrl};
use pypi_types::Requirement;
use pypi_types::{Metadata10, ParsedUrl, VerbatimParsedUrl};
use uv_distribution::{DistributionDatabase, Reporter};
use uv_normalize::PackageName;
use uv_resolver::{InMemoryIndex, MetadataResponse};
use uv_types::{BuildContext, HashStrategy};

#[derive(Debug, thiserror::Error)]
pub enum NamedRequirementsError {
    #[error(transparent)]
    Distribution(#[from] uv_distribution::Error),

    #[error(transparent)]
    DistributionTypes(#[from] distribution_types::Error),

    #[error(transparent)]
    WheelFilename(#[from] distribution_filename::WheelFilenameError),
}

/// Like [`RequirementsSpecification`], but with concrete names for all requirements.
pub struct NamedRequirementsResolver<'a, Context: BuildContext> {
    /// The requirements for the project.
    requirements: Vec<UnnamedRequirement<VerbatimParsedUrl>>,
    /// Whether to check hashes for distributions.
    hasher: &'a HashStrategy,
    /// The in-memory index for resolving dependencies.
    index: &'a InMemoryIndex,
    /// The database for fetching and building distributions.
    database: DistributionDatabase<'a, Context>,
}

impl<'a, Context: BuildContext> NamedRequirementsResolver<'a, Context> {
    /// Instantiate a new [`NamedRequirementsResolver`] for a given set of requirements.
    pub fn new(
        requirements: Vec<UnnamedRequirement<VerbatimParsedUrl>>,
        hasher: &'a HashStrategy,
        index: &'a InMemoryIndex,
        database: DistributionDatabase<'a, Context>,
    ) -> Self {
        Self {
            requirements,
            hasher,
            index,
            database,
        }
    }

    /// Set the [`Reporter`] to use for this resolver.
    #[must_use]
    pub fn with_reporter(self, reporter: impl Reporter + 'static) -> Self {
        Self {
            database: self.database.with_reporter(reporter),
            ..self
        }
    }

    /// Resolve any unnamed requirements in the specification.
    pub async fn resolve(self) -> Result<Vec<Requirement>, NamedRequirementsError> {
        let Self {
            requirements,
            hasher,
            index,
            database,
        } = self;
        requirements
            .into_iter()
            .map(|requirement| async {
                Self::resolve_requirement(requirement, hasher, index, &database)
                    .await
                    .map(Requirement::from)
            })
            .collect::<FuturesOrdered<_>>()
            .try_collect()
            .await
    }

    /// Infer the package name for a given "unnamed" requirement.
    async fn resolve_requirement(
        requirement: UnnamedRequirement<VerbatimParsedUrl>,
        hasher: &HashStrategy,
        index: &InMemoryIndex,
        database: &DistributionDatabase<'a, Context>,
    ) -> Result<pep508_rs::Requirement<VerbatimParsedUrl>, NamedRequirementsError> {
        // If the requirement is a wheel, extract the package name from the wheel filename.
        //
        // Ex) `anyio-4.3.0-py3-none-any.whl`
        if Path::new(requirement.url.verbatim.path())
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("whl"))
        {
            let filename = WheelFilename::from_str(&requirement.url.verbatim.filename()?)?;
            return Ok(pep508_rs::Requirement {
                name: filename.name,
                extras: requirement.extras,
                version_or_url: Some(VersionOrUrl::Url(requirement.url)),
                marker: requirement.marker,
                origin: requirement.origin,
            });
        }

        // If the requirement is a source archive, try to extract the package name from the archive
        // filename. This isn't guaranteed to work.
        //
        // Ex) `anyio-4.3.0.tar.gz`
        if let Some(filename) = requirement
            .url
            .verbatim
            .filename()
            .ok()
            .and_then(|filename| SourceDistFilename::parsed_normalized_filename(&filename).ok())
        {
            // But ignore GitHub archives, like:
            //   https://github.com/python/mypy/archive/refs/heads/release-1.11.zip
            //
            // These have auto-generated filenames that will almost never match the package name.
            if requirement.url.verbatim.host() == Some(Host::Domain("github.com"))
                && requirement
                    .url
                    .verbatim
                    .path_segments()
                    .is_some_and(|mut path_segments| {
                        path_segments.any(|segment| segment == "archive")
                    })
            {
                debug!(
                    "Rejecting inferred name from GitHub archive: {}",
                    requirement.url.verbatim
                );
            } else {
                return Ok(pep508_rs::Requirement {
                    name: filename.name,
                    extras: requirement.extras,
                    version_or_url: Some(VersionOrUrl::Url(requirement.url)),
                    marker: requirement.marker,
                    origin: requirement.origin,
                });
            }
        }

        let source = match &requirement.url.parsed_url {
            // If the path points to a directory, attempt to read the name from static metadata.
            ParsedUrl::Directory(parsed_directory_url) => {
                // Attempt to read a `PKG-INFO` from the directory.
                if let Some(metadata) =
                    fs_err::read(parsed_directory_url.install_path.join("PKG-INFO"))
                        .ok()
                        .and_then(|contents| Metadata10::parse_pkg_info(&contents).ok())
                {
                    debug!(
                        "Found PKG-INFO metadata for {path} ({name})",
                        path = parsed_directory_url.install_path.display(),
                        name = metadata.name
                    );
                    return Ok(pep508_rs::Requirement {
                        name: metadata.name,
                        extras: requirement.extras,
                        version_or_url: Some(VersionOrUrl::Url(requirement.url)),
                        marker: requirement.marker,
                        origin: requirement.origin,
                    });
                }

                // Attempt to read a `pyproject.toml` file.
                let project_path = parsed_directory_url.install_path.join("pyproject.toml");
                if let Some(pyproject) = fs_err::read_to_string(project_path)
                    .ok()
                    .and_then(|contents| toml::from_str::<PyProjectToml>(&contents).ok())
                {
                    // Read PEP 621 metadata from the `pyproject.toml`.
                    if let Some(project) = pyproject.project {
                        debug!(
                            "Found PEP 621 metadata for {path} in `pyproject.toml` ({name})",
                            path = parsed_directory_url.install_path.display(),
                            name = project.name
                        );
                        return Ok(pep508_rs::Requirement {
                            name: project.name,
                            extras: requirement.extras,
                            version_or_url: Some(VersionOrUrl::Url(requirement.url)),
                            marker: requirement.marker,
                            origin: requirement.origin,
                        });
                    }

                    // Read Poetry-specific metadata from the `pyproject.toml`.
                    if let Some(tool) = pyproject.tool {
                        if let Some(poetry) = tool.poetry {
                            if let Some(name) = poetry.name {
                                debug!(
                                    "Found Poetry metadata for {path} in `pyproject.toml` ({name})",
                                    path = parsed_directory_url.install_path.display(),
                                    name = name
                                );
                                return Ok(pep508_rs::Requirement {
                                    name,
                                    extras: requirement.extras,
                                    version_or_url: Some(VersionOrUrl::Url(requirement.url)),
                                    marker: requirement.marker,
                                    origin: requirement.origin,
                                });
                            }
                        }
                    }
                }

                // Attempt to read a `setup.cfg` from the directory.
                if let Some(setup_cfg) =
                    fs_err::read_to_string(parsed_directory_url.install_path.join("setup.cfg"))
                        .ok()
                        .and_then(|contents| {
                            let mut ini = Ini::new_cs();
                            ini.set_multiline(true);
                            ini.read(contents).ok()
                        })
                {
                    if let Some(section) = setup_cfg.get("metadata") {
                        if let Some(Some(name)) = section.get("name") {
                            if let Ok(name) = PackageName::from_str(name) {
                                debug!(
                                    "Found setuptools metadata for {path} in `setup.cfg` ({name})",
                                    path = parsed_directory_url.install_path.display(),
                                    name = name
                                );
                                return Ok(pep508_rs::Requirement {
                                    name,
                                    extras: requirement.extras,
                                    version_or_url: Some(VersionOrUrl::Url(requirement.url)),
                                    marker: requirement.marker,
                                    origin: requirement.origin,
                                });
                            }
                        }
                    }
                }

                SourceUrl::Directory(DirectorySourceUrl {
                    url: &requirement.url.verbatim,
                    install_path: Cow::Borrowed(&parsed_directory_url.install_path),
                    editable: parsed_directory_url.editable,
                })
            }
            ParsedUrl::Path(parsed_path_url) => {
                let ext = match parsed_path_url.ext {
                    DistExtension::Source(ext) => ext,
                    DistExtension::Wheel => unreachable!(),
                };
                SourceUrl::Path(PathSourceUrl {
                    url: &requirement.url.verbatim,
                    path: Cow::Borrowed(&parsed_path_url.install_path),
                    ext,
                })
            }
            ParsedUrl::Archive(parsed_archive_url) => {
                let ext = match parsed_archive_url.ext {
                    DistExtension::Source(ext) => ext,
                    DistExtension::Wheel => unreachable!(),
                };
                SourceUrl::Direct(DirectSourceUrl {
                    url: &parsed_archive_url.url,
                    subdirectory: parsed_archive_url.subdirectory.as_deref(),
                    ext,
                })
            }
            ParsedUrl::Git(parsed_git_url) => SourceUrl::Git(GitSourceUrl {
                url: &requirement.url.verbatim,
                git: &parsed_git_url.url,
                subdirectory: parsed_git_url.subdirectory.as_deref(),
            }),
        };

        // Fetch the metadata for the distribution.
        let name = {
            let id = VersionId::from_url(source.url());
            if let Some(archive) = index
                .distributions()
                .get(&id)
                .as_deref()
                .and_then(|response| {
                    if let MetadataResponse::Found(archive) = response {
                        Some(archive)
                    } else {
                        None
                    }
                })
            {
                // If the metadata is already in the index, return it.
                archive.metadata.name.clone()
            } else {
                // Run the PEP 517 build process to extract metadata from the source distribution.
                let hashes = hasher.get_url(source.url());
                let source = BuildableSource::Url(source);
                let archive = database.build_wheel_metadata(&source, hashes).await?;

                let name = archive.metadata.name.clone();

                // Insert the metadata into the index.
                index
                    .distributions()
                    .done(id, Arc::new(MetadataResponse::Found(archive)));

                name
            }
        };

        Ok(pep508_rs::Requirement {
            name,
            extras: requirement.extras,
            version_or_url: Some(VersionOrUrl::Url(requirement.url)),
            marker: requirement.marker,
            origin: requirement.origin,
        })
    }
}

/// A pyproject.toml as specified in PEP 517.
#[derive(Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
struct PyProjectToml {
    project: Option<Project>,
    tool: Option<Tool>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
struct Project {
    name: PackageName,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
struct Tool {
    poetry: Option<ToolPoetry>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
struct ToolPoetry {
    name: Option<PackageName>,
}
