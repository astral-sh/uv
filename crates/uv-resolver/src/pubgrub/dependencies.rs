use std::ops::Deref;

use itertools::Itertools;
use pubgrub::range::Range;
use rustc_hash::FxHashSet;
use tracing::warn;
use url::Url;

use distribution_types::{UvRequirement, UvSource, Verbatim};
use pep440_rs::Version;
use pep508_rs::{MarkerEnvironment, VerbatimUrl};
use uv_configuration::{Constraints, Overrides};
use uv_normalize::{ExtraName, PackageName};

use crate::pubgrub::specifier::PubGrubSpecifier;
use crate::pubgrub::PubGrubPackage;
use crate::resolver::{Locals, Urls};
use crate::ResolveError;

#[derive(Debug)]
pub struct PubGrubDependencies(Vec<(PubGrubPackage, Range<Version>)>);

impl PubGrubDependencies {
    /// Generate a set of `PubGrub` dependencies from a set of requirements.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_requirements(
        requirements: &[UvRequirement],
        constraints: &Constraints,
        overrides: &Overrides,
        source_name: Option<&PackageName>,
        source_extra: Option<&ExtraName>,
        urls: &Urls,
        locals: &Locals,
        env: &MarkerEnvironment,
    ) -> Result<Self, ResolveError> {
        let mut dependencies = Vec::default();
        let mut seen = FxHashSet::default();

        add_requirements(
            requirements,
            constraints,
            overrides,
            source_name,
            source_extra,
            urls,
            locals,
            env,
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
    requirements: &[UvRequirement],
    constraints: &Constraints,
    overrides: &Overrides,
    source_name: Option<&PackageName>,
    source_extra: Option<&ExtraName>,
    urls: &Urls,
    locals: &Locals,
    env: &MarkerEnvironment,
    dependencies: &mut Vec<(PubGrubPackage, Range<Version>)>,
    seen: &mut FxHashSet<ExtraName>,
) -> Result<(), ResolveError> {
    // Iterate over all declared requirements.
    for requirement in overrides.apply(requirements) {
        // If the requirement isn't relevant for the current platform, skip it.
        match source_extra {
            Some(source_extra) => {
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
        for result in std::iter::once(to_pubgrub(requirement, None, urls, locals)).chain(
            requirement
                .extras
                .clone()
                .into_iter()
                .map(|extra| to_pubgrub(requirement, Some(extra), urls, locals)),
        ) {
            let (package, version) = result?;

            match &package {
                PubGrubPackage::Package(name, ..) => {
                    // Detect self-dependencies.
                    if source_name.is_some_and(|source_name| source_name == name) {
                        warn!("{name} has a dependency on itself");
                        continue;
                    }

                    dependencies.push((package.clone(), version.clone()));
                }
                PubGrubPackage::Extra(name, extra, ..) => {
                    // Recursively add the dependencies of the current package (e.g., `black` depending on
                    // `black[colorama]`).
                    if source_name.is_some_and(|source_name| source_name == name) {
                        if seen.insert(extra.clone()) {
                            add_requirements(
                                requirements,
                                constraints,
                                overrides,
                                source_name,
                                Some(extra),
                                urls,
                                locals,
                                env,
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
                let (package, version) = to_pubgrub(constraint, None, urls, locals)?;

                // Ignore self-dependencies.
                if let PubGrubPackage::Package(name, ..) = &package {
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

/// Convert a [`Requirement`] to a `PubGrub`-compatible package and range.
fn to_pubgrub(
    requirement: &UvRequirement,
    extra: Option<ExtraName>,
    urls: &Urls,
    locals: &Locals,
) -> Result<(PubGrubPackage, Range<Version>), ResolveError> {
    match &requirement.source {
        UvSource::Registry { version, .. } => {
            // TODO(konsti): Index
            // If the specifier is an exact version, and the user requested a local version that's
            // more precise than the specifier, use the local version instead.
            let version = if let Some(expected) = locals.get(&requirement.name) {
                version
                    .iter()
                    .map(|specifier| {
                        Locals::map(expected, specifier)
                            .map_err(ResolveError::InvalidVersion)
                            .and_then(|specifier| PubGrubSpecifier::try_from(&specifier))
                    })
                    .fold_ok(Range::full(), |range, specifier| {
                        range.intersection(&specifier.into())
                    })?
            } else {
                version
                    .iter()
                    .map(PubGrubSpecifier::try_from)
                    .fold_ok(Range::full(), |range, specifier| {
                        range.intersection(&specifier.into())
                    })?
            };

            Ok((
                PubGrubPackage::from_package(requirement.name.clone(), extra, urls),
                version,
            ))
        }
        UvSource::Url { url, subdirectory } => {
            let mut url: Url = url.deref().clone();
            if let Some(subdirectory) = subdirectory {
                url.set_fragment(Some(&format!("subdirectory={}", subdirectory.display())));
            }
            let Some(expected) = urls.get(&requirement.name) else {
                return Err(ResolveError::DisallowedUrl(
                    requirement.name.clone(),
                    url.to_string(),
                ));
            };

            if !Urls::is_allowed(expected, &VerbatimUrl::from_url(url.clone())) {
                return Err(ResolveError::ConflictingUrlsTransitive(
                    requirement.name.clone(),
                    expected.verbatim().to_string(),
                    url.to_string(),
                ));
            }

            Ok((
                PubGrubPackage::Package(requirement.name.clone(), extra, Some(expected.clone())),
                Range::full(),
            ))
        }
        UvSource::Git { url, .. } => {
            let Some(expected) = urls.get(&requirement.name) else {
                return Err(ResolveError::DisallowedUrl(
                    requirement.name.clone(),
                    url.to_string(),
                ));
            };

            if !Urls::is_allowed(expected, url) {
                return Err(ResolveError::ConflictingUrlsTransitive(
                    requirement.name.clone(),
                    expected.verbatim().to_string(),
                    url.to_string(),
                ));
            }

            Ok((
                PubGrubPackage::Package(requirement.name.clone(), extra, Some(expected.clone())),
                Range::full(),
            ))
        }
        UvSource::Path { url, .. } => {
            let Some(expected) = urls.get(&requirement.name) else {
                return Err(ResolveError::DisallowedUrl(
                    requirement.name.clone(),
                    url.to_string(),
                ));
            };

            if !Urls::is_allowed(expected, url) {
                return Err(ResolveError::ConflictingUrlsTransitive(
                    requirement.name.clone(),
                    expected.verbatim().to_string(),
                    url.to_string(),
                ));
            }

            Ok((
                PubGrubPackage::Package(requirement.name.clone(), extra, Some(expected.clone())),
                Range::full(),
            ))
        }
    }
}
