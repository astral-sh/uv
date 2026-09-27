use std::fmt::Display;
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;

use rustc_hash::FxHashMap;

use uv_cache_key::CanonicalUrl;
use uv_configuration::{Constraints, HashCheckingMode};
use uv_distribution_filename::{WheelFilename, WheelFilenameKey};
use uv_distribution_types::{
    ArchiveHashPolicy, DistributionMetadata, HashCollection, HashComparison, HashValidation,
    MetadataHashPolicy, Name, RegistryHashTarget, Requirement, RequirementSource, Resolution,
    UnresolvedRequirement, VersionId,
};
use uv_normalize::PackageName;
use uv_pep440::{Operator, Version};
use uv_pypi_types::{HashAlgorithm, HashDigest, HashDigests, HashError, ResolverMarkerEnvironment};
use uv_redacted::DisplaySafeUrl;

type HashesById = FxHashMap<VersionId, Vec<HashDigest>>;

/// Hash collection and verification policies for a resolution.
///
/// Verification takes precedence for distributions with trusted hashes. The collection policy
/// applies to the remaining distributions.
#[derive(Debug, Default, Clone)]
pub struct HashStrategy {
    collection: HashCollection,
    verification: HashVerification,
}

/// The trusted hashes to enforce when retrieving distributions.
#[derive(Debug, Default, Clone)]
pub enum HashVerification {
    /// Hashes do not need to be validated.
    #[default]
    None,
    /// Validate known hashes, without requiring hashes for other distributions.
    IfPresent(Arc<FxHashMap<VersionId, Vec<HashDigest>>>),
    /// Validate build artifacts recorded in a lockfile, allowing other wheel variants.
    LockedBuild {
        /// Recorded hashes used to identify registry artifacts at different locations.
        hashes: Arc<FxHashMap<VersionId, Vec<HashDigest>>>,
        /// Registry wheels and source archives recorded in the lockfile.
        registry: Arc<LockedRegistryHashes>,
        /// Explicit constraints and hashes for non-registry artifacts.
        additional: Arc<HashStrategy>,
    },
    /// Every distribution must have a matching trusted hash.
    Required(Arc<FxHashMap<VersionId, Vec<HashDigest>>>),
}

/// The registry artifacts recorded in a lockfile for isolated build verification.
#[derive(Debug, Default, Clone)]
pub struct LockedRegistryHashes {
    hashes: FxHashMap<RegistryHashKey, Vec<HashDigest>>,
}

/// Registry identity used for build hash verification. A cached source retains its version but
/// not its original filename, so source archives are grouped by index, name, and version.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum RegistryHashKey {
    Wheel {
        index: CanonicalUrl,
        filename: WheelFilenameKey<WheelFilename>,
    },
    Source {
        index: CanonicalUrl,
        name: PackageName,
        version: Version,
    },
}

impl RegistryHashKey {
    fn from_target(target: RegistryHashTarget<'_>) -> Self {
        match target {
            RegistryHashTarget::Wheel { index, filename } => Self::Wheel {
                index: CanonicalUrl::new(index.url().clone()),
                filename: WheelFilenameKey::new(filename.clone()),
            },
            RegistryHashTarget::Source {
                index,
                name,
                version,
                built_wheel: _,
            } => Self::Source {
                index: CanonicalUrl::new(index.url().clone()),
                name: name.clone(),
                version: version.clone(),
            },
        }
    }

    fn name_and_version(&self) -> (&PackageName, &Version) {
        match self {
            Self::Wheel { filename, .. } => {
                (&filename.filename().name, &filename.filename().version)
            }
            Self::Source { name, version, .. } => (name, version),
        }
    }
}

impl LockedRegistryHashes {
    fn apply_constraints(&mut self, constraints: &HashStrategy) {
        for (key, hashes) in &mut self.hashes {
            let (name, version) = key.name_and_version();
            let id = VersionId::from_registry(name.clone(), version.clone());
            if let Some(allowed) = constraints.hashes_for_id(&id) {
                hashes.retain(|hash| allowed.contains(hash));
            } else if !constraints.allows_package(name, version) {
                hashes.clear();
            }
        }
    }

    /// Record a trusted hash for a registry artifact.
    pub fn insert(&mut self, target: RegistryHashTarget<'_>, hash: HashDigest) {
        let hashes = self
            .hashes
            .entry(RegistryHashKey::from_target(target))
            .or_default();
        if !hashes.contains(&hash) {
            hashes.push(hash);
        }
    }

    fn get(&self, target: RegistryHashTarget<'_>) -> Option<&[HashDigest]> {
        self.hashes
            .get(&RegistryHashKey::from_target(target))
            .map(Vec::as_slice)
    }
}

impl HashStrategy {
    /// Collect declared hashes for resolution, computing missing hashes according to the policy.
    pub fn collect(collection: HashCollection) -> Self {
        Self {
            collection,
            ..Self::default()
        }
    }

    /// Validate hashes when present.
    pub fn verify(hashes: Arc<FxHashMap<VersionId, Vec<HashDigest>>>) -> Self {
        Self::default().with_verification(HashVerification::IfPresent(hashes))
    }

    /// Verify build artifacts recorded in a lockfile without requiring every wheel variant to
    /// appear in the runtime resolution.
    pub fn verify_build(
        hashes: Arc<FxHashMap<VersionId, Vec<HashDigest>>>,
        registry: LockedRegistryHashes,
    ) -> Self {
        let additional = hashes
            .iter()
            .filter(|(id, _)| match id {
                VersionId::NameVersion(..) => false,
                VersionId::ArchiveUrl { .. }
                | VersionId::Git { .. }
                | VersionId::Path(_)
                | VersionId::Directory(_)
                | VersionId::Unknown(_) => true,
            })
            .map(|(id, hashes)| (id.clone(), hashes.clone()))
            .collect();
        Self::default().with_verification(HashVerification::LockedBuild {
            hashes,
            registry: Arc::new(registry),
            additional: Arc::new(Self::verify(Arc::new(additional))),
        })
    }

    /// Require a matching trusted hash for every distribution.
    fn require(hashes: Arc<FxHashMap<VersionId, Vec<HashDigest>>>) -> Self {
        Self::default().with_verification(HashVerification::Required(hashes))
    }

    /// Set verification independently of hash collection.
    #[must_use]
    pub fn with_verification(mut self, verification: HashVerification) -> Self {
        self.verification = verification;
        self
    }

    /// Apply constraint hashes using the same rules as [`Self::from_requirements`].
    ///
    /// Preserve hash collection and require hashes if either strategy requires them. Constraints
    /// for identities absent from this strategy remain available for newly resolved dependencies.
    pub fn with_constraint_hashes(mut self, constraints: &Self) -> Result<Self, HashStrategyError> {
        let (requirement_hashes, requirement_mode) = match &mut self.verification {
            HashVerification::None => {
                self.verification = constraints.verification.clone();
                return Ok(self);
            }
            HashVerification::IfPresent(hashes) => (Arc::clone(hashes), HashCheckingMode::Verify),
            HashVerification::Required(hashes) => (Arc::clone(hashes), HashCheckingMode::Require),
            HashVerification::LockedBuild {
                registry,
                additional,
                ..
            } => {
                let combined = additional
                    .as_ref()
                    .clone()
                    .with_constraint_hashes(constraints)?;
                Arc::make_mut(registry).apply_constraints(&combined);
                *additional = Arc::new(combined);
                return Ok(self);
            }
        };

        let Some((constraint_hashes, constraint_mode)) = constraints.hashes_and_mode() else {
            return Ok(self);
        };
        let mode = if requirement_mode.is_require() || constraint_mode.is_require() {
            HashCheckingMode::Require
        } else {
            HashCheckingMode::Verify
        };
        let mut constraint_hashes = constraint_hashes.clone();
        if mode.is_require() {
            constraint_hashes.retain(|_, digests| {
                digests.retain(|digest| digest.algorithm() != HashAlgorithm::Md5);
                !digests.is_empty()
            });
        }
        let mut hashes = constraint_hashes.clone();
        for (id, digests) in requirement_hashes.iter() {
            let mut digests = digests.clone();
            if mode.is_require() {
                digests.retain(|digest| digest.algorithm() != HashAlgorithm::Md5);
            }
            let digests =
                if let Some(constraint) = lookup_hashes(&constraint_hashes, id, constraint_mode) {
                    combine_constraint_hashes(id, digests, constraint, id, mode)?
                } else {
                    digests
                };
            if !digests.is_empty() {
                hashes.insert(id.clone(), digests);
            }
        }
        self.verification = match mode {
            HashCheckingMode::Verify => HashVerification::IfPresent(Arc::new(hashes)),
            HashCheckingMode::Require => HashVerification::Required(Arc::new(hashes)),
        };
        Ok(self)
    }

    fn hashes_and_mode(&self) -> Option<(&HashesById, HashCheckingMode)> {
        match &self.verification {
            HashVerification::None => None,
            HashVerification::IfPresent(hashes) => Some((hashes, HashCheckingMode::Verify)),
            HashVerification::Required(hashes) => Some((hashes, HashCheckingMode::Require)),
            HashVerification::LockedBuild { additional, .. } => additional.hashes_and_mode(),
        }
    }

    /// Return the hash collection policy.
    pub fn collection(&self) -> HashCollection {
        self.collection
    }

    /// Return the hash verification policy.
    pub fn verification(&self) -> &HashVerification {
        &self.verification
    }

    /// Return the [`ArchiveHashPolicy`] for the given distribution.
    pub fn archive_policy<T: DistributionMetadata>(
        &self,
        distribution: &T,
    ) -> ArchiveHashPolicy<'_> {
        self.archive_policy_from_validation(self.validation_for_distribution(distribution))
    }

    /// Return the [`MetadataHashPolicy`] for retrieving the given distribution's metadata.
    pub fn metadata_policy<T: DistributionMetadata>(
        &self,
        distribution: &T,
    ) -> MetadataHashPolicy<'_> {
        MetadataHashPolicy {
            collection: self.collection,
            validation: self.validation_for_distribution(distribution),
        }
    }

    /// Return the [`ArchiveHashPolicy`] for the given registry-based package.
    pub fn archive_policy_for_package(
        &self,
        name: &PackageName,
        version: &Version,
    ) -> ArchiveHashPolicy<'_> {
        self.archive_policy_for_id(|| VersionId::from_registry(name.clone(), version.clone()))
    }

    /// Return the policy for a registry artifact in the cache.
    pub fn archive_policy_for_registry(
        &self,
        target: RegistryHashTarget<'_>,
        computed: &[HashDigest],
    ) -> ArchiveHashPolicy<'_> {
        let validation = match &self.verification {
            HashVerification::LockedBuild { registry, .. } => {
                let (name, version) = target.name_and_version();
                self.locked_registry_validation(name, version, registry.get(target), computed)
            }
            HashVerification::None
            | HashVerification::IfPresent(_)
            | HashVerification::Required(_) => {
                let (name, version) = target.requirement_name_and_version();
                self.validation_for_id(|| VersionId::from_registry(name.clone(), version.clone()))
            }
        };
        self.archive_policy_from_validation(validation)
    }

    /// Compare a registry candidate's advertised hashes with the optional locked build hashes.
    ///
    /// An unrecorded build wheel is allowed, but ranks below a candidate with a recorded digest.
    /// This also recognizes trusted artifacts relocated through `--find-links`.
    /// Other verification modes return `None` to use the index's usual comparison.
    pub fn locked_registry_hash_comparison(
        &self,
        target: RegistryHashTarget<'_>,
        advertised: &[HashDigest],
    ) -> Option<HashComparison> {
        let HashVerification::LockedBuild {
            hashes,
            registry,
            additional,
        } = &self.verification
        else {
            return None;
        };
        if let Some(expected) = registry.get(target) {
            return Some(ArchiveHashPolicy::Any(expected).compare(advertised));
        }
        let (name, version) = target.name_and_version();
        let constraint = additional.archive_policy_for_package(name, version);
        if constraint.requires_validation() {
            return Some(constraint.compare(advertised));
        }
        if let Some(expected) = hashes.get(&VersionId::from_registry(name.clone(), version.clone()))
        {
            return Some(if ArchiveHashPolicy::Any(expected).matches(advertised) {
                HashComparison::Matched
            } else {
                HashComparison::Unrecorded
            });
        }
        Some(HashComparison::Matched)
    }

    /// Return the [`ArchiveHashPolicy`] for the given direct URL package.
    ///
    /// A direct URL identifies a single concrete artifact, so every provided digest must match.
    pub fn archive_policy_for_url(&self, url: &DisplaySafeUrl) -> ArchiveHashPolicy<'_> {
        self.archive_policy_for_id(|| VersionId::from_url(url))
    }

    /// Return the [`MetadataHashPolicy`] for a URL whose package name is not yet known.
    pub fn metadata_policy_for_url(&self, url: &DisplaySafeUrl) -> MetadataHashPolicy<'_> {
        MetadataHashPolicy {
            collection: self.collection,
            validation: self.validation_for_id(|| VersionId::from_url(url)),
        }
    }

    /// Return the archive hash policy for a distribution identity.
    fn archive_policy_for_id(&self, id: impl FnOnce() -> VersionId) -> ArchiveHashPolicy<'_> {
        self.archive_policy_from_validation(self.validation_for_id(id))
    }

    fn archive_policy_from_validation<'a>(
        &self,
        validation: HashValidation<'a>,
    ) -> ArchiveHashPolicy<'a> {
        match validation {
            HashValidation::None => match self.collection {
                HashCollection::None => ArchiveHashPolicy::None,
                HashCollection::Url | HashCollection::All => ArchiveHashPolicy::Generate,
            },
            HashValidation::Any(_) | HashValidation::All(_) => validation.into(),
        }
    }

    fn validation_for_distribution<T: DistributionMetadata>(
        &self,
        distribution: &T,
    ) -> HashValidation<'_> {
        if let HashVerification::LockedBuild { registry, .. } = &self.verification
            && let Some((target, advertised)) = distribution.registry_hash_target()
        {
            let (name, version) = target.name_and_version();
            return self.locked_registry_validation(
                name,
                version,
                registry.get(target),
                advertised,
            );
        }
        self.validation_for_id(|| distribution.version_id())
    }

    fn locked_registry_validation<'a>(
        &'a self,
        name: &PackageName,
        version: &Version,
        recorded: Option<&'a [HashDigest]>,
        advertised: &[HashDigest],
    ) -> HashValidation<'a> {
        let HashVerification::LockedBuild {
            hashes, additional, ..
        } = &self.verification
        else {
            return HashValidation::None;
        };
        if let Some(recorded) = recorded {
            return HashValidation::Any(recorded);
        }
        let validation = additional
            .validation_for_id(|| VersionId::from_registry(name.clone(), version.clone()));
        if validation != HashValidation::None {
            return validation;
        }
        // Recognize a recorded artifact at a different location without trusting a new index digest.
        if let Some(expected) = hashes.get(&VersionId::from_registry(name.clone(), version.clone()))
            && ArchiveHashPolicy::Any(expected).matches(advertised)
        {
            HashValidation::Any(expected)
        } else {
            HashValidation::None
        }
    }

    /// Construct an identity only when verification requires a lookup.
    fn validation_for_id(&self, id: impl FnOnce() -> VersionId) -> HashValidation<'_> {
        match &self.verification {
            HashVerification::IfPresent(_) => {
                let id = id();
                if let Some(hashes) = self.hashes_for_id(&id) {
                    return hash_validation(&id, hashes);
                }
            }
            HashVerification::Required(hashes) => {
                let id = id();
                return hash_validation(
                    &id,
                    hashes.get(&id).map(Vec::as_slice).unwrap_or_default(),
                );
            }
            HashVerification::LockedBuild { additional, .. } => {
                return additional.validation_for_id(id);
            }
            HashVerification::None => {}
        }
        HashValidation::None
    }

    /// Look up supplied hashes, including public-version pins in verification mode.
    fn hashes_for_id(&self, id: &VersionId) -> Option<&[HashDigest]> {
        match &self.verification {
            HashVerification::None => None,
            HashVerification::IfPresent(hashes) => {
                lookup_hashes(hashes, id, HashCheckingMode::Verify)
            }
            HashVerification::Required(hashes) => {
                lookup_hashes(hashes, id, HashCheckingMode::Require)
            }
            HashVerification::LockedBuild { additional, .. } => additional.hashes_for_id(id),
        }
    }

    /// Returns `true` if the given registry-based package is allowed.
    pub fn allows_package(&self, name: &PackageName, version: &Version) -> bool {
        match &self.verification {
            HashVerification::Required(hashes) => {
                hashes.contains_key(&VersionId::from_registry(name.clone(), version.clone()))
            }
            HashVerification::LockedBuild { additional, .. } => {
                additional.allows_package(name, version)
            }
            HashVerification::None | HashVerification::IfPresent(_) => true,
        }
    }

    /// Returns `true` if the given direct URL package is allowed.
    pub fn allows_url(&self, url: &DisplaySafeUrl) -> bool {
        match &self.verification {
            HashVerification::Required(hashes) => hashes.contains_key(&VersionId::from_url(url)),
            HashVerification::LockedBuild { additional, .. } => additional.allows_url(url),
            HashVerification::None | HashVerification::IfPresent(_) => true,
        }
    }

    /// Return a [`HashStrategy`] augmented with archive URL hashes discovered in additional
    /// requirements after the initial command-line parse.
    pub fn augment_with_requirements<'a>(
        mut self,
        requirements: impl Iterator<Item = &'a Requirement>,
    ) -> Result<Self, HashStrategyError> {
        match &mut self.verification {
            HashVerification::None => {}
            HashVerification::IfPresent(existing) | HashVerification::Required(existing) => {
                if let Some(hashes) = Self::augment_hashes(existing, requirements)? {
                    *existing = Arc::new(hashes);
                }
            }
            HashVerification::LockedBuild { additional, .. } => {
                *additional = Arc::new(
                    additional
                        .as_ref()
                        .clone()
                        .augment_with_requirements(requirements)?,
                );
            }
        }
        Ok(self)
    }

    /// Return a [`HashStrategy`] augmented with archive URL hashes discovered in distribution
    /// metadata.
    ///
    /// Required-hash verification is intentionally a closed set. In that mode, distribution
    /// untrusted metadata cannot authorize a requirement that was absent from the input hash set.
    /// Explicit requirements, such as `build-system.requires` and user-provided metadata, can still
    /// contribute hashes via [`Self::augment_with_requirements`].
    pub fn augment_with_metadata_requirements<'a>(
        self,
        requirements: impl Iterator<Item = &'a Requirement>,
    ) -> Result<Self, HashStrategyError> {
        if self
            .hashes_and_mode()
            .is_some_and(|(_, mode)| mode.is_require())
        {
            return Ok(self);
        }
        self.augment_with_requirements(requirements)
    }

    /// Collect hashes from [`UnresolvedRequirement`] entries and constraints.
    ///
    /// For duplicate registry pins and local files, the last nonempty hash list wins. Remote
    /// archive URLs combine hashes across algorithms and reject conflicting digests for the same
    /// algorithm. Registry pins accept any allowed digest; direct references must match all
    /// supplied digests. When requirements and constraints both supply hashes, registry pins
    /// permit only shared hashes; direct references must match hashes from both sources.
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
            if let Some(fragment_hashes) = requirement.hashes()? {
                let fragment_hashes = HashDigests::from(fragment_hashes);
                merge_digests(&mut digests, fragment_hashes.iter(), requirement)?;
            }

            if mode.is_require() {
                digests.retain(|digest| digest.algorithm() != HashAlgorithm::Md5);
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
                    VersionId::from_parsed_url(requirement.url.parsed_url.clone())
                }
            };

            // Parse the hashes provided directly on the requirement, then merge in any hashes from
            // the URL fragment.
            let mut digests = digests
                .iter()
                .map(|digest| HashDigest::from_str(digest))
                .collect::<Result<Vec<_>, _>>()?;
            if let Some(fragment_hashes) = requirement.hashes()? {
                let fragment_hashes = HashDigests::from(fragment_hashes);
                merge_digests(&mut digests, fragment_hashes.iter(), requirement)?;
            }

            let has_md5 = mode.is_require()
                && digests
                    .iter()
                    .any(|digest| digest.algorithm() == HashAlgorithm::Md5);
            if mode.is_require() {
                digests.retain(|digest| digest.algorithm() != HashAlgorithm::Md5);
            }

            let digests = if let Some(constraint) = constraint_hashes.get(&id) {
                // A hashless duplicate must not replace earlier requirement hashes with the
                // constraint's hashes.
                if digests.is_empty() && requirement_hashes.contains_key(&id) {
                    continue;
                }
                combine_constraint_hashes(&id, digests, constraint, requirement, mode)?
            } else {
                digests
            };

            // Under `--require-hashes`, every requirement needs a hash from the requirement or a
            // constraint.
            if digests.is_empty() {
                if mode.is_require() {
                    if has_md5 {
                        return Err(HashStrategyError::InsecureHashAlgorithm(
                            requirement.to_string(),
                            HashAlgorithm::Md5,
                            mode,
                        ));
                    }
                    return Err(HashStrategyError::MissingHashes(
                        requirement.to_string(),
                        mode,
                    ));
                }
                continue;
            }

            merge_hashes(&mut requirement_hashes, id, digests, requirement)?;
        }

        // Requirements take precedence because each matching constraint is already applied.
        let hashes: FxHashMap<VersionId, Vec<HashDigest>> = constraint_hashes
            .into_iter()
            .chain(requirement_hashes)
            .collect();
        match mode {
            HashCheckingMode::Verify => Ok(Self::verify(Arc::new(hashes))),
            HashCheckingMode::Require => Ok(Self::require(Arc::new(hashes))),
        }
    }

    /// Collect hashes from [`Constraints`] using the same handling as regular constraints in
    /// [`Self::from_requirements`], preserving declaration order.
    pub fn from_constraints(
        constraints: &Constraints,
        marker_env: Option<&ResolverMarkerEnvironment>,
        mode: HashCheckingMode,
    ) -> Result<Self, HashStrategyError> {
        Self::from_requirements(
            std::iter::empty(),
            constraints
                .specifications()
                .map(|entry| (&entry.requirement, entry.hashes.as_slice())),
            marker_env,
            mode,
        )
    }

    /// Read the required hashes from a [`Resolution`].
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
            HashCheckingMode::Verify => Ok(Self::verify(Arc::new(hashes))),
            HashCheckingMode::Require => Ok(Self::require(Arc::new(hashes))),
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
            let Some((id, digests)) = Self::requirement_hashes(requirement)? else {
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
    fn requirement_hashes(
        requirement: &Requirement,
    ) -> Result<Option<(VersionId, Vec<HashDigest>)>, HashStrategyError> {
        let Some(hashes) = requirement.hashes()? else {
            return Ok(None);
        };
        let mut digests = HashDigests::from(hashes).to_vec();
        if digests.is_empty() {
            return Ok(None);
        }
        digests.sort_unstable();
        let Some(id) = Self::pin(requirement) else {
            return Ok(None);
        };
        Ok(Some((id, digests)))
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
                let is_pinned =
                    matches!(specifier.operator(), Operator::Equal | Operator::ExactEqual);
                if !is_pinned {
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
            } => Some(VersionId::from_archive(
                location.clone(),
                subdirectory.clone().map(Path::into_path_buf),
            )),
            RequirementSource::GitDirectory {
                git, subdirectory, ..
            } => Some(VersionId::from_git(git, subdirectory.as_deref())),
            RequirementSource::GitPath {
                git, install_path, ..
            } => Some(VersionId::from_git(git, Some(install_path))),
            RequirementSource::Path { install_path, .. } => {
                Some(VersionId::from_path(install_path))
            }
            RequirementSource::Directory { install_path, .. } => {
                Some(VersionId::from_directory(install_path))
            }
        }
    }
}

/// Look up a pinned hash, including public-version pins in verification mode.
fn lookup_hashes<'a>(
    hashes: &'a FxHashMap<VersionId, Vec<HashDigest>>,
    id: &VersionId,
    mode: HashCheckingMode,
) -> Option<&'a [HashDigest]> {
    hashes
        .get(id)
        .or_else(|| {
            if !mode.is_require()
                && let VersionId::NameVersion(name, version) = id
                && version.is_local()
            {
                hashes.get(&VersionId::from_registry(
                    name.clone(),
                    version.clone().without_local(),
                ))
            } else {
                None
            }
        })
        .map(Vec::as_slice)
}

fn hash_validation<'a>(id: &VersionId, digests: &'a [HashDigest]) -> HashValidation<'a> {
    match id {
        VersionId::NameVersion { .. } => HashValidation::Any(digests),
        VersionId::ArchiveUrl { .. }
        | VersionId::Git { .. }
        | VersionId::Path { .. }
        | VersionId::Directory { .. }
        | VersionId::Unknown { .. } => HashValidation::All(digests),
    }
}

/// Combine hashes for a requirement and an applicable constraint.
fn combine_constraint_hashes(
    id: &VersionId,
    mut digests: Vec<HashDigest>,
    constraint: &[HashDigest],
    requirement: impl Display,
    mode: HashCheckingMode,
) -> Result<Vec<HashDigest>, HashStrategyError> {
    if digests.is_empty() {
        // If there are _only_ hashes on the constraints, use them.
        return Ok(constraint.to_vec());
    }
    match id {
        VersionId::NameVersion(..) => {
            // If there are constraint and requirement hashes, take the intersection.
            digests.retain(|digest| constraint.contains(digest));
            if digests.is_empty() {
                return Err(HashStrategyError::NoIntersection(
                    requirement.to_string(),
                    mode,
                ));
            }
        }
        VersionId::ArchiveUrl { .. }
        | VersionId::Git { .. }
        | VersionId::Path(..)
        | VersionId::Directory(..)
        | VersionId::Unknown(..) => {
            merge_digests(&mut digests, constraint, requirement)?;
        }
    }
    Ok(digests)
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
            .find(|candidate| candidate.algorithm() == digest.algorithm())
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
    #[error(
        "`{1}` hashes are insecure and cannot be used with `{2}` but no other hashes are available for: {0}"
    )]
    InsecureHashAlgorithm(String, HashAlgorithm, HashCheckingMode),
    #[error("In `{1}` mode, all requirements must have a hash, but none were provided for: {0}")]
    MissingHashes(String, HashCheckingMode),
    #[error(
        "In `{1}` mode, all requirements must have a hash, but there were no overlapping hashes between the requirements and constraints for: {0}"
    )]
    NoIntersection(String, HashCheckingMode),
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::slice;
    use std::str::FromStr;
    use std::sync::Arc;

    use rustc_hash::FxHashMap;
    use uv_configuration::HashCheckingMode;
    use uv_distribution_filename::{DistExtension, WheelFilename};
    use uv_distribution_types::{
        ArchiveHashPolicy, HashCollection, HashComparison, HashValidation, IndexUrl,
        MetadataHashPolicy, RegistryHashTarget, Requirement, RequirementScope, RequirementSource,
        UnresolvedRequirement, VersionId,
    };
    use uv_normalize::PackageName;
    use uv_pep440::Version;
    use uv_pypi_types::HashDigest;
    use uv_redacted::DisplaySafeUrl;

    use super::{HashStrategy, HashVerification, LockedRegistryHashes};

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
            scope: RequirementScope::Global,
            origin: None,
        }
    }

    #[test]
    fn from_requirements_merges_direct_url_hashes_across_fragments() {
        let first = UnresolvedRequirement::Named(requirement(
            "https://files.pythonhosted.org/packages/36/55/ad4de788d84a630656ece71059665e01ca793c04294c463fd84132f40fe6/anyio-4.0.0-py3-none-any.whl#sha256=CFDB2B588B9FC25EDE96D8DB56ED50848B0B649DCA3DD1DF0B11F683BB9E0B5F",
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
            assert_eq!(
                hasher.archive_policy_for_url(url),
                ArchiveHashPolicy::All(expected.as_slice())
            );
        }
    }

    #[test]
    fn generate_and_verify_validates_known_hashes_and_generates_unknown_hashes()
    -> Result<(), Box<dyn std::error::Error>> {
        let url: DisplaySafeUrl = "https://example.com/anyio-4.0.0.tar.gz".parse()?;
        let unknown_url: DisplaySafeUrl = "https://example.com/anyio-4.1.0.tar.gz".parse()?;
        let name: PackageName = "anyio".parse()?;
        let version: Version = "4.0.0".parse()?;
        let unknown_version: Version = "4.1.0".parse()?;
        let digest = HashDigest::from_str(
            "sha256:cfdb2b588b9fc25ede96d8db56ed50848b0b649dca3dd1df0b11f683bb9e0b5f",
        )?;
        let hashes = FxHashMap::from_iter([
            (VersionId::from_url(&url), vec![digest.clone()]),
            (
                VersionId::from_registry(name.clone(), version.clone()),
                vec![digest.clone()],
            ),
        ]);
        let strategy = HashStrategy::collect(HashCollection::All)
            .with_verification(HashVerification::IfPresent(Arc::new(hashes)));

        assert_eq!(
            strategy.archive_policy_for_url(&url),
            ArchiveHashPolicy::All(slice::from_ref(&digest))
        );
        for fragment in [
            "#subdirectory=.",
            "#subdirectory=./",
            "#subdirectory=",
            "#subdirectory=nested/..",
        ] {
            let root_url = format!("{url}{fragment}").parse()?;
            assert_eq!(
                strategy.archive_policy_for_url(&root_url),
                ArchiveHashPolicy::All(slice::from_ref(&digest))
            );
        }
        assert_eq!(
            strategy.archive_policy_for_url(&unknown_url),
            ArchiveHashPolicy::Generate
        );
        assert_eq!(
            strategy.metadata_policy_for_url(&unknown_url),
            MetadataHashPolicy {
                collection: HashCollection::All,
                validation: HashValidation::None,
            }
        );
        assert_eq!(
            strategy.metadata_policy_for_url(&url),
            MetadataHashPolicy {
                collection: HashCollection::All,
                validation: HashValidation::All(slice::from_ref(&digest)),
            }
        );
        assert_eq!(
            strategy.archive_policy_for_package(&name, &version),
            ArchiveHashPolicy::Any(slice::from_ref(&digest))
        );
        assert_eq!(
            strategy.archive_policy_for_package(&name, &unknown_version),
            ArchiveHashPolicy::Generate
        );

        Ok(())
    }

    #[test]
    fn required_hashes_take_precedence_over_collection() -> Result<(), Box<dyn std::error::Error>> {
        let url: DisplaySafeUrl = "https://example.com/anyio-4.0.0.tar.gz".parse()?;
        let name: PackageName = "anyio".parse()?;
        let version: Version = "4.0.0".parse()?;
        let strategy = HashStrategy::collect(HashCollection::All)
            .with_verification(HashVerification::Required(Arc::default()));

        assert_eq!(
            strategy.archive_policy_for_url(&url),
            ArchiveHashPolicy::All(&[])
        );
        assert_eq!(
            strategy.metadata_policy_for_url(&url),
            MetadataHashPolicy {
                collection: HashCollection::All,
                validation: HashValidation::All(&[]),
            }
        );
        assert_eq!(
            strategy.archive_policy_for_package(&name, &version),
            ArchiveHashPolicy::Any(&[])
        );
        assert!(!strategy.allows_url(&url));
        assert!(!strategy.allows_package(&name, &version));

        let digest = HashDigest::from_str(
            "sha256:cfdb2b588b9fc25ede96d8db56ed50848b0b649dca3dd1df0b11f683bb9e0b5f",
        )?;
        let constraints = HashStrategy::verify(Arc::new(FxHashMap::from_iter([
            (VersionId::from_url(&url), vec![digest.clone()]),
            (
                VersionId::from_registry(name.clone(), version.clone()),
                vec![HashDigest::from_str(
                    "md5:420d85e19168705cdf0223621b18831a",
                )?],
            ),
        ])));
        for strategy in [
            strategy.clone().with_constraint_hashes(&constraints)?,
            constraints.with_constraint_hashes(&strategy)?,
        ] {
            assert_eq!(
                strategy.archive_policy_for_url(&url),
                ArchiveHashPolicy::All(slice::from_ref(&digest))
            );
            assert_eq!(
                strategy.archive_policy_for_package(&name, &version),
                ArchiveHashPolicy::Any(&[])
            );
            assert!(!strategy.allows_package(&name, &version));
        }
        Ok(())
    }

    #[test]
    fn constraint_hashes_combine_with_locked_hashes() -> Result<(), Box<dyn std::error::Error>> {
        let name: PackageName = "anyio".parse()?;
        let version: Version = "4.0.0".parse()?;
        let local_version: Version = "4.0.0+local".parse()?;
        let digest = HashDigest::from_str(
            "sha256:cfdb2b588b9fc25ede96d8db56ed50848b0b649dca3dd1df0b11f683bb9e0b5f",
        )?;
        let other = HashDigest::from_str(
            "sha256:f7ed51751b2c2add651e5747c891b47e26d2a21be5d32d9311dfe9692f3e5d7a",
        )?;
        let sha512 = HashDigest::from_str(
            "sha512:f30761c1e8725b49c498273b90dba4b05c0fd157811994c806183062cb6647e773364ce45f0e1ff0b10e32fe6d0232ea5ad39476ccf37109d6b49603a09c11c2",
        )?;
        let registry = VersionId::from_registry(name.clone(), version);
        let local_registry = VersionId::from_registry(name, local_version);
        let archive = VersionId::from_url(&"https://example.com/anyio-4.0.0.tar.gz".parse()?);
        let path = VersionId::from_path(Path::new("anyio-4.0.0.tar.gz"));
        for (id, constraint_id, constraint, expected) in [
            (
                registry.clone(),
                registry,
                digest.clone(),
                vec![digest.clone()],
            ),
            (
                local_registry,
                VersionId::from_registry("anyio".parse()?, "4.0.0".parse()?),
                digest.clone(),
                vec![digest.clone()],
            ),
            (path.clone(), path, digest.clone(), vec![digest.clone()]),
            (
                archive.clone(),
                archive,
                sha512.clone(),
                vec![digest.clone(), sha512],
            ),
        ] {
            let locked = HashStrategy::collect(HashCollection::All).with_verification(
                HashVerification::IfPresent(Arc::new(FxHashMap::from_iter([(
                    id.clone(),
                    vec![digest.clone()],
                )]))),
            );
            let constraints = HashStrategy::verify(Arc::new(FxHashMap::from_iter([(
                constraint_id.clone(),
                vec![constraint],
            )])));
            let combined = locked.clone().with_constraint_hashes(&constraints)?;
            assert_eq!(combined.collection(), HashCollection::All);
            assert_eq!(combined.hashes_for_id(&id), Some(expected.as_slice()));

            let conflicting = HashStrategy::verify(Arc::new(FxHashMap::from_iter([(
                constraint_id,
                vec![other.clone()],
            )])));
            assert!(locked.with_constraint_hashes(&conflicting).is_err());
        }
        Ok(())
    }

    #[test]
    fn locked_build_hashes_scope_wheels_and_sources() -> Result<(), Box<dyn std::error::Error>> {
        let index = IndexUrl::parse("https://example.com/simple", None)?;
        let other_index = IndexUrl::parse("https://example.org/simple", None)?;
        let wheel: WheelFilename = "demo_pkg-1.0.0-1-py3-none-any.whl".parse()?;
        let other_wheel: WheelFilename = "demo_pkg-1.0.0-2-py3-none-any.whl".parse()?;
        let short_version_wheel: WheelFilename = "demo_pkg-1.0-1-py3-none-any.whl".parse()?;
        let local_wheel: WheelFilename = "demo_pkg-1.0.0+local-py3-none-any.whl".parse()?;
        let wheel_hash = HashDigest::from_str(
            "sha256:cfdb2b588b9fc25ede96d8db56ed50848b0b649dca3dd1df0b11f683bb9e0b5f",
        )?;
        let source_hash = HashDigest::from_str(
            "sha256:53a42340ae36747fb1471f9b4b7958be1f6e2e5fc234f931aafa3e454fd31dfb",
        )?;
        let expected = vec![wheel_hash.clone(), source_hash.clone()];
        let hashes = Arc::new(FxHashMap::from_iter([(
            VersionId::from_registry(wheel.name.clone(), wheel.version.clone()),
            expected.clone(),
        )]));
        let mut registry = LockedRegistryHashes::default();
        registry.insert(
            RegistryHashTarget::wheel(&index, &wheel),
            wheel_hash.clone(),
        );
        registry.insert(
            RegistryHashTarget::source(&index, &wheel.name, &wheel.version, None),
            source_hash.clone(),
        );
        let strategy = HashStrategy::verify_build(hashes.clone(), registry.clone());

        // Changed or missing index hashes cannot change a known artifact's authority.
        for advertised in [&[][..], slice::from_ref(&source_hash)] {
            assert_eq!(
                strategy.archive_policy_for_registry(
                    RegistryHashTarget::wheel(&index, &wheel),
                    advertised
                ),
                ArchiveHashPolicy::Any(slice::from_ref(&wheel_hash)),
            );
        }
        let equivalent_index = IndexUrl::parse("https://user:password@example.com/simple/", None)?;
        assert_eq!(
            strategy.archive_policy_for_registry(
                RegistryHashTarget::wheel(&equivalent_index, &wheel),
                &[]
            ),
            ArchiveHashPolicy::Any(slice::from_ref(&wheel_hash)),
        );
        assert_eq!(
            strategy
                .archive_policy_for_registry(RegistryHashTarget::wheel(&index, &other_wheel), &[]),
            ArchiveHashPolicy::None,
        );
        assert_eq!(
            strategy.archive_policy_for_registry(
                RegistryHashTarget::wheel(&index, &short_version_wheel),
                &[]
            ),
            ArchiveHashPolicy::None,
        );
        assert_eq!(
            strategy.archive_policy_for_registry(
                RegistryHashTarget::wheel(&index, &other_wheel),
                slice::from_ref(&wheel_hash),
            ),
            ArchiveHashPolicy::Any(&expected),
        );
        assert_eq!(
            strategy
                .archive_policy_for_registry(RegistryHashTarget::wheel(&other_index, &wheel), &[]),
            ArchiveHashPolicy::None,
        );
        for (index, wheel, advertised, comparison) in [
            (
                &index,
                &wheel,
                slice::from_ref(&source_hash),
                HashComparison::Mismatched,
            ),
            (&index, &wheel, &[][..], HashComparison::Missing),
            (&other_index, &wheel, &[][..], HashComparison::Unrecorded),
            (
                &other_index,
                &wheel,
                slice::from_ref(&wheel_hash),
                HashComparison::Matched,
            ),
            (&index, &other_wheel, &[][..], HashComparison::Unrecorded),
            (
                &index,
                &short_version_wheel,
                &[][..],
                HashComparison::Unrecorded,
            ),
            (
                &other_index,
                &other_wheel,
                slice::from_ref(&wheel_hash),
                HashComparison::Matched,
            ),
        ] {
            assert_eq!(
                strategy.locked_registry_hash_comparison(
                    RegistryHashTarget::wheel(index, wheel),
                    advertised,
                ),
                Some(comparison),
            );
        }

        // Source candidates and cached source revisions use the source-scoped policy.
        assert_eq!(
            strategy.locked_registry_hash_comparison(
                RegistryHashTarget::source(&index, &wheel.name, &wheel.version, None),
                slice::from_ref(&wheel_hash),
            ),
            Some(HashComparison::Mismatched),
        );
        assert_eq!(
            strategy.locked_registry_validation(
                &wheel.name,
                &wheel.version,
                registry.get(RegistryHashTarget::source(
                    &index,
                    &wheel.name,
                    &wheel.version,
                    None
                )),
                slice::from_ref(&wheel_hash),
            ),
            HashValidation::Any(slice::from_ref(&source_hash)),
        );
        assert_eq!(
            registry.get(RegistryHashTarget::source(
                &other_index,
                &wheel.name,
                &wheel.version,
                None
            )),
            None,
        );
        assert_eq!(
            strategy.archive_policy_for_registry(
                RegistryHashTarget::source(&index, &wheel.name, &wheel.version, Some(&local_wheel)),
                slice::from_ref(&wheel_hash),
            ),
            ArchiveHashPolicy::Any(slice::from_ref(&source_hash)),
        );

        // Lock identities are exact; a user-authored public-version pin still covers local builds.
        assert_eq!(
            strategy.archive_policy_for_registry(
                RegistryHashTarget::wheel(&index, &local_wheel),
                slice::from_ref(&wheel_hash),
            ),
            ArchiveHashPolicy::None,
        );
        assert_eq!(
            HashStrategy::verify(hashes.clone())
                .archive_policy_for_package(&local_wheel.name, &local_wheel.version),
            ArchiveHashPolicy::Any(&expected),
        );
        assert_eq!(
            HashStrategy::require(hashes.clone()).archive_policy_for_registry(
                RegistryHashTarget::source(&index, &wheel.name, &wheel.version, Some(&local_wheel)),
                slice::from_ref(&source_hash),
            ),
            ArchiveHashPolicy::Any(&[]),
        );
        assert_eq!(
            HashStrategy::require(hashes.clone()).locked_registry_hash_comparison(
                RegistryHashTarget::wheel(&index, &local_wheel),
                &[],
            ),
            None,
        );
        let mut local_pins = hashes.as_ref().clone();
        local_pins.insert(
            VersionId::from_registry(local_wheel.name.clone(), local_wheel.version.clone()),
            vec![wheel_hash.clone()],
        );
        let local_pins = Arc::new(local_pins);
        for strategy in [
            HashStrategy::verify(local_pins.clone()),
            HashStrategy::require(local_pins),
        ] {
            assert_eq!(
                strategy.archive_policy_for_registry(
                    RegistryHashTarget::source(
                        &index,
                        &wheel.name,
                        &wheel.version,
                        Some(&local_wheel)
                    ),
                    slice::from_ref(&source_hash),
                ),
                ArchiveHashPolicy::Any(slice::from_ref(&wheel_hash)),
            );
        }
        Ok(())
    }

    #[test]
    fn locked_build_hashes_intersect_constraints_per_artifact()
    -> Result<(), Box<dyn std::error::Error>> {
        let index = IndexUrl::parse("https://example.com/simple", None)?;
        let locked_wheel: WheelFilename = "demo_pkg-1.0.0-1-py3-none-any.whl".parse()?;
        let other_wheel: WheelFilename = "demo_pkg-1.0.0-2-py3-none-any.whl".parse()?;
        let locked_hash = HashDigest::from_str(
            "sha256:cfdb2b588b9fc25ede96d8db56ed50848b0b649dca3dd1df0b11f683bb9e0b5f",
        )?;
        let allowed_hash = HashDigest::from_str(
            "sha256:53a42340ae36747fb1471f9b4b7958be1f6e2e5fc234f931aafa3e454fd31dfb",
        )?;
        let id = VersionId::from_registry(locked_wheel.name.clone(), locked_wheel.version.clone());
        let mut registry = LockedRegistryHashes::default();
        registry.insert(
            RegistryHashTarget::wheel(&index, &locked_wheel),
            locked_hash.clone(),
        );
        let locked = HashStrategy::verify_build(
            Arc::new(FxHashMap::from_iter([(
                id.clone(),
                vec![locked_hash.clone()],
            )])),
            registry,
        );
        let constraints = HashStrategy::verify(Arc::new(FxHashMap::from_iter([(
            id,
            vec![allowed_hash.clone()],
        )])));
        let constrained = locked.clone().with_constraint_hashes(&constraints)?;

        // Both the locked artifact and the explicit constraint must authorize a recorded wheel.
        assert_eq!(
            constrained
                .archive_policy_for_registry(RegistryHashTarget::wheel(&index, &locked_wheel), &[]),
            ArchiveHashPolicy::Any(&[]),
        );
        // A wheel absent from the lock is governed by the explicit constraint on its own.
        assert_eq!(
            constrained
                .archive_policy_for_registry(RegistryHashTarget::wheel(&index, &other_wheel), &[]),
            ArchiveHashPolicy::Any(slice::from_ref(&allowed_hash)),
        );
        // Cloning a strategy must not change the original lock-backed policy.
        assert_eq!(
            locked
                .archive_policy_for_registry(RegistryHashTarget::wheel(&index, &locked_wheel), &[]),
            ArchiveHashPolicy::Any(slice::from_ref(&locked_hash)),
        );
        Ok(())
    }
}
