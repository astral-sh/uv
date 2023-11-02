use derivative::Derivative;
use url::Url;

use puffin_normalize::{ExtraName, PackageName};

/// A PubGrub-compatible wrapper around a "Python package", with two notable characteristics:
///
/// 1. Includes a [`PubGrubPackage::Root`] variant, to satisfy `PubGrub`'s requirement that a
///    resolution starts from a single root.
/// 2. Uses the same strategy as pip and posy to handle extras: for each extra, we create a virtual
///    package (e.g., `black[colorama]`), and mark it as a dependency of the real package (e.g.,
///    `black`). We then discard the virtual packages at the end of the resolution process.
#[derive(Debug, Clone, Eq, Derivative)]
#[derivative(PartialEq, Hash)]
pub enum PubGrubPackage {
    Root(Option<PackageName>),
    Package(
        PackageName,
        Option<ExtraName>,
        #[derivative(PartialEq = "ignore")]
        #[derivative(PartialOrd = "ignore")]
        #[derivative(Hash = "ignore")]
        Option<Url>,
    ),
}

impl std::fmt::Display for PubGrubPackage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PubGrubPackage::Root(name) => {
                if let Some(name) = name {
                    write!(f, "{}", name.as_ref())
                } else {
                    write!(f, "root")
                }
            }
            PubGrubPackage::Package(name, None, ..) => write!(f, "{name}"),
            PubGrubPackage::Package(name, Some(extra), ..) => {
                write!(f, "{name}[{extra}]")
            }
        }
    }
}
