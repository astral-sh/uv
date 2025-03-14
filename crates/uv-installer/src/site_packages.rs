use std::borrow::Cow;
use std::collections::BTreeSet;
use std::iter::Flatten;
use std::path::PathBuf;

use anyhow::{Context, Result};
use fs_err as fs;
use rustc_hash::{FxBuildHasher, FxHashMap, FxHashSet};
use url::Url;

use uv_distribution_types::{
    Diagnostic, InstalledDist, Name, NameRequirementSpecification, UnresolvedRequirement,
    UnresolvedRequirementSpecification,
};
use uv_fs::Simplified;
use uv_normalize::PackageName;
use uv_pep440::{Version, VersionSpecifiers};
use uv_pep508::VersionOrUrl;
use uv_pypi_types::{Requirement, ResolverMarkerEnvironment, VerbatimParsedUrl};
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

    /// Returns the [`Interpreter`] used to install the packages.
    pub fn interpreter(&self) -> &Interpreter {
        &self.interpreter
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
    pub fn diagnostics(
        &self,
        markers: &ResolverMarkerEnvironment,
    ) -> Result<Vec<SitePackagesDiagnostic>> {
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
                    paths: std::iter::once(distribution.install_path().to_owned())
                        .chain(std::iter::once(conflict.install_path().to_owned()))
                        .chain(distributions.map(|dist| dist.install_path().to_owned()))
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
                    diagnostics.push(SitePackagesDiagnostic::MetadataUnavailable {
                        package: package.clone(),
                        path: distribution.install_path().to_owned(),
                    });
                    continue;
                };

                // Verify that the package is compatible with the current Python version.
                if let Some(requires_python) = metadata.requires_python.as_ref() {
                    if !requires_python.contains(markers.python_full_version()) {
                        diagnostics.push(SitePackagesDiagnostic::IncompatiblePythonVersion {
                            package: package.clone(),
                            version: self.interpreter.python_version().clone(),
                            requires_python: requires_python.clone(),
                        });
                    }
                }

                // Verify that the dependencies are installed.
                for dependency in &metadata.requires_dist {
                    if !dependency.evaluate_markers(markers, &[]) {
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
                                None | Some(uv_pep508::VersionOrUrl::Url(_)) => {
                                    // Nothing to do (accept any installed version).
                                }
                                Some(uv_pep508::VersionOrUrl::VersionSpecifier(
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
    pub fn satisfies_spec(
        &self,
        requirements: &[UnresolvedRequirementSpecification],
        constraints: &[NameRequirementSpecification],
        overrides: &[UnresolvedRequirementSpecification],
        markers: &ResolverMarkerEnvironment,
    ) -> Result<SatisfiesResult> {
        // First, map all unnamed requirements to named requirements.
        let requirements = {
            let mut named = Vec::with_capacity(requirements.len());
            for requirement in requirements {
                match &requirement.requirement {
                    UnresolvedRequirement::Named(requirement) => {
                        named.push(Cow::Borrowed(requirement));
                    }
                    UnresolvedRequirement::Unnamed(requirement) => {
                        match self.get_urls(requirement.url.verbatim.raw()).as_slice() {
                            [] => {
                                return Ok(SatisfiesResult::Unsatisfied(
                                    requirement.url.verbatim.raw().to_string(),
                                ))
                            }
                            [distribution] => {
                                let requirement = uv_pep508::Requirement {
                                    name: distribution.name().clone(),
                                    version_or_url: Some(VersionOrUrl::Url(
                                        requirement.url.clone(),
                                    )),
                                    marker: requirement.marker,
                                    extras: requirement.extras.clone(),
                                    origin: requirement.origin.clone(),
                                };
                                named.push(Cow::Owned(Requirement::from(requirement)));
                            }
                            _ => {
                                return Ok(SatisfiesResult::Unsatisfied(
                                    requirement.url.verbatim.raw().to_string(),
                                ))
                            }
                        }
                    }
                }
            }
            named
        };

        // Second, map all overrides to named requirements. We assume that all overrides are
        // relevant.
        let overrides = {
            let mut named = Vec::with_capacity(overrides.len());
            for requirement in overrides {
                match &requirement.requirement {
                    UnresolvedRequirement::Named(requirement) => {
                        named.push(Cow::Borrowed(requirement));
                    }
                    UnresolvedRequirement::Unnamed(requirement) => {
                        match self.get_urls(requirement.url.verbatim.raw()).as_slice() {
                            [] => {
                                return Ok(SatisfiesResult::Unsatisfied(
                                    requirement.url.verbatim.raw().to_string(),
                                ))
                            }
                            [distribution] => {
                                let requirement = uv_pep508::Requirement {
                                    name: distribution.name().clone(),
                                    version_or_url: Some(VersionOrUrl::Url(
                                        requirement.url.clone(),
                                    )),
                                    marker: requirement.marker,
                                    extras: requirement.extras.clone(),
                                    origin: requirement.origin.clone(),
                                };
                                named.push(Cow::Owned(Requirement::from(requirement)));
                            }
                            _ => {
                                return Ok(SatisfiesResult::Unsatisfied(
                                    requirement.url.verbatim.raw().to_string(),
                                ))
                            }
                        }
                    }
                }
            }
            named
        };

        self.satisfies_requirements(
            requirements.iter().map(Cow::as_ref),
            constraints.iter().map(|constraint| &constraint.requirement),
            overrides.iter().map(Cow::as_ref),
            markers,
        )
    }

    /// Like [`SitePackages::satisfies_spec`], but with resolved names for all requirements.
    pub fn satisfies_requirements<'a>(
        &self,
        requirements: impl ExactSizeIterator<Item = &'a Requirement>,
        constraints: impl Iterator<Item = &'a Requirement>,
        overrides: impl Iterator<Item = &'a Requirement>,
        markers: &ResolverMarkerEnvironment,
    ) -> Result<SatisfiesResult> {
        // Collect the constraints and overrides by package name.
        let constraints: FxHashMap<&PackageName, Vec<&Requirement>> =
            constraints.fold(FxHashMap::default(), |mut constraints, constraint| {
                constraints
                    .entry(&constraint.name)
                    .or_default()
                    .push(constraint);
                constraints
            });
        let overrides: FxHashMap<&PackageName, Vec<&Requirement>> =
            overrides.fold(FxHashMap::default(), |mut overrides, r#override| {
                overrides
                    .entry(&r#override.name)
                    .or_default()
                    .push(r#override);
                overrides
            });

        let mut stack = Vec::with_capacity(requirements.len());
        let mut seen = FxHashSet::with_capacity_and_hasher(requirements.len(), FxBuildHasher);

        // Add the direct requirements to the queue.
        for requirement in requirements {
            if let Some(r#overrides) = overrides.get(&requirement.name) {
                for dependency in r#overrides {
                    if dependency.evaluate_markers(Some(markers), &[]) {
                        if seen.insert((*dependency).clone()) {
                            stack.push(Cow::Borrowed(*dependency));
                        }
                    }
                }
            } else {
                if requirement.evaluate_markers(Some(markers), &[]) {
                    if seen.insert(requirement.clone()) {
                        stack.push(Cow::Borrowed(requirement));
                    }
                }
            }
        }

        // Verify that all non-editable requirements are met.
        while let Some(requirement) = stack.pop() {
            let name = &requirement.name;
            let installed = self.get_packages(name);
            match installed.as_slice() {
                [] => {
                    // The package isn't installed.
                    return Ok(SatisfiesResult::Unsatisfied(requirement.to_string()));
                }
                [distribution] => {
                    // Validate that the requirement is satisfied.
                    if requirement.evaluate_markers(Some(markers), &[]) {
                        match RequirementSatisfaction::check(distribution, &requirement.source)? {
                            RequirementSatisfaction::Mismatch
                            | RequirementSatisfaction::OutOfDate => {
                                return Ok(SatisfiesResult::Unsatisfied(requirement.to_string()))
                            }
                            RequirementSatisfaction::Satisfied => {}
                        }
                    }

                    // Validate that the installed version satisfies the constraints.
                    for constraint in constraints.get(name).into_iter().flatten() {
                        if constraint.evaluate_markers(Some(markers), &[]) {
                            match RequirementSatisfaction::check(distribution, &constraint.source)?
                            {
                                RequirementSatisfaction::Mismatch
                                | RequirementSatisfaction::OutOfDate => {
                                    return Ok(SatisfiesResult::Unsatisfied(
                                        requirement.to_string(),
                                    ))
                                }
                                RequirementSatisfaction::Satisfied => {}
                            }
                        }
                    }

                    // Recurse into the dependencies.
                    let metadata = distribution
                        .metadata()
                        .with_context(|| format!("Failed to read metadata for: {distribution}"))?;

                    // Add the dependencies to the queue.
                    for dependency in metadata.requires_dist {
                        let dependency = Requirement::from(dependency);
                        if let Some(r#overrides) = overrides.get(&dependency.name) {
                            for dependency in r#overrides {
                                if dependency.evaluate_markers(Some(markers), &requirement.extras) {
                                    if seen.insert((*dependency).clone()) {
                                        stack.push(Cow::Borrowed(*dependency));
                                    }
                                }
                            }
                        } else {
                            if dependency.evaluate_markers(Some(markers), &requirement.extras) {
                                if seen.insert(dependency.clone()) {
                                    stack.push(Cow::Owned(dependency));
                                }
                            }
                        }
                    }
                }
                _ => {
                    // There are multiple installed distributions for the same package.
                    return Ok(SatisfiesResult::Unsatisfied(requirement.to_string()));
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
        recursive_requirements: FxHashSet<Requirement>,
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
    MetadataUnavailable {
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
        requirement: uv_pep508::Requirement<VerbatimParsedUrl>,
    },
    IncompatibleDependency {
        /// The package that has an incompatible dependency.
        package: PackageName,
        /// The version of the package that is installed.
        version: Version,
        /// The dependency that is incompatible.
        requirement: uv_pep508::Requirement<VerbatimParsedUrl>,
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
            Self::MetadataUnavailable { package, path } => format!(
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
            Self::MetadataUnavailable { package, .. } => name == package,
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
