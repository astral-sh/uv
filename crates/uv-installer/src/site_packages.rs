use std::collections::BTreeSet;
use std::iter::Flatten;
use std::path::PathBuf;

use anyhow::{Context, Result};
use fs_err as fs;
use rustc_hash::{FxBuildHasher, FxHashMap, FxHashSet};
use url::Url;

use distribution_types::{
    Diagnostic, InstalledDist, Name, UnresolvedRequirement, UnresolvedRequirementSpecification,
};
use pep440_rs::{Version, VersionSpecifiers};
use pypi_types::{Requirement, VerbatimParsedUrl};
use uv_fs::Simplified;
use uv_normalize::PackageName;
use uv_python::{Interpreter, PythonEnvironment};
use uv_types::InstalledPackagesProvider;
use uv_warnings::warn_user;

use crate::satisfies::RequirementSatisfaction;

/// An index over the packages installed in an environment.
///
/// Packages are indexed by both name and (for editable installs) URL.
#[derive(Debug, Clone)]
pub struct SitePackages {
    interpreter: Interpreter,
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

impl SitePackages {
    /// Build an index of installed packages from the given Python environment.
    pub fn from_environment(environment: &PythonEnvironment) -> Result<Self> {
        Self::from_interpreter(environment.interpreter())
    }

    /// Build an index of installed packages from the given Python executable.
    pub fn from_interpreter(interpreter: &Interpreter) -> Result<Self> {
        let mut distributions: Vec<Option<InstalledDist>> = Vec::new();
        let mut by_name = FxHashMap::default();
        let mut by_url = FxHashMap::default();

        for site_packages in interpreter.site_packages() {
            // Read the site-packages directory.
            let site_packages = match fs::read_dir(site_packages) {
                Ok(site_packages) => {
                    // Collect sorted directory paths; `read_dir` is not stable across platforms
                    let dist_likes: BTreeSet<_> = site_packages
                        .filter_map(|read_dir| match read_dir {
                            Ok(entry) => match entry.file_type() {
                                Ok(file_type) => (file_type.is_dir()
                                    || entry
                                        .path()
                                        .extension()
                                        .is_some_and(|ext| ext == "egg-link" || ext == "egg-info"))
                                .then_some(Ok(entry.path())),
                                Err(err) => Some(Err(err)),
                            },
                            Err(err) => Some(Err(err)),
                        })
                        .collect::<Result<_, std::io::Error>>()?;
                    dist_likes
                }
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                    return Ok(Self {
                        interpreter: interpreter.clone(),
                        distributions,
                        by_name,
                        by_url,
                    });
                }
                Err(err) => return Err(err).context("Failed to read site-packages directory"),
            };

            // Index all installed packages by name.
            for path in site_packages {
                let dist_info = match InstalledDist::try_from_path(&path) {
                    Ok(Some(dist_info)) => dist_info,
                    Ok(None) => continue,
                    Err(_)
                        if path.file_name().is_some_and(|name| {
                            name.to_str().is_some_and(|name| name.starts_with('~'))
                        }) =>
                    {
                        warn_user!(
                            "Ignoring dangling temporary directory: `{}`",
                            path.simplified_display().cyan()
                        );
                        continue;
                    }
                    Err(err) => {
                        return Err(err).context(format!(
                            "Failed to read metadata from: `{}`",
                            path.simplified_display()
                        ));
                    }
                };

                let idx = distributions.len();

                // Index the distribution by name.
                by_name
                    .entry(dist_info.name().clone())
                    .or_default()
                    .push(idx);

                // Index the distribution by URL.
                if let InstalledDist::Url(dist) = &dist_info {
                    by_url.entry(dist.url.clone()).or_default().push(idx);
                }

                // Add the distribution to the database.
                distributions.push(Some(dist_info));
            }
        }

        Ok(Self {
            interpreter: interpreter.clone(),
            distributions,
            by_name,
            by_url,
        })
    }

    /// Returns an iterator over the installed distributions.
    pub fn iter(&self) -> impl Iterator<Item = &InstalledDist> {
        self.distributions.iter().flatten()
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

    /// Returns the distributions installed from the given URL, if any.
    pub fn get_urls(&self, url: &Url) -> Vec<&InstalledDist> {
        let Some(indexes) = self.by_url.get(url) else {
            return Vec::new();
        };
        indexes
            .iter()
            .flat_map(|&index| &self.distributions[index])
            .collect()
    }

    /// Returns `true` if there are any installed packages.
    pub fn any(&self) -> bool {
        self.distributions.iter().any(Option::is_some)
    }

    /// Validate the installed packages in the virtual environment.
    pub fn diagnostics(&self) -> Result<Vec<SitePackagesDiagnostic>> {
        let mut diagnostics = Vec::new();

        for (package, indexes) in &self.by_name {
            let mut distributions = indexes.iter().flat_map(|index| &self.distributions[*index]);

            // Find the installed distribution for the given package.
            let Some(distribution) = distributions.next() else {
                continue;
            };

            if let Some(conflict) = distributions.next() {
                // There are multiple installed distributions for the same package.
                diagnostics.push(SitePackagesDiagnostic::DuplicatePackage {
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
                    diagnostics.push(SitePackagesDiagnostic::IncompletePackage {
                        package: package.clone(),
                        path: distribution.path().to_owned(),
                    });
                    continue;
                };

                // Verify that the package is compatible with the current Python version.
                if let Some(requires_python) = metadata.requires_python.as_ref() {
                    if !requires_python.contains(self.interpreter.python_version()) {
                        diagnostics.push(SitePackagesDiagnostic::IncompatiblePythonVersion {
                            package: package.clone(),
                            version: self.interpreter.python_version().clone(),
                            requires_python: requires_python.clone(),
                        });
                    }
                }

                // Verify that the dependencies are installed.
                for dependency in &metadata.requires_dist {
                    if !dependency.evaluate_markers(self.interpreter.markers(), &[]) {
                        continue;
                    }

                    let installed = self.get_packages(&dependency.name);
                    match installed.as_slice() {
                        [] => {
                            // No version installed.
                            diagnostics.push(SitePackagesDiagnostic::MissingDependency {
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
                                        diagnostics.push(
                                            SitePackagesDiagnostic::IncompatibleDependency {
                                                package: package.clone(),
                                                version: installed.version().clone(),
                                                requirement: dependency.clone(),
                                            },
                                        );
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

    /// Returns if the installed packages satisfy the given requirements.
    pub fn satisfies(
        &self,
        requirements: &[UnresolvedRequirementSpecification],
        constraints: &[Requirement],
    ) -> Result<SatisfiesResult> {
        // Collect the constraints.
        let constraints: FxHashMap<&PackageName, Vec<&Requirement>> =
            constraints
                .iter()
                .fold(FxHashMap::default(), |mut constraints, requirement| {
                    constraints
                        .entry(&requirement.name)
                        .or_default()
                        .push(requirement);
                    constraints
                });

        let mut stack = Vec::with_capacity(requirements.len());
        let mut seen = FxHashSet::with_capacity_and_hasher(requirements.len(), FxBuildHasher);

        // Add the direct requirements to the queue.
        for entry in requirements {
            if entry
                .requirement
                .evaluate_markers(Some(self.interpreter.markers()), &[])
            {
                if seen.insert(entry.clone()) {
                    stack.push(entry.clone());
                }
            }
        }

        // Verify that all non-editable requirements are met.
        while let Some(entry) = stack.pop() {
            let installed = match &entry.requirement {
                UnresolvedRequirement::Named(requirement) => self.get_packages(&requirement.name),
                UnresolvedRequirement::Unnamed(requirement) => {
                    self.get_urls(requirement.url.verbatim.raw())
                }
            };
            match installed.as_slice() {
                [] => {
                    // The package isn't installed.
                    return Ok(SatisfiesResult::Unsatisfied(entry.requirement.to_string()));
                }
                [distribution] => {
                    match RequirementSatisfaction::check(
                        distribution,
                        entry.requirement.source().as_ref(),
                    )? {
                        RequirementSatisfaction::Mismatch | RequirementSatisfaction::OutOfDate => {
                            return Ok(SatisfiesResult::Unsatisfied(entry.requirement.to_string()))
                        }
                        RequirementSatisfaction::Satisfied => {}
                    }

                    // Validate that the installed version satisfies the constraints.
                    for constraint in constraints.get(&distribution.name()).into_iter().flatten() {
                        match RequirementSatisfaction::check(distribution, &constraint.source)? {
                            RequirementSatisfaction::Mismatch
                            | RequirementSatisfaction::OutOfDate => {
                                return Ok(SatisfiesResult::Unsatisfied(
                                    entry.requirement.to_string(),
                                ))
                            }
                            RequirementSatisfaction::Satisfied => {}
                        }
                    }

                    // Recurse into the dependencies.
                    let metadata = distribution
                        .metadata()
                        .with_context(|| format!("Failed to read metadata for: {distribution}"))?;

                    // Add the dependencies to the queue.
                    for dependency in metadata.requires_dist {
                        if dependency.evaluate_markers(
                            self.interpreter.markers(),
                            entry.requirement.extras(),
                        ) {
                            let dependency = UnresolvedRequirementSpecification {
                                requirement: UnresolvedRequirement::Named(Requirement::from(
                                    dependency,
                                )),
                                hashes: vec![],
                            };
                            if seen.insert(dependency.clone()) {
                                stack.push(dependency);
                            }
                        }
                    }
                }
                _ => {
                    // There are multiple installed distributions for the same package.
                    return Ok(SatisfiesResult::Unsatisfied(entry.requirement.to_string()));
                }
            }
        }

        Ok(SatisfiesResult::Fresh {
            recursive_requirements: seen,
        })
    }
}

/// We check if all requirements are already satisfied, recursing through the requirements tree.
#[derive(Debug)]
pub enum SatisfiesResult {
    /// All requirements are recursively satisfied.
    Fresh {
        /// The flattened set (transitive closure) of all requirements checked.
        recursive_requirements: FxHashSet<UnresolvedRequirementSpecification>,
    },
    /// We found an unsatisfied requirement. Since we exit early, we only know about the first
    /// unsatisfied requirement.
    Unsatisfied(String),
}

impl IntoIterator for SitePackages {
    type Item = InstalledDist;
    type IntoIter = Flatten<std::vec::IntoIter<Option<InstalledDist>>>;

    fn into_iter(self) -> Self::IntoIter {
        self.distributions.into_iter().flatten()
    }
}

#[derive(Debug)]
pub enum SitePackagesDiagnostic {
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
        requirement: pep508_rs::Requirement<VerbatimParsedUrl>,
    },
    IncompatibleDependency {
        /// The package that has an incompatible dependency.
        package: PackageName,
        /// The version of the package that is installed.
        version: Version,
        /// The dependency that is incompatible.
        requirement: pep508_rs::Requirement<VerbatimParsedUrl>,
    },
    DuplicatePackage {
        /// The package that has multiple installed distributions.
        package: PackageName,
        /// The installed versions of the package.
        paths: Vec<PathBuf>,
    },
}

impl Diagnostic for SitePackagesDiagnostic {
    /// Convert the diagnostic into a user-facing message.
    fn message(&self) -> String {
        match self {
            Self::IncompletePackage { package, path } => format!(
                "The package `{package}` is broken or incomplete (unable to read `METADATA`). Consider recreating the virtualenv, or removing the package directory at: {}.", path.display(),
            ),
            Self::IncompatiblePythonVersion {
                package,
                version,
                requires_python,
            } => format!(
                "The package `{package}` requires Python {requires_python}, but `{version}` is installed"
            ),
            Self::MissingDependency {
                package,
                requirement,
            } => {
                format!("The package `{package}` requires `{requirement}`, but it's not installed")
            }
            Self::IncompatibleDependency {
                package,
                version,
                requirement,
            } => format!(
                "The package `{package}` requires `{requirement}`, but `{version}` is installed"
            ),
            Self::DuplicatePackage { package, paths } => {
                let mut paths = paths.clone();
                paths.sort();
                format!(
                    "The package `{package}` has multiple installed distributions: {}",
                    paths.iter().fold(String::new(), |acc, path| acc + &format!("\n  - {}", path.display()))
                )
            }
        }
    }

    /// Returns `true` if the [`PackageName`] is involved in this diagnostic.
    fn includes(&self, name: &PackageName) -> bool {
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

impl InstalledPackagesProvider for SitePackages {
    fn iter(&self) -> impl Iterator<Item = &InstalledDist> {
        self.iter()
    }

    fn get_packages(&self, name: &PackageName) -> Vec<&InstalledDist> {
        self.get_packages(name)
    }
}
