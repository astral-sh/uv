use std::borrow::Cow;
use std::collections::{BTreeMap, hash_map::Entry};
use std::sync::Arc;

use rustc_hash::FxHashMap;

use uv_configuration::Constraints;
use uv_distribution_types::{Dist, DistributionId, Identifier, ResolvedDist};
use uv_normalize::PackageName;
use uv_pep440::Version;

use crate::ResolveError;
use crate::candidate_selector::Candidate;
use crate::prioritized_distribution::CompatibleDist;
use crate::pubgrub::{DependencySource, PubGrubDependency, PubGrubPackage, Range};
use crate::resolver::UnsatisfiableRequirement;
use crate::resolver::{RegisteredMetadata, RegisteredVariantsJson};
use uv_pep508::MarkerEnvironment;
use uv_pypi_types::Conflicts;

#[derive(Clone, Debug)]
enum FilePin<'index> {
    Registry {
        /// The concrete distribution chosen for installation and locking.
        dist: ResolvedDist,
        /// The concrete distribution whose metadata is used during resolution.
        metadata: PinMetadata<'index>,
        /// The index-level variant metadata for the selected package version.
        variants_json: Option<RegisteredVariantsJson<'index>>,
    },
    Url(RegisteredMetadata<'index>),
}

#[derive(Clone, Debug)]
enum PinMetadata<'index> {
    /// Proxy selection and direct-only resolution do not require a metadata request.
    Unrequested(DistributionId),
    Registered(RegisteredMetadata<'index>),
}

/// The artifacts and metadata selected for package versions within a fork.
#[derive(Clone, Debug, Default)]
pub(crate) struct FilePins<'index> {
    files: FxHashMap<(PackageName, Version), FilePin<'index>>,
    pub(crate) wheels: FxHashMap<(PackageName, Version), BTreeMap<Version, WheelChoice<'index>>>,
}

/// One range per required ABI dependency, with alternatives for that dependency combined.
pub(crate) type AbiRequirements = Vec<(PackageName, Range<Version>)>;

/// One possible installation artifact of a release with ABI-dependent wheels.
#[derive(Clone, Debug)]
pub(crate) struct WheelChoice<'index> {
    pub(crate) dist: Arc<Dist>,
    pub(crate) abi: AbiRequirements,
    pub(crate) metadata: Option<RegisteredMetadata<'index>>,
}

impl WheelChoice<'_> {
    pub(crate) fn new(dist: Arc<Dist>, abi: AbiRequirements) -> Self {
        Self {
            dist,
            abi,
            metadata: None,
        }
    }

    /// Apply the ABI constraints even when the wheel omits the dependency from Requires-Dist.
    pub(crate) fn dependencies(
        &self,
        parent: &PackageName,
        constraints: &Constraints,
        conflicts: &Conflicts,
        env: Option<&MarkerEnvironment>,
    ) -> Result<Vec<PubGrubDependency>, UnsatisfiableRequirement> {
        let constraints = self
            .abi
            .iter()
            .flat_map(|(name, _)| constraints.get(name).into_iter().flatten())
            .filter(|requirement| requirement.evaluate_markers(env, &[]))
            .map(Cow::Borrowed);
        let mut dependencies =
            PubGrubDependency::from_requirements(conflicts, constraints, None, None)?;
        dependencies.extend(self.abi.iter().map(|(name, versions)| PubGrubDependency {
            package: PubGrubPackage::base(name.clone()),
            version: versions.clone(),
            parent: Some(parent.clone()),
            source: DependencySource::Unspecified,
        }));
        Ok(dependencies)
    }
}

impl<'index> FilePins<'index> {
    /// Pin the solver's final wheel decision, including the metadata read for that wheel.
    pub(crate) fn pin_wheel(&mut self, name: &PackageName, release: &Version, rank: &Version) {
        let key = (name.clone(), release.clone());
        let choice = &self.wheels[&key][rank];
        self.files.insert(
            key,
            FilePin::Registry {
                dist: ResolvedDist::Installable {
                    dist: choice.dist.clone(),
                    version: Some(release.clone()),
                },
                metadata: choice.metadata.clone().map_or_else(
                    || PinMetadata::Unrequested(choice.dist.distribution_id()),
                    PinMetadata::Registered,
                ),
                variants_json: None,
            },
        );
    }

    /// Pin a registry candidate, registering its metadata at most once in this fork.
    ///
    /// Within a fork, each `(name, version)` selects the same artifact. Proxy packages may pin it
    /// before its real package is selected; upgrade that pin when metadata is requested.
    pub(crate) fn insert(
        &mut self,
        candidate: &Candidate,
        dist: &CompatibleDist,
        variants_json: Option<RegisteredVariantsJson<'index>>,
        request: Option<impl FnOnce() -> Result<RegisteredMetadata<'index>, ResolveError>>,
    ) -> Result<(), ResolveError> {
        match self
            .files
            .entry((candidate.name().clone(), candidate.version().clone()))
        {
            Entry::Occupied(mut entry) => {
                if let FilePin::Registry {
                    variants_json: pinned_variants_json,
                    ..
                } = entry.get_mut()
                    && pinned_variants_json.is_none()
                {
                    *pinned_variants_json = variants_json;
                }
                if let Some(request) = request
                    && let FilePin::Registry {
                        metadata: metadata @ PinMetadata::Unrequested(_),
                        ..
                    } = entry.get_mut()
                {
                    *metadata = PinMetadata::Registered(request()?);
                }
            }
            Entry::Vacant(entry) => {
                let metadata = if let Some(request) = request {
                    PinMetadata::Registered(request()?)
                } else {
                    PinMetadata::Unrequested(dist.for_resolution().distribution_id())
                };
                entry.insert(FilePin::Registry {
                    dist: dist.for_installation().to_owned(),
                    metadata,
                    variants_json,
                });
            }
        }
        Ok(())
    }

    /// Retain the metadata used to select a URL package's version.
    pub(crate) fn insert_url(
        &mut self,
        name: &PackageName,
        version: &Version,
        metadata: RegisteredMetadata<'index>,
    ) {
        self.files
            .entry((name.clone(), version.clone()))
            .or_insert(FilePin::Url(metadata));
    }

    /// Return the pinned registry artifact, if one exists.
    pub(crate) fn get(&self, name: &PackageName, version: &Version) -> Option<&ResolvedDist> {
        match self.files.get(&(name.clone(), version.clone()))? {
            FilePin::Registry { dist, .. } => Some(dist),
            FilePin::Url(_) => None,
        }
    }

    /// Return the metadata registered when selecting this package version.
    pub(crate) fn metadata(
        &self,
        name: &PackageName,
        version: &Version,
    ) -> Option<&RegisteredMetadata<'index>> {
        match self.files.get(&(name.clone(), version.clone()))? {
            FilePin::Registry {
                metadata: PinMetadata::Registered(metadata),
                ..
            }
            | FilePin::Url(metadata) => Some(metadata),
            FilePin::Registry {
                metadata: PinMetadata::Unrequested(_),
                ..
            } => None,
        }
    }

    /// Return the variant metadata registered when selecting this package version.
    pub(crate) fn variants_json(
        &self,
        name: &PackageName,
        version: &Version,
    ) -> Option<&RegisteredVariantsJson<'index>> {
        match self.files.get(&(name.clone(), version.clone()))? {
            FilePin::Registry { variants_json, .. } => variants_json.as_ref(),
            FilePin::Url(_) => None,
        }
    }

    /// Return the pinned registry artifact and its metadata identity in a single lookup.
    pub(crate) fn dist_and_id(
        &self,
        name: &PackageName,
        version: &Version,
    ) -> Option<(&ResolvedDist, &DistributionId)> {
        match self.files.get(&(name.clone(), version.clone()))? {
            FilePin::Registry { dist, metadata, .. } => Some((
                dist,
                match metadata {
                    PinMetadata::Unrequested(id) => id,
                    PinMetadata::Registered(metadata) => metadata.id(),
                },
            )),
            FilePin::Url(_) => None,
        }
    }
}
