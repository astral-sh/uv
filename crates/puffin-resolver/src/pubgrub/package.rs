use puffin_package::dist_info_name::DistInfoName;
use puffin_package::package_name::PackageName;

/// A PubGrub-compatible wrapper around a "Python package", with two notable characteristics:
///
/// 1. Includes a [`PubGrubPackage::Root`] variant, to satisfy `PubGrub`'s requirement that a
///    resolution starts from a single root.
/// 2. Uses the same strategy as pip and posy to handle extras: for each extra, we create a virtual
///    package (e.g., `black[colorama]`), and mark it as a dependency of the real package (e.g.,
///    `black`). We then discard the virtual packages at the end of the resolution process.
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
