use itertools::Itertools;
use pubgrub::range::Range;
use tracing::warn;

use distribution_types::Verbatim;
use pep440_rs::Version;
use pep508_rs::{MarkerEnvironment, MarkerTree, Requirement, VersionOrUrl};
use uv_normalize::{ExtraName, PackageName};
use uv_types::{Constraints, Overrides};

use crate::pubgrub::specifier::PubGrubSpecifier;
use crate::pubgrub::PubGrubPackage;
use crate::resolver::{Locals, Urls};
use crate::ResolveError;

#[derive(Debug)]
pub struct PubGrubDependencies {
    dependencies: Vec<(PubGrubPackage, Range<Version>, Option<MarkerTree>)>,
}

impl PubGrubDependencies {
    /// Generate a set of `PubGrub` dependencies from a set of requirements.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_requirements(
        requirements: &[Requirement],
        constraints: &Constraints,
        overrides: &Overrides,
        source_name: Option<&PackageName>,
        source_extra: Option<&ExtraName>,
        source_marker: Option<MarkerTree>,
        urls: &Urls,
        locals: &Locals,
        env: &MarkerEnvironment,
    ) -> Result<Self, ResolveError> {
        let mut dependencies = Vec::default();

        // Iterate over all declared requirements.
        for requirement in overrides.apply(requirements) {
            // If the requirement isn't relevant for the current platform, skip it.
            if false
                && !requirement
                    .evaluate_markers(env, source_extra.map_or(&[], std::slice::from_ref))
            {
                continue;
            }

            // Add the package, plus any extra variants.
            let package = to_pubgrub(requirement, None, None, urls, locals);
            let virtual_extras = requirement
                .extras
                .clone()
                .into_iter()
                .map(|extra| to_pubgrub(requirement, Some(extra), None, urls, locals));
            let virtual_marker = requirement.marker.as_ref().map(|marker| {
                // Uncomment the below to get marker stacking (albeit it is incorrect
                // at time of writing).
                // let mut m = source_marker
                // .clone()
                // .unwrap_or_else(|| MarkerTree::And(vec![]));
                // m.and(marker.clone());
                let m = marker.clone();
                to_pubgrub(requirement, None, Some(m), urls, locals)
            });
            let it = std::iter::once(package)
                .chain(virtual_extras)
                .chain(virtual_marker);
            for result in it {
                let (mut package, version, _) = result?;

                // Detect self-dependencies.
                if let PubGrubPackage::Package { name, extra, .. } = &mut package {
                    if source_name.is_some_and(|source_name| source_name == name) {
                        // Allow, e.g., `black` to depend on `black[colorama]`.
                        if source_extra == extra.as_ref() {
                            warn!("{name} has a dependency on itself");
                            continue;
                        }
                    }
                }

                dependencies.push((package.clone(), version.clone(), None));

                // If the requirement was constrained, add those constraints.
                for constraint in constraints.get(&requirement.name).into_iter().flatten() {
                    // If the requirement isn't relevant for the current platform, skip it.
                    if false
                        && !requirement
                            .evaluate_markers(env, source_extra.map_or(&[], std::slice::from_ref))
                    {
                        continue;
                    }

                    // Add the package, plus any extra variants.
                    for result in std::iter::once(to_pubgrub(constraint, None, None, urls, locals))
                        .chain(
                            constraint.extras.clone().into_iter().map(|extra| {
                                to_pubgrub(constraint, Some(extra), None, urls, locals)
                            }),
                        )
                    {
                        let (mut package, version, marker) = result?;

                        // Detect self-dependencies.
                        if let PubGrubPackage::Package { name, extra, .. } = &mut package {
                            if source_name.is_some_and(|source_name| source_name == name) {
                                // Allow, e.g., `black` to depend on `black[colorama]`.
                                if source_extra == extra.as_ref() {
                                    warn!("{name} has a dependency on itself");
                                    continue;
                                }
                            }
                        }

                        dependencies.push((package.clone(), version.clone(), marker));
                    }
                }
            }
        }

        Ok(Self { dependencies })
    }

    /// Add a [`PubGrubPackage`] and [`PubGrubVersion`] range into the dependencies.
    pub(crate) fn push(
        &mut self,
        package: PubGrubPackage,
        version: Range<Version>,
        marker: Option<MarkerTree>,
    ) {
        self.dependencies.push((package, version, marker));
    }

    /// Iterate over the dependencies.
    pub(crate) fn iter(
        &self,
    ) -> impl Iterator<Item = &(PubGrubPackage, Range<Version>, Option<MarkerTree>)> {
        self.dependencies.iter()
    }
}

/// Convert a [`PubGrubDependencies`] to a [`DependencyConstraints`].
impl From<PubGrubDependencies> for Vec<(PubGrubPackage, Range<Version>, Option<MarkerTree>)> {
    fn from(dependencies: PubGrubDependencies) -> Self {
        dependencies.dependencies
    }
}

/// Convert a [`Requirement`] to a `PubGrub`-compatible package and range.
fn to_pubgrub(
    requirement: &Requirement,
    extra: Option<ExtraName>,
    marker: Option<MarkerTree>,
    urls: &Urls,
    locals: &Locals,
) -> Result<(PubGrubPackage, Range<Version>, Option<MarkerTree>), ResolveError> {
    // let marker = requirement.marker.clone();
    match requirement.version_or_url.as_ref() {
        // The requirement has no specifier (e.g., `flask`).
        None => Ok((
            PubGrubPackage::from_package(requirement.name.clone(), extra, marker, urls),
            Range::full(),
            None,
        )),

        // The requirement has a specifier (e.g., `flask>=1.0`).
        Some(VersionOrUrl::VersionSpecifier(specifiers)) => {
            // If the specifier is an exact version, and the user requested a local version that's
            // more precise than the specifier, use the local version instead.
            let version = if let Some(expected) = locals.get(&requirement.name) {
                specifiers
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
                specifiers
                    .iter()
                    .map(PubGrubSpecifier::try_from)
                    .fold_ok(Range::full(), |range, specifier| {
                        range.intersection(&specifier.into())
                    })?
            };

            Ok((
                PubGrubPackage::from_package(requirement.name.clone(), extra, marker, urls),
                version,
                None,
            ))
        }

        // The requirement has a URL (e.g., `flask @ file:///path/to/flask`).
        Some(VersionOrUrl::Url(url)) => {
            let Some(expected) = urls.get(&requirement.name) else {
                return Err(ResolveError::DisallowedUrl(
                    requirement.name.clone(),
                    url.verbatim().to_string(),
                ));
            };

            if !urls.is_allowed(expected, url) {
                return Err(ResolveError::ConflictingUrlsTransitive(
                    requirement.name.clone(),
                    expected.verbatim().to_string(),
                    url.verbatim().to_string(),
                ));
            }

            Ok((
                PubGrubPackage::Package {
                    name: requirement.name.clone(),
                    extra,
                    marker: marker.clone(),
                    url: Some(expected.clone()),
                },
                Range::full(),
                marker,
            ))
        }
    }
}
