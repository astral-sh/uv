use std::fmt::{Display, Formatter};

use pep508_rs::PackageName;
use uv_interpreter::PythonEnvironment;

use crate::{PackageNameSpecifier, PackageNameSpecifiers};

/// Whether to enforce build isolation when building source distributions.
#[derive(Debug, Copy, Clone)]
pub enum BuildIsolation<'a> {
    Isolated,
    Shared(&'a PythonEnvironment),
}

impl<'a> BuildIsolation<'a> {
    /// Returns `true` if build isolation is enforced.
    pub fn is_isolated(&self) -> bool {
        matches!(self, Self::Isolated)
    }
}

/// The strategy to use when building source distributions that lack a `pyproject.toml`.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum SetupPyStrategy {
    /// Perform a PEP 517 build.
    #[default]
    Pep517,
    /// Perform a build by invoking `setuptools` directly.
    Setuptools,
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum BuildKind {
    /// A regular PEP 517 wheel build
    #[default]
    Wheel,
    /// A PEP 660 editable installation wheel build
    Editable,
}

impl Display for BuildKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Wheel => f.write_str("wheel"),
            Self::Editable => f.write_str("editable"),
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub enum NoBinary {
    /// Allow installation of any wheel.
    #[default]
    None,

    /// Do not allow installation from any wheels.
    All,

    /// Do not allow installation from the specific wheels.
    Packages(Vec<PackageName>),
}

impl NoBinary {
    /// Determine the binary installation strategy to use for the given arguments.
    pub fn from_args(no_binary: Vec<PackageNameSpecifier>) -> Self {
        let combined = PackageNameSpecifiers::from_iter(no_binary.into_iter());
        match combined {
            PackageNameSpecifiers::All => Self::All,
            PackageNameSpecifiers::None => Self::None,
            PackageNameSpecifiers::Packages(packages) => Self::Packages(packages),
        }
    }

    /// Determine the binary installation strategy to use for the given argument.
    pub fn from_arg(no_binary: PackageNameSpecifier) -> Self {
        Self::from_args(vec![no_binary])
    }

    /// Combine a set of [`NoBinary`] values.
    #[must_use]
    pub fn combine(self, other: Self) -> Self {
        match (self, other) {
            // If both are `None`, the result is `None`.
            (Self::None, Self::None) => Self::None,
            // If either is `All`, the result is `All`.
            (Self::All, _) | (_, Self::All) => Self::All,
            // If one is `None`, the result is the other.
            (Self::Packages(a), Self::None) => Self::Packages(a),
            (Self::None, Self::Packages(b)) => Self::Packages(b),
            // If both are `Packages`, the result is the union of the two.
            (Self::Packages(mut a), Self::Packages(b)) => {
                a.extend(b);
                Self::Packages(a)
            }
        }
    }

    /// Extend a [`NoBinary`] value with another.
    pub fn extend(&mut self, other: Self) {
        match (&mut *self, other) {
            // If either is `All`, the result is `All`.
            (Self::All, _) | (_, Self::All) => *self = Self::All,
            // If both are `None`, the result is `None`.
            (Self::None, Self::None) => {
                // Nothing to do.
            }
            // If one is `None`, the result is the other.
            (Self::Packages(_), Self::None) => {
                // Nothing to do.
            }
            (Self::None, Self::Packages(b)) => {
                // Take ownership of `b`.
                *self = Self::Packages(b);
            }
            // If both are `Packages`, the result is the union of the two.
            (Self::Packages(a), Self::Packages(b)) => {
                a.extend(b);
            }
        }
    }
}

impl NoBinary {
    /// Returns `true` if all wheels are allowed.
    pub fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub enum NoBuild {
    /// Allow building wheels from any source distribution.
    #[default]
    None,

    /// Do not allow building wheels from any source distribution.
    All,

    /// Do not allow building wheels from the given package's source distributions.
    Packages(Vec<PackageName>),
}

impl NoBuild {
    /// Determine the build strategy to use for the given arguments.
    pub fn from_args(only_binary: Vec<PackageNameSpecifier>, no_build: bool) -> Self {
        if no_build {
            Self::All
        } else {
            let combined = PackageNameSpecifiers::from_iter(only_binary.into_iter());
            match combined {
                PackageNameSpecifiers::All => Self::All,
                PackageNameSpecifiers::None => Self::None,
                PackageNameSpecifiers::Packages(packages) => Self::Packages(packages),
            }
        }
    }

    /// Determine the build strategy to use for the given argument.
    pub fn from_arg(no_build: PackageNameSpecifier) -> Self {
        Self::from_args(vec![no_build], false)
    }

    /// Combine a set of [`NoBuild`] values.
    #[must_use]
    pub fn combine(self, other: Self) -> Self {
        match (self, other) {
            // If both are `None`, the result is `None`.
            (Self::None, Self::None) => Self::None,
            // If either is `All`, the result is `All`.
            (Self::All, _) | (_, Self::All) => Self::All,
            // If one is `None`, the result is the other.
            (Self::Packages(a), Self::None) => Self::Packages(a),
            (Self::None, Self::Packages(b)) => Self::Packages(b),
            // If both are `Packages`, the result is the union of the two.
            (Self::Packages(mut a), Self::Packages(b)) => {
                a.extend(b);
                Self::Packages(a)
            }
        }
    }

    /// Extend a [`NoBuild`] value with another.
    pub fn extend(&mut self, other: Self) {
        match (&mut *self, other) {
            // If either is `All`, the result is `All`.
            (Self::All, _) | (_, Self::All) => *self = Self::All,
            // If both are `None`, the result is `None`.
            (Self::None, Self::None) => {
                // Nothing to do.
            }
            // If one is `None`, the result is the other.
            (Self::Packages(_), Self::None) => {
                // Nothing to do.
            }
            (Self::None, Self::Packages(b)) => {
                // Take ownership of `b`.
                *self = Self::Packages(b);
            }
            // If both are `Packages`, the result is the union of the two.
            (Self::Packages(a), Self::Packages(b)) => {
                a.extend(b);
            }
        }
    }
}

impl NoBuild {
    /// Returns `true` if all builds are allowed.
    pub fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use anyhow::Error;

    use super::*;

    #[test]
    fn no_build_from_args() -> Result<(), Error> {
        assert_eq!(
            NoBuild::from_args(vec![PackageNameSpecifier::from_str(":all:")?], false),
            NoBuild::All,
        );
        assert_eq!(
            NoBuild::from_args(vec![PackageNameSpecifier::from_str(":all:")?], true),
            NoBuild::All,
        );
        assert_eq!(
            NoBuild::from_args(vec![PackageNameSpecifier::from_str(":none:")?], true),
            NoBuild::All,
        );
        assert_eq!(
            NoBuild::from_args(vec![PackageNameSpecifier::from_str(":none:")?], false),
            NoBuild::None,
        );
        assert_eq!(
            NoBuild::from_args(
                vec![
                    PackageNameSpecifier::from_str("foo")?,
                    PackageNameSpecifier::from_str("bar")?
                ],
                false
            ),
            NoBuild::Packages(vec![
                PackageName::from_str("foo")?,
                PackageName::from_str("bar")?
            ]),
        );
        assert_eq!(
            NoBuild::from_args(
                vec![
                    PackageNameSpecifier::from_str("test")?,
                    PackageNameSpecifier::All
                ],
                false
            ),
            NoBuild::All,
        );
        assert_eq!(
            NoBuild::from_args(
                vec![
                    PackageNameSpecifier::from_str("foo")?,
                    PackageNameSpecifier::from_str(":none:")?,
                    PackageNameSpecifier::from_str("bar")?
                ],
                false
            ),
            NoBuild::Packages(vec![PackageName::from_str("bar")?]),
        );

        Ok(())
    }
}
