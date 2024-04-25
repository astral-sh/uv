use std::borrow::Cow;
use std::path::Path;
use std::str::FromStr;

use anyhow::Result;
use configparser::ini::Ini;
use futures::{StreamExt, TryStreamExt};
use serde::Deserialize;
use tracing::debug;

use distribution_filename::{SourceDistFilename, WheelFilename};
use distribution_types::{
    BuildableSource, DirectSourceUrl, GitSourceUrl, PathSourceUrl, RemoteSource, SourceUrl,
    UvRequirement, VersionId,
};
use pep508_rs::{Requirement, Scheme, UnnamedRequirement, VersionOrUrl};
use pypi_types::Metadata10;
use requirements_txt::{RequirementEntry, RequirementsTxtRequirement};
use uv_client::RegistryClient;
use uv_distribution::{DistributionDatabase, Reporter};
use uv_normalize::PackageName;
use uv_resolver::{InMemoryIndex, MetadataResponse};
use uv_types::{BuildContext, HashStrategy};

/// Like [`RequirementsSpecification`], but with concrete names for all requirements.
pub struct NamedRequirementsResolver<'a, Context: BuildContext + Send + Sync> {
    /// The requirements for the project.
    requirements: Vec<RequirementEntry>,
    /// Whether to check hashes for distributions.
    hasher: &'a HashStrategy,
    /// The in-memory index for resolving dependencies.
    index: &'a InMemoryIndex,
    /// The database for fetching and building distributions.
    database: DistributionDatabase<'a, Context>,
}

impl<'a, Context: BuildContext + Send + Sync> NamedRequirementsResolver<'a, Context> {
    /// Instantiate a new [`NamedRequirementsResolver`] for a given set of requirements.
    pub fn new(
        requirements: Vec<RequirementEntry>,
        hasher: &'a HashStrategy,
        context: &'a Context,
        client: &'a RegistryClient,
        index: &'a InMemoryIndex,
    ) -> Self {
        Self {
            requirements,
            hasher,
            index,
            database: DistributionDatabase::new(client, context),
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
    pub async fn resolve(self) -> Result<Vec<UvRequirement>> {
        let Self {
            requirements,
            hasher,
            index,
            database,
        } = self;
        futures::stream::iter(requirements)
            .map(|entry| async {
                match entry.requirement {
                    RequirementsTxtRequirement::Uv(requirement) => Ok(requirement),
                    RequirementsTxtRequirement::Unnamed(requirement) => {
                        Ok(UvRequirement::from_requirement(
                            Self::resolve_requirement(requirement, hasher, index, &database)
                                .await?,
                        )?)
                    }
                }
            })
            .buffered(50)
            .try_collect()
            .await
    }

    /// Infer the package name for a given "unnamed" requirement.
    async fn resolve_requirement(
        requirement: UnnamedRequirement,
        hasher: &HashStrategy,
        index: &InMemoryIndex,
        database: &DistributionDatabase<'a, Context>,
    ) -> Result<Requirement> {
        // If the requirement is a wheel, extract the package name from the wheel filename.
        //
        // Ex) `anyio-4.3.0-py3-none-any.whl`
        if Path::new(requirement.url.path())
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("whl"))
        {
            let filename = WheelFilename::from_str(&requirement.url.filename()?)?;
            return Ok(Requirement {
                name: filename.name,
                extras: requirement.extras,
                version_or_url: Some(VersionOrUrl::Url(requirement.url)),
                marker: requirement.marker,
            });
        }

        // If the requirement is a source archive, try to extract the package name from the archive
        // filename. This isn't guaranteed to work.
        //
        // Ex) `anyio-4.3.0.tar.gz`
        if let Some(filename) = requirement
            .url
            .filename()
            .ok()
            .and_then(|filename| SourceDistFilename::parsed_normalized_filename(&filename).ok())
        {
            return Ok(Requirement {
                name: filename.name,
                extras: requirement.extras,
                version_or_url: Some(VersionOrUrl::Url(requirement.url)),
                marker: requirement.marker,
            });
        }

        let source = match Scheme::parse(requirement.url.scheme()) {
            Some(Scheme::File) => {
                let path = requirement
                    .url
                    .to_file_path()
                    .expect("URL to be a file path");

                // If the path points to a directory, attempt to read the name from static metadata.
                if path.is_dir() {
                    // Attempt to read a `PKG-INFO` from the directory.
                    if let Some(metadata) = fs_err::read(path.join("PKG-INFO"))
                        .ok()
                        .and_then(|contents| Metadata10::parse_pkg_info(&contents).ok())
                    {
                        debug!(
                            "Found PKG-INFO metadata for {path} ({name})",
                            path = path.display(),
                            name = metadata.name
                        );
                        return Ok(Requirement {
                            name: metadata.name,
                            extras: requirement.extras,
                            version_or_url: Some(VersionOrUrl::Url(requirement.url)),
                            marker: requirement.marker,
                        });
                    }

                    // Attempt to read a `pyproject.toml` file.
                    if let Some(pyproject) = fs_err::read_to_string(path.join("pyproject.toml"))
                        .ok()
                        .and_then(|contents| toml::from_str::<PyProjectToml>(&contents).ok())
                    {
                        // Read PEP 621 metadata from the `pyproject.toml`.
                        if let Some(project) = pyproject.project {
                            debug!(
                                "Found PEP 621 metadata for {path} in `pyproject.toml` ({name})",
                                path = path.display(),
                                name = project.name
                            );
                            return Ok(Requirement {
                                name: project.name,
                                extras: requirement.extras,
                                version_or_url: Some(VersionOrUrl::Url(requirement.url)),
                                marker: requirement.marker,
                            });
                        }

                        // Read Poetry-specific metadata from the `pyproject.toml`.
                        if let Some(tool) = pyproject.tool {
                            if let Some(poetry) = tool.poetry {
                                if let Some(name) = poetry.name {
                                    debug!(
                                        "Found Poetry metadata for {path} in `pyproject.toml` ({name})",
                                        path = path.display(),
                                        name = name
                                    );
                                    return Ok(Requirement {
                                        name,
                                        extras: requirement.extras,
                                        version_or_url: Some(VersionOrUrl::Url(requirement.url)),
                                        marker: requirement.marker,
                                    });
                                }
                            }
                        }
                    }

                    // Attempt to read a `setup.cfg` from the directory.
                    if let Some(setup_cfg) = fs_err::read_to_string(path.join("setup.cfg"))
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
                                        path = path.display(),
                                        name = name
                                    );
                                    return Ok(Requirement {
                                        name,
                                        extras: requirement.extras,
                                        version_or_url: Some(VersionOrUrl::Url(requirement.url)),
                                        marker: requirement.marker,
                                    });
                                }
                            }
                        }
                    }
                }

                SourceUrl::Path(PathSourceUrl {
                    url: &requirement.url,
                    path: Cow::Owned(path),
                })
            }
            Some(Scheme::Http | Scheme::Https) => SourceUrl::Direct(DirectSourceUrl {
                url: &requirement.url,
            }),
            Some(Scheme::GitSsh | Scheme::GitHttps) => SourceUrl::Git(GitSourceUrl {
                url: &requirement.url,
            }),
            _ => {
                return Err(anyhow::anyhow!(
                    "Unsupported scheme for unnamed requirement: {}",
                    requirement.url
                ));
            }
        };

        // Fetch the metadata for the distribution.
        let name = {
            let id = VersionId::from_url(source.url());
            if let Some(archive) = index.get_metadata(&id).as_deref().and_then(|response| {
                if let MetadataResponse::Found(archive) = response {
                    Some(archive)
                } else {
                    None
                }
            }) {
                // If the metadata is already in the index, return it.
                archive.metadata.name.clone()
            } else {
                // Run the PEP 517 build process to extract metadata from the source distribution.
                let hashes = hasher.get_url(source.url());
                let source = BuildableSource::Url(source);
                let archive = database.build_wheel_metadata(&source, hashes).await?;

                let name = archive.metadata.name.clone();

                // Insert the metadata into the index.
                index.insert_metadata(id, MetadataResponse::Found(archive));

                name
            }
        };

        Ok(Requirement {
            name,
            extras: requirement.extras,
            version_or_url: Some(VersionOrUrl::Url(requirement.url)),
            marker: requirement.marker,
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
