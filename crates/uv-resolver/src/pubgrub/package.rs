use std::ops::Deref;
use std::sync::Arc;

use uv_normalize::{ExtraName, GroupName, PackageName};
use uv_pep508::MarkerTree;
use uv_pypi_types::ConflictItemRef;
use uv_resolver_types::PackageNodeKind;

use crate::python_requirement::PythonRequirement;

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
    /// A system package, which is used to represent a non-Python package.
    System(PackageName),
    /// A Python package.
    Package {
        name: PackageName,
        kind: PackageNodeKind,
        marker: MarkerTree,
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
        marker: MarkerTree,
    },
    /// A proxy package to represent an enabled dependency group.
    ///
    /// This is similar in spirit to [PEP 735](https://peps.python.org/pep-0735/) and similar in
    /// implementation to the `Extra` variant. The main difference is that we treat groups as
    /// enabled globally, rather than on a per-requirement basis.
    Group {
        name: PackageName,
        group: GroupName,
        marker: MarkerTree,
    },
    /// A proxy package for a base package with a marker (e.g., `black; python_version >= "3.6"`).
    ///
    /// If a requirement has an extra _and_ a marker, it will be represented via the `Extra` variant
    /// rather than the `Marker` variant.
    Marker {
        name: PackageName,
        /// The marker associated with this proxy package.
        marker: MarkerTree,
    },
}

impl PubGrubPackage {
    /// Create a [`PubGrubPackage`] from a package name and [`PackageNodeKind`].
    pub(crate) fn from_package(
        name: PackageName,
        kind: PackageNodeKind,
        marker: MarkerTree,
    ) -> Self {
        // Remove all extra expressions from the marker, since we track extras
        // separately. This also avoids an issue where packages added via
        // extras end up having two distinct marker expressions, which in turn
        // makes them two distinct packages. This results in PubGrub being
        // unable to unify version constraints across such packages.
        let marker = marker.simplify_extras_with(|_| true);
        match kind {
            PackageNodeKind::Extra(extra) => Self(Arc::new(PubGrubPackageInner::Extra {
                name,
                extra,
                marker,
            })),
            PackageNodeKind::Group(group) => Self(Arc::new(PubGrubPackageInner::Group {
                name,
                group,
                marker,
            })),
            PackageNodeKind::Base if !marker.is_true() => {
                Self(Arc::new(PubGrubPackageInner::Marker { name, marker }))
            }
            PackageNodeKind::Base => Self(Arc::new(PubGrubPackageInner::Package {
                name,
                kind,
                marker,
            })),
        }
    }

    /// If this package is a proxy package, return the base package it depends on.
    ///
    /// While dependency groups may be attached to a package, we don't consider them here as
    /// there is no (mandatory) dependency from a dependency group to the package.
    pub(crate) fn base_package(&self) -> Option<Self> {
        match &**self {
            PubGrubPackageInner::Root(_)
            | PubGrubPackageInner::Python(_)
            | PubGrubPackageInner::System(_)
            | PubGrubPackageInner::Package { .. } => None,
            PubGrubPackageInner::Group { .. } => {
                // The dependency groups of a package do not by themselves require the package
                // itself.
                None
            }
            PubGrubPackageInner::Extra { name, .. } | PubGrubPackageInner::Marker { name, .. } => {
                Some(Self::from_package(
                    name.clone(),
                    PackageNodeKind::Base,
                    MarkerTree::TRUE,
                ))
            }
        }
    }

    /// Returns the name of this PubGrub package, if it has one.
    pub(crate) fn name(&self) -> Option<&PackageName> {
        match &**self {
            // A root can never be a dependency of another package, and a `Python` pubgrub
            // package is never returned by `get_dependencies`. So these cases never occur.
            PubGrubPackageInner::Root(None) | PubGrubPackageInner::Python(_) => None,
            PubGrubPackageInner::Root(Some(name))
            | PubGrubPackageInner::System(name)
            | PubGrubPackageInner::Package { name, .. }
            | PubGrubPackageInner::Extra { name, .. }
            | PubGrubPackageInner::Group { name, .. }
            | PubGrubPackageInner::Marker { name, .. } => Some(name),
        }
    }

    /// Returns the name of this PubGrub package, if it is not the root package, a Python version
    /// constraint, or a system package.
    pub(crate) fn name_no_root(&self) -> Option<&PackageName> {
        match &**self {
            PubGrubPackageInner::Root(_)
            | PubGrubPackageInner::Python(_)
            | PubGrubPackageInner::System(_) => None,
            PubGrubPackageInner::Package { name, .. }
            | PubGrubPackageInner::Extra { name, .. }
            | PubGrubPackageInner::Group { name, .. }
            | PubGrubPackageInner::Marker { name, .. } => Some(name),
        }
    }

    /// Returns the marker expression associated with this PubGrub package, if
    /// it has one.
    pub(crate) fn marker(&self) -> MarkerTree {
        match &**self {
            // A root can never be a dependency of another package, and a `Python` pubgrub
            // package is never returned by `get_dependencies`. So these cases never occur.
            PubGrubPackageInner::Root(_)
            | PubGrubPackageInner::Python(_)
            | PubGrubPackageInner::System(_) => MarkerTree::TRUE,
            PubGrubPackageInner::Package { marker, .. }
            | PubGrubPackageInner::Extra { marker, .. }
            | PubGrubPackageInner::Group { marker, .. } => *marker,
            PubGrubPackageInner::Marker { marker, .. } => *marker,
        }
    }

    /// Returns the extra name associated with this PubGrub package, if it has
    /// one.
    ///
    /// Note that if this returns `Some`, then `group` must return `None`.
    pub(crate) fn extra(&self) -> Option<&ExtraName> {
        match &**self {
            // A root can never be a dependency of another package, and a `Python` pubgrub
            // package is never returned by `get_dependencies`. So these cases never occur.
            PubGrubPackageInner::Root(_)
            | PubGrubPackageInner::Python(_)
            | PubGrubPackageInner::System(_)
            | PubGrubPackageInner::Package {
                kind: PackageNodeKind::Base | PackageNodeKind::Group(_),
                ..
            }
            | PubGrubPackageInner::Group { .. }
            | PubGrubPackageInner::Marker { .. } => None,
            PubGrubPackageInner::Package {
                kind: PackageNodeKind::Extra(extra),
                ..
            }
            | PubGrubPackageInner::Extra { extra, .. } => Some(extra),
        }
    }

    /// Returns the dependency group name associated with this PubGrub
    /// package, if it has one.
    ///
    /// Note that if this returns `Some`, then `extra` must return `None`.
    pub(crate) fn group(&self) -> Option<&GroupName> {
        match &**self {
            // A root can never be a dependency of another package, and a `Python` pubgrub
            // package is never returned by `get_dependencies`. So these cases never occur.
            PubGrubPackageInner::Root(_)
            | PubGrubPackageInner::Python(_)
            | PubGrubPackageInner::System(_)
            | PubGrubPackageInner::Package {
                kind: PackageNodeKind::Base | PackageNodeKind::Extra(_),
                ..
            }
            | PubGrubPackageInner::Extra { .. }
            | PubGrubPackageInner::Marker { .. } => None,
            PubGrubPackageInner::Package {
                kind: PackageNodeKind::Group(group),
                ..
            }
            | PubGrubPackageInner::Group { group, .. } => Some(group),
        }
    }

    /// Extracts a possible conflicting item from this package.
    ///
    /// If this package can't possibly be classified as conflicting, then
    /// this returns `None`.
    pub(crate) fn conflicting_item(&self) -> Option<ConflictItemRef<'_>> {
        match &**self {
            PubGrubPackageInner::Root(_)
            | PubGrubPackageInner::Python(_)
            | PubGrubPackageInner::System(_) => None,
            PubGrubPackageInner::Package {
                name,
                kind: PackageNodeKind::Base,
                ..
            }
            | PubGrubPackageInner::Marker { name, .. } => Some(ConflictItemRef::from(name)),
            PubGrubPackageInner::Package {
                name,
                kind: PackageNodeKind::Extra(extra),
                ..
            }
            | PubGrubPackageInner::Extra { name, extra, .. } => {
                Some(ConflictItemRef::from((name, extra)))
            }
            PubGrubPackageInner::Package {
                name,
                kind: PackageNodeKind::Group(group),
                ..
            }
            | PubGrubPackageInner::Group { name, group, .. } => {
                Some(ConflictItemRef::from((name, group)))
            }
        }
    }

    /// Returns `true` if this PubGrub package is the root package.
    pub(crate) fn is_root(&self) -> bool {
        matches!(&**self, PubGrubPackageInner::Root(_))
    }

    /// Returns `true` if this PubGrub package is a proxy package.
    pub(crate) fn is_proxy(&self) -> bool {
        matches!(
            &**self,
            PubGrubPackageInner::Extra { .. }
                | PubGrubPackageInner::Group { .. }
                | PubGrubPackageInner::Marker { .. }
        )
    }

    /// This simplifies the markers on this package (if any exist) using the
    /// given Python requirement as assumed context.
    ///
    /// See `RequiresPython::simplify_markers` for more details.
    ///
    /// NOTE: This routine is kind of weird, because this should only really be
    /// applied in contexts where the `PubGrubPackage` is printed as output.
    /// So in theory, this should be a transformation into a new type with a
    /// "printable" `PubGrubPackage` coupled with a `Requires-Python`. But at
    /// time of writing, this was a larger refactor, particularly in the error
    /// reporting where this routine is used.
    pub(crate) fn simplify_markers(&mut self, python_requirement: &PythonRequirement) {
        match *Arc::make_mut(&mut self.0) {
            PubGrubPackageInner::Root(_)
            | PubGrubPackageInner::Python(_)
            | PubGrubPackageInner::System(_) => {}
            PubGrubPackageInner::Package { ref mut marker, .. }
            | PubGrubPackageInner::Extra { ref mut marker, .. }
            | PubGrubPackageInner::Group { ref mut marker, .. }
            | PubGrubPackageInner::Marker { ref mut marker, .. } => {
                *marker = python_requirement.simplify_markers(*marker);
            }
        }
    }

    /// This isn't actually used anywhere, but can be useful for printf-debugging.
    #[allow(dead_code)]
    pub(crate) fn kind(&self) -> &'static str {
        match &**self {
            PubGrubPackageInner::Root(_) => "root",
            PubGrubPackageInner::Python(_) => "python",
            PubGrubPackageInner::System(_) => "system",
            PubGrubPackageInner::Package { .. } => "package",
            PubGrubPackageInner::Extra { .. } => "extra",
            PubGrubPackageInner::Group { .. } => "group",
            PubGrubPackageInner::Marker { .. } => "marker",
        }
    }

    /// Returns a new [`PubGrubPackage`] representing the base package with the given name.
    pub(crate) fn base(name: PackageName) -> Self {
        Self::from_package(name, PackageNodeKind::Base, MarkerTree::TRUE)
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, PartialOrd, Hash, Ord)]
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
            Self::System(name) => write!(f, "system:{name}"),
            Self::Package {
                name,
                kind: PackageNodeKind::Base,
                marker,
            } => {
                if let Some(marker) = marker.contents() {
                    write!(f, "{name}{{{marker}}}")
                } else {
                    write!(f, "{name}")
                }
            }
            Self::Package {
                name,
                kind: PackageNodeKind::Extra(extra),
                marker,
            } => {
                if let Some(marker) = marker.contents() {
                    write!(f, "{name}[{extra}]{{{marker}}}")
                } else {
                    write!(f, "{name}[{extra}]")
                }
            }
            Self::Package {
                name,
                kind: PackageNodeKind::Group(dev),
                marker,
            } => {
                if let Some(marker) = marker.contents() {
                    write!(f, "{name}:{dev}{{{marker}}}")
                } else {
                    write!(f, "{name}:{dev}")
                }
            }
            Self::Marker { name, marker, .. } => {
                if let Some(marker) = marker.contents() {
                    write!(f, "{name}{{{marker}}}")
                } else {
                    write!(f, "{name}")
                }
            }
            Self::Extra { name, extra, .. } => write!(f, "{name}[{extra}]"),
            Self::Group {
                name, group: dev, ..
            } => write!(f, "{name}:{dev}"),
        }
    }
}

impl From<&Self> for PubGrubPackage {
    fn from(package: &Self) -> Self {
        package.clone()
    }
}
