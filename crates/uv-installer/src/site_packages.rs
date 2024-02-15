use std::hash::BuildHasherDefault;
use std::path::PathBuf;

use anyhow::{Context, Result};
use fs_err as fs;
use rustc_hash::{FxHashMap, FxHashSet};
use url::Url;

use distribution_types::{InstalledDist, InstalledMetadata, InstalledVersion, Name};
use pep440_rs::{Version, VersionSpecifiers};
use pep508_rs::{Requirement, VerbatimUrl};
use requirements_txt::EditableRequirement;
use uv_interpreter::Virtualenv;
use uv_normalize::PackageName;

/// An index over the packages installed in an environment.
///
/// Packages are indexed by both name and (for editable installs) URL.
#[derive(Debug)]
pub struct SitePackages<'a> {
    venv: &'a Virtualenv,
    /// The vector of all installed distributions.
    distributions: Vec<InstalledDist>,
    /// The installed distributions, keyed by name.
    by_name: FxHashMap<PackageName, usize>,
    /// The installed editable distributions, keyed by URL.
    by_url: FxHashMap<Url, usize>,
}

impl<'a> SitePackages<'a> {
    /// Build an index of installed packages from the given Python executable.
    pub fn from_executable(venv: &'a Virtualenv) -> Result<SitePackages<'a>> {
        let mut distributions: Vec<InstalledDist> = Vec::new();
        let mut by_name = FxHashMap::default();
        let mut by_url = FxHashMap::default();

        // Index all installed packages by name.
        for entry in fs::read_dir(venv.site_packages())? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                let path = entry.path();

                let Some(dist_info) = InstalledDist::try_from_path(&path)
                    .with_context(|| format!("Failed to read metadata: from {}", path.display()))?
                else {
                    continue;
                };

                let idx = distributions.len();

                // Index the distribution by name.
                if let Some(existing) = by_name.insert(dist_info.name().clone(), idx) {
                    let existing = &distributions[existing];
                    anyhow::bail!(
                        "Found duplicate package in environment: {} ({} vs. {})",
                        existing.name(),
                        existing.path().display(),
                        path.display()
                    );
                }

                // Index the distribution by URL.
                if let Some(url) = dist_info.as_editable() {
                    if let Some(existing) = by_url.insert(url.clone(), idx) {
                        let existing = &distributions[existing];
                        anyhow::bail!(
                            "Found duplicate editable in environment: {} ({} vs. {})",
                            existing.name(),
                            existing.path().display(),
                            path.display()
                        );
                    }
                }

                // Add the distribution to the database.
                distributions.push(dist_info);
            }
        }

        Ok(Self {
            venv,
            distributions,
            by_name,
            by_url,
        })
    }

    /// Returns an iterator over the installed distributions.
    pub fn iter(&self) -> impl Iterator<Item = &InstalledDist> {
        self.distributions.iter()
    }

    /// Returns an iterator over the the installed distributions, represented as requirements.
    pub fn requirements(&self) -> impl Iterator<Item = Requirement> + '_ {
        self.iter().map(|dist| Requirement {
            name: dist.name().clone(),
            extras: vec![],
            version_or_url: Some(match dist.installed_version() {
                InstalledVersion::Version(version) => {
                    pep508_rs::VersionOrUrl::VersionSpecifier(pep440_rs::VersionSpecifiers::from(
                        pep440_rs::VersionSpecifier::equals_version(version.clone()),
                    ))
                }
                InstalledVersion::Url(url, ..) => {
                    pep508_rs::VersionOrUrl::Url(VerbatimUrl::unknown(url.clone()))
                }
            }),
            marker: None,
        })
    }

    /// Returns the version of the given package, if it is installed.
    pub fn get(&self, name: &PackageName) -> Option<&InstalledDist> {
        self.by_name.get(name).map(|idx| &self.distributions[*idx])
    }

    /// Remove the given package from the index, returning its version if it was installed.
    pub fn remove(&mut self, name: &PackageName) -> Option<InstalledDist> {
        let idx = self.by_name.get(name)?;
        Some(self.swap_remove(*idx))
    }

    /// Returns the editable distribution installed from the given URL, if any.
    pub fn get_editable(&self, url: &Url) -> Option<&InstalledDist> {
        self.by_url.get(url).map(|idx| &self.distributions[*idx])
    }

    /// Remove the editable distribution installed from the given URL, if any.
    pub fn remove_editable(&mut self, url: &Url) -> Option<InstalledDist> {
        let idx = self.by_url.get(url)?;
        Some(self.swap_remove(*idx))
    }

    /// Remove the distribution at the given index.
    fn swap_remove(&mut self, idx: usize) -> InstalledDist {
        // Remove from the existing index.
        let dist = self.distributions.swap_remove(idx);

        // If the distribution wasn't at the end, rewrite the entries for the moved distribution.
        if idx < self.distributions.len() {
            let moved = &self.distributions[idx];
            if let Some(prev) = self.by_name.get_mut(moved.name()) {
                *prev = idx;
            }
            if let Some(url) = moved.as_editable() {
                if let Some(prev) = self.by_url.get_mut(url) {
                    *prev = idx;
                }
            }
        }

        dist
    }

    /// Returns `true` if there are no installed packages.
    pub fn is_empty(&self) -> bool {
        self.distributions.is_empty()
    }

    /// Returns the number of installed packages.
    pub fn len(&self) -> usize {
        self.distributions.len()
    }

    /// Validate the installed packages in the virtual environment.
    pub fn diagnostics(&self) -> Result<Vec<Diagnostic>> {
        let mut diagnostics = Vec::new();

        for (package, index) in &self.by_name {
            let distribution = &self.distributions[*index];

            // Determine the dependencies for the given package.
            let Ok(metadata) = distribution.metadata() else {
                diagnostics.push(Diagnostic::IncompletePackage {
                    package: package.clone(),
                    path: distribution.path().to_owned(),
                });
                continue;
            };

            // Verify that the package is compatible with the current Python version.
            if let Some(requires_python) = metadata.requires_python.as_ref() {
                if !requires_python.contains(self.venv.interpreter().python_version()) {
                    diagnostics.push(Diagnostic::IncompatiblePythonVersion {
                        package: package.clone(),
                        version: self.venv.interpreter().python_version().clone(),
                        requires_python: requires_python.clone(),
                    });
                }
            }

            // Verify that the dependencies are installed.
            for requirement in &metadata.requires_dist {
                if !requirement.evaluate_markers(self.venv.interpreter().markers(), &[]) {
                    continue;
                }

                let Some(installed) = self
                    .by_name
                    .get(&requirement.name)
                    .map(|idx| &self.distributions[*idx])
                else {
                    diagnostics.push(Diagnostic::MissingDependency {
                        package: package.clone(),
                        requirement: requirement.clone(),
                    });
                    continue;
                };

                match &requirement.version_or_url {
                    None | Some(pep508_rs::VersionOrUrl::Url(_)) => {
                        // Nothing to do (accept any installed version).
                    }
                    Some(pep508_rs::VersionOrUrl::VersionSpecifier(version_specifier)) => {
                        if !version_specifier.contains(installed.version()) {
                            diagnostics.push(Diagnostic::IncompatibleDependency {
                                package: package.clone(),
                                version: installed.version().clone(),
                                requirement: requirement.clone(),
                            });
                        }
                    }
                }
            }
        }

        Ok(diagnostics)
    }

    /// Returns `true` if the installed packages satisfy the given requirements.
    pub fn satisfies(
        &self,
        requirements: &[Requirement],
        editables: &[EditableRequirement],
        constraints: &[Requirement],
    ) -> Result<bool> {
        let mut requirements = requirements.to_vec();
        let mut seen =
            FxHashSet::with_capacity_and_hasher(requirements.len(), BuildHasherDefault::default());

        // Verify that all editable requirements are met.
        for requirement in editables {
            let Some(distribution) = self
                .by_url
                .get(requirement.raw())
                .map(|idx| &self.distributions[*idx])
            else {
                // The package isn't installed.
                return Ok(false);
            };

            // Recurse into the dependencies.
            let metadata = distribution
                .metadata()
                .with_context(|| format!("Failed to read metadata for: {distribution}"))?;
            requirements.extend(metadata.requires_dist);
        }

        // Verify that all non-editable requirements are met.
        while let Some(requirement) = requirements.pop() {
            if !requirement.evaluate_markers(self.venv.interpreter().markers(), &[]) {
                continue;
            }

            let Some(distribution) = self
                .by_name
                .get(&requirement.name)
                .map(|idx| &self.distributions[*idx])
            else {
                // The package isn't installed.
                return Ok(false);
            };

            // Validate that the installed version matches the requirement.
            match &requirement.version_or_url {
                None | Some(pep508_rs::VersionOrUrl::Url(_)) => {}
                Some(pep508_rs::VersionOrUrl::VersionSpecifier(version_specifier)) => {
                    // The installed version doesn't satisfy the requirement.
                    if !version_specifier.contains(distribution.version()) {
                        return Ok(false);
                    }
                }
            }

            // Validate that the installed version satisfies the constraints.
            for constraint in constraints {
                if !constraint.evaluate_markers(self.venv.interpreter().markers(), &[]) {
                    continue;
                }

                match &constraint.version_or_url {
                    None | Some(pep508_rs::VersionOrUrl::Url(_)) => {}
                    Some(pep508_rs::VersionOrUrl::VersionSpecifier(version_specifier)) => {
                        // The installed version doesn't satisfy the constraint.
                        if !version_specifier.contains(distribution.version()) {
                            return Ok(false);
                        }
                    }
                }
            }

            // Recurse into the dependencies.
            if seen.insert(requirement) {
                let metadata = distribution
                    .metadata()
                    .with_context(|| format!("Failed to read metadata for: {distribution}"))?;
                requirements.extend(metadata.requires_dist);
            }
        }

        Ok(true)
    }
}

impl IntoIterator for SitePackages<'_> {
    type Item = InstalledDist;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.distributions.into_iter()
    }
}

#[derive(Debug)]
pub enum Diagnostic {
    IncompletePackage {
        /// The package that is missing metadata.
        package: PackageName,
        /// The path to the package.
        path: PathBuf,
    },
    IncompatiblePythonVersion {
        /// The package that requires a different version of Python.
        package: PackageName,
        /// The version of Python that is installed.
        version: Version,
        /// The version of Python that is required.
        requires_python: VersionSpecifiers,
    },
    MissingDependency {
        /// The package that is missing a dependency.
        package: PackageName,
        /// The dependency that is missing.
        requirement: Requirement,
    },
    IncompatibleDependency {
        /// The package that has an incompatible dependency.
        package: PackageName,
        /// The version of the package that is installed.
        version: Version,
        /// The dependency that is incompatible.
        requirement: Requirement,
    },
}

impl Diagnostic {
    /// Convert the diagnostic into a user-facing message.
    pub fn message(&self) -> String {
        match self {
            Self::IncompletePackage { package, path } => format!(
                "The package `{package}` is broken or incomplete (unable to read `METADATA`). Consider recreating the virtualenv, or removing the package directory at: {}.", path.display(),
            ),
            Self::IncompatiblePythonVersion {
                package,
                version,
                requires_python,
            } => format!(
                "The package `{package}` requires Python {requires_python}, but `{version}` is installed."
            ),
            Self::MissingDependency {
                package,
                requirement,
            } => {
                format!("The package `{package}` requires `{requirement}`, but it's not installed.")
            }
            Self::IncompatibleDependency {
                package,
                version,
                requirement,
            } => format!(
                "The package `{package}` requires `{requirement}`, but `{version}` is installed."
            ),
        }
    }

    /// Returns `true` if the [`PackageName`] is involved in this diagnostic.
    pub fn includes(&self, name: &PackageName) -> bool {
        match self {
            Self::IncompletePackage { package, .. } => name == package,
            Self::IncompatiblePythonVersion { package, .. } => name == package,
            Self::MissingDependency { package, .. } => name == package,
            Self::IncompatibleDependency {
                package,
                requirement,
                ..
            } => name == package || &requirement.name == name,
        }
    }
}
