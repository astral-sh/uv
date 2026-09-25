//! Normalize dependency declarations for lockfile serialization and semantic comparison.
//!
//! Each collection retains the declarations that affect its behavior: false overrides suppress
//! dependencies, standalone pins permit yanked versions, and build hashes restrict allowed artifacts.

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::ops::Deref;
use std::{iter, mem};

use indexmap::IndexMap;
use uv_distribution_types::{NameRequirementSpecification, Requirement, RequirementSource};
use uv_pep440::{
    Operator, Version, VersionSpecifier, VersionSpecifiers, canonicalize_version_ranges,
};
use uv_pep508::MarkerTree;
use version_ranges::Ranges;

use crate::{ExcludeDependency, Excludes, Override, PackageOverride, PackageOverrideTarget};

/// Requirements with equivalent declarations combined.
///
/// False markers remain because overrides can replace them before resolution.
#[derive(Debug, Clone, Eq)]
pub struct NormalizedRequirements(Vec<Requirement>);

impl NormalizedRequirements {
    pub fn into_inner(self) -> Vec<Requirement> {
        self.0
    }
}

impl From<Vec<Requirement>> for NormalizedRequirements {
    fn from(requirements: Vec<Requirement>) -> Self {
        Self(normalize(requirements))
    }
}

/// Compare registry constraints by accepted versions, prerelease opt-in, and yanked-version
/// eligibility. All other requirement fields use [`Requirement`]'s equality.
impl PartialEq for NormalizedRequirements {
    fn eq(&self, other: &Self) -> bool {
        self.0.len() == other.0.len()
            && self
                .0
                .iter()
                .zip(&other.0)
                .all(|(left, right)| SemanticRequirement(left) == SemanticRequirement(right))
    }
}

impl Deref for NormalizedRequirements {
    type Target = [Requirement];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Constraints compared by their restrictions, without extras or empty declarations.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct NormalizedConstraints(NormalizedRequirements);

impl NormalizedConstraints {
    pub fn into_inner(self) -> Vec<Requirement> {
        self.0.into_inner()
    }
}

impl From<Vec<Requirement>> for NormalizedConstraints {
    /// Normalize constraints, dropping false markers and empty declarations and ignoring extras.
    fn from(mut constraints: Vec<Requirement>) -> Self {
        constraints.retain_mut(|requirement| {
            if let RequirementSource::Registry { specifier, .. } = &requirement.source
                && specifier.is_empty()
            {
                return false;
            }
            requirement.extras = Box::new([]);
            !requirement.marker.is_false()
        });
        Self(NormalizedRequirements::from(constraints))
    }
}

impl Deref for NormalizedConstraints {
    type Target = [Requirement];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Replacement requirements, including false markers that suppress the original dependency.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct NormalizedOverrides(NormalizedRequirements);

impl NormalizedOverrides {
    pub fn into_inner(self) -> Vec<Requirement> {
        self.0.into_inner()
    }
}

impl From<Vec<Requirement>> for NormalizedOverrides {
    /// Normalize replacements without dropping false markers that suppress dependencies.
    fn from(overrides: Vec<Requirement>) -> Self {
        Self(NormalizedRequirements::from(overrides))
    }
}

impl Deref for NormalizedOverrides {
    type Target = [Requirement];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Overrides normalized independently within their global or package-version scope.
///
/// Empty package scopes remain because they shadow versionless scopes.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct NormalizedOverrideEntries {
    global: NormalizedOverrides,
    scoped: BTreeMap<PackageOverrideTarget, NormalizedOverrides>,
}

impl From<Vec<Override<Requirement>>> for NormalizedOverrideEntries {
    fn from(entries: Vec<Override<Requirement>>) -> Self {
        let mut global = Vec::new();
        let mut scoped = BTreeMap::<PackageOverrideTarget, Vec<Requirement>>::new();
        for entry in entries {
            match entry {
                Override::Requirement(requirement) => global.push(requirement),
                Override::Package(package) => {
                    scoped
                        .entry(package.package)
                        .or_default()
                        .extend(package.dependencies);
                }
            }
        }
        Self {
            global: NormalizedOverrides::from(global),
            scoped: scoped
                .into_iter()
                .map(|(package, requirements)| (package, NormalizedOverrides::from(requirements)))
                .collect(),
        }
    }
}

impl NormalizedOverrideEntries {
    /// Return global overrides followed by one declaration per package scope, including empty scopes.
    pub fn into_inner(self) -> Vec<Override<Requirement>> {
        self.global
            .into_inner()
            .into_iter()
            .map(Override::Requirement)
            .chain(self.scoped.into_iter().map(|(package, dependencies)| {
                Override::Package(PackageOverride {
                    package,
                    dependencies: dependencies.into_inner().into_boxed_slice(),
                })
            }))
            .collect()
    }
}

/// Exclusions sorted and deduplicated within each package and version scope.
///
/// Empty version-specific scopes remain because they shadow versionless exclusions.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct NormalizedExcludes(Vec<ExcludeDependency>);

impl NormalizedExcludes {
    pub fn into_inner(self) -> Vec<ExcludeDependency> {
        self.0
    }
}

impl From<Vec<ExcludeDependency>> for NormalizedExcludes {
    /// Combine duplicate exclusions within each package and version scope.
    fn from(excludes: Vec<ExcludeDependency>) -> Self {
        Self(Excludes::from_entries(excludes).into_entries())
    }
}

impl Deref for NormalizedExcludes {
    type Target = [ExcludeDependency];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Build constraints with equivalent hashless declarations combined and hashes retained.
#[derive(Debug, Clone, Eq)]
pub struct NormalizedBuildConstraints(Vec<NameRequirementSpecification>);

impl NormalizedBuildConstraints {
    pub fn into_inner(self) -> Vec<NameRequirementSpecification> {
        self.0
    }
}

impl From<Vec<NameRequirementSpecification>> for NormalizedBuildConstraints {
    /// Normalize hashless registry declarations and sort hash-bearing declarations.
    ///
    /// Hash-bearing declarations remain separate so hash validation can intersect their allowed
    /// artifacts. URL fragments can also carry hashes, so only hashless registry entries combine.
    ///
    /// Bare constraints remain because hash validation checks for unpinned declarations.
    fn from(constraints: Vec<NameRequirementSpecification>) -> Self {
        let mut normalized = Vec::new();
        let mut unhashed = Vec::new();
        for mut constraint in constraints {
            if constraint.requirement.marker.is_false() {
                continue;
            }
            constraint.requirement.extras = Box::new([]);
            if constraint.hashes.is_empty()
                && let RequirementSource::Registry { .. } = &constraint.requirement.source
            {
                unhashed.push(constraint.requirement);
                continue;
            }
            if let RequirementSource::Registry { specifier, .. } =
                &mut constraint.requirement.source
            {
                *specifier = simplify_specifiers(mem::take(specifier));
            }
            constraint.requirement.groups.sort();
            constraint.hashes.sort();
            constraint.hashes.dedup();
            normalized.push(constraint);
        }
        normalized.extend(
            normalize(unhashed)
                .into_iter()
                .map(NameRequirementSpecification::from),
        );
        normalized.sort_by(|left, right| {
            compare_requirements(&left.requirement, &right.requirement)
                .then_with(|| left.hashes.cmp(&right.hashes))
        });
        normalized.dedup_by(|left, right| {
            left.hashes == right.hashes
                && SemanticRequirement(&left.requirement) == SemanticRequirement(&right.requirement)
        });
        Self(normalized)
    }
}

impl PartialEq for NormalizedBuildConstraints {
    fn eq(&self, other: &Self) -> bool {
        self.len() == other.len()
            && self.iter().zip(other.iter()).all(|(left, right)| {
                left.hashes == right.hashes
                    && SemanticRequirement(&left.requirement)
                        == SemanticRequirement(&right.requirement)
            })
    }
}

impl Deref for NormalizedBuildConstraints {
    type Target = [NameRequirementSpecification];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// A requirement compared by accepted versions and prerelease and yanked-version policies.
#[derive(Eq)]
struct SemanticRequirement<'a>(&'a Requirement);

impl PartialEq for SemanticRequirement<'_> {
    fn eq(&self, other: &Self) -> bool {
        let left = self.0;
        let right = other.0;
        // Compare identical declarations without allocating semantic keys. Version equality ignores
        // release precision, which can affect wildcard and compatible clauses.
        if left == right
            && if let (
                RequirementSource::Registry {
                    specifier: left, ..
                },
                RequirementSource::Registry {
                    specifier: right, ..
                },
            ) = (&left.source, &right.source)
            {
                left.iter().zip(right.iter()).all(|(left, right)| {
                    left.version().release().len() == right.version().release().len()
                })
            } else {
                true
            }
        {
            return true;
        }
        RequirementsKey::new(left.clone()) == RequirementsKey::new(right.clone())
    }
}

/// A requirement compared by accepted versions and prerelease and yanked-version policies.
#[derive(Eq, Hash, PartialEq)]
struct RequirementsKey {
    requirement: Requirement,
    range: Ranges<Version>,
    prerelease: bool,
    yanked: bool,
}

impl RequirementsKey {
    /// Replace registry specifiers with their accepted range for comparison.
    /// Track prerelease and yanked-version policies separately from the accepted range.
    fn new(mut requirement: Requirement) -> Self {
        let mut range = Ranges::full();
        let mut prerelease = false;
        let mut yanked = false;
        if let RequirementSource::Registry { specifier, .. } = &mut requirement.source {
            yanked = allows_yanked(specifier.iter());
            for specifier in mem::take(specifier) {
                prerelease |= allows_prereleases(&specifier);
                range = range.intersection(&Ranges::from(specifier));
            }
        }
        Self {
            requirement,
            range: canonicalize_version_ranges(&range).unwrap_or(range),
            prerelease,
            yanked,
        }
    }
}

/// Merge requirements with the same source and scope across disjoint marker regions.
///
/// Extras are unioned and version constraints intersected wherever markers overlap.
/// Standalone pins stay separate because they permit yanked versions.
/// False requirements and overrides remain because overrides can replace their markers.
fn normalize(mut requirements: Vec<Requirement>) -> Vec<Requirement> {
    for requirement in &mut requirements {
        let mut extras = mem::take(&mut requirement.extras).into_vec();
        extras.sort();
        extras.dedup();
        requirement.extras = extras.into_boxed_slice();
        requirement.groups.sort();
    }
    requirements.sort_by(compare_requirements);

    let mut normalized = Vec::with_capacity(requirements.len());
    let mut requirements = requirements.into_iter().peekable();
    while let Some(mut requirement) = requirements.next() {
        if requirements
            .peek()
            .is_none_or(|next| next.name != requirement.name)
        {
            if let RequirementSource::Registry { specifier, .. } = &mut requirement.source {
                *specifier = simplify_specifiers(mem::take(specifier));
            }
            normalized.push(requirement);
        } else {
            let name = requirement.name.clone();
            normalized.extend(normalize_package_requirements(
                iter::once(requirement).chain(iter::from_fn(|| {
                    requirements.next_if(|requirement| requirement.name == name)
                })),
            ));
        }
    }
    normalized.sort_by(compare_requirements);
    normalized
}

/// Merge declarations for a package that occurs more than once in the input.
fn normalize_package_requirements(
    requirements: impl IntoIterator<Item = Requirement>,
) -> Vec<Requirement> {
    let mut sources = BTreeMap::<Requirement, Vec<Requirement>>::new();
    for requirement in requirements {
        let mut key = requirement.clone();
        key.extras = Box::new([]);
        // Overrides retain a dependency's top-level extra condition. Combining different extra
        // markers can change that condition, so only merge declarations with identical markers
        // when they mention extras.
        if key.marker.without_extras() == key.marker {
            key.marker = MarkerTree::TRUE;
        }
        if let RequirementSource::Registry { specifier, .. } = &mut key.source
            && !allows_yanked(specifier.iter())
        {
            *specifier = VersionSpecifiers::empty();
        }
        sources.entry(key).or_default().push(requirement);
    }

    let mut normalized = Vec::new();
    for requirements in sources.into_values() {
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
                    && !allows_yanked(specifier.iter())
                {
                    *specifier = mem::take(specifier)
                        .into_iter()
                        .chain(other.iter().cloned())
                        .collect();
                }
                next.push(combined);
            }
            if !remaining.is_false() || requirement.marker.is_false() {
                next.push(Requirement {
                    marker: remaining,
                    ..requirement
                });
            }
            regions = coalesce(next);
        }
        normalized.extend(regions);
    }

    normalized
}

/// Order declarations deterministically, including precision-sensitive specifiers.
/// `==1.*` and `==1.0.*` accept different versions even though `1` and `1.0` compare equal.
fn compare_requirements(left: &Requirement, right: &Requirement) -> Ordering {
    left.cmp(right)
        .then_with(|| left.to_string().cmp(&right.to_string()))
}

/// Combine disjoint marker regions with equivalent extras, accepted versions, and candidate policies.
/// Keep the first region's simplified specifiers when multiple forms accept the same versions.
fn coalesce(mut requirements: Vec<Requirement>) -> Vec<Requirement> {
    for requirement in &mut requirements {
        if let RequirementSource::Registry { specifier, .. } = &mut requirement.source {
            *specifier = simplify_specifiers(mem::take(specifier));
        }
    }
    if requirements.len() <= 1 {
        return requirements;
    }

    let mut combined = IndexMap::<RequirementsKey, Requirement>::new();
    for requirement in requirements {
        let mut key = requirement.clone();
        key.marker = MarkerTree::TRUE;
        combined
            .entry(RequirementsKey::new(key))
            .and_modify(|existing| existing.marker = existing.marker.or(requirement.marker))
            .or_insert(requirement);
    }
    combined.into_values().collect()
}

/// Remove implied clauses without changing prerelease opt-in or yanked-version eligibility.
/// For example, `>=1rc1,>=1` keeps both clauses because the first opts into prereleases.
/// Likewise, `==1,>=0` keeps both clauses because a standalone pin would permit yanked versions.
fn simplify_specifiers(specifiers: VersionSpecifiers) -> VersionSpecifiers {
    let mut specifiers = specifiers
        .into_iter()
        .map(normalize_specifier)
        .collect::<Vec<_>>();
    specifiers.sort_by_cached_key(ToString::to_string);
    let mut index = 0;
    while index < specifiers.len() {
        let specifier = &specifiers[index];
        let remaining = specifiers
            .iter()
            .enumerate()
            .filter(|(other_index, _)| *other_index != index)
            .map(|(_, specifier)| specifier);
        if allows_yanked(remaining.clone())
            || (allows_prereleases(specifier) && !remaining.clone().any(allows_prereleases))
        {
            index += 1;
            continue;
        }
        let remaining = remaining.fold(Ranges::full(), |range, specifier| {
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

/// A standalone equality clause permits its version even when the index marks it as yanked.
fn allows_yanked<'a>(mut specifiers: impl Iterator<Item = &'a VersionSpecifier>) -> bool {
    let Some(specifier) = specifiers.next() else {
        return false;
    };
    specifiers.next().is_none()
        && match specifier.operator() {
            Operator::Equal | Operator::ExactEqual => true,
            Operator::NotEqual
            | Operator::TildeEqual
            | Operator::LessThan
            | Operator::LessThanEqual
            | Operator::GreaterThan
            | Operator::GreaterThanEqual
            | Operator::EqualStar
            | Operator::NotEqualStar => false,
        }
}

/// Normalize insignificant trailing zeros, keeping precision-sensitive operators intact.
/// Wildcard and compatible-release clauses, such as `==1.0.*` and `~=1.0.0`, retain their release
/// precision because it changes which versions they accept.
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
            let mut release_len = release.len();
            while release_len > 1 && release[release_len - 1] == 0 {
                release_len -= 1;
            }
            if release_len == release.len() {
                return specifier;
            }
            let version = specifier
                .version()
                .clone()
                .with_release(&release[..release_len]);
            VersionSpecifier::from_version(*specifier.operator(), version).unwrap_or(specifier)
        }
    }
}

/// Whether this clause opts into prereleases under the explicit prerelease policy.
/// Excluding a prerelease, as in `!=1rc1`, does not opt in.
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
    use uv_pep508::{MarkerTree, Requirement as Pep508Requirement};
    use uv_pypi_types::VerbatimParsedUrl;

    use crate::{ExcludeDependency, Excludes, Overrides};

    use super::{
        NormalizedExcludes, NormalizedRequirements, allows_prereleases, allows_yanked,
        simplify_specifiers,
    };

    fn requirements(inputs: &[&str]) -> Result<Vec<Requirement>> {
        inputs
            .iter()
            .map(|input| {
                Ok(Requirement::from(
                    Pep508Requirement::<VerbatimParsedUrl>::from_str(input)?,
                ))
            })
            .collect()
    }

    /// Optional dependencies must retain the extra condition applied to their overrides.
    #[test]
    fn overridden_optional_requirements() -> Result<()> {
        let original = requirements(&["a; extra == 'x'", "a; extra == 'y'"])?;
        let normalized = NormalizedRequirements::from(original.clone());
        let overrides = Overrides::from_requirements(requirements(&["a>=2"])?);
        let overridden = |requirements: &[Requirement]| {
            overrides
                .apply(requirements)
                .fold(MarkerTree::FALSE, |marker, requirement| {
                    marker.or(requirement.marker)
                })
        };
        assert_eq!(overridden(&original), overridden(&normalized));
        Ok(())
    }

    #[test]
    fn scoped_exclusions() -> Result<()> {
        #[derive(serde::Deserialize)]
        struct Input {
            excludes: Vec<ExcludeDependency>,
        }

        let original = toml::from_str::<Input>(
            r#"
            excludes = [
                "global", "global",
                { package = { name = "tool" }, dependencies = ["b", "a", "a"] },
                { package = { name = "tool" }, dependencies = ["c"] },
                { package = { name = "tool", version = "1" }, dependencies = [] },
                { package = { name = "other" }, dependencies = ["a"] },
            ]
        "#,
        )?
        .excludes;
        let normalized = NormalizedExcludes::from(original.clone());
        let indexed = Excludes::from_entries(original);
        let normalized_index = Excludes::from_entries(normalized.iter().cloned());
        for package in ["tool", "other", "unrelated"] {
            for version in ["1", "2"] {
                for dependency in ["a", "b", "c", "global", "unrelated"] {
                    assert_eq!(
                        indexed.contains_for(
                            &package.parse()?,
                            &version.parse()?,
                            &dependency.parse()?
                        ),
                        normalized_index.contains_for(
                            &package.parse()?,
                            &version.parse()?,
                            &dependency.parse()?
                        ),
                    );
                }
            }
        }
        let expected = toml::from_str::<Input>(
            r#"
            excludes = [
                "global",
                { package = { name = "other" }, dependencies = ["a"] },
                { package = { name = "tool" }, dependencies = ["a", "b", "c"] },
                { package = { name = "tool", version = "1" }, dependencies = [] },
            ]
        "#,
        )?
        .excludes;
        assert_eq!(normalized, NormalizedExcludes::from(expected));
        Ok(())
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
        let normalized = NormalizedRequirements::from(original.clone());
        assert_snapshot!(normalized.iter().map(ToString::to_string).collect::<Vec<_>>().join("\n"), @"
        a
        b @ https://example.org/b.whl#sha256=1111
        b @ https://example.org/b.whl#sha256=2222
        z-tool
        ");
        let mut reordered = original;
        reordered.reverse();
        assert_eq!(normalized, NormalizedRequirements::from(reordered));
        assert_eq!(
            normalized,
            NormalizedRequirements::from(normalized.clone().into_inner())
        );
        Ok(())
    }

    #[test]
    fn precision_sensitive_requirements() -> Result<()> {
        for (left, right) in [
            ("foo==1.*", "foo==1.0.*"),
            ("foo!=1.*", "foo!=1.0.*"),
            ("foo===1", "foo==1"),
            ("foo~=1.0", "foo~=1.0.0"),
            ("foo>=1,<=1.0", "foo==1.0"),
            ("foo==1,>=0", "foo==1"),
        ] {
            assert_ne!(
                NormalizedRequirements::from(requirements(&[left])?),
                NormalizedRequirements::from(requirements(&[right])?),
                "{left} != {right}"
            );
        }
        assert_eq!(
            NormalizedRequirements::from(requirements(&["foo===1"])?),
            NormalizedRequirements::from(requirements(&["foo===1.0"])?),
        );
        assert_eq!(
            NormalizedRequirements::from(requirements(&["foo~=1.2"])?),
            NormalizedRequirements::from(requirements(&["foo>=1.2,<2"])?),
        );
        let normalized = NormalizedRequirements::from(requirements(&[
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
        >=1,<=1 -> <=1, >=1
        ===1,===1.0 -> ===1, ===1
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
                assert_eq!(
                    allows_yanked(original.iter()),
                    allows_yanked(normalized.iter()),
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
        let normalized = NormalizedRequirements::from(original.clone());
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
            display(&NormalizedRequirements::from(
                normalized.clone().into_inner()
            ))
        );
        let mut reordered = original.clone();
        reordered.reverse();
        assert_eq!(
            display(&normalized),
            display(&NormalizedRequirements::from(reordered))
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
