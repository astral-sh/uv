use std::fmt::Display;
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;

use rustc_hash::FxHashMap;

use uv_configuration::HashCheckingMode;
use uv_distribution_filename::{DistExtension, WheelFilename};
use uv_distribution_types::{
    ArchiveHashPolicy, DistributionMetadata, HashCollection, HashComparison, HashValidation,
    IndexUrl, MetadataHashPolicy, Name, RegistryVersionId, Requirement, RequirementSource,
    Resolution, UnresolvedRequirement, VersionId,
};
use uv_normalize::PackageName;
use uv_pep440::{Operator, Version};
use uv_pypi_types::{HashAlgorithm, HashDigest, HashDigests, HashError, ResolverMarkerEnvironment};
use uv_redacted::DisplaySafeUrl;

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
        /// All recorded hashes, used for direct URLs and registry artifact preferences.
        hashes: Arc<FxHashMap<VersionId, Vec<HashDigest>>>,
        /// Registry artifacts, scoped to their source and exact version.
        registry: Arc<LockedRegistryHashes>,
    },
    /// Every distribution must have a matching trusted hash.
    Required(Arc<FxHashMap<VersionId, Vec<HashDigest>>>),
}

/// The registry artifacts recorded in a lockfile for isolated build verification.
#[derive(Debug, Default, Clone)]
pub struct LockedRegistryHashes {
    packages: FxHashMap<RegistryVersionId, LockedRegistryPackageHashes>,
}

#[derive(Debug, Default, Clone)]
struct LockedRegistryPackageHashes {
    /// A lockfile can omit wheels unreachable under its runtime markers.
    wheels: FxHashMap<WheelFilename, Vec<HashDigest>>,
    /// Source archives for a known source and version must match a recorded source hash.
    sources: Vec<HashDigest>,
}

impl LockedRegistryHashes {
    /// Record a wheel's trusted hash under its source and complete filename.
    pub fn insert_wheel(&mut self, index: &IndexUrl, filename: &WheelFilename, hash: HashDigest) {
        let hashes = self
            .packages
            .entry(RegistryVersionId::new(
                &filename.name,
                &filename.version,
                index,
            ))
            .or_default()
            .wheels
            .entry(filename.clone())
            .or_default();
        if !hashes.contains(&hash) {
            hashes.push(hash);
        }
    }

    /// Record a source archive's trusted hash under its source and exact version.
    pub fn insert_source(
        &mut self,
        index: &IndexUrl,
        name: &PackageName,
        version: &Version,
        hash: HashDigest,
    ) {
        let hashes = &mut self
            .packages
            .entry(RegistryVersionId::new(name, version, index))
            .or_default()
            .sources;
        if !hashes.contains(&hash) {
            hashes.push(hash);
        }
    }

    fn artifact_hashes(
        &self,
        name: &PackageName,
        version: &Version,
        index: &IndexUrl,
        filename: &str,
    ) -> Option<&[HashDigest]> {
        let package = self
            .packages
            .get(&RegistryVersionId::new(name, version, index))?;
        match DistExtension::from_path(filename) {
            Ok(DistExtension::Wheel) => WheelFilename::from_str(filename)
                .ok()
                .and_then(|filename| package.wheels.get(&filename))
                .map(Vec::as_slice),
            Ok(DistExtension::Source(_)) => {
                (!package.sources.is_empty()).then_some(package.sources.as_slice())
            }
            Err(_) => None,
        }
    }

    fn source_hashes(
        &self,
        name: &PackageName,
        version: &Version,
        index: &IndexUrl,
    ) -> Option<&[HashDigest]> {
        let package = self
            .packages
            .get(&RegistryVersionId::new(name, version, index))?;
        (!package.sources.is_empty()).then_some(package.sources.as_slice())
    }

    fn wheel_hashes(&self, index: &IndexUrl, filename: &WheelFilename) -> Option<&[HashDigest]> {
        self.packages
            .get(&RegistryVersionId::new(
                &filename.name,
                &filename.version,
                index,
            ))?
            .wheels
            .get(filename)
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
        Self::default().with_verification(HashVerification::LockedBuild {
            hashes,
            registry: Arc::new(registry),
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

    /// Return the policy for a downloaded wheel in the registry cache.
    pub fn archive_policy_for_registry_wheel(
        &self,
        index: &IndexUrl,
        filename: &WheelFilename,
        computed: &[HashDigest],
    ) -> ArchiveHashPolicy<'_> {
        let validation = match &self.verification {
            HashVerification::LockedBuild { hashes, registry } => registry
                .wheel_hashes(index, filename)
                .map(HashValidation::Any)
                .unwrap_or_else(|| {
                    relocated_registry_validation(
                        hashes,
                        &filename.name,
                        &filename.version,
                        computed,
                    )
                }),
            HashVerification::None
            | HashVerification::IfPresent(_)
            | HashVerification::Required(_) => self.validation_for_id(|| {
                VersionId::from_registry(filename.name.clone(), filename.version.clone())
            }),
        };
        self.archive_policy_from_validation(validation)
    }

    /// Return the policy for a cached wheel built from a registry source archive.
    ///
    /// The source cache retains the original source version, which may differ from the built wheel
    /// version, but not the source filename. If that source and version has recorded source hashes,
    /// a different revision must be resolved again with its full identity.
    pub fn archive_policy_for_cached_source(
        &self,
        name: &PackageName,
        version: &Version,
        index: &IndexUrl,
        wheel: &WheelFilename,
        computed: &[HashDigest],
    ) -> ArchiveHashPolicy<'_> {
        let validation = match &self.verification {
            HashVerification::LockedBuild { hashes, registry } => registry
                .source_hashes(name, version, index)
                .map(HashValidation::Any)
                .unwrap_or_else(|| relocated_registry_validation(hashes, name, version, computed)),
            HashVerification::None
            | HashVerification::IfPresent(_)
            | HashVerification::Required(_) => self.validation_for_id(|| {
                VersionId::from_registry(wheel.name.clone(), wheel.version.clone())
            }),
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
        name: &PackageName,
        version: &Version,
        index: &IndexUrl,
        filename: &str,
        advertised: &[HashDigest],
    ) -> Option<HashComparison> {
        let HashVerification::LockedBuild { hashes, registry } = &self.verification else {
            return None;
        };
        if let Some(expected) = registry.artifact_hashes(name, version, index, filename) {
            return Some(compare_hashes(ArchiveHashPolicy::Any(expected), advertised));
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
        if let HashVerification::LockedBuild { .. } = &self.verification
            && let Some(target) = distribution.registry_hash_target()
        {
            return self.validation_for_registry(
                target.name,
                target.version,
                target.index,
                target.file.filename.as_ref(),
                target.file.hashes.as_slice(),
            );
        }
        self.validation_for_id(|| distribution.version_id())
    }

    fn validation_for_registry(
        &self,
        name: &PackageName,
        version: &Version,
        index: &IndexUrl,
        filename: &str,
        advertised: &[HashDigest],
    ) -> HashValidation<'_> {
        match &self.verification {
            HashVerification::LockedBuild { hashes, registry } => registry
                .artifact_hashes(name, version, index, filename)
                .map(HashValidation::Any)
                .unwrap_or_else(|| {
                    relocated_registry_validation(hashes, name, version, advertised)
                }),
            HashVerification::None
            | HashVerification::IfPresent(_)
            | HashVerification::Required(_) => {
                self.validation_for_id(|| VersionId::from_registry(name.clone(), version.clone()))
            }
        }
    }

    /// Construct an identity only when verification requires a lookup.
    fn validation_for_id(&self, id: impl FnOnce() -> VersionId) -> HashValidation<'_> {
        match &self.verification {
            HashVerification::IfPresent(hashes) => {
                let id = id();
                if let Some(hashes) = hashes.get(&id) {
                    return hash_validation(&id, hashes);
                }
                // `==1.0.0` can also select `1.0.0+local`. If the local version has no hash
                // of its own, check it against the hash for `1.0.0`.
                if let VersionId::NameVersion(name, version) = &id
                    && version.is_local()
                    && let Some(hashes) = hashes.get(&VersionId::from_registry(
                        name.clone(),
                        version.clone().without_local(),
                    ))
                {
                    return HashValidation::Any(hashes);
                }
            }
            HashVerification::Required(hashes) => {
                let id = id();
                return hash_validation(
                    &id,
                    hashes.get(&id).map(Vec::as_slice).unwrap_or_default(),
                );
            }
            HashVerification::LockedBuild { hashes, .. } => {
                let id = id();
                // Registry build hashes require a concrete source and artifact. In particular,
                // exact lockfile versions do not inherit hashes from a public-version pin.
                if let VersionId::NameVersion(..) = id {
                    return HashValidation::None;
                }
                if let Some(hashes) = hashes.get(&id) {
                    return hash_validation(&id, hashes);
                }
            }
            HashVerification::None => {}
        }
        HashValidation::None
    }

    /// Returns `true` if the given registry-based package is allowed.
    pub fn allows_package(&self, name: &PackageName, version: &Version) -> bool {
        match &self.verification {
            HashVerification::Required(hashes) => {
                hashes.contains_key(&VersionId::from_registry(name.clone(), version.clone()))
            }
            HashVerification::None
            | HashVerification::IfPresent(_)
            | HashVerification::LockedBuild { .. } => true,
        }
    }

    /// Returns `true` if the given direct URL package is allowed.
    pub fn allows_url(&self, url: &DisplaySafeUrl) -> bool {
        match &self.verification {
            HashVerification::Required(hashes) => hashes.contains_key(&VersionId::from_url(url)),
            HashVerification::None
            | HashVerification::IfPresent(_)
            | HashVerification::LockedBuild { .. } => true,
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
            HashVerification::IfPresent(existing)
            | HashVerification::Required(existing)
            | HashVerification::LockedBuild {
                hashes: existing, ..
            } => {
                if let Some(hashes) = Self::augment_hashes(existing, requirements)? {
                    *existing = Arc::new(hashes);
                }
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
        if matches!(&self.verification, HashVerification::Required(_)) {
            return Ok(self);
        }
        self.augment_with_requirements(requirements)
    }

    /// Read the required hashes from a set of [`UnresolvedRequirement`] entries.
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
            if let Some(fragment_hashes) = requirement.hashes().map(HashDigests::from) {
                merge_digests(&mut digests, fragment_hashes.iter(), requirement)?;
            }

            let has_md5 = mode.is_require()
                && digests
                    .iter()
                    .any(|digest| digest.algorithm() == HashAlgorithm::Md5);
            if mode.is_require() {
                digests.retain(|digest| digest.algorithm() != HashAlgorithm::Md5);
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

        // Merge the hashes, preferring requirements over constraints, since overlapping
        // requirements were already merged.
        let hashes: FxHashMap<VersionId, Vec<HashDigest>> = constraint_hashes
            .into_iter()
            .chain(requirement_hashes)
            .collect();
        match mode {
            HashCheckingMode::Verify => Ok(Self::verify(Arc::new(hashes))),
            HashCheckingMode::Require => Ok(Self::require(Arc::new(hashes))),
        }
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

/// Recognize a recorded artifact at a different location without trusting a new index digest.
fn relocated_registry_validation<'a>(
    hashes: &'a FxHashMap<VersionId, Vec<HashDigest>>,
    name: &PackageName,
    version: &Version,
    advertised: &[HashDigest],
) -> HashValidation<'a> {
    if let Some(expected) = hashes.get(&VersionId::from_registry(name.clone(), version.clone()))
        && ArchiveHashPolicy::Any(expected).matches(advertised)
    {
        HashValidation::Any(expected)
    } else {
        HashValidation::None
    }
}

fn compare_hashes(policy: ArchiveHashPolicy<'_>, advertised: &[HashDigest]) -> HashComparison {
    if !policy.requires_validation() {
        HashComparison::Matched
    } else if advertised.is_empty() {
        HashComparison::Missing
    } else if policy.matches(advertised) {
        HashComparison::Matched
    } else {
        HashComparison::Mismatched
    }
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
    use std::slice;
    use std::str::FromStr;
    use std::sync::Arc;

    use rustc_hash::FxHashMap;
    use uv_configuration::HashCheckingMode;
    use uv_distribution_filename::{DistExtension, WheelFilename};
    use uv_distribution_types::{
        ArchiveHashPolicy, HashCollection, HashComparison, HashValidation, IndexUrl,
        MetadataHashPolicy, Requirement, RequirementSource, UnresolvedRequirement, VersionId,
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
        Ok(())
    }

    #[test]
    fn locked_build_hashes_scope_wheels_and_sources() -> Result<(), Box<dyn std::error::Error>> {
        let index = IndexUrl::parse("https://example.com/simple", None)?;
        let other_index = IndexUrl::parse("https://example.org/simple", None)?;
        let wheel: WheelFilename = "demo_pkg-1.0.0-1-py3-none-any.whl".parse()?;
        let other_wheel: WheelFilename = "demo_pkg-1.0.0-2-py3-none-any.whl".parse()?;
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
        registry.insert_wheel(&index, &wheel, wheel_hash.clone());
        registry.insert_source(&index, &wheel.name, &wheel.version, source_hash.clone());
        let strategy = HashStrategy::verify_build(hashes.clone(), registry);

        // Changed or missing index hashes cannot change a known artifact's authority.
        for advertised in [&[][..], slice::from_ref(&source_hash)] {
            assert_eq!(
                strategy.archive_policy_for_registry_wheel(&index, &wheel, advertised),
                ArchiveHashPolicy::Any(slice::from_ref(&wheel_hash)),
            );
        }
        assert_eq!(
            strategy.archive_policy_for_registry_wheel(&index, &other_wheel, &[]),
            ArchiveHashPolicy::None,
        );
        assert_eq!(
            strategy.archive_policy_for_registry_wheel(&other_index, &wheel, &[]),
            ArchiveHashPolicy::None,
        );
        assert_eq!(
            strategy.archive_policy_for_registry_wheel(
                &other_index,
                &other_wheel,
                slice::from_ref(&wheel_hash),
            ),
            ArchiveHashPolicy::Any(&expected),
        );
        assert_eq!(
            strategy.locked_registry_hash_comparison(
                &wheel.name,
                &wheel.version,
                &index,
                &wheel.to_string(),
                slice::from_ref(&source_hash),
            ),
            Some(HashComparison::Mismatched),
        );
        assert_eq!(
            strategy.locked_registry_hash_comparison(
                &wheel.name,
                &wheel.version,
                &index,
                &wheel.to_string(),
                &[],
            ),
            Some(HashComparison::Missing),
        );
        assert_eq!(
            strategy.locked_registry_hash_comparison(
                &other_wheel.name,
                &other_wheel.version,
                &index,
                &other_wheel.to_string(),
                &[],
            ),
            Some(HashComparison::Unrecorded),
        );
        assert_eq!(
            strategy.locked_registry_hash_comparison(
                &other_wheel.name,
                &other_wheel.version,
                &other_index,
                &other_wheel.to_string(),
                slice::from_ref(&wheel_hash),
            ),
            Some(HashComparison::Matched),
        );

        // A renamed source archive and a cached source revision keep the source-scoped policy.
        assert_eq!(
            strategy.validation_for_registry(
                &wheel.name,
                &wheel.version,
                &index,
                "demo__pkg-1.0.0.zip",
                slice::from_ref(&wheel_hash),
            ),
            HashValidation::Any(slice::from_ref(&source_hash)),
        );
        assert_eq!(
            strategy.archive_policy_for_cached_source(
                &wheel.name,
                &wheel.version,
                &index,
                &local_wheel,
                slice::from_ref(&wheel_hash),
            ),
            ArchiveHashPolicy::Any(slice::from_ref(&source_hash)),
        );

        // Lock identities are exact; a user-authored public-version pin still covers local builds.
        assert_eq!(
            strategy.archive_policy_for_registry_wheel(
                &index,
                &local_wheel,
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
            HashStrategy::require(hashes.clone()).archive_policy_for_cached_source(
                &wheel.name,
                &wheel.version,
                &index,
                &local_wheel,
                slice::from_ref(&source_hash),
            ),
            ArchiveHashPolicy::Any(&[]),
        );
        assert_eq!(
            HashStrategy::require(hashes.clone()).locked_registry_hash_comparison(
                &local_wheel.name,
                &local_wheel.version,
                &index,
                &local_wheel.to_string(),
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
                strategy.archive_policy_for_cached_source(
                    &wheel.name,
                    &wheel.version,
                    &index,
                    &local_wheel,
                    slice::from_ref(&source_hash),
                ),
                ArchiveHashPolicy::Any(slice::from_ref(&wheel_hash)),
            );
        }
        Ok(())
    }
}
