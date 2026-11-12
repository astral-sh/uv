use std::collections::hash_map::Entry;
use std::sync::Arc;

use rustc_hash::FxHashMap;

use uv_distribution_types::{DistributionId, Identifier, ResolvedDist};
use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_pep508::VariantMarkerEnvironment;

use crate::ResolveError;
use crate::candidate_selector::Candidate;
use crate::prioritized_distribution::CompatibleDist;
use crate::resolver::{RegisteredMetadata, RegisteredVariantsJson};

#[derive(Clone, Debug)]
struct FilePin<'index> {
    source: PinSource<'index>,
    variant: Option<Arc<VariantMarkerEnvironment>>,
}

#[derive(Clone, Debug)]
enum PinSource<'index> {
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

/// The registered metadata and selected artifact used to expand a package's dependencies.
pub(crate) struct RegisteredPin<'pin, 'index> {
    pub(crate) metadata: &'pin RegisteredMetadata<'index>,
    pub(crate) dist: Option<&'pin ResolvedDist>,
    pub(crate) variant: Option<&'pin VariantMarkerEnvironment>,
}

/// The artifacts and metadata selected for package versions within a fork.
#[derive(Clone, Debug, Default)]
pub(crate) struct FilePins<'index>(FxHashMap<(PackageName, Version), FilePin<'index>>);

impl<'index> FilePins<'index> {
    /// Pin a registry candidate, registering its metadata at most once in this fork.
    ///
    /// Within a fork, each `(name, version)` selects the same artifact. Proxy packages may pin it
    /// before its real package is selected; upgrade that pin when metadata is requested.
    pub(crate) fn insert(
        &mut self,
        candidate: &Candidate,
        dist: &CompatibleDist,
        variants_json: Option<RegisteredVariantsJson<'index>>,
        variant: Option<VariantMarkerEnvironment>,
        request: Option<impl FnOnce() -> Result<RegisteredMetadata<'index>, ResolveError>>,
    ) -> Result<(), ResolveError> {
        match self
            .0
            .entry((candidate.name().clone(), candidate.version().clone()))
        {
            Entry::Occupied(mut entry) => {
                if let PinSource::Registry {
                    variants_json: pinned_variants_json,
                    ..
                } = &mut entry.get_mut().source
                    && pinned_variants_json.is_none()
                {
                    *pinned_variants_json = variants_json;
                }
                if let Some(request) = request
                    && let PinSource::Registry {
                        metadata: metadata @ PinMetadata::Unrequested(_),
                        ..
                    } = &mut entry.get_mut().source
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
                entry.insert(FilePin {
                    variant: variant.map(Arc::new),
                    source: PinSource::Registry {
                        dist: dist.for_installation().to_owned(),
                        metadata,
                        variants_json,
                    },
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
        variant: Option<VariantMarkerEnvironment>,
    ) {
        self.0
            .entry((name.clone(), version.clone()))
            .or_insert_with(|| FilePin {
                source: PinSource::Url(metadata),
                variant: variant.map(Arc::new),
            });
    }

    /// Return the pinned registry artifact, if one exists.
    pub(crate) fn get(&self, name: &PackageName, version: &Version) -> Option<&ResolvedDist> {
        match &self.0.get(&(name.clone(), version.clone()))?.source {
            PinSource::Registry { dist, .. } => Some(dist),
            PinSource::Url(_) => None,
        }
    }

    /// Return the registered metadata and the context of the selected wheel.
    pub(crate) fn metadata(
        &self,
        name: &PackageName,
        version: &Version,
    ) -> Option<RegisteredPin<'_, 'index>> {
        let pin = self.0.get(&(name.clone(), version.clone()))?;
        let (metadata, dist) = match &pin.source {
            PinSource::Registry {
                metadata: PinMetadata::Registered(metadata),
                dist,
                ..
            } => (metadata, Some(dist)),
            PinSource::Url(metadata) => (metadata, None),
            PinSource::Registry {
                metadata: PinMetadata::Unrequested(_),
                ..
            } => return None,
        };
        Some(RegisteredPin {
            metadata,
            dist,
            variant: pin.variant.as_deref(),
        })
    }

    /// Return the variant metadata registered when selecting this package version.
    pub(crate) fn variants_json(
        &self,
        name: &PackageName,
        version: &Version,
    ) -> Option<&RegisteredVariantsJson<'index>> {
        match &self.0.get(&(name.clone(), version.clone()))?.source {
            PinSource::Registry { variants_json, .. } => variants_json.as_ref(),
            PinSource::Url(_) => None,
        }
    }

    /// Return the pinned registry artifact and its metadata identity in a single lookup.
    pub(crate) fn dist_and_id(
        &self,
        name: &PackageName,
        version: &Version,
    ) -> Option<(&ResolvedDist, &DistributionId)> {
        match &self.0.get(&(name.clone(), version.clone()))?.source {
            PinSource::Registry { dist, metadata, .. } => Some((
                dist,
                match metadata {
                    PinMetadata::Unrequested(id) => id,
                    PinMetadata::Registered(metadata) => metadata.id(),
                },
            )),
            PinSource::Url(_) => None,
        }
    }
}
