use std::io;
use std::path::Path;
use std::str::FromStr;

use fs_err as fs;
use pyproject_toml::PyProjectToml;
use toml_edit::Document;

use pep508_rs::Requirement;
use puffin_normalize::PackageName;

use crate::toml::format_multiline_array;
use crate::verbatim::VerbatimRequirement;
use crate::WorkspaceError;

#[derive(Debug)]
pub struct Workspace {
    /// The parsed `pyproject.toml`.
    #[allow(unused)]
    pyproject_toml: PyProjectToml,

    /// The raw document.
    document: Document,
}

impl Workspace {
    /// Add a dependency to the workspace.
    pub fn add_dependency(&mut self, requirement: &VerbatimRequirement<'_>) {
        let Some(project) = self
            .document
            .get_mut("project")
            .map(|project| project.as_table_mut().unwrap())
        else {
            // No `project` table.
            let mut dependencies = toml_edit::Array::new();
            dependencies.push(requirement.given_name);
            format_multiline_array(&mut dependencies);

            let mut project = toml_edit::Table::new();
            project.insert(
                "dependencies",
                toml_edit::Item::Value(toml_edit::Value::Array(dependencies)),
            );

            self.document
                .insert("project", toml_edit::Item::Table(project));

            return;
        };

        let Some(dependencies) = project
            .get_mut("dependencies")
            .map(|dependencies| dependencies.as_array_mut().unwrap())
        else {
            // No `dependencies` array.
            let mut dependencies = toml_edit::Array::new();
            dependencies.push(requirement.given_name);
            format_multiline_array(&mut dependencies);

            project.insert(
                "dependencies",
                toml_edit::Item::Value(toml_edit::Value::Array(dependencies)),
            );
            return;
        };

        let index = dependencies.iter().position(|item| {
            let Some(item) = item.as_str() else {
                return false;
            };

            let Ok(existing) = Requirement::from_str(item) else {
                return false;
            };

            requirement.requirement.name == existing.name
        });

        if let Some(index) = index {
            dependencies.replace(index, requirement.given_name);
        } else {
            dependencies.push(requirement.given_name);
        }

        format_multiline_array(dependencies);
    }

    /// Remove a dependency from the workspace.
    pub fn remove_dependency(&mut self, name: &PackageName) -> Result<(), WorkspaceError> {
        let Some(project) = self
            .document
            .get_mut("project")
            .map(|project| project.as_table_mut().unwrap())
        else {
            return Err(WorkspaceError::MissingProjectTable);
        };

        let Some(dependencies) = project
            .get_mut("dependencies")
            .map(|dependencies| dependencies.as_array_mut().unwrap())
        else {
            return Err(WorkspaceError::MissingProjectDependenciesArray);
        };

        let index = dependencies.iter().position(|item| {
            let Some(item) = item.as_str() else {
                return false;
            };

            let Ok(existing) = Requirement::from_str(item) else {
                return false;
            };

            name == &existing.name
        });

        let Some(index) = index else {
            return Err(WorkspaceError::MissingPackage(name.to_string()));
        };

        dependencies.remove(index);
        format_multiline_array(dependencies);

        Ok(())
    }

    /// Save the workspace to disk.
    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), WorkspaceError> {
        let file = fs::File::create(path.as_ref())?;
        self.write(file)
    }

    /// Write the workspace to a writer.
    fn write(&self, mut writer: impl io::Write) -> Result<(), WorkspaceError> {
        writer.write_all(self.document.to_string().as_bytes())?;
        Ok(())
    }
}

impl TryFrom<&Path> for Workspace {
    type Error = WorkspaceError;

    fn try_from(path: &Path) -> Result<Self, Self::Error> {
        // Read the `pyproject.toml` from disk.
        let contents = fs::read_to_string(path)?;

        // Parse the `pyproject.toml` file.
        let pyproject_toml = toml_edit::de::from_str::<PyProjectToml>(&contents)?;

        // Parse the raw document.
        let document = contents.parse::<Document>()?;

        Ok(Self {
            pyproject_toml,
            document,
        })
    }
}
