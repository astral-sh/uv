use serde::Deserialize;
use std::path::{Path, PathBuf};

use tracing::debug;
use uv_fs::Simplified;
use uv_normalize::PackageName;

use uv_requirements::RequirementsSource;

#[derive(thiserror::Error, Debug)]
pub(crate) enum ProjectError {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Toml(#[from] toml::de::Error),

    #[error("No `project` section found in: {}", _0.user_display())]
    MissingProject(PathBuf),

    #[error("No `name` found in `project` section in: {}", _0.user_display())]
    MissingName(PathBuf),
}

#[derive(Debug, Clone)]
pub(crate) struct Project {
    /// The name of the package.
    name: PackageName,
    /// The path to the `pyproject.toml` file.
    path: PathBuf,
    /// The path to the project root.
    root: PathBuf,
    /// aaa.
    overrides: Vec<String>,
}

impl Project {
    /// Find the current project.
    pub(crate) fn find(path: impl AsRef<Path>) -> Result<Option<Self>, ProjectError> {
        for ancestor in path.as_ref().ancestors() {
            let pyproject_path = ancestor.join("pyproject.toml");
            if pyproject_path.exists() {
                debug!(
                    "Loading requirements from: {}",
                    pyproject_path.user_display()
                );

                // Read the `pyproject.toml`.
                let contents = fs_err::read_to_string(&pyproject_path)?;
                let pyproject_toml: PyProjectToml = toml::from_str(&contents)?;                
                let mut overrides: Vec<String> =  Vec::<String>::new();

                // `override`属性を抽出する
                if let Some(tool) = &pyproject_toml.tool {
                    if let Some(uv) = &tool.uv {
                        if let Some(overrides_deps) = &uv.overrides {
                            for value in overrides_deps {
                                debug!("{}", value);
                                overrides.push(value.clone());
                            }
                        } else {
                            debug!("'override' attribute not found");
                        }
                        
                    }
                }

                // Extract the package name.
                let Some(project) = pyproject_toml.project else {
                    return Err(ProjectError::MissingProject(pyproject_path));
                };
                let Some(name) = project.name else {
                    return Err(ProjectError::MissingName(pyproject_path));
                };

                return Ok(Some(Self {
                    name,
                    path: pyproject_path,
                    root: ancestor.to_path_buf(),
                    overrides: overrides
                }));
            }
        }

        Ok(None)
    }

    /// Return the [`PackageName`] for the project.
    pub(crate) fn name(&self) -> &PackageName {
        &self.name
    }

    /// Return the root path for the project.
    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    /// Return the requirements for the project.
    pub(crate) fn requirements(&self) -> Vec<RequirementsSource> {
        vec![
            RequirementsSource::from_requirements_file(self.path.clone()),
            RequirementsSource::from_source_tree(self.root.clone()),
        ]
    }
    /// Return the requirements for the project.
    pub(crate) fn overrides(&self) -> Vec<RequirementsSource> {
        let mut requirements: Vec<RequirementsSource> = Vec::<RequirementsSource>::new();

        for override_req in &self.overrides {
            println!("Reqs: {}", override_req);
            requirements.push(RequirementsSource::from_package(override_req.clone()));
        }

        requirements
    }
}

/// A pyproject.toml as specified in PEP 517.
#[derive(Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
struct PyProjectToml {
    project: Option<PyProjectProject>,
    tool: Option<Tool>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
struct PyProjectProject {
    name: Option<PackageName>,
}

#[derive(Deserialize, Debug)]
struct Tool {
    uv: Option<UV>,
}

#[derive(Deserialize, Debug)]
struct UV {
    overrides: Option<Vec<String>>,
}