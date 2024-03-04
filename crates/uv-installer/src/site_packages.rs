use std::hash::BuildHasherDefault;
use std::iter::Flatten;
use std::path::PathBuf;

use anyhow::{Context, Result};
use fs_err as fs;
use rustc_hash::{FxHashMap, FxHashSet};
use url::Url;

use distribution_types::{InstalledDist, InstalledMetadata, InstalledVersion, Name};
use pep440_rs::{Version, VersionSpecifiers};
use pep508_rs::{Requirement, VerbatimUrl};
use requirements_txt::EditableRequirement;
use uv_cache::{ArchiveTarget, ArchiveTimestamp};
use uv_interpreter::PythonEnvironment;
use uv_normalize::PackageName;

use crate::is_dynamic;

/// An index over the packages installed in an environment.
///
/// Packages are indexed by both name and (for editable installs) URL.
#[derive(Debug)]
pub struct SitePackages<'a> {
    venv: &'a PythonEnvironment,
    /// The vector of all installed distributions. The `by_name` and `by_url` indices index into
    /// this vector. The vector may contain `None` values, which represent distributions that were
    /// removed from the virtual environment.
    distributions: Vec<Option<InstalledDist>>,
    /// The installed distributions, keyed by name. Although the Python runtime does not support it,
    /// it is possible to have multiple distributions with the same name to be present in the
    /// virtual environment, which we handle gracefully.
    by_name: FxHashMap<PackageName, Vec<usize>>,
    /// The installed editable distributions, keyed by URL.
    by_url: FxHashMap<Url, Vec<usize>>,
}

impl<'a> SitePackages<'a> {
    /// Build an index of installed packages from the given Python executable.
    pub fn from_executable(venv: &'a PythonEnvironment) -> Result<SitePackages<'a>> {
        let mut distributions: Vec<Option<InstalledDist>> = Vec::new();
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
                by_name
                    .entry(dist_info.name().clone())
                    .or_insert_with(Vec::new)
                    .push(idx);

                // Index the distribution by URL.
                if let Some(url) = dist_info.as_editable() {
                    by_url.entry(url.clone()).or_insert_with(Vec::new).push(idx);
                }

                // Add the distribution to the database.
                distributions.push(Some(dist_info));
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
        self.distributions.iter().flatten()
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

    /// Returns the installed distributions for a given package.
    pub fn get_packages(&self, name: &PackageName) -> Vec<&InstalledDist> {
        let Some(indexes) = self.by_name.get(name) else {
            return Vec::new();
        };
        indexes
            .iter()
            .flat_map(|&index| &self.distributions[index])
            .collect()
    }

    /// Remove the given packages from the index, returning all installed versions, if any.
    pub fn remove_packages(&mut self, name: &PackageName) -> Vec<InstalledDist> {
        let Some(indexes) = self.by_name.get(name) else {
            return Vec::new();
        };
        indexes
            .iter()
            .filter_map(|index| std::mem::take(&mut self.distributions[*index]))
            .collect()
    }

    /// Returns the editable distribution installed from the given URL, if any.
    pub fn get_editables(&self, url: &Url) -> Vec<&InstalledDist> {
        let Some(indexes) = self.by_url.get(url) else {
            return Vec::new();
        };
        indexes
            .iter()
            .flat_map(|&index| &self.distributions[index])
            .collect()
    }

    /// Remove the editable distribution installed from the given URL, if any.
    pub fn remove_editables(&mut self, url: &Url) -> Vec<InstalledDist> {
        let Some(indexes) = self.by_url.get(url) else {
            return Vec::new();
        };
        indexes
            .iter()
            .filter_map(|index| std::mem::take(&mut self.distributions[*index]))
            .collect()
    }

    /// Returns `true` if there are any installed packages.
    pub fn any(&self) -> bool {
        self.distributions.iter().any(Option::is_some)
    }

    /// Validate the installed packages in the virtual environment.
    pub fn diagnostics(&self) -> Result<Vec<Diagnostic>> {
        let mut diagnostics = Vec::new();

        for (package, indexes) in &self.by_name {
            let mut distributions = indexes.iter().flat_map(|index| &self.distributions[*index]);

            // Find the installed distribution for the given package.
            let Some(distribution) = distributions.next() else {
                continue;
            };

            if let Some(conflict) = distributions.next() {
                // There are multiple installed distributions for the same package.
                diagnostics.push(Diagnostic::DuplicatePackage {
                    package: package.clone(),
                    paths: std::iter::once(distribution.path().to_owned())
                        .chain(std::iter::once(conflict.path().to_owned()))
                        .chain(distributions.map(|dist| dist.path().to_owned()))
                        .collect(),
                });
                continue;
            }

            for index in indexes {
                let Some(distribution) = &self.distributions[*index] else {
                    continue;
                };

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
                for dependency in &metadata.requires_dist {
                    if !dependency.evaluate_markers(self.venv.interpreter().markers(), &[]) {
                        continue;
                    }

                    let installed = self.get_packages(&dependency.name);
                    match installed.as_slice() {
                        [] => {
                            // No version installed.
                            diagnostics.push(Diagnostic::MissingDependency {
                                package: package.clone(),
                                requirement: dependency.clone(),
                            });
                        }
                        [installed] => {
                            match &dependency.version_or_url {
                                None | Some(pep508_rs::VersionOrUrl::Url(_)) => {
                                    // Nothing to do (accept any installed version).
                                }
                                Some(pep508_rs::VersionOrUrl::VersionSpecifier(
                                    version_specifier,
                                )) => {
                                    // The installed version doesn't satisfy the requirement.
                                    if !version_specifier.contains(installed.version()) {
                                        diagnostics.push(Diagnostic::IncompatibleDependency {
                                            package: package.clone(),
                                            version: installed.version().clone(),
                                            requirement: dependency.clone(),
                                        });
                                    }
                                }
                            }
                        }
                        _ => {
                            // There are multiple installed distributions for the same package.
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
        let mut stack = Vec::<Requirement>::with_capacity(requirements.len());
        let mut seen =
            FxHashSet::with_capacity_and_hasher(requirements.len(), BuildHasherDefault::default());

        // Add the direct requirements to the queue.
        for dependency in requirements {
            if dependency.evaluate_markers(self.venv.interpreter().markers(), &[])
                && seen.insert(dependency.clone())
            {
                stack.push(dependency.clone());
            }
        }

        // Verify that all editable requirements are met.
        for requirement in editables {
            let installed = self.get_editables(requirement.raw());
            match installed.as_slice() {
                [] => {
                    // The package isn't installed.
                    return Ok(false);
                }
                [distribution] => {
                    // Is the editable out-of-date?
                    if !ArchiveTimestamp::up_to_date_with(
                        &requirement.path,
                        ArchiveTarget::Install(distribution),
                    )? {
                        return Ok(false);
                    }

                    // Does the editable have dynamic metadata?
                    if is_dynamic(requirement) {
                        return Ok(false);
                    }

                    // Recurse into the dependencies.
                    let metadata = distribution
                        .metadata()
                        .with_context(|| format!("Failed to read metadata for: {distribution}"))?;

                    // Add the dependencies to the queue.
                    for dependency in metadata.requires_dist {
                        if dependency.evaluate_markers(
                            self.venv.interpreter().markers(),
                            &requirement.extras,
                        ) && seen.insert(dependency.clone())
                        {
                            stack.push(dependency);
                        }
                    }
                }
                _ => {
                    // There are multiple installed distributions for the same package.
                    return Ok(false);
                }
            }
        }

        // Verify that all non-editable requirements are met.
        while let Some(requirement) = stack.pop() {
            let installed = self.get_packages(&requirement.name);
            match installed.as_slice() {
                [] => {
                    // The package isn't installed.
                    return Ok(false);
                }
                [distribution] => {
                    // Validate that the installed version matches the requirement.
                    match &requirement.version_or_url {
                        // Accept any installed version.
                        None => {}

                        // If the requirement comes from a URL, verify by URL.
                        Some(pep508_rs::VersionOrUrl::Url(url)) => {
                            let InstalledDist::Url(installed) = &distribution else {
                                return Ok(false);
                            };

                            if &installed.url != url.raw() {
                                return Ok(false);
                            }

                            // If the requirement came from a local path, check freshness.
                            if let Ok(archive) = url.to_file_path() {
                                if !ArchiveTimestamp::up_to_date_with(
                                    &archive,
                                    ArchiveTarget::Install(distribution),
                                )? {
                                    return Ok(false);
                                }
                            }
                        }

                        Some(pep508_rs::VersionOrUrl::VersionSpecifier(version_specifier)) => {
                            // The installed version doesn't satisfy the requirement.
                            if !version_specifier.contains(distribution.version()) {
                                return Ok(false);
                            }
                        }
                    }

                    // Validate that the installed version satisfies the constraints.
                    for constraint in constraints {
                        if constraint.name != requirement.name {
                            continue;
                        }

                        if !constraint.evaluate_markers(self.venv.interpreter().markers(), &[]) {
                            continue;
                        }

                        match &constraint.version_or_url {
                            // Accept any installed version.
                            None => {}

                            // If the requirement comes from a URL, verify by URL.
                            Some(pep508_rs::VersionOrUrl::Url(url)) => {
                                let InstalledDist::Url(installed) = &distribution else {
                                    return Ok(false);
                                };

                                if &installed.url != url.raw() {
                                    return Ok(false);
                                }

                                // If the requirement came from a local path, check freshness.
                                if let Ok(archive) = url.to_file_path() {
                                    if !ArchiveTimestamp::up_to_date_with(
                                        &archive,
                                        ArchiveTarget::Install(distribution),
                                    )? {
                                        return Ok(false);
                                    }
                                }
                            }

                            Some(pep508_rs::VersionOrUrl::VersionSpecifier(version_specifier)) => {
                                // The installed version doesn't satisfy the requirement.
                                if !version_specifier.contains(distribution.version()) {
                                    return Ok(false);
                                }
                            }
                        }
                    }

                    // Recurse into the dependencies.
                    let metadata = distribution
                        .metadata()
                        .with_context(|| format!("Failed to read metadata for: {distribution}"))?;

                    // Add the dependencies to the queue.
                    for dependency in metadata.requires_dist {
                        if dependency.evaluate_markers(
                            self.venv.interpreter().markers(),
                            &requirement.extras,
                        ) && seen.insert(dependency.clone())
                        {
                            stack.push(dependency);
                        }
                    }
                }
                _ => {
                    // There are multiple installed distributions for the same package.
                    return Ok(false);
                }
            }
        }

        Ok(true)
    }
}

impl IntoIterator for SitePackages<'_> {
    type Item = InstalledDist;
    type IntoIter = Flatten<std::vec::IntoIter<Option<InstalledDist>>>;

    fn into_iter(self) -> Self::IntoIter {
        self.distributions.into_iter().flatten()
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
    DuplicatePackage {
        /// The package that has multiple installed distributions.
        package: PackageName,
        /// The installed versions of the package.
        paths: Vec<PathBuf>,
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
            Self::DuplicatePackage { package, paths} => {
                let mut paths = paths.clone();
                paths.sort();
                format!(
                    "The package `{package}` has multiple installed distributions:{}",
                    paths.iter().fold(String::new(), |acc, path| acc + &format!("\n  - {}", path.display()))
                )
            },
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
            Self::DuplicatePackage { package, .. } => name == package,
        }
    }
}
