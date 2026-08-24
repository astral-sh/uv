//! A library for python version numbers and specifiers, implementing
//! [PEP 440](https://peps.python.org/pep-0440)
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
#![warn(missing_docs)]

#[cfg(feature = "version-ranges")]
pub use version_ranges::{
    LowerBound, UpperBound, canonicalize_version_ranges, release_specifier_to_range,
    release_specifiers_to_ranges, strip_local_version_sentinels,
};
pub use {
    version::{
        BumpCommand, LocalSegment, LocalVersion, LocalVersionSlice, MIN_VERSION, Operator,
        OperatorParseError, Prerelease, PrereleaseKind, Version, VersionParseError, VersionPattern,
        VersionPatternParseError,
    },
    version_specifier::{
        TildeVersionSpecifier, VersionSpecifier, VersionSpecifierBuildError, VersionSpecifiers,
        VersionSpecifiersParseError,
    },
};

mod version;
mod version_specifier;

#[cfg(feature = "version-ranges")]
mod version_ranges;

#[cfg(test)]
mod tests {
    use super::{Version, VersionSpecifier, VersionSpecifiers};
    use std::str::FromStr;

    #[test]
    fn test_version() {
        let version = Version::from_str("1.19").unwrap();
        let version_specifier = VersionSpecifier::from_str("== 1.*").unwrap();
        assert!(version_specifier.contains(&version));
        let version_specifiers = VersionSpecifiers::from_str(">=1.16, <2.0").unwrap();
        assert!(version_specifiers.contains(&version));
    }

    #[test]
    fn test_version_helpers() {
        let v = Version::from_str("3.11.4").unwrap();
        assert_eq!(v.major(), Some(3));
        assert_eq!(v.minor(), Some(11));
        assert_eq!(v.patch(), Some(4));
        assert!(v.is_stable());
        assert!(!v.is_prerelease());
        assert!(!v.is_postrelease());
        assert!(!v.is_devrelease());

        let v_pre = Version::from_str("3.12.0a1").unwrap();
        assert_eq!(v_pre.major(), Some(3));
        assert_eq!(v_pre.minor(), Some(12));
        assert_eq!(v_pre.patch(), Some(0));
        assert!(!v_pre.is_stable());
        assert!(v_pre.is_prerelease());
        assert!(!v_pre.is_postrelease());
        assert!(!v_pre.is_devrelease());

        let v_post = Version::from_str("1.0.post1").unwrap();
        assert!(v_post.is_postrelease());
        assert!(!v_post.is_prerelease());

        let v_dev = Version::from_str("2.0.dev0").unwrap();
        assert!(v_dev.is_devrelease());
        assert!(v_dev.is_prerelease());
        assert!(!v_dev.is_stable());

        let v_single = Version::from_str("42").unwrap();
        assert_eq!(v_single.major(), Some(42));
        assert_eq!(v_single.minor(), None);
        assert_eq!(v_single.patch(), None);
    }
}
