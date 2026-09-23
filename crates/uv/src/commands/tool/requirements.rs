use std::collections::BTreeMap;
use std::{iter, mem};

use indexmap::IndexMap;
use uv_distribution_types::{Requirement, RequirementSource};
use uv_pep440::{
    Operator, Version, VersionSpecifier, VersionSpecifiers, canonicalize_version_ranges,
};
use uv_pep508::MarkerTree;
use version_ranges::Ranges;

/// Compare normalized requirements, including precision-sensitive version clauses.
pub(super) fn requirements_equal(left: &[Requirement], right: &[Requirement]) -> bool {
    left.len() == right.len()
        && left.iter().zip(right).all(|(left, right)| {
            RequirementsKey::new(left.clone()) == RequirementsKey::new(right.clone())
        })
}

/// A requirement with registry constraints compared by accepted versions and prerelease policy.
#[derive(Eq, Hash, PartialEq)]
struct RequirementsKey {
    requirement: Requirement,
    range: Ranges<Version>,
    prerelease: bool,
}

impl RequirementsKey {
    fn new(mut requirement: Requirement) -> Self {
        let mut range = Ranges::full();
        let mut prerelease = false;
        if let RequirementSource::Registry { specifier, .. } = &mut requirement.source {
            for specifier in mem::take(specifier) {
                prerelease |= allows_prereleases(&specifier);
                range = range.intersection(&Ranges::from(specifier));
            }
        }
        Self {
            requirement,
            range: canonicalize_version_ranges(&range).unwrap_or(range),
            prerelease,
        }
    }
}

/// Merge requirements with the same source and scope across disjoint marker regions.
/// Extras are unioned and version constraints intersected wherever markers overlap.
/// Sort the result with the tool target first.
pub(super) fn normalize_requirements(requirements: Vec<Requirement>) -> Vec<Requirement> {
    let target = requirements.first().cloned();
    let mut sources = BTreeMap::<Requirement, Vec<Requirement>>::new();
    for mut requirement in requirements {
        let mut extras = requirement.extras.into_vec();
        extras.sort();
        extras.dedup();
        requirement.extras = extras.into_boxed_slice();
        requirement.groups.sort();
        let mut key = requirement.clone();
        key.extras = Box::new([]);
        key.marker = MarkerTree::TRUE;
        if let RequirementSource::Registry { specifier, .. } = &mut key.source {
            *specifier = VersionSpecifiers::empty();
        }
        sources.entry(key).or_default().push(requirement);
    }

    let mut normalized = Vec::new();
    for mut requirements in sources.into_values() {
        // Include specifier precision: `==1.*` and `==1.0.*` accept different versions,
        // even though `1` and `1.0` compare equal as PEP 440 versions.
        requirements.sort_by(|left, right| {
            left.cmp(right)
                .then_with(|| left.to_string().cmp(&right.to_string()))
        });
        let mut regions: Vec<Requirement> = Vec::new();
        for requirement in requirements {
            let mut remaining = requirement.marker;
            let mut next = Vec::new();
            for region in regions {
                let overlap = region.marker.and(requirement.marker);
                if overlap.is_false() {
                    next.push(region);
                    continue;
                }
                remaining = remaining.and(region.marker.negate());
                let outside = region.marker.and(requirement.marker.negate());
                if !outside.is_false() {
                    next.push(Requirement {
                        marker: outside,
                        ..region.clone()
                    });
                }
                let mut combined = region;
                combined.marker = overlap;
                let mut extras = combined.extras.into_vec();
                extras.extend_from_slice(&requirement.extras);
                extras.sort();
                extras.dedup();
                combined.extras = extras.into_boxed_slice();
                if let (
                    RequirementSource::Registry { specifier, .. },
                    RequirementSource::Registry {
                        specifier: other, ..
                    },
                ) = (&mut combined.source, &requirement.source)
                {
                    *specifier = mem::take(specifier)
                        .into_iter()
                        .chain(other.iter().cloned())
                        .collect();
                }
                next.push(combined);
            }
            if !remaining.is_false() {
                next.push(Requirement {
                    marker: remaining,
                    ..requirement
                });
            }
            regions = coalesce(next);
        }
        normalized.extend(regions);
    }

    // A false target marker should still identify the tool in its receipt.
    let target_name = target.as_ref().map(|target| target.name.clone());
    if let Some(target) = target
        && !normalized
            .iter()
            .any(|requirement| requirement.name == target.name)
    {
        normalized.push(target);
    }
    normalized.sort_by(|left, right| {
        (Some(&left.name) != target_name.as_ref(), left)
            .cmp(&(Some(&right.name) != target_name.as_ref(), right))
            .then_with(|| left.to_string().cmp(&right.to_string()))
    });
    normalized
}

/// Combine disjoint regions with identical extras and constraints.
fn coalesce(requirements: Vec<Requirement>) -> Vec<Requirement> {
    let mut combined = IndexMap::<RequirementsKey, Requirement>::new();
    for mut requirement in requirements {
        if let RequirementSource::Registry { specifier, .. } = &mut requirement.source {
            *specifier = simplify_specifiers(mem::take(specifier));
        }
        let mut key = requirement.clone();
        key.marker = MarkerTree::TRUE;
        combined
            .entry(RequirementsKey::new(key))
            .and_modify(|existing| existing.marker = existing.marker.or(requirement.marker))
            .or_insert(requirement);
    }
    combined.into_values().collect()
}

/// Remove clauses implied by the remaining constraints without changing prerelease opt-in.
fn simplify_specifiers(specifiers: VersionSpecifiers) -> VersionSpecifiers {
    let mut specifiers = specifiers
        .into_iter()
        .map(normalize_specifier)
        .collect::<Vec<_>>();
    specifiers.sort_by_cached_key(ToString::to_string);
    specifiers.dedup_by(|left, right| left.to_string() == right.to_string());

    // A pair of inclusive bounds can be an exact pin. Use PEP 440 ranges for this comparison,
    // including local versions; an ordinary numeric interval does not model `==` correctly.
    let range = specifiers.iter().fold(Ranges::full(), |range, specifier| {
        range.intersection(&Ranges::from(specifier.clone()))
    });
    let range = canonicalize_version_ranges(&range).unwrap_or(range);
    for specifier in &specifiers {
        let equal = normalize_specifier(VersionSpecifier::equals_version(
            specifier.version().clone(),
        ));
        let equal_range = Ranges::<Version>::from(equal.clone());
        if range == canonicalize_version_ranges(&equal_range).unwrap_or(equal_range) {
            let prerelease = if allows_prereleases(&equal) {
                None
            } else {
                specifiers.into_iter().find(allows_prereleases)
            };
            return iter::once(equal).chain(prerelease).collect();
        }
    }
    let mut index = 0;
    while index < specifiers.len() {
        let specifier = &specifiers[index];
        if allows_prereleases(specifier)
            && !specifiers
                .iter()
                .enumerate()
                .any(|(other_index, other)| other_index != index && allows_prereleases(other))
        {
            index += 1;
            continue;
        }
        let remaining = specifiers
            .iter()
            .enumerate()
            .filter(|(other_index, _)| *other_index != index)
            .fold(Ranges::full(), |range, (_, specifier)| {
                range.intersection(&Ranges::from(specifier.clone()))
            });
        let range = Ranges::<Version>::from(specifier.clone());
        let remaining = canonicalize_version_ranges(&remaining).unwrap_or(remaining);
        let range = canonicalize_version_ranges(&range).unwrap_or(range);
        if remaining.subset_of(&range) {
            specifiers.remove(index);
        } else {
            index += 1;
        }
    }
    specifiers.into_iter().collect()
}

/// Normalize insignificant trailing zeros, keeping precision-sensitive operators intact.
fn normalize_specifier(specifier: VersionSpecifier) -> VersionSpecifier {
    match specifier.operator() {
        Operator::EqualStar | Operator::NotEqualStar | Operator::TildeEqual => specifier,
        Operator::Equal
        | Operator::ExactEqual
        | Operator::NotEqual
        | Operator::LessThan
        | Operator::LessThanEqual
        | Operator::GreaterThan
        | Operator::GreaterThanEqual => {
            let release = specifier.version().release();
            if release.len() <= 1 || release.last() != Some(&0) {
                return specifier;
            }
            let mut release = release.to_vec();
            while release.len() > 1 && release.last() == Some(&0) {
                release.pop();
            }
            VersionSpecifier::from_version(
                *specifier.operator(),
                specifier.version().clone().with_release(release),
            )
            .unwrap_or(specifier)
        }
    }
}

fn allows_prereleases(specifier: &VersionSpecifier) -> bool {
    match specifier.operator() {
        Operator::NotEqual | Operator::NotEqualStar => false,
        Operator::Equal
        | Operator::EqualStar
        | Operator::ExactEqual
        | Operator::TildeEqual
        | Operator::LessThan
        | Operator::LessThanEqual
        | Operator::GreaterThan
        | Operator::GreaterThanEqual => specifier.any_prerelease(),
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use anyhow::Result;
    use insta::assert_snapshot;
    use uv_distribution_types::{Requirement, RequirementSource};
    use uv_pep440::{Version, VersionSpecifiers};
    use uv_pep508::MarkerTree;
    use uv_pypi_types::VerbatimParsedUrl;

    use super::{
        allows_prereleases, normalize_requirements, requirements_equal, simplify_specifiers,
    };

    fn requirements(inputs: &[&str]) -> Result<Vec<Requirement>> {
        inputs
            .iter()
            .map(|input| {
                Ok(Requirement::from(uv_pep508::Requirement::<
                    VerbatimParsedUrl,
                >::from_str(input)?))
            })
            .collect()
    }

    #[test]
    fn source_identity_and_order() -> Result<()> {
        let original = requirements(&[
            "z-tool",
            "b @ https://example.org/b.whl#sha256=1111",
            "a",
            "b @ https://example.org/b.whl#sha256=2222",
            "b @ https://EXAMPLE.ORG:443/./b.whl#sha256=1111",
            "z-tool",
        ])?;
        let normalized = normalize_requirements(original.clone());
        assert_snapshot!(normalized.iter().map(ToString::to_string).collect::<Vec<_>>().join("\n"), @"
        z-tool
        a
        b @ https://example.org/b.whl#sha256=1111
        b @ https://example.org/b.whl#sha256=2222
        ");
        let mut reordered = original;
        reordered[1..].reverse();
        assert!(requirements_equal(
            &normalized,
            &normalize_requirements(reordered)
        ));
        assert!(requirements_equal(
            &normalized,
            &normalize_requirements(normalized.clone())
        ));
        Ok(())
    }

    #[test]
    fn precision_sensitive_requirements() -> Result<()> {
        for (left, right) in [
            ("foo==1.*", "foo==1.0.*"),
            ("foo!=1.*", "foo!=1.0.*"),
            ("foo===1", "foo==1"),
            ("foo~=1.0", "foo~=1.0.0"),
        ] {
            assert!(
                !requirements_equal(
                    &normalize_requirements(requirements(&[left])?),
                    &normalize_requirements(requirements(&[right])?),
                ),
                "{left} != {right}"
            );
        }
        assert!(requirements_equal(
            &normalize_requirements(requirements(&["foo===1"])?),
            &normalize_requirements(requirements(&["foo===1.0"])?),
        ));
        assert!(requirements_equal(
            &normalize_requirements(requirements(&["foo>=1,<=1.0"])?),
            &normalize_requirements(requirements(&["foo==1.0"])?),
        ));
        assert!(requirements_equal(
            &normalize_requirements(requirements(&["foo~=1.2"])?),
            &normalize_requirements(requirements(&["foo>=1.2,<2"])?),
        ));
        let normalized = normalize_requirements(requirements(&[
            "foo~=1.2; python_version < '3.12'",
            "foo>=1.2,<2; python_version >= '3.12'",
        ])?);
        assert_eq!(normalized.len(), 1);
        assert_eq!(normalized[0].marker, MarkerTree::TRUE);
        Ok(())
    }

    #[test]
    fn redundant_specifiers() -> Result<()> {
        let inputs = [
            "<1,<2,<1",
            ">=1,>=2,<3,!=4",
            "~=1.2,>=1,<2",
            "==1.*,==1.0.*",
            "!=1.*,!=1.0.*",
            ">=1,<=1",
            "===1,===1.0",
            ">=1a1,>=1",
            ">=1a1,>=1b1,>=1",
            "!=1a1,>=1",
            ">1,!=1.post1",
            ">=1,!=1+local",
            ">=1!1,>=2",
            ">=2,<1,!=3",
            "~=1.0,~=1.0.0",
        ];
        let output = inputs
            .into_iter()
            .map(|input| {
                Ok(format!(
                    "{input} -> {}",
                    simplify_specifiers(input.parse()?)
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        assert_snapshot!(output.join("\n"), @"
        <1,<2,<1 -> <1
        >=1,>=2,<3,!=4 -> >=2, <3
        ~=1.2,>=1,<2 -> ~=1.2
        ==1.*,==1.0.* -> ==1.0.*
        !=1.*,!=1.0.* -> !=1.*
        >=1,<=1 -> ==1
        ===1,===1.0 -> ===1
        >=1a1,>=1 -> >=1a1, >=1
        >=1a1,>=1b1,>=1 -> >=1b1, >=1
        !=1a1,>=1 -> >=1
        >1,!=1.post1 -> >1
        >=1,!=1+local -> >=1, !=1+local
        >=1!1,>=2 -> >=1!1
        >=2,<1,!=3 -> <1, >=2
        ~=1.0,~=1.0.0 -> ~=1.0.0
        ");
        Ok(())
    }

    #[test]
    fn specifier_membership() -> Result<()> {
        let clauses = [
            "<1",
            "<=1",
            ">1",
            ">=1",
            "==1",
            "!=1",
            "==1.*",
            "==1.0.*",
            "!=1.*",
            "!=1.0.*",
            "~=1.0",
            "~=1.0.0",
            ">=1a1",
            "<1b1",
            "!=1a1",
            ">1.post1",
            "<1.post2",
            "==1+local",
            "!=1+local",
            "===1",
            "===1.0",
            "===1.0+local",
            ">=1!1",
            "<2!0",
            ">=0",
            "<0",
            "==0.*",
        ];
        let versions = [
            "0.dev0",
            "0",
            "0.9",
            "1.dev0",
            "1a1",
            "1b1",
            "1rc1",
            "1",
            "1.0",
            "1+local",
            "1.post0.dev0",
            "1.post0",
            "1.post1.dev1",
            "1.post1",
            "1.post1+local",
            "1.post2",
            "1.0.1",
            "1.1.dev0",
            "1.1",
            "2a1",
            "2",
            "1!0",
            "1!1",
            "2!0",
        ]
        .into_iter()
        .map(Version::from_str)
        .collect::<Result<Vec<_>, _>>()?;
        for left in clauses {
            for right in clauses {
                let original = VersionSpecifiers::from_str(&format!("{left},{right}"))?;
                let normalized = simplify_specifiers(original.clone());
                assert_eq!(
                    original.iter().any(allows_prereleases),
                    normalized.iter().any(allows_prereleases),
                    "{original} -> {normalized}"
                );
                for version in &versions {
                    assert_eq!(
                        original.contains(version),
                        normalized.contains(version),
                        "{original} -> {normalized} at {version}"
                    );
                }
                assert_eq!(
                    normalized.to_string(),
                    simplify_specifiers(normalized.clone()).to_string()
                );
            }
        }
        Ok(())
    }

    #[test]
    fn overlapping_markers() -> Result<()> {
        let original = requirements(&[
            "tool>=1",
            "tool[a]<3; python_version >= '3.11'",
            "tool[b]>=2; python_version < '3.13'",
            "tool[a]<3; python_version < '3.12'",
            "tool[c]; sys_platform == 'win32'",
            "tool[c]; sys_platform != 'win32'",
        ])?;
        let normalized = normalize_requirements(original.clone());
        let display = |requirements: &[Requirement]| {
            requirements
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n")
        };
        assert_snapshot!(display(&normalized), @"
        tool[a,b,c]>=2, <3 ; python_full_version < '3.13'
        tool[a,c]>=1, <3 ; python_full_version >= '3.13'
        ");
        assert_eq!(
            display(&normalized),
            display(&normalize_requirements(normalized.clone()))
        );
        let mut reordered = original.clone();
        reordered[1..].reverse();
        assert_eq!(
            display(&normalized),
            display(&normalize_requirements(reordered))
        );

        for version in ["0", "1", "1.5", "2", "3", "3.1"] {
            let version = Version::from_str(version)?;
            let excluded = |requirements: &[Requirement]| {
                requirements
                    .iter()
                    .fold(MarkerTree::FALSE, |marker, requirement| {
                        if let RequirementSource::Registry { specifier, .. } = &requirement.source
                            && !specifier.contains(&version)
                        {
                            marker.or(requirement.marker)
                        } else {
                            marker
                        }
                    })
            };
            assert_eq!(excluded(&original), excluded(&normalized), "{version}");
        }
        for extra in ["a", "b", "c"] {
            let activated = |requirements: &[Requirement]| {
                requirements
                    .iter()
                    .filter(|requirement| {
                        requirement.extras.iter().any(|name| name.as_str() == extra)
                    })
                    .fold(MarkerTree::FALSE, |marker, requirement| {
                        marker.or(requirement.marker)
                    })
            };
            assert_eq!(activated(&original), activated(&normalized), "{extra}");
        }
        Ok(())
    }
}
