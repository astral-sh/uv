use std::collections::{BTreeMap, BTreeSet};

use url::Url;

use pep508_rs::{RequirementOrigin, VerbatimUrl};
use uv_fs::Simplified;
use uv_normalize::PackageName;

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
                    write!(f, "-r {}", path.user_display())
                }
                RequirementOrigin::Project(path, project_name) => {
                    write!(f, "{project_name} ({})", path.user_display())
                }
            },
            Self::Constraint(origin) => {
                write!(f, "-c {}", origin.path().user_display())
            }
            Self::Override(origin) => {
                write!(f, "--override {}", origin.path().user_display())
            }
        }
    }
}

/// A collection of source annotations.
#[derive(Default, Debug, Clone)]
pub struct SourceAnnotations {
    packages: BTreeMap<PackageName, BTreeSet<SourceAnnotation>>,
    editables: BTreeMap<Url, BTreeSet<SourceAnnotation>>,
}

impl SourceAnnotations {
    /// Add a source annotation to the collection for the given package.
    pub fn add(&mut self, package: &PackageName, annotation: SourceAnnotation) {
        self.packages
            .entry(package.clone())
            .or_default()
            .insert(annotation);
    }

    /// Add an source annotation to the collection for the given editable.
    pub fn add_editable(&mut self, url: &VerbatimUrl, annotation: SourceAnnotation) {
        self.editables
            .entry(url.to_url())
            .or_default()
            .insert(annotation);
    }

    /// Return the source annotations for a given package.
    pub fn get(&self, package: &PackageName) -> Option<&BTreeSet<SourceAnnotation>> {
        self.packages.get(package)
    }

    /// Return the source annotations for a given editable.
    pub fn get_editable(&self, url: &VerbatimUrl) -> Option<&BTreeSet<SourceAnnotation>> {
        self.editables.get(url.raw())
    }
}
