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
//! assert!(version_specifiers.iter().all(|specifier| specifier.contains(&version)));
//! ```
//!
//! One thing that's a bit awkward about the API is that there's two kinds of
//! [Version]: One that doesn't allow stars (i.e. a package version), and one that does
//! (i.e. a version in a specifier), but they both use the same struct.
//!
//! The error handling and diagnostics is a bit overdone because this my parser-and-diagnostics
//! learning project (which kinda failed because the byte based regex crate and char-based
//! diagnostics don't mix well)
//!
//! PEP 440 has a lot of unintuitive features, including:
//!
//! * An epoch that you can prefix the version which, e.g. `1!1.2.3`. Lower epoch always means lower
//!   version (`1.0 <=2!0.1`)
//! * post versions, which can be attached to both stable releases and prereleases
//! * dev versions, which can be attached to sbpth table releases and prereleases. When attached to a
//!   prerelease the dev version is ordered just below the normal prerelease, however when attached
//!   to a stable version, the dev version is sorted before a prereleases
//! * prerelease handling is a mess: "Pre-releases of any kind, including developmental releases,
//!   are implicitly excluded from all version specifiers, unless they are already present on the
//!   system, explicitly requested by the user, or if the only available version that satisfies
//!   the version specifier is a pre-release.". This means that we can't say whether a specifier
//!   matches without also looking at the environment
//! * prelease vs. prerelease incl. dev is fuzzy
//! * local versions on top of all the others, which are added with a + and have implicitly typed
//!   string and number segments
//! * no semver-caret (`^`), but a pseudo-semver tilde (`~=`)
//! * ordering contradicts matching: We have e.g. `1.0+local > 1.0` when sorting,
//!   but `==1.0` matches `1.0+local`. While the ordering of versions itself is a total order
//!   the version matching needs to catch all sorts of special cases
#![deny(missing_docs)]

#[cfg(feature = "pyo3")]
use pyo3::{pymodule, types::PyModule, PyResult, Python};
use std::error::Error;
use std::fmt::{Display, Formatter};
#[cfg(feature = "pyo3")]
pub use version::PyVersion;
pub use version::{LocalSegment, Operator, PreRelease, Version};
pub use version_specifier::{parse_version_specifiers, VersionSpecifier, VersionSpecifiers};

mod version;
mod version_specifier;

/// Error with span information (unicode width) inside the parsed line
#[derive(Debug, Eq, PartialEq, Clone)]
pub struct Pep440Error {
    /// The actual error message
    pub message: String,
    /// The string that failed to parse
    pub line: String,
    /// First character for underlining (unicode width)
    pub start: usize,
    /// Number of characters to underline (unicode width)
    pub width: usize,
}

impl Display for Pep440Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Failed to parse version:")?;
        writeln!(f, "{}", self.line)?;
        writeln!(f, "{}{}", " ".repeat(self.start), "^".repeat(self.width))?;
        Ok(())
    }
}

impl Error for Pep440Error {}

/// Python bindings shipped as `pep440_rs`
#[cfg(feature = "pyo3")]
#[pymodule]
#[pyo3(name = "_pep440_rs")]
pub fn python_module(_py: Python, module: &PyModule) -> PyResult<()> {
    module.add_class::<PyVersion>()?;
    module.add_class::<Operator>()?;
    module.add_class::<VersionSpecifier>()?;
    module.add_class::<VersionSpecifiers>()?;
    Ok(())
}
