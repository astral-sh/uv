use std::borrow::Cow;
use std::collections::BTreeSet;
use std::path::PathBuf;

use anyhow::{Context, Result};
use fs_err as fs;
use rustc_hash::{FxBuildHasher, FxHashMap, FxHashSet};
use same_file::is_same_file;
use tracing::{debug, trace};

use uv_distribution_types::{
    ConfigSettings, DependencyMetadata, Diagnostic, ExtraBuildRequires, ExtraBuildVariables,
    InstalledDist, InstalledDistKind, Name, NameRequirementSpecification, PackageConfigSettings,
    Requirement, UnresolvedRequirement, UnresolvedRequirementSpecification,
};
use uv_fs::Simplified;
use uv_normalize::PackageName;
use uv_pep440::{Version, VersionSpecifiers};
use uv_pep508::VersionOrUrl;
use uv_platform_tags::Tags;
use uv_pypi_types::{ResolverMarkerEnvironment, VerbatimParsedUrl};
use uv_python::{Interpreter, PythonEnvironment};
use uv_redacted::DisplaySafeUrl;
use uv_types::InstalledPackagesProvider;
use uv_warnings::warn_user;

use crate::satisfies::RequirementSatisfaction;

/// An index over the packages installed in an environment.
///
/// Packages are indexed by both name and (for editable installs) URL.
#[derive(Debug, Clone)]
pub struct InstalledPackages {
    interpreter: Interpreter,
    /// All discovered distributions, including shadowed and read-only distributions.
    distributions: Vec<Option<IndexedDistribution>>,
    /// The effective installed distributions, keyed by name.
    ///
    /// Only distributions from the first discovery root containing a package are indexed. Multiple
    /// distributions with the same name in that root are retained as a corrupt installation.
    by_name: FxHashMap<PackageName, Vec<usize>>,
    /// The effective installed editable distributions, keyed by URL.
    by_url: FxHashMap<DisplaySafeUrl, Vec<usize>>,
    /// All mutable distributions, keyed by name.
    mutable_by_name: FxHashMap<PackageName, Vec<usize>>,
    /// All mutable editable distributions, keyed by URL.
    mutable_by_url: FxHashMap<DisplaySafeUrl, Vec<usize>>,
    /// Effective distributions, in discovery order.
    effective: Vec<usize>,
}

#[derive(Debug, Clone)]
pub(crate) struct IndexedDistribution {
    distribution: InstalledDist,
    mutable: bool,
    /// Whether a mutable installation root precedes this distribution's discovery root.
    shadowable: bool,
}

impl IndexedDistribution {
    pub(crate) fn distribution(&self) -> &InstalledDist {
        &self.distribution
    }

    pub(crate) fn is_mutable(&self) -> bool {
        self.mutable
    }

    pub(crate) fn is_shadowable(&self) -> bool {
        self.shadowable
    }
}

impl InstalledPackages {
    /// Build an index of installed packages from the given Python environment.
    pub fn from_environment(environment: &PythonEnvironment) -> Result<Self> {
        Self::from_interpreter(environment.interpreter())
    }

    /// Build an index of installed packages from the given Python executable.
    pub fn from_interpreter(interpreter: &Interpreter) -> Result<Self> {
        let mut distributions: Vec<Option<IndexedDistribution>> = Vec::new();
        let mut by_name: FxHashMap<PackageName, Vec<usize>> = FxHashMap::default();
        let mut by_url: FxHashMap<DisplaySafeUrl, Vec<usize>> = FxHashMap::default();
        let mut mutable_by_name: FxHashMap<PackageName, Vec<usize>> = FxHashMap::default();
        let mut mutable_by_url: FxHashMap<DisplaySafeUrl, Vec<usize>> = FxHashMap::default();
        let mut all_by_name: FxHashMap<PackageName, Vec<usize>> = FxHashMap::default();
        let mut effective = Vec::new();

        let mutable_paths = interpreter.site_packages().collect::<Vec<_>>();
        let mut mutable_path_seen = false;

        for import_path in interpreter.discovery_paths() {
            let mutable = mutable_paths.iter().any(|mutable_path| {
                mutable_path.as_ref() == import_path.as_ref()
                    || is_same_file(mutable_path.as_ref(), import_path.as_ref()).unwrap_or(false)
            });
            let shadowable = mutable || mutable_path_seen;
            let names_from_earlier_paths = by_name.keys().cloned().collect::<FxHashSet<_>>();
            if mutable {
                // The root may not exist yet, but an installation can create it and shadow later
                // read-only roots.
                mutable_path_seen = true;
            }

            // Read the site-packages directory.
            let ordered_directory_paths = match fs::read_dir(import_path.as_ref()) {
                Ok(import_path_entry) => {
                    trace!("Discovering packages in: `{}`", import_path.user_display());
                    // Collect sorted directory paths; `read_dir` is not stable across platforms
                    let dist_likes: BTreeSet<_> = import_path_entry
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
                        .collect::<Result<_, std::io::Error>>()
                        .with_context(|| {
                            format!(
                                "Failed to read site-packages directory contents: {}",
                                import_path.user_display()
                            )
                        })?;
                    dist_likes
                }
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                    debug!(
                        "Package directory does not exist: `{}`",
                        import_path.user_display()
                    );
                    // The site-packages directory doesn't exist, skip it.
                    continue;
                }
                Err(err) => return Err(err).context("Failed to read site-packages directory"),
            };

            // Index all installed packages by name.
            for path in ordered_directory_paths {
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

                if Self::is_duplicate_distribution(&distributions, &all_by_name, &dist_info) {
                    continue;
                }

                let index = distributions.len();
                let name = dist_info.name().clone();
                let url = match &dist_info.kind {
                    InstalledDistKind::Url(dist) => Some(dist.url.clone()),
                    _ => None,
                };

                all_by_name.entry(name.clone()).or_default().push(index);

                if mutable {
                    mutable_by_name.entry(name.clone()).or_default().push(index);
                    if let Some(url) = &url {
                        mutable_by_url.entry(url.clone()).or_default().push(index);
                    }
                }

                // Python and importlib.metadata use the first discovery root containing a package.
                // Retain all same-root duplicates, but do not promote later shadowed copies.
                if !names_from_earlier_paths.contains(&name) {
                    by_name.entry(name).or_default().push(index);
                    if let Some(url) = url {
                        by_url.entry(url).or_default().push(index);
                    }
                    effective.push(index);
                }

                distributions.push(Some(IndexedDistribution {
                    distribution: dist_info,
                    mutable,
                    shadowable,
                }));
            }
        }

        Ok(Self {
            interpreter: interpreter.clone(),
            distributions,
            by_name,
            by_url,
            mutable_by_name,
            mutable_by_url,
            effective,
        })
    }

    /// Whether the distribution is an exact duplicate of one already tracked.
    fn is_duplicate_distribution(
        distributions: &[Option<IndexedDistribution>],
        all_by_name: &FxHashMap<PackageName, Vec<usize>>,
        dist_info: &InstalledDist,
    ) -> bool {
        let Some(existing_ids) = all_by_name.get(dist_info.name()) else {
            return false;
        };

        existing_ids.iter().any(|existing_id| {
            let Some(existing) = distributions[*existing_id].as_ref() else {
                return false;
            };
            existing.distribution == *dist_info
                || is_same_file(
                    existing.distribution.install_path(),
                    dist_info.install_path(),
                )
                .unwrap_or(false)
        })
    }

    /// Returns the [`Interpreter`] used to install the packages.
    pub fn interpreter(&self) -> &Interpreter {
        &self.interpreter
    }

    /// Returns an iterator over the effective installed distributions.
    pub fn iter(&self) -> impl Iterator<Item = &InstalledDist> {
        self.effective.iter().filter_map(|index| {
            self.distributions[*index]
                .as_ref()
                .map(IndexedDistribution::distribution)
        })
    }

    /// Returns the effective installed distributions for a given package.
    pub fn get_packages(&self, name: &PackageName) -> Vec<&InstalledDist> {
        self.get_indexed_packages(name)
            .into_iter()
            .map(IndexedDistribution::distribution)
            .collect()
    }

    /// Returns the effective indexed distributions for a given package.
    pub(crate) fn get_indexed_packages(&self, name: &PackageName) -> Vec<&IndexedDistribution> {
        let Some(indexes) = self.by_name.get(name) else {
            return Vec::new();
        };
        indexes
            .iter()
            .filter_map(|index| self.distributions[*index].as_ref())
            .collect()
    }

    /// Returns all mutable distributions for a given package.
    pub fn get_mutable_packages(&self, name: &PackageName) -> Vec<&InstalledDist> {
        let Some(indexes) = self.mutable_by_name.get(name) else {
            return Vec::new();
        };
        indexes
            .iter()
            .filter_map(|index| self.distributions[*index].as_ref())
            .map(IndexedDistribution::distribution)
            .collect()
    }

    /// Remove all mutable distributions for a package from the index.
    pub(crate) fn remove_mutable_packages(&mut self, name: &PackageName) -> Vec<InstalledDist> {
        let Some(indexes) = self.mutable_by_name.get(name) else {
            return Vec::new();
        };
        indexes
            .iter()
            .filter_map(|index| std::mem::take(&mut self.distributions[*index]))
            .map(|indexed| indexed.distribution)
            .collect()
    }

    /// Remove effective mutable distributions for a package from the index.
    ///
    /// Shadowed mutable distributions remain in the index so exact reconciliation can remove
    /// them as extraneous, while sufficient reconciliation leaves them untouched.
    pub(crate) fn remove_effective_mutable_packages(&mut self, name: &PackageName) {
        let Some(indexes) = self.by_name.get(name) else {
            return;
        };
        for index in indexes {
            if self.distributions[*index]
                .as_ref()
                .is_some_and(IndexedDistribution::is_mutable)
            {
                self.distributions[*index] = None;
            }
        }
    }

    /// Returns the effective distributions installed from the given URL, if any.
    pub fn get_urls(&self, url: &DisplaySafeUrl) -> Vec<&InstalledDist> {
        let Some(indexes) = self.by_url.get(url) else {
            return Vec::new();
        };
        indexes
            .iter()
            .filter_map(|index| self.distributions[*index].as_ref())
            .map(IndexedDistribution::distribution)
            .collect()
    }

    /// Returns the mutable distributions installed from the given URL, if any.
    pub fn get_mutable_urls(&self, url: &DisplaySafeUrl) -> Vec<&InstalledDist> {
        let Some(indexes) = self.mutable_by_url.get(url) else {
            return Vec::new();
        };
        indexes
            .iter()
            .filter_map(|index| self.distributions[*index].as_ref())
            .map(IndexedDistribution::distribution)
            .collect()
    }

    /// Returns `true` if there are any mutable installed packages.
    pub(crate) fn any(&self) -> bool {
        self.distributions
            .iter()
            .flatten()
            .any(IndexedDistribution::is_mutable)
    }

    /// Validate the installed packages in the virtual environment.
    pub fn diagnostics(
        &self,
        markers: &ResolverMarkerEnvironment,
        tags: &Tags,
        dependency_metadata: &DependencyMetadata,
    ) -> Result<Vec<InstalledPackagesDiagnostic>> {
        let mut diagnostics = Vec::new();

        for (package, indexes) in &self.by_name {
            let mut distributions = indexes
                .iter()
                .filter_map(|index| self.distributions[*index].as_ref())
                .map(IndexedDistribution::distribution);

            // Find the installed distribution for the given package.
            let Some(distribution) = distributions.next() else {
                continue;
            };

            if let Some(conflict) = distributions.next() {
                // There are multiple installed distributions for the same package.
                diagnostics.push(InstalledPackagesDiagnostic::DuplicatePackage {
                    package: package.clone(),
                    paths: std::iter::once(distribution.install_path().to_owned())
                        .chain(std::iter::once(conflict.install_path().to_owned()))
                        .chain(distributions.map(|dist| dist.install_path().to_owned()))
                        .collect(),
                });
                continue;
            }

            for index in indexes {
                let Some(distribution) = self.distributions[*index]
                    .as_ref()
                    .map(IndexedDistribution::distribution)
                else {
                    continue;
                };

                // Determine the dependencies for the given package.
                let metadata = if let Some(metadata) =
                    dependency_metadata.get(package, Some(distribution.version()))
                {
                    Cow::Owned(metadata)
                } else {
                    let Ok(metadata) = distribution.read_metadata() else {
                        diagnostics.push(InstalledPackagesDiagnostic::MetadataUnavailable {
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
                        diagnostics.push(InstalledPackagesDiagnostic::IncompatiblePythonVersion {
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
                            diagnostics.push(InstalledPackagesDiagnostic::IncompatiblePlatform {
                                package: package.clone(),
                            });
                        }
                    }
                    Ok(None) => {}
                    Err(_) => {
                        diagnostics.push(InstalledPackagesDiagnostic::TagsUnavailable {
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
                            diagnostics.push(InstalledPackagesDiagnostic::MissingDependency {
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
                                            InstalledPackagesDiagnostic::IncompatibleDependency {
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

        self.satisfies_requirements(
            requirements.iter().map(Cow::as_ref),
            constraints.iter().map(|constraint| &constraint.requirement),
            overrides.iter().map(Cow::as_ref),
            installation,
            markers,
            tags,
            config_settings,
            config_settings_package,
            extra_build_requires,
            extra_build_variables,
        )
    }

    /// Like [`InstalledPackages::satisfies_spec`], but with resolved names for all requirements.
    pub fn satisfies_requirements<'a>(
        &self,
        requirements: impl ExactSizeIterator<Item = &'a Requirement>,
        constraints: impl Iterator<Item = &'a Requirement>,
        overrides: impl Iterator<Item = &'a Requirement>,
        installation: InstallationStrategy,
        markers: &ResolverMarkerEnvironment,
        tags: &Tags,
        config_settings: &ConfigSettings,
        config_settings_package: &PackageConfigSettings,
        extra_build_requires: &ExtraBuildRequires,
        extra_build_variables: &ExtraBuildVariables,
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
                    for dependency in &metadata.requires_dist {
                        let dependency = Requirement::from(dependency.clone());
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

impl IntoIterator for InstalledPackages {
    type Item = InstalledDist;
    type IntoIter = std::vec::IntoIter<InstalledDist>;

    fn into_iter(self) -> Self::IntoIter {
        self.distributions
            .into_iter()
            .flatten()
            .filter(|indexed| indexed.mutable)
            .map(|indexed| indexed.distribution)
            .collect::<Vec<_>>()
            .into_iter()
    }
}

#[derive(Debug)]
pub enum InstalledPackagesDiagnostic {
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

impl Diagnostic for InstalledPackagesDiagnostic {
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

impl InstalledPackagesProvider for InstalledPackages {
    fn iter(&self) -> impl Iterator<Item = &InstalledDist> {
        self.iter()
    }

    fn get_packages(&self, name: &PackageName) -> Vec<&InstalledDist> {
        self.get_packages(name)
    }
}
