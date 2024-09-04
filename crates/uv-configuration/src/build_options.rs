use std::fmt::{Display, Formatter};

use pep508_rs::PackageName;

use crate::{PackageNameSpecifier, PackageNameSpecifiers};

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum BuildKind {
    /// A PEP 517 wheel build.
    #[default]
    Wheel,
    /// A PEP 517 source distribution build.
    Sdist,
    /// A PEP 660 editable installation wheel build.
    Editable,
}

impl Display for BuildKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Wheel => f.write_str("wheel"),
            Self::Sdist => f.write_str("sdist"),
            Self::Editable => f.write_str("editable"),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum BuildOutput {
    /// Send the build backend output to `stderr`.
    Stderr,
    /// Send the build backend output to `tracing`.
    Debug,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct BuildOptions {
    no_binary: NoBinary,
    no_build: NoBuild,
}

impl BuildOptions {
    pub fn new(no_binary: NoBinary, no_build: NoBuild) -> Self {
        Self {
            no_binary,
            no_build,
        }
    }

    #[must_use]
    pub fn combine(self, no_binary: NoBinary, no_build: NoBuild) -> Self {
        Self {
            no_binary: self.no_binary.combine(no_binary),
            no_build: self.no_build.combine(no_build),
        }
    }

    pub fn no_binary_package(&self, package_name: &PackageName) -> bool {
        match &self.no_binary {
            NoBinary::None => false,
            NoBinary::All => match &self.no_build {
                // Allow `all` to be overridden by specific build exclusions
                NoBuild::Packages(packages) => !packages.contains(package_name),
                _ => true,
            },
            NoBinary::Packages(packages) => packages.contains(package_name),
        }
    }

    pub fn no_build_package(&self, package_name: &PackageName) -> bool {
        match &self.no_build {
            NoBuild::All => match &self.no_binary {
                // Allow `all` to be overridden by specific binary exclusions
                NoBinary::Packages(packages) => !packages.contains(package_name),
                _ => true,
            },
            NoBuild::None => false,
            NoBuild::Packages(packages) => packages.contains(package_name),
        }
    }

    pub fn no_build_requirement(&self, package_name: Option<&PackageName>) -> bool {
        match package_name {
            Some(name) => self.no_build_package(name),
            None => self.no_build_all(),
        }
    }

    pub fn no_binary_requirement(&self, package_name: Option<&PackageName>) -> bool {
        match package_name {
            Some(name) => self.no_binary_package(name),
            None => self.no_binary_all(),
        }
    }

    pub fn no_build_all(&self) -> bool {
        matches!(self.no_build, NoBuild::All)
    }

    pub fn no_binary_all(&self) -> bool {
        matches!(self.no_binary, NoBinary::All)
    }

    /// Return the [`NoBuild`] strategy to use.
    pub fn no_build(&self) -> &NoBuild {
        &self.no_build
    }

    /// Return the [`NoBinary`] strategy to use.
    pub fn no_binary(&self) -> &NoBinary {
        &self.no_binary
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
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
    pub fn from_args(no_binary: Option<bool>, no_binary_package: Vec<PackageName>) -> Self {
        match no_binary {
            Some(true) => Self::All,
            Some(false) => Self::None,
            None => {
                if no_binary_package.is_empty() {
                    Self::None
                } else {
                    Self::Packages(no_binary_package)
                }
            }
        }
    }

    /// Determine the binary installation strategy to use for the given arguments from the pip CLI.
    pub fn from_pip_args(no_binary: Vec<PackageNameSpecifier>) -> Self {
        let combined = PackageNameSpecifiers::from_iter(no_binary.into_iter());
        match combined {
            PackageNameSpecifiers::All => Self::All,
            PackageNameSpecifiers::None => Self::None,
            PackageNameSpecifiers::Packages(packages) => Self::Packages(packages),
        }
    }

    /// Determine the binary installation strategy to use for the given argument from the pip CLI.
    pub fn from_pip_arg(no_binary: PackageNameSpecifier) -> Self {
        Self::from_pip_args(vec![no_binary])
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

#[derive(Debug, Default, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
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
    pub fn from_args(no_build: Option<bool>, no_build_package: Vec<PackageName>) -> Self {
        match no_build {
            Some(true) => Self::All,
            Some(false) => Self::None,
            None => {
                if no_build_package.is_empty() {
                    Self::None
                } else {
                    Self::Packages(no_build_package)
                }
            }
        }
    }

    /// Determine the build strategy to use for the given arguments from the pip CLI.
    pub fn from_pip_args(only_binary: Vec<PackageNameSpecifier>, no_build: bool) -> Self {
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

    /// Determine the build strategy to use for the given argument from the pip CLI.
    pub fn from_pip_arg(no_build: PackageNameSpecifier) -> Self {
        Self::from_pip_args(vec![no_build], false)
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

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum IndexStrategy {
    /// Only use results from the first index that returns a match for a given package name.
    ///
    /// While this differs from pip's behavior, it's the default index strategy as it's the most
    /// secure.
    #[default]
    #[cfg_attr(feature = "clap", clap(alias = "first-match"))]
    FirstIndex,
    /// Search for every package name across all indexes, exhausting the versions from the first
    /// index before moving on to the next.
    ///
    /// In this strategy, we look for every package across all indexes. When resolving, we attempt
    /// to use versions from the indexes in order, such that we exhaust all available versions from
    /// the first index before moving on to the next. Further, if a version is found to be
    /// incompatible in the first index, we do not reconsider that version in subsequent indexes,
    /// even if the secondary index might contain compatible versions (e.g., variants of the same
    /// versions with different ABI tags or Python version constraints).
    ///
    /// See: <https://peps.python.org/pep-0708/>
    #[cfg_attr(feature = "clap", clap(alias = "unsafe-any-match"))]
    #[serde(alias = "unsafe-any-match")]
    UnsafeFirstMatch,
    /// Search for every package name across all indexes, preferring the "best" version found. If a
    /// package version is in multiple indexes, only look at the entry for the first index.
    ///
    /// In this strategy, we look for every package across all indexes. When resolving, we consider
    /// all versions from all indexes, choosing the "best" version found (typically, the highest
    /// compatible version).
    ///
    /// This most closely matches pip's behavior, but exposes the resolver to "dependency confusion"
    /// attacks whereby malicious actors can publish packages to public indexes with the same name
    /// as internal packages, causing the resolver to install the malicious package in lieu of
    /// the intended internal package.
    ///
    /// See: <https://peps.python.org/pep-0708/>
    UnsafeBestMatch,
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use anyhow::Error;

    use super::*;

    #[test]
    fn no_build_from_args() -> Result<(), Error> {
        assert_eq!(
            NoBuild::from_pip_args(vec![PackageNameSpecifier::from_str(":all:")?], false),
            NoBuild::All,
        );
        assert_eq!(
            NoBuild::from_pip_args(vec![PackageNameSpecifier::from_str(":all:")?], true),
            NoBuild::All,
        );
        assert_eq!(
            NoBuild::from_pip_args(vec![PackageNameSpecifier::from_str(":none:")?], true),
            NoBuild::All,
        );
        assert_eq!(
            NoBuild::from_pip_args(vec![PackageNameSpecifier::from_str(":none:")?], false),
            NoBuild::None,
        );
        assert_eq!(
            NoBuild::from_pip_args(
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
            NoBuild::from_pip_args(
                vec![
                    PackageNameSpecifier::from_str("test")?,
                    PackageNameSpecifier::All
                ],
                false
            ),
            NoBuild::All,
        );
        assert_eq!(
            NoBuild::from_pip_args(
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
