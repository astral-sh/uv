use std::str::FromStr;

use once_cell::sync::Lazy;
use pep508_rs::{MarkerEnvironment, Requirement};
use pubgrub::range::Range;

use crate::specifier::to_ranges;
use anyhow::Result;
use puffin_package::dist_info_name::DistInfoName;
use puffin_package::package_name::PackageName;
use tracing::debug;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PubGrubVersion(pep440_rs::Version);

impl<'a> From<&'a PubGrubVersion> for &'a pep440_rs::Version {
    fn from(version: &'a PubGrubVersion) -> Self {
        &version.0
    }
}

impl From<pep440_rs::Version> for PubGrubVersion {
    fn from(version: pep440_rs::Version) -> Self {
        Self(version)
    }
}

impl From<PubGrubVersion> for pep440_rs::Version {
    fn from(version: PubGrubVersion) -> Self {
        version.0
    }
}

pub(crate) static VERSION_ZERO: Lazy<PubGrubVersion> =
    Lazy::new(|| PubGrubVersion::from(pep440_rs::Version::from_str("0a0.dev0").unwrap()));

pub(crate) static VERSION_INFINITY: Lazy<PubGrubVersion> = Lazy::new(|| {
    PubGrubVersion(pep440_rs::Version {
        epoch: usize::MAX,
        release: vec![usize::MAX, usize::MAX, usize::MAX],
        pre: None,
        post: Some(usize::MAX),
        dev: None,
        local: None,
    })
});

impl PubGrubVersion {
    /// Returns `true` if this is a pre-release version.
    pub fn is_prerelease(&self) -> bool {
        self.0.pre.is_some() || self.0.dev.is_some()
    }

    /// Returns the smallest PEP 440 version that is larger than `self`.
    pub fn next(&self) -> PubGrubVersion {
        let mut new = self.clone();
        // The rules are here:
        //
        //   https://www.python.org/dev/peps/pep-0440/#summary-of-permitted-suffixes-and-relative-ordering
        //
        // The relevant ones for this:
        //
        // - You can't attach a .postN after a .devN. So if you have a .devN,
        //   then the next possible version is .dev(N+1)
        //
        // - You can't attach a .postN after a .postN. So if you already have
        //   a .postN, then the next possible value is .post(N+1).
        //
        // - You *can* attach a .postN after anything else. And a .devN after that. So
        // to get the next possible value, attach a .post0.dev0.
        if let Some(dev) = &mut new.0.dev {
            *dev += 1;
        } else if let Some(post) = &mut new.0.post {
            *post += 1;
        } else {
            new.0.post = Some(0);
            new.0.dev = Some(0);
        }
        new
    }
}

impl std::fmt::Display for PubGrubVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl pubgrub::version::Version for PubGrubVersion {
    fn lowest() -> Self {
        VERSION_ZERO.to_owned()
    }

    fn bump(&self) -> Self {
        self.next()
    }
}

// A "package" for purposes of resolving. This is an extended version of what PyPI
// considers a package, in two ways.
//
// First, the pubgrub crate assumes that resolution always starts with a single required
// package==version. So we make a virtual "root" package, pass that to pubgrub as our
// initial requirement, and then we tell pubgrub that Root package depends on our actual
// requirements. (It'd be nice if pubgrub just took a DependencyConstraints to start
// with, but, whatever.)
//
// Second, extras. To handle them properly, we create virtual packages for each extra.
// So e.g. "foo[bar,baz]" really means "foo, but with the [bar] and [baz] requirements
// added to its normal set". But that's not a concept that pubgrub understands. So
// instead, we pretend that there are two packages "foo[bar]" and "foo[baz]", and their
// requirements are:
//
// - the requirements of 'foo', when evaluated with the appropriate 'extra' set
// - a special requirement on itself 'foo', with the exact same version.
//
// Result: if we wanted "foo[bar,baz]", we end up with "foo", plus all the [bar] and
// [baz] requirements at the same version. So at the end, we can just go through and
// discard all the virtual extra packages, to get the real set of packages.
//
// This trick is stolen from pip's resolver. It also puts us in a good place if reified
// extras[1] ever become a thing, because we're basically reifying them already.
//
// [1] https://mail.python.org/pipermail/distutils-sig/2015-October/027364.html
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PubGrubPackage {
    Root,
    Package(PackageName, Option<DistInfoName>),
}

impl std::fmt::Display for PubGrubPackage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PubGrubPackage::Root => write!(f, "<root>"),
            PubGrubPackage::Package(name, None) => write!(f, "{name}"),
            PubGrubPackage::Package(name, Some(extra)) => {
                write!(f, "{name}[{extra}]")
            }
        }
    }
}

/// Convert a PEP 508 specifier to a `PubGrub` range.
fn pubgrub_range(specifiers: Option<&pep508_rs::VersionOrUrl>) -> Result<Range<PubGrubVersion>> {
    let Some(specifiers) = specifiers else {
        return Ok(Range::any());
    };

    let pep508_rs::VersionOrUrl::VersionSpecifier(specifiers) = specifiers else {
        return Ok(Range::any());
    };

    let mut final_range = Range::any();
    for spec in specifiers.iter() {
        let spec_range = to_ranges(spec)?
            .into_iter()
            .fold(Range::none(), |accum, r| {
                accum.union(&if r.end < *VERSION_INFINITY {
                    Range::between(r.start, r.end)
                } else {
                    Range::higher_than(r.start)
                })
            });
        final_range = final_range.intersection(&spec_range);
    }
    Ok(final_range)
}

pub(crate) fn pubgrub_requirements<'a>(
    requirements: impl Iterator<Item = &'a Requirement> + 'a,
    extra: Option<&'a DistInfoName>,
    env: &'a MarkerEnvironment,
) -> impl Iterator<Item = (PubGrubPackage, Range<PubGrubVersion>)> + 'a {
    requirements
        .filter(move |requirement| {
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
            let version = pubgrub_range(requirement.version_or_url.as_ref()).unwrap();

            std::iter::once((package, version)).chain(
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
                        let version = pubgrub_range(requirement.version_or_url.as_ref()).unwrap();
                        (package, version)
                    }),
            )
        })
}
