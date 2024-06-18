use std::collections::BTreeMap;

use either::Either;
use itertools::Itertools;
use pubgrub::range::Range;
use rustc_hash::FxHashSet;
use tracing::{trace, warn};

use distribution_types::Verbatim;
use pep440_rs::Version;
use pep508_rs::{MarkerEnvironment, MarkerTree};
use pypi_types::{
    ParsedArchiveUrl, ParsedDirectoryUrl, ParsedGitUrl, ParsedPathUrl, ParsedUrl, Requirement,
    RequirementSource,
};
use uv_configuration::{Constraints, Overrides};
use uv_git::GitResolver;
use uv_normalize::{ExtraName, GroupName, PackageName};

use crate::pubgrub::specifier::PubGrubSpecifier;
use crate::pubgrub::{PubGrubPackage, PubGrubPackageInner};
use crate::resolver::{Locals, Urls};
use crate::ResolveError;

#[derive(Debug)]
pub struct PubGrubDependencies(Vec<(PubGrubPackage, Range<Version>)>);

impl PubGrubDependencies {
    /// Generate a set of PubGrub dependencies from a set of requirements.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_requirements(
        requirements: &[Requirement],
        dev_dependencies: &BTreeMap<GroupName, Vec<Requirement>>,
        constraints: &Constraints,
        overrides: &Overrides,
        source_name: Option<&PackageName>,
        source_extra: Option<&ExtraName>,
        source_dev: Option<&GroupName>,
        urls: &Urls,
        locals: &Locals,
        git: &GitResolver,
        env: Option<&MarkerEnvironment>,
        requires_python: Option<&MarkerTree>,
        fork_markers: &MarkerTree,
    ) -> Result<Self, ResolveError> {
        let mut dependencies = Vec::default();
        let mut seen = FxHashSet::default();

        add_requirements(
            requirements,
            dev_dependencies,
            constraints,
            overrides,
            source_name,
            source_extra,
            source_dev,
            urls,
            locals,
            git,
            env,
            requires_python,
            fork_markers,
            &mut dependencies,
            &mut seen,
        )?;

        Ok(Self(dependencies))
    }

    /// Add a [`PubGrubPackage`] and [`PubGrubVersion`] range into the dependencies.
    pub(crate) fn push(&mut self, package: PubGrubPackage, version: Range<Version>) {
        self.0.push((package, version));
    }

    /// Iterate over the dependencies.
    pub(crate) fn iter(&self) -> impl Iterator<Item = &(PubGrubPackage, Range<Version>)> {
        self.0.iter()
    }
}

/// Add a set of requirements to a list of dependencies.
#[allow(clippy::too_many_arguments)]
fn add_requirements(
    requirements: &[Requirement],
    dev_dependencies: &BTreeMap<GroupName, Vec<Requirement>>,
    constraints: &Constraints,
    overrides: &Overrides,
    source_name: Option<&PackageName>,
    source_extra: Option<&ExtraName>,
    source_dev: Option<&GroupName>,
    urls: &Urls,
    locals: &Locals,
    git: &GitResolver,
    env: Option<&MarkerEnvironment>,
    requires_python: Option<&MarkerTree>,
    fork_markers: &MarkerTree,
    dependencies: &mut Vec<(PubGrubPackage, Range<Version>)>,
    seen: &mut FxHashSet<ExtraName>,
) -> Result<(), ResolveError> {
    // Iterate over all declared requirements.
    for requirement in overrides.apply(if let Some(source_dev) = source_dev {
        Either::Left(dev_dependencies.get(source_dev).into_iter().flatten())
    } else {
        Either::Right(requirements.iter())
    }) {
        // If the requirement would not be selected with any Python version
        // supported by the root, skip it.
        if !satisfies_requires_python(requires_python, requirement) {
            trace!(
                "skipping {requirement} because of Requires-Python {requires_python}",
                // OK because this filter only applies when there is a present
                // Requires-Python specifier.
                requires_python = requires_python.unwrap()
            );
            continue;
        }
        // If we're in universal mode, `fork_markers` might correspond to a
        // non-trivial marker expression that provoked the resolver to fork.
        // In that case, we should ignore any dependency that cannot possibly
        // satisfy the markers that provoked the fork.
        if !possible_to_satisfy_markers(fork_markers, requirement) {
            trace!("skipping {requirement} because of context resolver markers {fork_markers}");
            continue;
        }
        // If the requirement isn't relevant for the current platform, skip it.
        match source_extra {
            Some(source_extra) => {
                // Only include requirements that are relevant for the current extra.
                if requirement.evaluate_markers(env, &[]) {
                    continue;
                }
                if !requirement.evaluate_markers(env, std::slice::from_ref(source_extra)) {
                    continue;
                }
            }
            None => {
                if !requirement.evaluate_markers(env, &[]) {
                    continue;
                }
            }
        }

        // Add the package, plus any extra variants.
        for result in std::iter::once(PubGrubRequirement::from_requirement(
            requirement,
            None,
            urls,
            locals,
            git,
        ))
        .chain(requirement.extras.clone().into_iter().map(|extra| {
            PubGrubRequirement::from_requirement(requirement, Some(extra), urls, locals, git)
        })) {
            let PubGrubRequirement { package, version } = result?;

            match &*package {
                PubGrubPackageInner::Package { name, .. } => {
                    // Detect self-dependencies.
                    if source_name.is_some_and(|source_name| source_name == name) {
                        warn!("{name} has a dependency on itself");
                        continue;
                    }

                    dependencies.push((package.clone(), version.clone()));
                }
                PubGrubPackageInner::Marker { .. } => {
                    dependencies.push((package.clone(), version.clone()));
                }
                PubGrubPackageInner::Extra { name, extra, .. } => {
                    // Recursively add the dependencies of the current package (e.g., `black` depending on
                    // `black[colorama]`).
                    if source_name.is_some_and(|source_name| source_name == name) {
                        if seen.insert(extra.clone()) {
                            add_requirements(
                                requirements,
                                dev_dependencies,
                                constraints,
                                overrides,
                                source_name,
                                Some(extra),
                                None,
                                urls,
                                locals,
                                git,
                                env,
                                requires_python,
                                fork_markers,
                                dependencies,
                                seen,
                            )?;
                        }
                    } else {
                        dependencies.push((package.clone(), version.clone()));
                    }
                }
                _ => {}
            }

            // If the requirement was constrained, add those constraints.
            for constraint in constraints.get(&requirement.name).into_iter().flatten() {
                // If the requirement would not be selected with any Python
                // version supported by the root, skip it.
                if !satisfies_requires_python(requires_python, constraint) {
                    trace!(
                        "skipping {requirement} because of Requires-Python {requires_python}",
                        // OK because this filter only applies when there is a present
                        // Requires-Python specifier.
                        requires_python = requires_python.unwrap()
                    );
                    continue;
                }
                // If we're in universal mode, `fork_markers` might correspond
                // to a non-trivial marker expression that provoked the
                // resolver to fork. In that case, we should ignore any
                // dependency that cannot possibly satisfy the markers that
                // provoked the fork.
                if !possible_to_satisfy_markers(fork_markers, constraint) {
                    trace!(
                        "skipping {requirement} because of context resolver markers {fork_markers}"
                    );
                    continue;
                }
                // If the requirement isn't relevant for the current platform, skip it.
                match source_extra {
                    Some(source_extra) => {
                        if !constraint.evaluate_markers(env, std::slice::from_ref(source_extra)) {
                            continue;
                        }
                    }
                    None => {
                        if !constraint.evaluate_markers(env, &[]) {
                            continue;
                        }
                    }
                }

                // Add the package.
                let PubGrubRequirement { package, version } =
                    PubGrubRequirement::from_constraint(constraint, urls, locals, git)?;

                // Ignore self-dependencies.
                if let PubGrubPackageInner::Package { name, .. } = &*package {
                    // Detect self-dependencies.
                    if source_name.is_some_and(|source_name| source_name == name) {
                        warn!("{name} has a dependency on itself");
                        continue;
                    }
                }

                dependencies.push((package.clone(), version.clone()));
            }
        }
    }

    Ok(())
}

/// Convert a [`PubGrubDependencies`] to a [`DependencyConstraints`].
impl From<PubGrubDependencies> for Vec<(PubGrubPackage, Range<Version>)> {
    fn from(dependencies: PubGrubDependencies) -> Self {
        dependencies.0
    }
}

/// A PubGrub-compatible package and version range.
#[derive(Debug, Clone)]
pub(crate) struct PubGrubRequirement {
    pub(crate) package: PubGrubPackage,
    pub(crate) version: Range<Version>,
}

impl PubGrubRequirement {
    /// Convert a [`Requirement`] to a PubGrub-compatible package and range.
    pub(crate) fn from_requirement(
        requirement: &Requirement,
        extra: Option<ExtraName>,
        urls: &Urls,
        locals: &Locals,
        git: &GitResolver,
    ) -> Result<Self, ResolveError> {
        match &requirement.source {
            RequirementSource::Registry { specifier, .. } => {
                // TODO(konsti): We're currently losing the index information here, but we need
                // either pass it to `PubGrubPackage` or the `ResolverProvider` beforehand.
                // If the specifier is an exact version, and the user requested a local version that's
                // more precise than the specifier, use the local version instead.
                let version = if let Some(expected) = locals.get(&requirement.name) {
                    specifier
                        .iter()
                        .map(|specifier| {
                            Locals::map(expected, specifier)
                                .map_err(ResolveError::InvalidVersion)
                                .and_then(|specifier| Ok(PubGrubSpecifier::try_from(&specifier)?))
                        })
                        .fold_ok(Range::full(), |range, specifier| {
                            range.intersection(&specifier.into())
                        })?
                } else {
                    PubGrubSpecifier::try_from(specifier)?.into()
                };

                Ok(Self {
                    package: PubGrubPackage::from_package(
                        requirement.name.clone(),
                        extra,
                        requirement.marker.clone(),
                        urls,
                    ),
                    version,
                })
            }
            RequirementSource::Url {
                subdirectory,
                location,
                url,
            } => {
                let Some(expected) = urls.get(&requirement.name) else {
                    return Err(ResolveError::DisallowedUrl(
                        requirement.name.clone(),
                        url.to_string(),
                    ));
                };

                let parsed_url = ParsedUrl::Archive(ParsedArchiveUrl::from_source(
                    location.clone(),
                    subdirectory.clone(),
                ));
                if !Urls::same_resource(&expected.parsed_url, &parsed_url, git) {
                    return Err(ResolveError::ConflictingUrlsTransitive(
                        requirement.name.clone(),
                        expected.verbatim.verbatim().to_string(),
                        url.verbatim().to_string(),
                    ));
                }

                Ok(Self {
                    package: PubGrubPackage::from_url(
                        requirement.name.clone(),
                        extra,
                        requirement.marker.clone(),
                        expected.clone(),
                    ),
                    version: Range::full(),
                })
            }
            RequirementSource::Git {
                repository,
                reference,
                precise,
                url,
                subdirectory,
            } => {
                let Some(expected) = urls.get(&requirement.name) else {
                    return Err(ResolveError::DisallowedUrl(
                        requirement.name.clone(),
                        url.to_string(),
                    ));
                };

                let parsed_url = ParsedUrl::Git(ParsedGitUrl::from_source(
                    repository.clone(),
                    reference.clone(),
                    *precise,
                    subdirectory.clone(),
                ));
                if !Urls::same_resource(&expected.parsed_url, &parsed_url, git) {
                    return Err(ResolveError::ConflictingUrlsTransitive(
                        requirement.name.clone(),
                        expected.verbatim.verbatim().to_string(),
                        url.verbatim().to_string(),
                    ));
                }

                Ok(Self {
                    package: PubGrubPackage::from_url(
                        requirement.name.clone(),
                        extra,
                        requirement.marker.clone(),
                        expected.clone(),
                    ),
                    version: Range::full(),
                })
            }
            RequirementSource::Path {
                url,
                install_path,
                lock_path,
            } => {
                let Some(expected) = urls.get(&requirement.name) else {
                    return Err(ResolveError::DisallowedUrl(
                        requirement.name.clone(),
                        url.to_string(),
                    ));
                };

                let parsed_url = ParsedUrl::Path(ParsedPathUrl::from_source(
                    install_path.clone(),
                    lock_path.clone(),
                    url.to_url(),
                ));
                if !Urls::same_resource(&expected.parsed_url, &parsed_url, git) {
                    return Err(ResolveError::ConflictingUrlsTransitive(
                        requirement.name.clone(),
                        expected.verbatim.verbatim().to_string(),
                        url.verbatim().to_string(),
                    ));
                }

                Ok(Self {
                    package: PubGrubPackage::from_url(
                        requirement.name.clone(),
                        extra,
                        requirement.marker.clone(),
                        expected.clone(),
                    ),
                    version: Range::full(),
                })
            }
            RequirementSource::Directory {
                editable,
                url,
                install_path,
                lock_path,
            } => {
                let Some(expected) = urls.get(&requirement.name) else {
                    return Err(ResolveError::DisallowedUrl(
                        requirement.name.clone(),
                        url.to_string(),
                    ));
                };

                let parsed_url = ParsedUrl::Directory(ParsedDirectoryUrl::from_source(
                    install_path.clone(),
                    lock_path.clone(),
                    *editable,
                    url.to_url(),
                ));
                if !Urls::same_resource(&expected.parsed_url, &parsed_url, git) {
                    return Err(ResolveError::ConflictingUrlsTransitive(
                        requirement.name.clone(),
                        expected.verbatim.verbatim().to_string(),
                        url.verbatim().to_string(),
                    ));
                }

                Ok(Self {
                    package: PubGrubPackage::from_url(
                        requirement.name.clone(),
                        extra,
                        requirement.marker.clone(),
                        expected.clone(),
                    ),
                    version: Range::full(),
                })
            }
        }
    }

    /// Convert a constraint to a PubGrub-compatible package and range.
    pub(crate) fn from_constraint(
        constraint: &Requirement,
        urls: &Urls,
        locals: &Locals,
        git: &GitResolver,
    ) -> Result<Self, ResolveError> {
        Self::from_requirement(constraint, None, urls, locals, git)
    }
}

/// Returns true if and only if the given requirement's marker expression has a
/// possible true value given the `requires_python` specifier given.
///
/// While this is always called, a `requires_python` is only non-None when in
/// universal resolution mode. In non-universal mode, `requires_python` is
/// `None` and this always returns `true`.
fn satisfies_requires_python(
    requires_python: Option<&MarkerTree>,
    requirement: &Requirement,
) -> bool {
    let Some(requires_python) = requires_python else {
        return true;
    };
    possible_to_satisfy_markers(requires_python, requirement)
}

/// Returns true if and only if the given requirement's marker expression has a
/// possible true value given the `markers` expression given.
fn possible_to_satisfy_markers(markers: &MarkerTree, requirement: &Requirement) -> bool {
    let Some(marker) = requirement.marker.as_ref() else {
        return true;
    };
    !crate::marker::is_disjoint(markers, marker)
}
