use std::borrow::Cow;
use std::collections::BTreeSet;
use std::iter::Flatten;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::{Context, Result};
use fs_err as fs;
use rustc_hash::{FxBuildHasher, FxHashMap, FxHashSet};

use uv_configuration::{ExcludeDependency, Excludes, Override, Overrides};
use uv_distribution_filename::EggInfoFilename;
use uv_distribution_types::{
    ConfigSettings, DependencyMetadata, Diagnostic, ExtraBuildRequires, ExtraBuildVariables,
    InstalledDist, InstalledDistKind, Name, NameRequirementSpecification, PackageConfigSettings,
    Requirement, UnresolvedRequirement, UnresolvedRequirementSpecification,
};
use uv_fs::Simplified;
use uv_normalize::{ExtraName, PackageName};
use uv_pep440::{Version, VersionSpecifiers};
use uv_pep508::VersionOrUrl;
use uv_platform_tags::Tags;
use uv_pypi_types::{ResolverMarkerEnvironment, VerbatimParsedUrl};
use uv_python::{Interpreter, PythonEnvironment};
use uv_redacted::DisplaySafeUrl;
use uv_types::{DependencyTraversal, InstalledPackagesProvider};
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
    by_url: FxHashMap<DisplaySafeUrl, Vec<usize>>,
}

/// Packages reachable from installed package metadata.
#[derive(Debug)]
pub struct InstalledReachability {
    packages: BTreeSet<PackageName>,
    incomplete: BTreeSet<PackageName>,
}

impl InstalledReachability {
    /// Return all installed package names reached by the traversal.
    pub fn packages(&self) -> &BTreeSet<PackageName> {
        &self.packages
    }

    /// Return packages whose dependency metadata could not be read completely.
    pub fn incomplete(&self) -> &BTreeSet<PackageName> {
        &self.incomplete
    }
}

impl SitePackages {
    /// Build an index of installed packages from the given Python environment.
    pub fn from_environment(environment: &PythonEnvironment) -> Result<Self> {
        Self::from_interpreter(environment.interpreter())
    }

    /// Build an index of the requested installed packages from the given Python environment.
    pub fn from_environment_for_packages<'a>(
        environment: &PythonEnvironment,
        package_names: impl IntoIterator<Item = &'a PackageName>,
    ) -> Result<Self> {
        let package_names = package_names.into_iter().collect::<FxHashSet<_>>();
        Self::from_interpreter_with_filter(environment.interpreter(), Some(&package_names))
    }

    /// Build an index of installed packages from the given Python executable.
    pub fn from_interpreter(interpreter: &Interpreter) -> Result<Self> {
        Self::from_interpreter_with_filter(interpreter, None)
    }

    /// Build an index of installed packages from the given Python executable.
    fn from_interpreter_with_filter(
        interpreter: &Interpreter,
        package_names: Option<&FxHashSet<&PackageName>>,
    ) -> Result<Self> {
        let mut distributions: Vec<Option<InstalledDist>> = Vec::new();
        let mut by_name: FxHashMap<PackageName, Vec<usize>> = FxHashMap::default();
        let mut by_url: FxHashMap<DisplaySafeUrl, Vec<usize>> = FxHashMap::default();

        for site_packages in interpreter.site_packages() {
            // Read the site-packages directory.
            let site_packages = match fs::read_dir(site_packages.as_ref()) {
                Ok(read_dir) => sorted_dist_like_paths(read_dir).with_context(|| {
                    format!(
                        "Failed to read site-packages directory contents: {}",
                        site_packages.user_display()
                    )
                })?,
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                    continue;
                }
                Err(err) => return Err(err).context("Failed to read site-packages directory"),
            };

            // Index all installed packages by name.
            for path in site_packages {
                if let Some(package_names) = package_names
                    && let Some(package_name) = installed_dist_name(&path)
                    && !package_names.contains(&package_name)
                {
                    continue;
                }

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

                if let Some(package_names) = package_names
                    && !package_names.contains(dist_info.name())
                {
                    continue;
                }

                let idx = distributions.len();

                // Index the distribution by name.
                by_name
                    .entry(dist_info.name().clone())
                    .or_default()
                    .push(idx);

                // Index the distribution by URL.
                if let InstalledDistKind::Url(dist) = &dist_info.kind {
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

    /// Return the installed packages reachable from the given roots.
    ///
    /// Every package is traversed once for its base dependencies and once for each explicitly
    /// requested extra. Root extras are supplied by the caller; extras on transitive dependency
    /// edges are read from installed metadata. Dependencies selected only by the historical
    /// command that installed a root package are therefore intentionally not inferred.
    pub fn reachable_packages<'root>(
        &self,
        roots: impl IntoIterator<Item = (&'root PackageName, &'root [ExtraName])>,
        markers: &ResolverMarkerEnvironment,
    ) -> InstalledReachability {
        let mut packages = BTreeSet::new();
        let mut incomplete = BTreeSet::new();
        let mut traversal = DependencyTraversal::default();

        for (name, extras) in roots {
            traversal.enqueue_package(name.clone(), extras.iter().cloned());
        }

        traversal.walk(|package, extra, traversal| {
            let distributions = self.get_packages(&package);
            if distributions.is_empty() {
                return;
            }
            packages.insert(package.clone());

            for distribution in distributions {
                // Legacy egg metadata can store dependencies in `requires.txt`, which
                // [`InstalledDist::read_metadata`] does not read. Treat it as incomplete rather
                // than incorrectly concluding that the distribution has no dependencies.
                if matches!(
                    distribution.kind,
                    InstalledDistKind::EggInfoFile(_)
                        | InstalledDistKind::EggInfoDirectory(_)
                        | InstalledDistKind::LegacyEditable(_)
                ) {
                    incomplete.insert(package.clone());
                    continue;
                }

                let Ok(metadata) = distribution.read_metadata() else {
                    incomplete.insert(package.clone());
                    continue;
                };

                for dependency in &metadata.requires_dist {
                    let applies_to_base = dependency.evaluate_markers(markers, &[]);
                    let applies = match extra.as_ref() {
                        None => applies_to_base,
                        Some(extra) => {
                            !applies_to_base
                                && dependency.evaluate_markers(markers, std::slice::from_ref(extra))
                        }
                    };
                    if !applies || self.get_packages(&dependency.name).is_empty() {
                        continue;
                    }

                    traversal.enqueue_package(
                        dependency.name.clone(),
                        dependency.extras.iter().cloned(),
                    );
                }
            }
        });

        InstalledReachability {
            packages,
            incomplete,
        }
    }

    /// Remove the given packages from the index, returning all installed versions, if any.
    pub(crate) fn remove_packages(&mut self, name: &PackageName) -> Vec<InstalledDist> {
        let Some(indexes) = self.by_name.get(name) else {
            return Vec::new();
        };
        indexes
            .iter()
            .filter_map(|index| std::mem::take(&mut self.distributions[*index]))
            .collect()
    }

    /// Returns the distributions installed from the given URL, if any.
    pub fn get_urls(&self, url: &DisplaySafeUrl) -> Vec<&InstalledDist> {
        let Some(indexes) = self.by_url.get(url) else {
            return Vec::new();
        };
        indexes
            .iter()
            .flat_map(|&index| &self.distributions[index])
            .collect()
    }

    /// Returns `true` if there are any installed packages.
    pub(crate) fn any(&self) -> bool {
        self.distributions.iter().any(Option::is_some)
    }

    /// Validate the installed packages in the virtual environment.
    pub fn diagnostics(
        &self,
        markers: &ResolverMarkerEnvironment,
        tags: &Tags,
        dependency_metadata: &DependencyMetadata,
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
                let metadata = if let Some(metadata) =
                    dependency_metadata.get(package, Some(distribution.version()))
                {
                    Cow::Owned(metadata)
                } else {
                    let Ok(metadata) = distribution.read_metadata() else {
                        diagnostics.push(SitePackagesDiagnostic::MetadataUnavailable {
                            package: package.clone(),
                            path: distribution.install_path().to_owned(),
                        });
                        continue;
                    };
                    Cow::Borrowed(metadata)
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

                // Verify that the package is compatible with the current tags.
                match distribution.read_tags() {
                    Ok(Some(wheel_tags)) => {
                        if !wheel_tags.is_compatible(tags) {
                            // TODO(charlie): Show the expanded tag hint, that explains _why_ it doesn't match.
                            diagnostics.push(SitePackagesDiagnostic::IncompatiblePlatform {
                                package: package.clone(),
                            });
                        }
                    }
                    Ok(None) => {}
                    Err(_) => {
                        diagnostics.push(SitePackagesDiagnostic::TagsUnavailable {
                            package: package.clone(),
                            path: distribution.install_path().to_owned(),
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
                                None | Some(VersionOrUrl::Url(_)) => {
                                    // Nothing to do (accept any installed version).
                                }
                                Some(VersionOrUrl::VersionSpecifier(version_specifier)) => {
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
        override_dependencies: &[Override<Requirement>],
        exclude_dependencies: &[ExcludeDependency],
        installation: InstallationStrategy,
        markers: &ResolverMarkerEnvironment,
        tags: &Tags,
        config_settings: &ConfigSettings,
        config_settings_package: &PackageConfigSettings,
        extra_build_requires: &ExtraBuildRequires,
        extra_build_variables: &ExtraBuildVariables,
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
                                ));
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
                                ));
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
                                ));
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
                                ));
                            }
                        }
                    }
                }
            }
            named
        };

        let overrides = Overrides::from_entries(
            override_dependencies
                .iter()
                .cloned()
                .chain(
                    overrides
                        .iter()
                        .map(Cow::as_ref)
                        .cloned()
                        .map(Override::Requirement),
                )
                .collect(),
        )?;
        let excludes = Excludes::from_entries(exclude_dependencies.iter().cloned());

        self.satisfies_requirements(
            requirements.iter().map(Cow::as_ref),
            constraints.iter().map(|constraint| &constraint.requirement),
            &overrides,
            &excludes,
            installation,
            markers,
            tags,
            config_settings,
            config_settings_package,
            extra_build_requires,
            extra_build_variables,
        )
    }

    /// Like [`SitePackages::satisfies_spec`], but with resolved names for all requirements.
    pub fn satisfies_requirements<'a>(
        &self,
        requirements: impl ExactSizeIterator<Item = &'a Requirement>,
        constraints: impl Iterator<Item = &'a Requirement>,
        overrides: &'a Overrides,
        excludes: &'a Excludes,
        installation: InstallationStrategy,
        markers: &ResolverMarkerEnvironment,
        tags: &Tags,
        config_settings: &ConfigSettings,
        config_settings_package: &PackageConfigSettings,
        extra_build_requires: &ExtraBuildRequires,
        extra_build_variables: &ExtraBuildVariables,
    ) -> Result<SatisfiesResult> {
        // Collect the constraints by package name.
        let constraints: FxHashMap<&PackageName, Vec<&Requirement>> =
            constraints.fold(FxHashMap::default(), |mut constraints, constraint| {
                constraints
                    .entry(&constraint.name)
                    .or_default()
                    .push(constraint);
                constraints
            });
        let mut stack = Vec::with_capacity(requirements.len());
        let mut seen = FxHashSet::with_capacity_and_hasher(requirements.len(), FxBuildHasher);

        // Add the direct requirements to the queue.
        for requirement in overrides
            .apply(requirements)
            .filter(|requirement| !excludes.contains(&requirement.name))
        {
            if requirement.evaluate_markers(Some(markers), &[]) {
                let requirement = requirement.into_owned();
                if seen.insert(requirement.clone()) {
                    stack.push(requirement);
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
                        match RequirementSatisfaction::check(
                            name,
                            distribution,
                            &requirement.source,
                            None,
                            installation,
                            tags,
                            config_settings,
                            config_settings_package,
                            extra_build_requires,
                            extra_build_variables,
                        ) {
                            RequirementSatisfaction::Mismatch
                            | RequirementSatisfaction::OutOfDate
                            | RequirementSatisfaction::CacheInvalid => {
                                return Ok(SatisfiesResult::Unsatisfied(requirement.to_string()));
                            }
                            RequirementSatisfaction::Satisfied => {}
                        }
                    }

                    // Validate that the installed version satisfies the constraints.
                    for constraint in constraints.get(name).into_iter().flatten() {
                        if constraint.evaluate_markers(Some(markers), &[]) {
                            match RequirementSatisfaction::check(
                                name,
                                distribution,
                                &constraint.source,
                                None,
                                installation,
                                tags,
                                config_settings,
                                config_settings_package,
                                extra_build_requires,
                                extra_build_variables,
                            ) {
                                RequirementSatisfaction::Mismatch
                                | RequirementSatisfaction::OutOfDate
                                | RequirementSatisfaction::CacheInvalid => {
                                    return Ok(SatisfiesResult::Unsatisfied(
                                        requirement.to_string(),
                                    ));
                                }
                                RequirementSatisfaction::Satisfied => {}
                            }
                        }
                    }

                    // Recurse into the dependencies.
                    let metadata = distribution
                        .read_metadata()
                        .with_context(|| format!("Failed to read metadata for: {distribution}"))?;

                    // Add the dependencies to the queue.
                    let dependencies = metadata
                        .requires_dist
                        .iter()
                        .cloned()
                        .map(Requirement::from)
                        .collect::<Vec<_>>();
                    for dependency in overrides
                        .apply_for(name, distribution.version(), &dependencies)
                        .filter(|dependency| {
                            !excludes.contains_for(name, distribution.version(), &dependency.name)
                        })
                    {
                        if dependency.evaluate_markers(Some(markers), &requirement.extras) {
                            let dependency = dependency.into_owned();
                            if seen.insert(dependency.clone()) {
                                stack.push(dependency);
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallationStrategy {
    /// A permissive installation strategy, which accepts existing installations even if the source
    /// type differs, as in the `pip` and `uv pip` CLIs.
    ///
    /// In this strategy, packages that are already installed in the environment may be reused if
    /// they implicitly match the requirements. For example, if the user installs `./path/to/idna`,
    /// then runs `uv pip install anyio` (which depends on `idna`), the existing `idna` installation
    /// will be reused if its version matches the requirement, even though it was installed from a
    /// path and is being implicitly requested from a registry.
    Permissive,

    /// A strict installation strategy, which requires that existing installations match the source
    /// type, as in the `uv sync` CLI.
    ///
    /// This strategy enforces that the installation source must match the requirement source.
    /// It prevents reusing packages that were installed from different sources, ensuring
    /// declarative and reproducible environments.
    Strict,
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

/// Infer the package name from an installed distribution path without reading its metadata.
///
/// Returns `None` when the name cannot safely be derived from the filename alone.
fn installed_dist_name(path: &Path) -> Option<PackageName> {
    let extension = path.extension()?.to_str()?;
    let file_stem = path.file_stem()?.to_str()?;

    match extension {
        "dist-info" => {
            let (name, version) = file_stem.split_once('-')?;
            Version::from_str(version).ok()?;
            PackageName::from_str(name).ok()
        }
        "egg-info" => {
            let filename = EggInfoFilename::parse(file_stem).ok()?;
            filename.version?;
            Some(filename.name)
        }
        // Legacy editables require reading metadata to determine their package name.
        _ => None,
    }
}

impl IntoIterator for SitePackages {
    type Item = InstalledDist;
    type IntoIter = Flatten<std::vec::IntoIter<Option<InstalledDist>>>;

    fn into_iter(self) -> Self::IntoIter {
        self.distributions.into_iter().flatten()
    }
}

fn sorted_dist_like_paths(read_dir: fs::ReadDir) -> Result<Vec<PathBuf>, std::io::Error> {
    let mut paths = read_dir
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
        .collect::<Result<Vec<_>, std::io::Error>>()?;
    paths.sort_unstable();
    Ok(paths)
}

#[derive(Debug)]
pub enum SitePackagesDiagnostic {
    MetadataUnavailable {
        /// The package that is missing metadata.
        package: PackageName,
        /// The path to the package.
        path: PathBuf,
    },
    TagsUnavailable {
        /// The package that is missing tags.
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
    IncompatiblePlatform {
        /// The package that was built for a different platform.
        package: PackageName,
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
                "The package `{package}` is broken or incomplete (unable to read `METADATA`). Consider recreating the virtualenv, or removing the package directory at: {}.",
                path.display(),
            ),
            Self::TagsUnavailable { package, path } => format!(
                "The package `{package}` is broken or incomplete (unable to read `WHEEL` file). Consider recreating the virtualenv, or removing the package directory at: {}.",
                path.display(),
            ),
            Self::IncompatiblePythonVersion {
                package,
                version,
                requires_python,
            } => format!(
                "The package `{package}` requires Python {requires_python}, but `{version}` is installed"
            ),
            Self::IncompatiblePlatform { package } => {
                format!("The package `{package}` was built for a different platform")
            }
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
                    paths.iter().fold(String::new(), |acc, path| acc
                        + &format!("\n  - {}", path.display()))
                )
            }
        }
    }

    /// Returns `true` if the [`PackageName`] is involved in this diagnostic.
    fn includes(&self, name: &PackageName) -> bool {
        match self {
            Self::MetadataUnavailable { package, .. } => name == package,
            Self::TagsUnavailable { package, .. } => name == package,
            Self::IncompatiblePythonVersion { package, .. } => name == package,
            Self::IncompatiblePlatform { package } => name == package,
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

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    use anyhow::Result;
    #[cfg(unix)]
    use uv_cache::Cache;
    #[cfg(unix)]
    use uv_distribution_types::Name;
    #[cfg(unix)]
    use uv_python::Interpreter;

    #[cfg(unix)]
    use super::SitePackages;
    use super::sorted_dist_like_paths;

    #[test]
    fn sorted_dist_like_paths_filters_and_sorts() -> Result<()> {
        let site_packages = tempfile::tempdir()?;
        fs_err::create_dir(site_packages.path().join("z_package-1.0.0.dist-info"))?;
        fs_err::create_dir(site_packages.path().join("a_package"))?;
        fs_err::write(site_packages.path().join("editable.egg-link"), "")?;
        fs_err::write(site_packages.path().join("module.py"), "")?;
        fs_err::write(site_packages.path().join("metadata.egg-info"), "")?;

        let paths = sorted_dist_like_paths(fs_err::read_dir(site_packages.path())?)?;
        let names = paths
            .iter()
            .filter_map(|path| path.file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .collect::<Vec<_>>();

        assert_eq!(
            names,
            vec![
                "a_package".to_string(),
                "editable.egg-link".to_string(),
                "metadata.egg-info".to_string(),
                "z_package-1.0.0.dist-info".to_string(),
            ]
        );

        Ok(())
    }

    /// A missing `purelib` directory must not prevent indexing an existing, distinct `platlib`.
    #[cfg(unix)]
    #[tokio::test]
    async fn site_packages_scans_platlib_when_purelib_is_missing() -> Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let purelib = temp_dir.path().join("purelib");
        let platlib = temp_dir.path().join("platlib");
        let dist_info = platlib.join("demo-1.0.dist-info");
        fs_err::create_dir_all(&dist_info)?;
        fs_err::write(
            dist_info.join("METADATA"),
            "Metadata-Version: 2.1\nName: demo\nVersion: 1.0\n",
        )?;

        let executable = temp_dir.path().join("python");
        let json = r#"{
            "result": "success",
            "platform": {"os": {"name": "manylinux", "major": 2, "minor": 38}, "arch": "x86_64"},
            "manylinux_compatible": true,
            "standalone": false,
            "markers": {
                "implementation_name": "cpython",
                "implementation_version": "3.12.0",
                "os_name": "posix",
                "platform_machine": "x86_64",
                "platform_python_implementation": "CPython",
                "platform_release": "6.5.0",
                "platform_system": "Linux",
                "platform_version": "test",
                "python_full_version": "3.12.0",
                "python_version": "3.12",
                "sys_platform": "linux"
            },
            "sys_base_exec_prefix": "/python",
            "sys_base_prefix": "/python",
            "sys_prefix": "/python",
            "sys_executable": "{EXECUTABLE}",
            "sys_path": [],
            "site_packages": [],
            "stdlib": "/python/lib/python3.12",
            "extension_suffixes": [".cpython-312-x86_64-linux-gnu.so", ".abi3.so", ".so"],
            "scheme": {
                "data": "/python",
                "include": "/python/include",
                "platlib": "{PLATLIB}",
                "purelib": "{PURELIB}",
                "scripts": "/python/bin"
            },
            "virtualenv": {
                "data": "",
                "include": "include",
                "platlib": "lib64/python3.12/site-packages",
                "purelib": "lib/python3.12/site-packages",
                "scripts": "bin"
            },
            "pointer_size": "64",
            "gil_disabled": false,
            "debug_enabled": false
        }"#
        .replace("{EXECUTABLE}", &executable.to_string_lossy())
        .replace("{PLATLIB}", &platlib.to_string_lossy())
        .replace("{PURELIB}", &purelib.to_string_lossy());
        fs_err::write(&executable, format!("#!/bin/sh\necho '{json}'\n"))?;
        fs_err::set_permissions(&executable, PermissionsExt::from_mode(0o770))?;

        let cache = Cache::temp()?.init().await?;
        let interpreter = Interpreter::query(&executable, &cache)?;
        let site_packages = SitePackages::from_interpreter(&interpreter)?;

        assert_eq!(
            site_packages
                .iter()
                .map(|distribution| distribution.name().as_ref())
                .collect::<Vec<_>>(),
            ["demo"]
        );

        Ok(())
    }
}
