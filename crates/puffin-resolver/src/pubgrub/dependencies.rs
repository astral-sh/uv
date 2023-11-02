use itertools::Itertools;
use pubgrub::range::Range;
use pubgrub::type_aliases::DependencyConstraints;
use tracing::warn;

use pep508_rs::{MarkerEnvironment, Requirement, VersionOrUrl};
use puffin_cache::CanonicalUrl;
use puffin_normalize::{ExtraName, PackageName};

use crate::pubgrub::specifier::PubGrubSpecifier;
use crate::pubgrub::{PubGrubPackage, PubGrubVersion};
use crate::ResolveError;

#[derive(Debug)]
pub struct PubGrubDependencies(DependencyConstraints<PubGrubPackage, Range<PubGrubVersion>>);

impl PubGrubDependencies {
    /// Generate a set of `PubGrub` dependencies from a set of requirements.
    pub(crate) fn try_from_requirements<'a>(
        requirements: &[Requirement],
        constraints: &[Requirement],
        extra: Option<&'a ExtraName>,
        source: Option<&'a PackageName>,
        env: &'a MarkerEnvironment,
    ) -> Result<Self, ResolveError> {
        let mut dependencies =
            DependencyConstraints::<PubGrubPackage, Range<PubGrubVersion>>::default();

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

                if let Some(entry) = dependencies.get_key_value(&package) {
                    // Merge the versions.
                    let version = merge_versions(entry.1, &version);

                    // Merge the package.
                    if let Some(package) = merge_package(entry.0, &package)? {
                        dependencies.remove(&package);
                        dependencies.insert(package, version);
                    } else {
                        dependencies.insert(package, version);
                    }
                } else {
                    dependencies.insert(package.clone(), version.clone());
                }
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

                if let Some(entry) = dependencies.get_key_value(&package) {
                    // Merge the versions.
                    let version = merge_versions(entry.1, &version);

                    // Merge the package.
                    if let Some(package) = merge_package(entry.0, &package)? {
                        dependencies.insert(package, version);
                    } else {
                        dependencies.insert(package, version);
                    }
                }
            }
        }

        Ok(Self(dependencies))
    }

    /// Insert a [`PubGrubPackage`] and [`PubGrubVersion`] range into the set of dependencies.
    pub(crate) fn insert(
        &mut self,
        package: PubGrubPackage,
        version: Range<PubGrubVersion>,
    ) -> Option<Range<PubGrubVersion>> {
        self.0.insert(package, version)
    }

    /// Iterate over the dependencies.
    pub(crate) fn iter(&self) -> impl Iterator<Item = (&PubGrubPackage, &Range<PubGrubVersion>)> {
        self.0.iter()
    }
}

/// Convert a [`PubGrubDependencies`] to a [`DependencyConstraints`].
impl From<PubGrubDependencies> for DependencyConstraints<PubGrubPackage, Range<PubGrubVersion>> {
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
            PubGrubPackage::Package(requirement.name.clone(), extra, None),
            Range::full(),
        )),
        // The requirement has a URL (e.g., `flask @ file:///path/to/flask`).
        Some(VersionOrUrl::Url(url)) => Ok((
            PubGrubPackage::Package(requirement.name.clone(), extra, Some(url.clone())),
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
                PubGrubPackage::Package(requirement.name.clone(), extra, None),
                version,
            ))
        }
    }
}

/// Merge two [`PubGrubVersion`] ranges.
fn merge_versions(
    left: &Range<PubGrubVersion>,
    right: &Range<PubGrubVersion>,
) -> Range<PubGrubVersion> {
    left.intersection(right)
}

/// Merge two [`PubGrubPackage`] instances.
fn merge_package(
    left: &PubGrubPackage,
    right: &PubGrubPackage,
) -> Result<Option<PubGrubPackage>, ResolveError> {
    match (left, right) {
        // Either package is `root`.
        (PubGrubPackage::Root(_), _) | (_, PubGrubPackage::Root(_)) => Ok(None),

        // Left package has a URL. Propagate the URL.
        (PubGrubPackage::Package(name, extra, Some(url)), PubGrubPackage::Package(.., None)) => {
            Ok(Some(PubGrubPackage::Package(
                name.clone(),
                extra.clone(),
                Some(url.clone()),
            )))
        }

        // Right package has a URL.
        (PubGrubPackage::Package(.., None), PubGrubPackage::Package(name, extra, Some(url))) => {
            Ok(Some(PubGrubPackage::Package(
                name.clone(),
                extra.clone(),
                Some(url.clone()),
            )))
        }

        // Neither package has a URL.
        (PubGrubPackage::Package(_name, _extra, None), PubGrubPackage::Package(.., None)) => {
            Ok(None)
        }

        // Both packages have a URL.
        (
            PubGrubPackage::Package(name, _extra, Some(left)),
            PubGrubPackage::Package(.., Some(right)),
        ) => {
            if CanonicalUrl::new(left) == CanonicalUrl::new(right) {
                Ok(None)
            } else {
                Err(ResolveError::ConflictingUrls(
                    name.clone(),
                    left.to_string(),
                    right.to_string(),
                ))
            }
        }
    }
}
