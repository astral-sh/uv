use uv_pep440::{Operator, Version, VersionSpecifier, VersionSpecifierBuildError};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Constraint must be a tilde equal specifier, got {0:?}")]
    InvalidConstraint(Operator),
    #[error(transparent)]
    VersionSpecifierBuildError(#[from] VersionSpecifierBuildError),
}

/// Upgrades a tilde equal (~=) version specifier to the closest fix candidate, if possible.
pub fn upgrade_version_specifier(
    constraint: &VersionSpecifier,
    fix_candidates: &[Version],
) -> Result<Option<VersionSpecifier>, Error> {
    if constraint.operator() != &Operator::TildeEqual {
        return Err(Error::InvalidConstraint(constraint.operator().clone()));
    }

    // Relax the bounds of the constraint to allow upgrading the minor version if possible
    let relaxed_constraint = constraint.only_minor_release();

    // Filter only to versions that satisfy the constraint
    let elligible_versions = fix_candidates
        .into_iter()
        .filter(|version| relaxed_constraint.contains(version));

    // The minimum elligible version will be the closest to the existing current_version
    match elligible_versions.min() {
        Some(new_version) => Ok(Some(VersionSpecifier::from_version(
            Operator::TildeEqual,
            new_version.clone(),
        )?)),
        _ => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use uv_pep440::{Version, VersionSpecifier};

    use super::upgrade_version_specifier;

    #[test]
    fn test_semver_patch_compatible() {
        // Should choose the minimum 1.2.4
        let constraint = VersionSpecifier::from_str("~=1.2.3").unwrap();
        let fix_candidates = [
            Version::from_str("1.3.0").unwrap(),
            Version::from_str("2.0.0").unwrap(),
            Version::from_str("1.2.4").unwrap(),
        ];

        let result = upgrade_version_specifier(&constraint, &fix_candidates).unwrap();
        assert_eq!(result, Some(VersionSpecifier::from_str("~=1.2.4").unwrap()));
    }

    #[test]
    fn test_semver_minor_compatible() {
        // Should still work as the minor version 1.3.0 is available
        let constraint = VersionSpecifier::from_str("~=1.2.3").unwrap();
        let fix_candidates = [
            Version::from_str("1.3.0").unwrap(),
            Version::from_str("2.0.0").unwrap(),
        ];

        let result = upgrade_version_specifier(&constraint, &fix_candidates).unwrap();
        assert_eq!(result, Some(VersionSpecifier::from_str("~=1.3.0").unwrap()));
    }

    #[test]
    fn test_semver_major_compatible() {
        // Should return None as the major version 2.0.0 is not compatible with ~=1.2.3
        let constraint = VersionSpecifier::from_str("~=1.2.3").unwrap();
        let fix_candidates = [Version::from_str("2.0.0").unwrap()];

        let result = upgrade_version_specifier(&constraint, &fix_candidates).unwrap();
        assert!(result.is_none());
    }
}
