use derivative::Derivative;

use pep508_rs::VerbatimUrl;
use uv_normalize::{ExtraName, PackageName};

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
    /// The root package, which is used to start the resolution process.
    Root(Option<PackageName>),
    /// A Python version.
    Python(PubGrubPython),
    /// A Python package.
    Package(
        PackageName,
        Option<ExtraName>,
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
        /// Removing the URL from the hash and equality operators is a hack to enable this behavior.
        /// When we visit a URL dependency, we include the URL here. But that dependency "looks"
        /// equal to the registry version from the solver's perspective, since they hash to the
        /// same value.
        ///
        /// However, this creates the unfortunate requirement that we _must_ visit URL dependencies
        /// before their registry variant, so that the URL-based version is used as the "canonical"
        /// version. This is because the solver will always prefer the first version it sees, and
        /// we need to ensure that the first version is the URL-based version.
        ///
        /// To enforce this requirement, we in turn require that all possible URL dependencies are
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
        #[derivative(PartialEq = "ignore")]
        #[derivative(PartialOrd = "ignore")]
        #[derivative(Hash = "ignore")]
        Option<VerbatimUrl>,
    ),
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum PubGrubPython {
    /// The Python version installed in the current environment.
    Installed,
    /// The Python version for which dependencies are being resolved.
    Target,
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
            PubGrubPackage::Python(_) => write!(f, "Python"),
            PubGrubPackage::Package(name, None, ..) => write!(f, "{name}"),
            PubGrubPackage::Package(name, Some(extra), ..) => {
                write!(f, "{name}[{extra}]")
            }
        }
    }
}
