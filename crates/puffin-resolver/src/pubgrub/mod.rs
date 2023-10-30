use anyhow::Result;
use pubgrub::range::Range;

use pep508_rs::{MarkerEnvironment, Requirement};
use puffin_package::dist_info_name::DistInfoName;
use puffin_package::package_name::PackageName;

pub(crate) use crate::pubgrub::package::PubGrubPackage;
pub(crate) use crate::pubgrub::priority::{PubGrubPriorities, PubGrubPriority};
pub(crate) use crate::pubgrub::specifier::PubGrubSpecifier;
pub(crate) use crate::pubgrub::version::{PubGrubVersion, MAX_VERSION, MIN_VERSION};

mod package;
mod priority;
mod specifier;
mod version;

/// Convert a set of requirements to a set of `PubGrub` packages and ranges.
pub(crate) fn iter_requirements<'a>(
    requirements: impl Iterator<Item = &'a Requirement> + 'a,
    extra: Option<&'a DistInfoName>,
    env: &'a MarkerEnvironment,
) -> impl Iterator<Item = (PubGrubPackage, Range<PubGrubVersion>)> + 'a {
    requirements
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
            let normalized_name = PackageName::normalize(&requirement.name);

            let package = PubGrubPackage::Package(normalized_name.clone(), None);
            let versions = version_range(requirement.version_or_url.as_ref()).unwrap();

            std::iter::once((package, versions)).chain(
                requirement
                    .extras
                    .clone()
                    .into_iter()
                    .flatten()
                    .map(move |extra| {
                        let package = PubGrubPackage::Package(
                            normalized_name.clone(),
                            Some(DistInfoName::normalize(extra)),
                        );
                        let versions = version_range(requirement.version_or_url.as_ref()).unwrap();
                        (package, versions)
                    }),
            )
        })
}

/// Convert a PEP 508 specifier to a `PubGrub` range.
pub(crate) fn version_range(
    specifiers: Option<&pep508_rs::VersionOrUrl>,
) -> Result<Range<PubGrubVersion>> {
    let Some(specifiers) = specifiers else {
        return Ok(Range::full());
    };

    let pep508_rs::VersionOrUrl::VersionSpecifier(specifiers) = specifiers else {
        return Ok(Range::full());
    };

    let mut final_range = Range::full();
    for spec in specifiers.iter() {
        let spec_range =
            PubGrubSpecifier::try_from(spec)?
                .into_iter()
                .fold(Range::empty(), |accum, range| {
                    accum.union(&if range.end < *MAX_VERSION {
                        Range::between(range.start, range.end)
                    } else {
                        Range::higher_than(range.start)
                    })
                });
        final_range = final_range.intersection(&spec_range);
    }
    Ok(final_range)
}
