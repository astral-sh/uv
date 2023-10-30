use anyhow::Result;
use itertools::Itertools;
use pubgrub::range::Range;
use tracing::warn;

use pep508_rs::{MarkerEnvironment, Requirement, VersionOrUrl};
use puffin_package::dist_info_name::DistInfoName;
use puffin_package::package_name::PackageName;

pub(crate) use crate::pubgrub::package::PubGrubPackage;
pub(crate) use crate::pubgrub::priority::{PubGrubPriorities, PubGrubPriority};
pub(crate) use crate::pubgrub::specifier::PubGrubSpecifier;
pub(crate) use crate::pubgrub::version::{PubGrubVersion, MIN_VERSION};

mod package;
mod priority;
mod specifier;
mod version;

/// Convert a set of requirements to a set of `PubGrub` packages and ranges.
pub(crate) fn iter_requirements<'a>(
    requirements: impl Iterator<Item = &'a Requirement> + 'a,
    extra: Option<&'a DistInfoName>,
    source: Option<&'a PackageName>,
    env: &'a MarkerEnvironment,
) -> impl Iterator<Item = (PubGrubPackage, Range<PubGrubVersion>)> + 'a {
    requirements
        .filter(move |requirement| {
            let normalized = PackageName::normalize(&requirement.name);
            if source.is_some_and(|source| source == &normalized) {
                // TODO(konstin): Warn only once here
                warn!("{normalized} depends on itself");
                false
            } else {
                true
            }
        })
        .filter(move |requirement| {
            // TODO(charlie): We shouldn't need a vector here.
            let extra = if let Some(extra) = extra {
                vec![extra.as_ref()]
            } else {
                vec![]
            };
            requirement.evaluate_markers(env, &extra)
        })
        .flat_map(|requirement| {
            std::iter::once(pubgrub_package(requirement, None).unwrap()).chain(
                requirement
                    .extras
                    .clone()
                    .into_iter()
                    .flatten()
                    .map(|extra| {
                        pubgrub_package(requirement, Some(DistInfoName::normalize(extra))).unwrap()
                    }),
            )
        })
}

/// Convert a PEP 508 specifier to a `PubGrub` range.
pub(crate) fn version_range(specifiers: Option<&VersionOrUrl>) -> Result<Range<PubGrubVersion>> {
    let Some(specifiers) = specifiers else {
        return Ok(Range::full());
    };

    let VersionOrUrl::VersionSpecifier(specifiers) = specifiers else {
        return Ok(Range::full());
    };

    specifiers
        .iter()
        .map(PubGrubSpecifier::try_from)
        .fold_ok(Range::full(), |range, specifier| {
            range.intersection(&specifier.into())
        })
}

/// Convert a [`Requirement`] to a `PubGrub`-compatible package and range.
fn pubgrub_package(
    requirement: &Requirement,
    extra: Option<DistInfoName>,
) -> Result<(PubGrubPackage, Range<PubGrubVersion>)> {
    match requirement.version_or_url.as_ref() {
        // The requirement has no specifier (e.g., `flask`).
        None => Ok((
            PubGrubPackage::Package(PackageName::normalize(&requirement.name), extra, None),
            Range::full(),
        )),
        // The requirement has a URL (e.g., `flask @ file:///path/to/flask`).
        Some(VersionOrUrl::Url(url)) => Ok((
            PubGrubPackage::Package(
                PackageName::normalize(&requirement.name),
                extra,
                Some(url.clone()),
            ),
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
                PubGrubPackage::Package(PackageName::normalize(&requirement.name), extra, None),
                version,
            ))
        }
    }
}
