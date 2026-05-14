use std::fmt::Display;
use std::str::FromStr;
use std::sync::Arc;

use rustc_hash::FxHashMap;

use uv_configuration::HashCheckingMode;
use uv_distribution_types::{
    DistributionMetadata, HashGeneration, HashPolicy, Name, Requirement, RequirementSource,
    Resolution, UnresolvedRequirement, VersionId,
};
use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_pypi_types::{HashDigest, HashDigests, HashError, ResolverMarkerEnvironment};
use uv_redacted::DisplaySafeUrl;

#[derive(Debug, Default, Clone)]
pub enum HashStrategy {
    /// No hash policy is specified.
    #[default]
    None,
    /// Hashes should be generated (specifically, a SHA-256 hash), but not validated.
    Generate(HashGeneration),
    /// Hashes should be validated, if present, but ignored if absent.
    ///
    /// If necessary, hashes should be generated to ensure that the archive is valid.
    Verify(Arc<FxHashMap<VersionId, Vec<HashDigest>>>),
    /// Hashes should be validated against a pre-defined list of hashes.
    ///
    /// If necessary, hashes should be generated to ensure that the archive is valid.
    Require(Arc<FxHashMap<VersionId, Vec<HashDigest>>>),
}

impl HashStrategy {
    /// Return the [`HashPolicy`] for the given distribution.
    pub fn get<T: DistributionMetadata>(&self, distribution: &T) -> HashPolicy<'_> {
        match self {
            Self::None => HashPolicy::None,
            Self::Generate(mode) => HashPolicy::Generate(*mode),
            Self::Verify(hashes) => {
                let id = distribution.version_id();
                if let Some(hashes) = hashes.get(&id) {
                    hash_policy(&id, hashes.as_slice())
                } else {
                    HashPolicy::None
                }
            }
            Self::Require(hashes) => {
                let id = distribution.version_id();
                hash_policy(&id, hashes.get(&id).map(Vec::as_slice).unwrap_or_default())
            }
        }
    }

    /// Return the [`HashPolicy`] for the given registry-based package.
    pub fn get_package(&self, name: &PackageName, version: &Version) -> HashPolicy<'_> {
        let id = VersionId::from_registry(name.clone(), version.clone());
        match self {
            Self::None => HashPolicy::None,
            Self::Generate(mode) => HashPolicy::Generate(*mode),
            Self::Verify(hashes) => {
                if let Some(hashes) = hashes.get(&id) {
                    HashPolicy::Any(hashes.as_slice())
                } else {
                    HashPolicy::None
                }
            }
            Self::Require(hashes) => {
                HashPolicy::Any(hashes.get(&id).map(Vec::as_slice).unwrap_or_default())
            }
        }
    }

    /// Return the [`HashPolicy`] for the given direct URL package.
    ///
    /// A direct URL identifies a single concrete artifact, so every provided digest must match.
    pub fn get_url(&self, url: &DisplaySafeUrl) -> HashPolicy<'_> {
        let id = VersionId::from_url(url);
        match self {
            Self::None => HashPolicy::None,
            Self::Generate(mode) => HashPolicy::Generate(*mode),
            Self::Verify(hashes) => {
                if let Some(hashes) = hashes.get(&id) {
                    HashPolicy::All(hashes.as_slice())
                } else {
                    HashPolicy::None
                }
            }
            Self::Require(hashes) => {
                HashPolicy::All(hashes.get(&id).map(Vec::as_slice).unwrap_or_default())
            }
        }
    }

    /// Returns `true` if the given registry-based package is allowed.
    pub fn allows_package(&self, name: &PackageName, version: &Version) -> bool {
        match self {
            Self::None => true,
            Self::Generate(_) => true,
            Self::Verify(_) => true,
            Self::Require(hashes) => {
                hashes.contains_key(&VersionId::from_registry(name.clone(), version.clone()))
            }
        }
    }

    /// Returns `true` if the given direct URL package is allowed.
    pub fn allows_url(&self, url: &DisplaySafeUrl) -> bool {
        match self {
            Self::None => true,
            Self::Generate(_) => true,
            Self::Verify(_) => true,
            Self::Require(hashes) => hashes.contains_key(&VersionId::from_url(url)),
        }
    }

    /// Return a [`HashStrategy`] augmented with archive URL hashes discovered in additional
    /// requirements after the initial command-line parse.
    pub fn augment_with_requirements<'a>(
        self,
        requirements: impl Iterator<Item = &'a Requirement>,
    ) -> Result<Self, HashStrategyError> {
        Ok(match self {
            Self::None => Self::None,
            Self::Generate(mode) => Self::Generate(mode),
            Self::Verify(existing) => {
                if let Some(hashes) = Self::augment_hashes(existing.as_ref(), requirements)? {
                    Self::Verify(Arc::new(hashes))
                } else {
                    Self::Verify(existing)
                }
            }
            Self::Require(existing) => {
                if let Some(hashes) = Self::augment_hashes(existing.as_ref(), requirements)? {
                    Self::Require(Arc::new(hashes))
                } else {
                    Self::Require(existing)
                }
            }
        })
    }

    /// Generate the required hashes from a set of [`UnresolvedRequirement`] entries.
    ///
    /// When the environment is not given, this treats all marker expressions
    /// that reference the environment as true. In other words, it does
    /// environment independent expression evaluation. (Which in turn devolves
    /// to "only evaluate marker expressions that reference an extra name.")
    pub fn from_requirements<'a>(
        requirements: impl Iterator<Item = (&'a UnresolvedRequirement, &'a [String])>,
        constraints: impl Iterator<Item = (&'a Requirement, &'a [String])>,
        marker_env: Option<&ResolverMarkerEnvironment>,
        mode: HashCheckingMode,
    ) -> Result<Self, HashStrategyError> {
        let mut constraint_hashes = FxHashMap::<VersionId, Vec<HashDigest>>::default();

        // First, index the constraints by name.
        for (requirement, digests) in constraints {
            if !requirement
                .evaluate_markers(marker_env.map(ResolverMarkerEnvironment::markers), &[])
            {
                continue;
            }

            // Every constraint must be a pinned version.
            let Some(id) = Self::pin(requirement) else {
                if mode.is_require() {
                    return Err(HashStrategyError::UnpinnedRequirement(
                        requirement.to_string(),
                        mode,
                    ));
                }
                continue;
            };

            // Parse the hashes provided directly on the requirement, then merge in any hashes from
            // the URL fragment.
            let mut digests = digests
                .iter()
                .map(|digest| HashDigest::from_str(digest))
                .collect::<Result<Vec<_>, _>>()?;
            if let Some(fragment_hashes) = requirement.hashes().map(HashDigests::from) {
                merge_digests(&mut digests, fragment_hashes.iter(), requirement)?;
            }

            if digests.is_empty() {
                continue;
            }

            merge_hashes(&mut constraint_hashes, id, digests, requirement)?;
        }

        // For each requirement, map from hash identity to allowed hashes.
        let mut requirement_hashes = FxHashMap::<VersionId, Vec<HashDigest>>::default();
        for (requirement, digests) in requirements {
            if !requirement
                .evaluate_markers(marker_env.map(ResolverMarkerEnvironment::markers), &[])
            {
                continue;
            }

            // Every requirement must be either a pinned version or a direct URL.
            let id = match &requirement {
                UnresolvedRequirement::Named(requirement) => {
                    if let Some(id) = Self::pin(requirement) {
                        id
                    } else {
                        if mode.is_require() {
                            return Err(HashStrategyError::UnpinnedRequirement(
                                requirement.to_string(),
                                mode,
                            ));
                        }
                        continue;
                    }
                }
                UnresolvedRequirement::Unnamed(requirement) => {
                    // Direct URLs are always allowed.
                    VersionId::from_parsed_url(&requirement.url.parsed_url)
                }
            };

            // Parse the hashes provided directly on the requirement, then merge in any hashes from
            // the URL fragment.
            let mut digests = digests
                .iter()
                .map(|digest| HashDigest::from_str(digest))
                .collect::<Result<Vec<_>, _>>()?;
            if let Some(fragment_hashes) = requirement.hashes().map(HashDigests::from) {
                merge_digests(&mut digests, fragment_hashes.iter(), requirement)?;
            }

            let digests = if let Some(constraint) = constraint_hashes.remove(&id) {
                if digests.is_empty() {
                    // If there are _only_ hashes on the constraints, use them.
                    constraint
                } else if matches!(id, VersionId::ArchiveUrl { .. }) {
                    let mut merged = digests;
                    merge_digests(&mut merged, &constraint, requirement)?;
                    merged
                } else {
                    // If there are constraint and requirement hashes, take the intersection.
                    let intersection: Vec<_> = digests
                        .into_iter()
                        .filter(|digest| constraint.contains(digest))
                        .collect();
                    if intersection.is_empty() {
                        return Err(HashStrategyError::NoIntersection(
                            requirement.to_string(),
                            mode,
                        ));
                    }
                    intersection
                }
            } else {
                digests
            };

            // Under `--require-hashes`, every requirement must include a hash.
            if digests.is_empty() {
                if mode.is_require() {
                    return Err(HashStrategyError::MissingHashes(
                        requirement.to_string(),
                        mode,
                    ));
                }
                continue;
            }

            merge_hashes(&mut requirement_hashes, id, digests, requirement)?;
        }

        // Merge the hashes, preferring requirements over constraints, since overlapping
        // requirements were already merged.
        let hashes: FxHashMap<VersionId, Vec<HashDigest>> = constraint_hashes
            .into_iter()
            .chain(requirement_hashes)
            .collect();
        match mode {
            HashCheckingMode::Verify => Ok(Self::Verify(Arc::new(hashes))),
            HashCheckingMode::Require => Ok(Self::Require(Arc::new(hashes))),
        }
    }

    /// Generate the required hashes from a [`Resolution`].
    pub fn from_resolution(
        resolution: &Resolution,
        mode: HashCheckingMode,
    ) -> Result<Self, HashStrategyError> {
        let mut hashes = FxHashMap::<VersionId, Vec<HashDigest>>::default();

        for (dist, digests) in resolution.hashes() {
            if digests.is_empty() {
                // Under `--require-hashes`, every requirement must include a hash.
                if mode.is_require() {
                    return Err(HashStrategyError::MissingHashes(
                        dist.name().to_string(),
                        mode,
                    ));
                }
                continue;
            }
            hashes.insert(dist.version_id(), digests.to_vec());
        }

        match mode {
            HashCheckingMode::Verify => Ok(Self::Verify(Arc::new(hashes))),
            HashCheckingMode::Require => Ok(Self::Require(Arc::new(hashes))),
        }
    }

    /// Augment an existing set of hashes with archive URL hashes discovered in additional
    /// requirements.
    ///
    /// Archive URL requirements are keyed by a [`VersionId`] so that requirements that refer to
    /// the same underlying archive but differ only in hash fragments are merged onto the same
    /// digest set.
    ///
    /// Returns `Ok(None)` if no new hashes were added or updated.
    fn augment_hashes<'a>(
        existing: &FxHashMap<VersionId, Vec<HashDigest>>,
        requirements: impl Iterator<Item = &'a Requirement>,
    ) -> Result<Option<FxHashMap<VersionId, Vec<HashDigest>>>, HashStrategyError> {
        let mut hashes = None;

        for requirement in requirements {
            let Some((id, digests)) = Self::requirement_hashes(requirement) else {
                continue;
            };
            let current = hashes.as_ref().unwrap_or(existing);
            let current_digests = current.get(&id);
            let mut merged = current_digests.cloned().unwrap_or_default();
            merge_digests(&mut merged, &digests, requirement)?;

            if current_digests.map(Vec::as_slice) == Some(merged.as_slice()) {
                continue;
            }

            hashes
                .get_or_insert_with(|| existing.clone())
                .insert(id, merged);
        }

        Ok(hashes)
    }

    /// Extract the archive URL hash target and digests for a requirement, if any.
    fn requirement_hashes(requirement: &Requirement) -> Option<(VersionId, Vec<HashDigest>)> {
        let mut digests = HashDigests::from(requirement.hashes()?).to_vec();
        if digests.is_empty() {
            return None;
        }
        digests.sort_unstable();
        let id = Self::pin(requirement)?;
        Some((id, digests))
    }

    /// Pin a [`Requirement`] to a [`VersionId`], if possible.
    fn pin(requirement: &Requirement) -> Option<VersionId> {
        match &requirement.source {
            RequirementSource::Registry { specifier, .. } => {
                // Must be a single specifier.
                let [specifier] = specifier.as_ref() else {
                    return None;
                };

                // Must be pinned to a specific version.
                if *specifier.operator() != uv_pep440::Operator::Equal {
                    return None;
                }

                Some(VersionId::from_registry(
                    requirement.name.clone(),
                    specifier.version().clone(),
                ))
            }
            RequirementSource::Url {
                location,
                subdirectory,
                ..
            } => Some(VersionId::from_archive(location, subdirectory.as_deref())),
            RequirementSource::Git {
                git, subdirectory, ..
            } => Some(VersionId::from_git(git, subdirectory.as_deref())),
            RequirementSource::Path { install_path, .. } => {
                Some(VersionId::from_path(install_path))
            }
            RequirementSource::Directory { install_path, .. } => {
                Some(VersionId::from_directory(install_path))
            }
        }
    }
}

fn hash_policy<'a>(id: &VersionId, digests: &'a [HashDigest]) -> HashPolicy<'a> {
    match id {
        VersionId::NameVersion { .. } => HashPolicy::Any(digests),
        VersionId::ArchiveUrl { .. }
        | VersionId::Git { .. }
        | VersionId::Path { .. }
        | VersionId::Directory { .. }
        | VersionId::Unknown { .. } => HashPolicy::All(digests),
    }
}

/// Merge repeated hashes for a requirement or constraint into the hash map.
fn merge_hashes(
    hashes: &mut FxHashMap<VersionId, Vec<HashDigest>>,
    id: VersionId,
    incoming: Vec<HashDigest>,
    requirement: impl Display,
) -> Result<(), HashStrategyError> {
    if incoming.is_empty() {
        return Ok(());
    }

    if !matches!(&id, VersionId::ArchiveUrl { .. }) {
        hashes.insert(id, incoming);
        return Ok(());
    }

    if let Some(existing) = hashes.get_mut(&id) {
        return merge_digests(existing, &incoming, requirement);
    }

    let mut merged = Vec::new();
    merge_digests(&mut merged, &incoming, requirement)?;
    hashes.insert(id, merged);
    Ok(())
}

/// Merge `incoming` digests into `existing`.
///
/// Exact duplicates are ignored. Digests for different algorithms are accumulated. If the
/// same algorithm appears with two different values, returns
/// [`HashStrategyError::ConflictingArchiveUrlHashes`].
fn merge_digests<'a>(
    existing: &mut Vec<HashDigest>,
    incoming: impl IntoIterator<Item = &'a HashDigest>,
    requirement: impl Display,
) -> Result<(), HashStrategyError> {
    for digest in incoming {
        match existing
            .iter()
            .find(|candidate| candidate.algorithm == digest.algorithm)
        {
            Some(candidate) if candidate == digest => {}
            Some(conflict) => {
                return Err(HashStrategyError::ConflictingArchiveUrlHashes(
                    requirement.to_string(),
                    conflict.clone(),
                    digest.clone(),
                ));
            }
            None => existing.push(digest.clone()),
        }
    }
    existing.sort_unstable();

    Ok(())
}

#[derive(thiserror::Error, Debug)]
pub enum HashStrategyError {
    #[error(transparent)]
    Hash(#[from] HashError),
    #[error("Conflicting archive URL hashes for `{0}`: `{1}` conflicts with `{2}`")]
    ConflictingArchiveUrlHashes(String, HashDigest, HashDigest),
    #[error(
        "In `{1}` mode, all requirements must have their versions pinned with `==`, but found: {0}"
    )]
    UnpinnedRequirement(String, HashCheckingMode),
    #[error("In `{1}` mode, all requirements must have a hash, but none were provided for: {0}")]
    MissingHashes(String, HashCheckingMode),
    #[error(
        "In `{1}` mode, all requirements must have a hash, but there were no overlapping hashes between the requirements and constraints for: {0}"
    )]
    NoIntersection(String, HashCheckingMode),
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;
    use uv_configuration::HashCheckingMode;
    use uv_distribution_filename::DistExtension;
    use uv_distribution_types::{
        HashPolicy, Requirement, RequirementSource, UnresolvedRequirement,
    };
    use uv_pypi_types::HashDigest;

    use super::HashStrategy;

    fn requirement(url: &str) -> Requirement {
        Requirement {
            name: "anyio".parse().unwrap(),
            extras: Box::default(),
            groups: Box::default(),
            marker: "python_version >= '3.8'".parse().unwrap(),
            source: RequirementSource::Url {
                location: "https://files.pythonhosted.org/packages/36/55/ad4de788d84a630656ece71059665e01ca793c04294c463fd84132f40fe6/anyio-4.0.0-py3-none-any.whl"
                    .parse()
                    .unwrap(),
                subdirectory: None,
                ext: DistExtension::Wheel,
                url: url.parse().unwrap(),
            },
            origin: None,
        }
    }

    #[test]
    fn from_requirements_merges_direct_url_hashes_across_fragments() {
        let first = UnresolvedRequirement::Named(requirement(
            "https://files.pythonhosted.org/packages/36/55/ad4de788d84a630656ece71059665e01ca793c04294c463fd84132f40fe6/anyio-4.0.0-py3-none-any.whl#sha256=cfdb2b588b9fc25ede96d8db56ed50848b0b649dca3dd1df0b11f683bb9e0b5f",
        ));
        let second = UnresolvedRequirement::Named(requirement(
            "https://files.pythonhosted.org/packages/36/55/ad4de788d84a630656ece71059665e01ca793c04294c463fd84132f40fe6/anyio-4.0.0-py3-none-any.whl#sha512=f30761c1e8725b49c498273b90dba4b05c0fd157811994c806183062cb6647e773364ce45f0e1ff0b10e32fe6d0232ea5ad39476ccf37109d6b49603a09c11c2",
        ));

        let hasher = HashStrategy::from_requirements(
            [(&first, &[][..]), (&second, &[][..])].into_iter(),
            std::iter::empty(),
            None,
            HashCheckingMode::Require,
        )
        .unwrap();

        let mut expected = vec![
            HashDigest::from_str(
                "sha256:cfdb2b588b9fc25ede96d8db56ed50848b0b649dca3dd1df0b11f683bb9e0b5f",
            )
            .unwrap(),
            HashDigest::from_str(
                "sha512:f30761c1e8725b49c498273b90dba4b05c0fd157811994c806183062cb6647e773364ce45f0e1ff0b10e32fe6d0232ea5ad39476ccf37109d6b49603a09c11c2",
            )
            .unwrap(),
        ];
        expected.sort_unstable();

        for requirement in [&first, &second] {
            let UnresolvedRequirement::Named(requirement) = requirement else {
                panic!("expected named requirement");
            };
            let RequirementSource::Url { url, .. } = &requirement.source else {
                panic!("expected direct URL requirement");
            };
            assert_eq!(hasher.get_url(url), HashPolicy::All(expected.as_slice()));
        }
    }
}
