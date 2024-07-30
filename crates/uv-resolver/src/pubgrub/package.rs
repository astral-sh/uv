use std::ops::Deref;
use std::sync::Arc;

use pep508_rs::MarkerTree;
use uv_normalize::{ExtraName, GroupName, PackageName};

/// [`Arc`] wrapper around [`PubGrubPackageInner`] to make cloning (inside PubGrub) cheap.
#[derive(Debug, Clone, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub struct PubGrubPackage(Arc<PubGrubPackageInner>);

impl Deref for PubGrubPackage {
    type Target = PubGrubPackageInner;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::fmt::Display for PubGrubPackage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.0, f)
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
    ///
    /// Note that it is guaranteed that `extra` and `dev` are never both
    /// `Some`. That is, if one is `Some` then the other must be `None`.
    Package {
        name: PackageName,
        extra: Option<ExtraName>,
        dev: Option<GroupName>,
        marker: Option<MarkerTree>,
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
    },
    /// A proxy package for a base package with a marker (e.g., `black; python_version >= "3.6"`).
    ///
    /// If a requirement has an extra _and_ a marker, it will be represented via the `Extra` variant
    /// rather than the `Marker` variant.
    Marker {
        name: PackageName,
        marker: MarkerTree,
    },
}

impl PubGrubPackage {
    /// Create a [`PubGrubPackage`] from a package name and extra.
    pub(crate) fn from_package(
        name: PackageName,
        extra: Option<ExtraName>,
        mut marker: Option<MarkerTree>,
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
            }))
        } else if let Some(marker) = marker {
            Self(Arc::new(PubGrubPackageInner::Marker { name, marker }))
        } else {
            Self(Arc::new(PubGrubPackageInner::Package {
                name,
                extra,
                dev: None,
                marker,
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

    /// Returns the name of this PubGrub package, if it is not the root package or a Python version
    /// constraint.
    pub(crate) fn name_no_root(&self) -> Option<&PackageName> {
        match &**self {
            PubGrubPackageInner::Root(_) | PubGrubPackageInner::Python(_) => None,
            PubGrubPackageInner::Package { name, .. }
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
