use std::fmt::{Display, Formatter};
use std::ops::Deref;
use std::sync::Arc;

use pep508_rs::MarkerTree;
use pypi_types::VerbatimParsedUrl;
use uv_normalize::{ExtraName, GroupName, PackageName};

use crate::resolver::Urls;

/// [`Arc`] wrapper around [`PubGrubPackageInner`] to make cloning (inside PubGrub) cheap.
#[derive(Debug, Clone, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub struct PubGrubPackage(Arc<PubGrubPackageInner>);

impl Deref for PubGrubPackage {
    type Target = PubGrubPackageInner;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Display for PubGrubPackage {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.0, f)
    }
}

impl From<PubGrubPackageInner> for PubGrubPackage {
    fn from(package: PubGrubPackageInner) -> Self {
        Self(Arc::new(package))
    }
}

/// A PubGrub-compatible wrapper around a "Python package", with two notable characteristics:
///
/// 1. Includes a [`PubGrubPackage::Root`] variant, to satisfy PubGrub's requirement that a
///    resolution starts from a single root.
/// 2. Uses the same strategy as pip and posy to handle extras: for each extra, we create a virtual
///    package (e.g., `black[colorama]`), and mark it as a dependency of the real package (e.g.,
///    `black`). We then discard the virtual packages at the end of the resolution process.
#[derive(Debug, Clone, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub enum PubGrubPackageInner {
    /// The root package, which is used to start the resolution process.
    Root(Option<PackageName>),
    /// A Python version.
    Python(PubGrubPython),
    /// A Python package.
    Package {
        name: PackageName,
        extra: Option<ExtraName>,
        dev: Option<GroupName>,
        marker: Option<MarkerTree>,
        /// The URL of the package, if it was specified in the requirement.
        ///
        /// There are a few challenges that come with URL-based packages, and how they map to
        /// PubGrub.
        ///
        /// If the user declares a direct URL dependency, and then a transitive dependency
        /// appears for the same package, we need to ensure that the direct URL dependency can
        /// "satisfy" that requirement. So if the user declares a URL dependency on Werkzeug, and a
        /// registry dependency on Flask, we need to ensure that Flask's dependency on Werkzeug
        /// is resolved by the URL dependency. This means: (1) we need to avoid adding a second
        /// Werkzeug variant from PyPI; and (2) we need to error if the Werkzeug version requested
        /// by Flask doesn't match that of the URL dependency.
        ///
        /// Additionally, we need to ensure that we disallow multiple versions of the same package,
        /// even if requested from different URLs.
        ///
        /// To enforce this requirement, we require that all possible URL dependencies are
        /// defined upfront, as `requirements.txt` or `constraints.txt` or similar. Otherwise,
        /// solving these graphs becomes far more complicated -- and the "right" behavior isn't
        /// even clear. For example, imagine that you define a direct dependency on Werkzeug, and
        /// then one of your other direct dependencies declares a dependency on Werkzeug at some
        /// URL. Which is correct? By requiring direct dependencies, the semantics are at least
        /// clear.
        ///
        /// With the list of known URLs available upfront, we then only need to do two things:
        ///
        /// 1. When iterating over the dependencies for a single package, ensure that we respect
        ///    URL variants over registry variants, if the package declares a dependency on both
        ///    `Werkzeug==2.0.0` _and_ `Werkzeug @ https://...` , which is strange but possible.
        ///    This is enforced by [`crate::pubgrub::dependencies::PubGrubDependencies`].
        /// 2. Reject any URL dependencies that aren't known ahead-of-time.
        ///
        /// Eventually, we could relax this constraint, in favor of something more lenient, e.g., if
        /// we're going to have a dependency that's provided as a URL, we _need_ to visit the URL
        /// version before the registry version. So we could just error if we visit a URL variant
        /// _after_ a registry variant.
        url: Option<VerbatimParsedUrl>,
    },
    /// A proxy package to represent a dependency with an extra (e.g., `black[colorama]`).
    ///
    /// For a given package `black`, and an extra `colorama`, we create a virtual package
    /// with exactly two dependencies: `PubGrubPackage::Package("black", None)` and
    /// `PubGrubPackage::Package("black", Some("colorama")`. Both dependencies are pinned to the
    /// same version, and the virtual package is discarded at the end of the resolution process.
    ///
    /// The benefit of the proxy package (versus `PubGrubPackage::Package("black", Some("colorama")`
    /// on its own) is that it enables us to avoid attempting to retrieve metadata for irrelevant
    /// versions the extra variants by making it clear to PubGrub that the extra variant must match
    /// the exact same version of the base variant. Without the proxy package, then when provided
    /// requirements like `black==23.0.1` and `black[colorama]`, PubGrub may attempt to retrieve
    /// metadata for `black[colorama]` versions other than `23.0.1`.
    Extra {
        name: PackageName,
        extra: ExtraName,
        marker: Option<MarkerTree>,
        url: Option<VerbatimParsedUrl>,
    },
    /// A proxy package to represent an enabled "dependency group" (e.g., development dependencies).
    ///
    /// This is similar in spirit to [PEP 735](https://peps.python.org/pep-0735/) and similar in
    /// implementation to the `Extra` variant. The main difference is that we treat groups as
    /// enabled globally, rather than on a per-requirement basis.
    Dev {
        name: PackageName,
        dev: GroupName,
        marker: Option<MarkerTree>,
        url: Option<VerbatimParsedUrl>,
    },
    /// A proxy package for a base package with a marker (e.g., `black; python_version >= "3.6"`).
    ///
    /// If a requirement has an extra _and_ a marker, it will be represented via the `Extra` variant
    /// rather than the `Marker` variant.
    Marker {
        name: PackageName,
        marker: MarkerTree,
        url: Option<VerbatimParsedUrl>,
    },
}

impl PubGrubPackage {
    /// Create a [`PubGrubPackage`] from a package name and version.
    pub(crate) fn from_package(
        name: PackageName,
        extra: Option<ExtraName>,
        mut marker: Option<MarkerTree>,
        urls: &Urls,
    ) -> Self {
        let url = urls.get(&name).cloned();
        // Remove all extra expressions from the marker, since we track extras
        // separately. This also avoids an issue where packages added via
        // extras end up having two distinct marker expressions, which in turn
        // makes them two distinct packages. This results in PubGrub being
        // unable to unify version constraints across such packages.
        marker = marker.and_then(|m| m.simplify_extras_with(|_| true));
        if let Some(extra) = extra {
            Self(Arc::new(PubGrubPackageInner::Extra {
                name,
                extra,
                marker,
                url,
            }))
        } else if let Some(marker) = marker {
            Self(Arc::new(PubGrubPackageInner::Marker { name, marker, url }))
        } else {
            Self(Arc::new(PubGrubPackageInner::Package {
                name,
                extra,
                dev: None,
                marker,
                url,
            }))
        }
    }

    /// Create a [`PubGrubPackage`] from a package name and URL.
    pub(crate) fn from_url(
        name: PackageName,
        extra: Option<ExtraName>,
        mut marker: Option<MarkerTree>,
        url: VerbatimParsedUrl,
    ) -> Self {
        // Remove all extra expressions from the marker, since we track extras
        // separately. This also avoids an issue where packages added via
        // extras end up having two distinct marker expressions, which in turn
        // makes them two distinct packages. This results in PubGrub being
        // unable to unify version constraints across such packages.
        marker = marker.and_then(|m| m.simplify_extras_with(|_| true));
        if let Some(extra) = extra {
            Self(Arc::new(PubGrubPackageInner::Extra {
                name,
                extra,
                marker,
                url: Some(url),
            }))
        } else if let Some(marker) = marker {
            Self(Arc::new(PubGrubPackageInner::Marker {
                name,
                marker,
                url: Some(url),
            }))
        } else {
            Self(Arc::new(PubGrubPackageInner::Package {
                name,
                extra,
                dev: None,
                marker,
                url: Some(url),
            }))
        }
    }

    /// Returns the name of this PubGrub package, if it has one.
    pub(crate) fn name(&self) -> Option<&PackageName> {
        match &**self {
            // A root can never be a dependency of another package, and a `Python` pubgrub
            // package is never returned by `get_dependencies`. So these cases never occur.
            PubGrubPackageInner::Root(None) | PubGrubPackageInner::Python(_) => None,
            PubGrubPackageInner::Root(Some(name))
            | PubGrubPackageInner::Package { name, .. }
            | PubGrubPackageInner::Extra { name, .. }
            | PubGrubPackageInner::Dev { name, .. }
            | PubGrubPackageInner::Marker { name, .. } => Some(name),
        }
    }

    /// Returns the marker expression associated with this PubGrub package, if
    /// it has one.
    pub(crate) fn marker(&self) -> Option<&MarkerTree> {
        match &**self {
            // A root can never be a dependency of another package, and a `Python` pubgrub
            // package is never returned by `get_dependencies`. So these cases never occur.
            PubGrubPackageInner::Root(_) | PubGrubPackageInner::Python(_) => None,
            PubGrubPackageInner::Package { marker, .. }
            | PubGrubPackageInner::Extra { marker, .. }
            | PubGrubPackageInner::Dev { marker, .. } => marker.as_ref(),
            PubGrubPackageInner::Marker { marker, .. } => Some(marker),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Hash, Ord)]
pub enum PubGrubPython {
    /// The Python version installed in the current environment.
    Installed,
    /// The Python version for which dependencies are being resolved.
    Target,
}

impl std::fmt::Display for PubGrubPackageInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Root(name) => {
                if let Some(name) = name {
                    write!(f, "{}", name.as_ref())
                } else {
                    write!(f, "root")
                }
            }
            Self::Python(_) => write!(f, "Python"),
            Self::Package {
                name,
                extra: None,
                marker: None,
                ..
            } => write!(f, "{name}"),
            Self::Package {
                name,
                extra: Some(extra),
                marker: None,
                ..
            } => {
                write!(f, "{name}[{extra}]")
            }
            Self::Package {
                name,
                extra: None,
                marker: Some(marker),
                ..
            } => write!(f, "{name}{{{marker}}}"),
            Self::Package {
                name,
                extra: Some(extra),
                marker: Some(marker),
                ..
            } => {
                write!(f, "{name}[{extra}]{{{marker}}}")
            }
            Self::Marker { name, marker, .. } => write!(f, "{name}{{{marker}}}"),
            Self::Extra { name, extra, .. } => write!(f, "{name}[{extra}]"),
            Self::Dev { name, dev, .. } => write!(f, "{name}:{dev}"),
        }
    }
}
