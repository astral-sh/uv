use uv_pep440::{Operator, VersionSpecifiers};

use pubgrub::Ranges;

use crate::pubgrub::{PrereleasePreference, Range};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum PrereleaseMode {
    /// Disallow all pre-release versions.
    Disallow,

    /// Allow all pre-release versions.
    Allow,

    /// Allow pre-release versions when no stable candidate satisfies the active constraints.
    IfNecessary,

    /// Allow pre-release versions when no stable candidate satisfies the active constraints, or
    /// when an active direct or transitive requirement contains an explicit pre-release marker.
    #[default]
    #[serde(alias = "explicit")]
    #[cfg_attr(feature = "clap", value(alias("explicit")))]
    IfNecessaryOrExplicit,
}

impl std::fmt::Display for PrereleaseMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Disallow => write!(f, "disallow"),
            Self::Allow => write!(f, "allow"),
            Self::IfNecessary => write!(f, "if-necessary"),
            Self::IfNecessaryOrExplicit => write!(f, "if-necessary-or-explicit"),
        }
    }
}

impl PrereleaseMode {
    /// Expand a structural version range into PubGrub's pre-release preference dimensions.
    pub(crate) fn range<T: Clone>(
        self,
        versions: Ranges<T>,
        explicit_prerelease: bool,
    ) -> Range<T> {
        match (self, explicit_prerelease) {
            (Self::Disallow | Self::IfNecessary, _) => Range::prefer_stable(versions),
            (Self::Allow, _) | (Self::IfNecessaryOrExplicit, true) => Range::allow(versions),
            (Self::IfNecessaryOrExplicit, false) => Range::both(versions),
        }
    }

    /// Return the candidate ordering for one preference dimension.
    pub(crate) fn selection(self, preference: PrereleasePreference) -> PrereleaseSelection {
        match (self, preference) {
            (Self::Disallow, _) => PrereleaseSelection::Disallow,
            (Self::Allow, _) | (_, PrereleasePreference::Allow) => PrereleaseSelection::Allow,
            (
                Self::IfNecessary | Self::IfNecessaryOrExplicit,
                PrereleasePreference::PreferStable,
            ) => PrereleaseSelection::PreferStable,
        }
    }
}

/// Returns `true` if the specifiers explicitly mention a pre-release version.
///
/// Exclusions do not opt a package into pre-releases. For example, `!=1.0a1` should not change
/// which candidate kinds are considered.
pub(crate) fn contains_prerelease(specifiers: &VersionSpecifiers) -> bool {
    specifiers
        .iter()
        .filter(|specifier| {
            !matches!(
                specifier.operator(),
                Operator::NotEqual | Operator::NotEqualStar
            )
        })
        .any(uv_pep440::VersionSpecifier::any_prerelease)
}

/// How pre-release candidates participate in version selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PrereleaseSelection {
    /// Do not consider pre-release candidates.
    Disallow,
    /// Consider stable and pre-release candidates in normal version order.
    Allow,
    /// Prefer stable candidates, falling back to pre-releases only after stable candidates are
    /// exhausted.
    PreferStable,
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    #[cfg(feature = "clap")]
    use clap::ValueEnum;
    use pubgrub::{Ranges, VersionSet};
    use uv_pep440::VersionSpecifiers;

    use super::{PrereleaseMode, contains_prerelease};
    use crate::pubgrub::{PrereleasePreference, PubGrubVersion};

    #[test]
    fn default_range_uses_preference_dimensions() {
        let ordinary = PrereleaseMode::default().range(Ranges::singleton(1), false);
        assert!(ordinary.contains(&PubGrubVersion::new(PrereleasePreference::PreferStable, 1,)));
        assert!(ordinary.contains(&PubGrubVersion::new(PrereleasePreference::Allow, 1,)));

        let explicit = PrereleaseMode::default().range(Ranges::singleton(1), true);
        assert!(!explicit.contains(&PubGrubVersion::new(PrereleasePreference::PreferStable, 1,)));
        assert!(explicit.contains(&PubGrubVersion::new(PrereleasePreference::Allow, 1,)));
    }

    #[test]
    fn exclusion_does_not_enable_prereleases() {
        assert!(!contains_prerelease(
            &VersionSpecifiers::from_str("!=2.0a1").expect("valid version specifier")
        ));
        assert!(contains_prerelease(
            &VersionSpecifiers::from_str(">=2.0a1").expect("valid version specifier")
        ));
    }

    #[test]
    fn legacy_explicit_mode_deserializes_to_default() {
        let mode = serde_json::from_str::<PrereleaseMode>(r#""explicit""#)
            .expect("legacy pre-release mode should deserialize");
        assert_eq!(mode, PrereleaseMode::IfNecessaryOrExplicit);
        assert_eq!(
            serde_json::to_string(&mode).expect("pre-release mode should serialize"),
            r#""if-necessary-or-explicit""#
        );
    }

    #[cfg(feature = "clap")]
    #[test]
    fn legacy_explicit_mode_is_a_cli_alias_for_default() {
        assert_eq!(
            PrereleaseMode::from_str("explicit", false).expect("legacy CLI value should parse"),
            PrereleaseMode::IfNecessaryOrExplicit
        );
    }
}
