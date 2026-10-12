use std::fmt::{Display, Formatter};
use std::{fmt, iter};

use serde::{Deserialize, Serialize};

use uv_pep440::{Version, VersionSpecifier, VersionSpecifiers};

/// The default version specifier when adding a dependency.
// While PEP 440 allows an arbitrary number of version digits, the `major` and `minor` build on
// most projects sticking to two or three components and a SemVer-ish versioning system, so can
// bump the major or minor version of a major.minor or major.minor.patch input version.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum AddBoundsKind {
    /// Only a lower bound, e.g., `>=1.2.3`.
    #[default]
    Lower,
    /// Allow the same major version, similar to the semver caret, e.g., `>=1.2.3, <2.0.0`.
    ///
    /// Leading zeroes are skipped, e.g. `>=0.1.2, <0.2.0`.
    Major,
    /// Allow the same minor version, similar to the semver tilde, e.g., `>=1.2.3, <1.3.0`.
    ///
    /// Leading zeroes are skipped, e.g. `>=0.1.2, <0.1.3`.
    Minor,
    /// Pin the exact version, e.g., `==1.2.3`.
    ///
    /// This option is not recommended, as versions are already pinned in the uv lockfile.
    Exact,
}

impl Display for AddBoundsKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Lower => write!(f, "lower"),
            Self::Major => write!(f, "major"),
            Self::Minor => write!(f, "minor"),
            Self::Exact => write!(f, "exact"),
        }
    }
}

impl AddBoundsKind {
    /// Return the version specifiers for this bound policy and a resolved version.
    pub fn specifiers(self, version: Version) -> VersionSpecifiers {
        // Nomenclature: "major" is the most significant component of the version, "minor" is the
        // second most significant component, so most versions are either major.minor.patch or
        // 0.major.minor.
        match self {
            Self::Lower => {
                VersionSpecifiers::from(VersionSpecifier::greater_than_equal_version(version))
            }
            Self::Major => {
                let leading_zeroes = version
                    .release()
                    .iter()
                    .take_while(|digit| **digit == 0)
                    .count();

                // Special case: The version is 0.
                if leading_zeroes == version.release().len() {
                    let upper_bound = Version::new(
                        [0, 1]
                            .into_iter()
                            .chain(iter::repeat_n(0, version.release().iter().skip(2).len())),
                    );
                    return VersionSpecifiers::from_iter([
                        VersionSpecifier::greater_than_equal_version(version),
                        VersionSpecifier::less_than_version(upper_bound),
                    ]);
                }

                // Compute the new major version and pad it to the same length:
                // 1.2.3 -> 2.0.0
                // 1.2 -> 2.0
                // 1 -> 2
                // We ignore leading zeroes, adding Semver-style semantics to 0.x versions, too:
                // 0.1.2 -> 0.2.0
                // 0.0.1 -> 0.0.2
                let major = version.release().get(leading_zeroes).copied().unwrap_or(0);
                // The length of the lower bound minus the leading zero and bumped component.
                let trailing_zeros = version.release().iter().skip(leading_zeroes + 1).len();
                let upper_bound = Version::new(
                    iter::repeat_n(0, leading_zeroes)
                        .chain(iter::once(major + 1))
                        .chain(iter::repeat_n(0, trailing_zeros)),
                );

                VersionSpecifiers::from_iter([
                    VersionSpecifier::greater_than_equal_version(version),
                    VersionSpecifier::less_than_version(upper_bound),
                ])
            }
            Self::Minor => {
                let leading_zeroes = version
                    .release()
                    .iter()
                    .take_while(|digit| **digit == 0)
                    .count();

                // Special case: The version is 0.
                if leading_zeroes == version.release().len() {
                    let upper_bound = [0, 0, 1]
                        .into_iter()
                        .chain(iter::repeat_n(0, version.release().iter().skip(3).len()));
                    return VersionSpecifiers::from_iter([
                        VersionSpecifier::greater_than_equal_version(version),
                        VersionSpecifier::less_than_version(Version::new(upper_bound)),
                    ]);
                }

                // If both major and minor version are 0, the concept of bumping the minor version
                // instead of the major version is not useful. Instead, we bump the next
                // non-zero part of the version. This avoids extending the three components of 0.0.1
                // to the four components of 0.0.1.1.
                if leading_zeroes >= 2 {
                    let most_significant =
                        version.release().get(leading_zeroes).copied().unwrap_or(0);
                    // The length of the lower bound minus the leading zero and bumped component.
                    let trailing_zeros = version.release().iter().skip(leading_zeroes + 1).len();
                    let upper_bound = Version::new(
                        iter::repeat_n(0, leading_zeroes)
                            .chain(iter::once(most_significant + 1))
                            .chain(iter::repeat_n(0, trailing_zeros)),
                    );
                    return VersionSpecifiers::from_iter([
                        VersionSpecifier::greater_than_equal_version(version),
                        VersionSpecifier::less_than_version(upper_bound),
                    ]);
                }

                // Compute the new minor version and pad it to the same length where possible:
                // 1.2.3 -> 1.3.0
                // 1.2 -> 1.3
                // 1 -> 1.1
                // We ignore leading zero, adding Semver-style semantics to 0.x versions, too:
                // 0.1.2 -> 0.1.3
                // 0.0.1 -> 0.0.2

                // If the version has only one digit, say `1`, or if there are only leading zeroes,
                // pad with zeroes.
                let major = version.release().get(leading_zeroes).copied().unwrap_or(0);
                let minor = version
                    .release()
                    .get(leading_zeroes + 1)
                    .copied()
                    .unwrap_or(0);
                let upper_bound = Version::new(
                    iter::repeat_n(0, leading_zeroes)
                        .chain(iter::once(major))
                        .chain(iter::once(minor + 1))
                        .chain(iter::repeat_n(
                            0,
                            version.release().iter().skip(leading_zeroes + 2).len(),
                        )),
                );

                VersionSpecifiers::from_iter([
                    VersionSpecifier::greater_than_equal_version(version),
                    VersionSpecifier::less_than_version(upper_bound),
                ])
            }
            Self::Exact => {
                VersionSpecifiers::from_iter([VersionSpecifier::equals_version(version)])
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use uv_pep440::Version;

    use super::AddBoundsKind;

    #[test]
    fn bound_kind_to_specifiers_exact() {
        let tests = [
            ("0", "==0"),
            ("0.0", "==0.0"),
            ("0.0.0", "==0.0.0"),
            ("0.1", "==0.1"),
            ("0.0.1", "==0.0.1"),
            ("0.0.0.1", "==0.0.0.1"),
            ("1.0.0", "==1.0.0"),
            ("1.2", "==1.2"),
            ("1.2.3", "==1.2.3"),
            ("1.2.3.4", "==1.2.3.4"),
            ("1.2.3.4a1.post1", "==1.2.3.4a1.post1"),
        ];

        for (version, expected) in tests {
            let actual = AddBoundsKind::Exact
                .specifiers(Version::from_str(version).unwrap())
                .to_string();
            assert_eq!(actual, expected, "{version}");
        }
    }

    #[test]
    fn bound_kind_to_specifiers_lower() {
        let tests = [
            ("0", ">=0"),
            ("0.0", ">=0.0"),
            ("0.0.0", ">=0.0.0"),
            ("0.1", ">=0.1"),
            ("0.0.1", ">=0.0.1"),
            ("0.0.0.1", ">=0.0.0.1"),
            ("1", ">=1"),
            ("1.0.0", ">=1.0.0"),
            ("1.2", ">=1.2"),
            ("1.2.3", ">=1.2.3"),
            ("1.2.3.4", ">=1.2.3.4"),
            ("1.2.3.4a1.post1", ">=1.2.3.4a1.post1"),
        ];

        for (version, expected) in tests {
            let actual = AddBoundsKind::Lower
                .specifiers(Version::from_str(version).unwrap())
                .to_string();
            assert_eq!(actual, expected, "{version}");
        }
    }

    #[test]
    fn bound_kind_to_specifiers_major() {
        let tests = [
            ("0", ">=0, <0.1"),
            ("0.0", ">=0.0, <0.1"),
            ("0.0.0", ">=0.0.0, <0.1.0"),
            ("0.0.0.0", ">=0.0.0.0, <0.1.0.0"),
            ("0.1", ">=0.1, <0.2"),
            ("0.0.1", ">=0.0.1, <0.0.2"),
            ("0.0.1.1", ">=0.0.1.1, <0.0.2.0"),
            ("0.0.0.1", ">=0.0.0.1, <0.0.0.2"),
            ("1", ">=1, <2"),
            ("1.0.0", ">=1.0.0, <2.0.0"),
            ("1.2", ">=1.2, <2.0"),
            ("1.2.3", ">=1.2.3, <2.0.0"),
            ("1.2.3.4", ">=1.2.3.4, <2.0.0.0"),
            ("1.2.3.4a1.post1", ">=1.2.3.4a1.post1, <2.0.0.0"),
        ];

        for (version, expected) in tests {
            let actual = AddBoundsKind::Major
                .specifiers(Version::from_str(version).unwrap())
                .to_string();
            assert_eq!(actual, expected, "{version}");
        }
    }

    #[test]
    fn bound_kind_to_specifiers_minor() {
        let tests = [
            ("0", ">=0, <0.0.1"),
            ("0.0", ">=0.0, <0.0.1"),
            ("0.0.0", ">=0.0.0, <0.0.1"),
            ("0.0.0.0", ">=0.0.0.0, <0.0.1.0"),
            ("0.1", ">=0.1, <0.1.1"),
            ("0.0.1", ">=0.0.1, <0.0.2"),
            ("0.0.1.1", ">=0.0.1.1, <0.0.2.0"),
            ("0.0.0.1", ">=0.0.0.1, <0.0.0.2"),
            ("1", ">=1, <1.1"),
            ("1.0.0", ">=1.0.0, <1.1.0"),
            ("1.2", ">=1.2, <1.3"),
            ("1.2.3", ">=1.2.3, <1.3.0"),
            ("1.2.3.4", ">=1.2.3.4, <1.3.0.0"),
            ("1.2.3.4a1.post1", ">=1.2.3.4a1.post1, <1.3.0.0"),
        ];

        for (version, expected) in tests {
            let actual = AddBoundsKind::Minor
                .specifiers(Version::from_str(version).unwrap())
                .to_string();
            assert_eq!(actual, expected, "{version}");
        }
    }
}
