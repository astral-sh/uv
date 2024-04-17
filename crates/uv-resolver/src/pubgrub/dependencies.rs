use itertools::Itertools;
use pubgrub::range::Range;
use std::ops::Deref;
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

        // Iterate over all declared requirements.
        for requirement in overrides.apply(requirements) {
            // If the requirement isn't relevant for the current platform, skip it.
            if let Some(extra) = source_extra {
                if !requirement.evaluate_markers(env, std::slice::from_ref(extra)) {
                    continue;
                }
            } else if !requirement.evaluate_markers(env, &[]) {
                continue;
            }

            // Add the package, plus any extra variants.
            for result in std::iter::once(to_pubgrub(requirement, None, urls, locals)).chain(
                requirement
                    .extras
                    .clone()
                    .into_iter()
                    .map(|extra| to_pubgrub(requirement, Some(extra), urls, locals)),
            ) {
                let (mut package, version) = result?;

                // Detect self-dependencies.
                if let PubGrubPackage::Package(name, extra, ..) = &mut package {
                    if source_name.is_some_and(|source_name| source_name == name) {
                        // Allow, e.g., `black` to depend on `black[colorama]`.
                        if source_extra == extra.as_ref() {
                            warn!("{name} has a dependency on itself");
                            continue;
                        }
                    }
                }

                dependencies.push((package.clone(), version.clone()));

                // If the requirement was constrained, add those constraints.
                for constraint in constraints.get(&requirement.name).into_iter().flatten() {
                    // If the requirement isn't relevant for the current platform, skip it.
                    if let Some(extra) = source_extra {
                        if !constraint.evaluate_markers(env, std::slice::from_ref(extra)) {
                            continue;
                        }
                    } else if !constraint.evaluate_markers(env, &[]) {
                        continue;
                    }

                    // Add the package, plus any extra variants.
                    for result in std::iter::once(to_pubgrub(constraint, None, urls, locals)).chain(
                        constraint
                            .extras
                            .clone()
                            .into_iter()
                            .map(|extra| to_pubgrub(constraint, Some(extra), urls, locals)),
                    ) {
                        let (mut package, version) = result?;

                        // Detect self-dependencies.
                        if let PubGrubPackage::Package(name, extra, ..) = &mut package {
                            if source_name.is_some_and(|source_name| source_name == name) {
                                // Allow, e.g., `black` to depend on `black[colorama]`.
                                if source_extra == extra.as_ref() {
                                    warn!("{name} has a dependency on itself");
                                    continue;
                                }
                            }
                        }

                        dependencies.push((package.clone(), version.clone()));
                    }
                }
            }
        }

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
                url.set_fragment(Some(&format!("subdirectory={subdirectory}")));
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
