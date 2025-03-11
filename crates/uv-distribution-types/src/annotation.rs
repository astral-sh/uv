use std::collections::{BTreeMap, BTreeSet};

use uv_fs::Simplified;
use uv_normalize::PackageName;
use uv_pep508::RequirementOrigin;

/// Source of a dependency, e.g., a `-r requirements.txt` file.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SourceAnnotation {
    /// A `-c constraints.txt` file.
    Constraint(RequirementOrigin),
    /// An `--override overrides.txt` file.
    Override(RequirementOrigin),
    /// A `-r requirements.txt` file.
    Requirement(RequirementOrigin),
}

impl std::fmt::Display for SourceAnnotation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Requirement(origin) => match origin {
                RequirementOrigin::File(path) => {
                    write!(f, "-r {}", path.portable_display())
                }
                RequirementOrigin::Project(path, project_name) => {
                    write!(f, "{project_name} ({})", path.portable_display())
                }
                RequirementOrigin::Group(path, project_name, group) => {
                    write!(f, "{project_name} ({}:{group})", path.portable_display())
                }
                RequirementOrigin::Workspace => {
                    write!(f, "(workspace)")
                }
            },
            Self::Constraint(origin) => {
                write!(f, "-c {}", origin.path().portable_display())
            }
            Self::Override(origin) => match origin {
                RequirementOrigin::File(path) => {
                    write!(f, "--override {}", path.portable_display())
                }
                RequirementOrigin::Project(path, project_name) => {
                    // Project is not used for override
                    write!(f, "--override {project_name} ({})", path.portable_display())
                }
                RequirementOrigin::Group(path, project_name, group) => {
                    // Group is not used for override
                    write!(
                        f,
                        "--override {project_name} ({}:{group})",
                        path.portable_display()
                    )
                }
                RequirementOrigin::Workspace => {
                    write!(f, "--override (workspace)")
                }
            },
        }
    }
}

/// A collection of source annotations.
#[derive(Default, Debug, Clone)]
pub struct SourceAnnotations(BTreeMap<PackageName, BTreeSet<SourceAnnotation>>);

impl SourceAnnotations {
    /// Add a source annotation to the collection for the given package.
    pub fn add(&mut self, package: &PackageName, annotation: SourceAnnotation) {
        self.0
            .entry(package.clone())
            .or_default()
            .insert(annotation);
    }

    /// Return the source annotations for a given package.
    pub fn get(&self, package: &PackageName) -> Option<&BTreeSet<SourceAnnotation>> {
        self.0.get(package)
    }
}
