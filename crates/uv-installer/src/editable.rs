use pyproject_toml::Project;
use serde::Deserialize;

use distribution_types::{
    CachedDist, InstalledDist, InstalledMetadata, InstalledVersion, LocalEditable, Name,
};
use pypi_types::Metadata23;
use requirements_txt::EditableRequirement;

use uv_normalize::PackageName;

/// An editable distribution that has been built.
#[derive(Debug, Clone)]
pub struct BuiltEditable {
    pub editable: LocalEditable,
    pub wheel: CachedDist,
    pub metadata: Metadata23,
}

/// An editable distribution that has been resolved to a concrete distribution.
#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum ResolvedEditable {
    /// The editable is already installed in the environment.
    Installed(InstalledDist),
    /// The editable has been built and is ready to be installed.
    Built(BuiltEditable),
}

impl Name for BuiltEditable {
    fn name(&self) -> &PackageName {
        &self.metadata.name
    }
}

impl Name for ResolvedEditable {
    fn name(&self) -> &PackageName {
        match self {
            Self::Installed(dist) => dist.name(),
            Self::Built(dist) => dist.name(),
        }
    }
}

impl InstalledMetadata for BuiltEditable {
    fn installed_version(&self) -> InstalledVersion {
        self.wheel.installed_version()
    }
}

impl InstalledMetadata for ResolvedEditable {
    fn installed_version(&self) -> InstalledVersion {
        match self {
            Self::Installed(dist) => dist.installed_version(),
            Self::Built(dist) => dist.installed_version(),
        }
    }
}

impl std::fmt::Display for BuiltEditable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.name(), self.installed_version())
    }
}

impl std::fmt::Display for ResolvedEditable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.name(), self.installed_version())
    }
}

/// Returns `true` if the [`EditableRequirement`] contains dynamic metadata.
pub fn is_dynamic(editable: &EditableRequirement) -> bool {
    // If there's no `pyproject.toml`, we assume it's dynamic.
    let Ok(contents) = fs_err::read_to_string(editable.path.join("pyproject.toml")) else {
        return true;
    };
    let Ok(pyproject_toml) = toml::from_str::<PyProjectToml>(&contents) else {
        return true;
    };
    // If `[project]` is not present, we assume it's dynamic.
    let Some(project) = pyproject_toml.project else {
        // ...unless it appears to be a Poetry project.
        return pyproject_toml
            .tool
            .map_or(true, |tool| tool.poetry.is_none());
    };
    // `[project.dynamic]` must be present and non-empty.
    project.dynamic.is_some_and(|dynamic| !dynamic.is_empty())
}

/// A pyproject.toml as specified in PEP 517.
#[derive(Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
struct PyProjectToml {
    project: Option<Project>,
    tool: Option<Tool>,
}

#[derive(Deserialize, Debug)]
struct Tool {
    poetry: Option<ToolPoetry>,
}

#[derive(Deserialize, Debug)]
struct ToolPoetry {
    #[allow(dead_code)]
    name: Option<String>,
}
