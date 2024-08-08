//! A library for python version numbers and specifiers, implementing
//! [PEP 440](https://peps.python.org/pep-0440)
//!
//! ```rust
//! use std::str::FromStr;
//! use pep440_rs::{VersionSpecifiers, Version, VersionSpecifier};
//!
//! let version = Version::from_str("1.19").unwrap();
//! let version_specifier = VersionSpecifier::from_str("== 1.*").unwrap();
//! assert!(version_specifier.contains(&version));
//! let version_specifiers = VersionSpecifiers::from_str(">=1.16, <2.0").unwrap();
//! assert!(version_specifiers.contains(&version));
//! ```
//!
//! PEP 440 has a lot of unintuitive features, including:
//!
//! * An epoch that you can prefix the version which, e.g. `1!1.2.3`. Lower epoch always means lower
//!   version (`1.0 <=2!0.1`)
//! * post versions, which can be attached to both stable releases and pre-releases
//! * dev versions, which can be attached to both table releases and pre-releases. When attached to a
//!   pre-release the dev version is ordered just below the normal pre-release, however when attached
//!   to a stable version, the dev version is sorted before a pre-releases
//! * pre-release handling is a mess: "Pre-releases of any kind, including developmental releases,
//!   are implicitly excluded from all version specifiers, unless they are already present on the
//!   system, explicitly requested by the user, or if the only available version that satisfies
//!   the version specifier is a pre-release.". This means that we can't say whether a specifier
//!   matches without also looking at the environment
//! * pre-release vs. pre-release incl. dev is fuzzy
//! * local versions on top of all the others, which are added with a + and have implicitly typed
//!   string and number segments
//! * no semver-caret (`^`), but a pseudo-semver tilde (`~=`)
//! * ordering contradicts matching: We have e.g. `1.0+local > 1.0` when sorting,
//!   but `==1.0` matches `1.0+local`. While the ordering of versions itself is a total order
//!   the version matching needs to catch all sorts of special cases
#![deny(missing_docs)]

#[cfg(feature = "pyo3")]
pub use version::PyVersion;
pub use {
    version::{
        LocalSegment, Operator, OperatorParseError, Prerelease, PrereleaseKind, Version,
        VersionParseError, VersionPattern, VersionPatternParseError, MIN_VERSION,
    },
    version_specifier::{
        VersionSpecifier, VersionSpecifierBuildError, VersionSpecifiers,
        VersionSpecifiersParseError,
    },
};

mod version;
mod version_specifier;

/// Python bindings shipped as `pep440_rs`
#[cfg(feature = "pyo3")]
#[pyo3::pymodule]
#[pyo3(name = "_pep440_rs")]
pub fn python_module(
    _py: pyo3::Python,
    module: &pyo3::Bound<'_, pyo3::types::PyModule>,
) -> pyo3::PyResult<()> {
    module.add_class::<PyVersion>()?;
    module.add_class::<Operator>()?;
    module.add_class::<VersionSpecifier>()?;
    module.add_class::<VersionSpecifiers>()?;
    Ok(())
}
