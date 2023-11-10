use puffin_normalize::{ExtraName, PackageName};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum ResolvoPackage {
    Package(PackageName),
    Extra(PackageName, ExtraName),
}

impl ResolvoPackage {
    /// Return the [`PackageName`] of the [`ResolvoPackage`].
    pub(crate) fn name(&self) -> &PackageName {
        match self {
            ResolvoPackage::Package(name) => name,
            ResolvoPackage::Extra(name, ..) => name,
        }
    }

    /// Return the [`ExtraName`] of the [`ResolvoPackage`], if any.
    pub(crate) fn extra(&self) -> Option<&ExtraName> {
        match self {
            ResolvoPackage::Package(_) => None,
            ResolvoPackage::Extra(_, extra) => Some(extra),
        }
    }
}

impl std::fmt::Display for ResolvoPackage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResolvoPackage::Package(name) => write!(f, "{name}"),
            ResolvoPackage::Extra(name, extra) => write!(f, "{name}[{extra}]"),
        }
    }
}
