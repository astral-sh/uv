use itertools::Itertools;
use pubgrub::range::Range;
use tracing::warn;

use pep508_rs::{MarkerEnvironment, Requirement, VersionOrUrl};

use puffin_normalize::{ExtraName, PackageName};

use crate::pubgrub::specifier::PubGrubSpecifier;
use crate::pubgrub::{PubGrubPackage, PubGrubVersion};
use crate::ResolveError;

#[derive(Debug)]
pub struct PubGrubDependencies(Vec<(PubGrubPackage, Range<PubGrubVersion>)>);

impl PubGrubDependencies {
    /// Generate a set of `PubGrub` dependencies from a set of requirements.
    pub(crate) fn try_from_requirements<'a>(
        requirements: &[Requirement],
        constraints: &[Requirement],
        extra: Option<&'a ExtraName>,
        source: Option<&'a PackageName>,
        env: &'a MarkerEnvironment,
    ) -> Result<Self, ResolveError> {
        let mut dependencies = Vec::new();

        // Iterate over all declared requirements.
        for requirement in requirements {
            // Avoid self-dependencies.
            if source.is_some_and(|source| source == &requirement.name) {
                // TODO(konstin): Warn only once here
                warn!("{} depends on itself", requirement.name);
                continue;
            }

            // If the requirement isn't relevant for the current platform, skip it.
            if let Some(extra) = extra {
                if !requirement.evaluate_markers(env, &[extra.as_ref()]) {
                    continue;
                }
            } else {
                if !requirement.evaluate_markers(env, &[]) {
                    continue;
                }
            }

            // Add the package, plus any extra variants.
            for result in std::iter::once(to_pubgrub(requirement, None)).chain(
                requirement
                    .extras
                    .clone()
                    .into_iter()
                    .flatten()
                    .map(|extra| to_pubgrub(requirement, Some(extra))),
            ) {
                let (package, version) = result?;

                dependencies.push((package.clone(), version.clone()));
            }
        }

        // If any requirements were further constrained by the user, add those constraints.
        for constraint in constraints {
            // Avoid self-dependencies.
            if source.is_some_and(|source| source == &constraint.name) {
                // TODO(konstin): Warn only once here
                warn!("{} depends on itself", constraint.name);
                continue;
            }

            // If the requirement isn't relevant for the current platform, skip it.
            if let Some(extra) = extra {
                if !constraint.evaluate_markers(env, &[extra.as_ref()]) {
                    continue;
                }
            } else {
                if !constraint.evaluate_markers(env, &[]) {
                    continue;
                }
            }

            // Add the package, plus any extra variants.
            for result in std::iter::once(to_pubgrub(constraint, None)).chain(
                constraint
                    .extras
                    .clone()
                    .into_iter()
                    .flatten()
                    .map(|extra| to_pubgrub(constraint, Some(extra))),
            ) {
                let (package, version) = result?;

                dependencies.push((package.clone(), version.clone()));
            }
        }

        Ok(Self(dependencies))
    }

    // Insert a [`PubGrubPackage`] and [`PubGrubVersion`] range into the set of dependencies.
    pub(crate) fn insert(&mut self, package: PubGrubPackage, version: Range<PubGrubVersion>) {
        self.0.push((package, version))
    }

    /// Iterate over the dependencies.
    pub(crate) fn iter(&self) -> impl Iterator<Item = &(PubGrubPackage, Range<PubGrubVersion>)> {
        self.0.iter()
    }
}

/// Convert a [`PubGrubDependencies`] to a [`DependencyConstraints`].
impl From<PubGrubDependencies> for Vec<(PubGrubPackage, Range<PubGrubVersion>)> {
    fn from(dependencies: PubGrubDependencies) -> Self {
        dependencies.0
    }
}

/// Convert a [`Requirement`] to a `PubGrub`-compatible package and range.
fn to_pubgrub(
    requirement: &Requirement,
    extra: Option<ExtraName>,
) -> Result<(PubGrubPackage, Range<PubGrubVersion>), ResolveError> {
    match requirement.version_or_url.as_ref() {
        // The requirement has no specifier (e.g., `flask`).
        None => Ok((
            PubGrubPackage::Package(requirement.name.clone(), extra),
            Range::full(),
        )),
        // The requirement has a URL (e.g., `flask @ file:///path/to/flask`).
        Some(VersionOrUrl::Url(url)) => Ok((
            PubGrubPackage::UrlPackage(requirement.name.clone(), extra, url.clone()),
            Range::full(),
        )),
        // The requirement has a specifier (e.g., `flask>=1.0`).
        Some(VersionOrUrl::VersionSpecifier(specifiers)) => {
            let version = specifiers
                .iter()
                .map(PubGrubSpecifier::try_from)
                .fold_ok(Range::full(), |range, specifier| {
                    range.intersection(&specifier.into())
                })?;
            Ok((
                PubGrubPackage::Package(requirement.name.clone(), extra),
                version,
            ))
        }
    }
}
