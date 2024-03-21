use std::path::Path;
use std::str::FromStr;

use anyhow::{Context, Result};
use configparser::ini::Ini;
use futures::{StreamExt, TryStreamExt};
use once_cell::sync::Lazy;
use regex::Regex;
use serde::Deserialize;
use tracing::debug;

use distribution_filename::{SourceDistFilename, WheelFilename};
use distribution_types::RemoteSource;
use pep508_rs::{Requirement, RequirementsTxtRequirement, UnnamedRequirement, VersionOrUrl};
use pypi_types::Metadata10;
use requirements_txt::EditableRequirement;
use uv_cache::Cache;
use uv_client::RegistryClient;
use uv_distribution::download_and_extract_archive;
use uv_normalize::PackageName;

/// Like [`RequirementsSpecification`], but with concrete names for all requirements.
#[derive(Debug, Default)]
pub struct NamedRequirements {
    /// The requirements for the project.
    pub requirements: Vec<Requirement>,
    /// The constraints for the project.
    pub constraints: Vec<Requirement>,
    /// The overrides for the project.
    pub overrides: Vec<Requirement>,
    /// Package to install as editable installs
    pub editables: Vec<EditableRequirement>,
}

impl NamedRequirements {
    /// Convert a [`RequirementsSpecification`] into a [`NamedRequirements`].
    pub async fn from_spec(
        requirements: Vec<RequirementsTxtRequirement>,
        constraints: Vec<Requirement>,
        overrides: Vec<Requirement>,
        editables: Vec<EditableRequirement>,
        cache: &Cache,
        client: &RegistryClient,
    ) -> Result<Self> {
        // Resolve all unnamed references.
        let requirements = futures::stream::iter(requirements)
            .map(|requirement| async {
                match requirement {
                    RequirementsTxtRequirement::Pep508(requirement) => Ok(requirement),
                    RequirementsTxtRequirement::Unnamed(requirement) => {
                        Self::name_requirement(requirement, cache, client).await
                    }
                }
            })
            .buffer_unordered(50)
            .try_collect()
            .await?;

        Ok(Self {
            requirements,
            constraints,
            overrides,
            editables,
        })
    }

    /// Infer the package name for a given "unnamed" requirement.
    async fn name_requirement(
        requirement: UnnamedRequirement,
        cache: &Cache,
        client: &RegistryClient,
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

        // Download the archive and attempt to infer the package name from the archive contents.
        let source = download_and_extract_archive(&requirement.url, cache, client)
            .await
            .with_context(|| {
                format!("Unable to infer package name for the unnamed requirement: {requirement}")
            })?;

        // Extract the path to the root of the distribution.
        let path = source.path();

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

        // Attempt to read a `setup.py` from the directory.
        if let Ok(setup_py) = fs_err::read_to_string(path.join("setup.py")) {
            static SETUP_PY_NAME: Lazy<Regex> =
                Lazy::new(|| Regex::new(r#"name\s*[=:]\s*['"](?P<name>[^'"]+)['"]"#).unwrap());

            if let Some(name) = SETUP_PY_NAME
                .captures(&setup_py)
                .and_then(|captures| captures.name("name"))
                .map(|name| name.as_str())
            {
                if let Ok(name) = PackageName::from_str(name) {
                    debug!(
                        "Found setuptools metadata for {path} in `setup.py` ({name})",
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

        // TODO(charlie): If this is common, consider running the PEP 517 build hooks.
        Err(anyhow::anyhow!(
            "Unable to infer package name for the unnamed requirement: {requirement}"
        ))
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
