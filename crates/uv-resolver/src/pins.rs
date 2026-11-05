use std::collections::hash_map::Entry;

use rustc_hash::FxHashMap;

use uv_distribution_types::{DistributionId, Identifier, ResolvedDist, SelectedVariant};
use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_preview::PreviewFeature;

use crate::ResolveError;
use crate::candidate_selector::Candidate;
use crate::prioritized_distribution::CompatibleDist;
use crate::resolver::{RegisteredMetadata, RegisteredVariantsJson};

#[derive(Clone, Debug)]
enum FilePin<'index> {
    Registry {
        /// The concrete distribution chosen for installation and locking.
        dist: ResolvedDist,
        /// The concrete distribution whose metadata is used during resolution.
        metadata: PinMetadata<'index>,
        /// The index-level variant metadata for the selected package version.
        variants_json: Option<RegisteredVariantsJson<'index>>,
        /// The selected wheel properties used for dependency markers.
        variant: Option<SelectedVariant>,
    },
    Url {
        metadata: RegisteredMetadata<'index>,
        variant: Option<SelectedVariant>,
    },
}

#[derive(Clone, Debug)]
enum PinMetadata<'index> {
    /// Proxy selection and direct-only resolution do not require a metadata request.
    Unrequested(DistributionId),
    Registered(RegisteredMetadata<'index>),
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
        request: Option<impl FnOnce() -> Result<RegisteredMetadata<'index>, ResolveError>>,
    ) -> Result<(), ResolveError> {
        match self
            .0
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
                let variant = match dist {
                    CompatibleDist::InstalledDist(installed)
                        if uv_preview::is_enabled(PreviewFeature::WheelVariants) =>
                    {
                        installed.read_variant_context().map_err(|error| {
                            uv_distribution::Error::ReadInstalled(
                                Box::new((*installed).clone()),
                                error,
                            )
                        })?
                    }
                    CompatibleDist::InstalledDist(_)
                    | CompatibleDist::SourceDist { .. }
                    | CompatibleDist::IncompatibleWheel { .. } => None,
                    CompatibleDist::CompatibleWheel { prioritized, .. } => {
                        prioritized.selected_variant().cloned()
                    }
                };
                entry.insert(FilePin::Registry {
                    dist: dist.for_installation().to_owned(),
                    metadata,
                    variants_json,
                    variant,
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
        variant: Option<SelectedVariant>,
    ) {
        self.0
            .entry((name.clone(), version.clone()))
            .or_insert(FilePin::Url { metadata, variant });
    }

    /// Return the pinned registry artifact, if one exists.
    pub(crate) fn get(&self, name: &PackageName, version: &Version) -> Option<&ResolvedDist> {
        match self.0.get(&(name.clone(), version.clone()))? {
            FilePin::Registry { dist, .. } => Some(dist),
            FilePin::Url { .. } => None,
        }
    }

    /// Return the metadata registered when selecting this package version.
    pub(crate) fn metadata(
        &self,
        name: &PackageName,
        version: &Version,
    ) -> Option<&RegisteredMetadata<'index>> {
        match self.0.get(&(name.clone(), version.clone()))? {
            FilePin::Registry {
                metadata: PinMetadata::Registered(metadata),
                ..
            }
            | FilePin::Url { metadata, .. } => Some(metadata),
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
        match self.0.get(&(name.clone(), version.clone()))? {
            FilePin::Registry { variants_json, .. } => variants_json.as_ref(),
            FilePin::Url { .. } => None,
        }
    }

    pub(crate) fn variant(
        &self,
        name: &PackageName,
        version: &Version,
    ) -> Option<&SelectedVariant> {
        match self.0.get(&(name.clone(), version.clone()))? {
            FilePin::Registry { variant, .. } | FilePin::Url { variant, .. } => variant.as_ref(),
        }
    }

    /// Return the pinned registry artifact and its metadata identity in a single lookup.
    pub(crate) fn dist_and_id(
        &self,
        name: &PackageName,
        version: &Version,
    ) -> Option<(&ResolvedDist, &DistributionId)> {
        match self.0.get(&(name.clone(), version.clone()))? {
            FilePin::Registry { dist, metadata, .. } => Some((
                dist,
                match metadata {
                    PinMetadata::Unrequested(id) => id,
                    PinMetadata::Registered(metadata) => metadata.id(),
                },
            )),
            FilePin::Url { .. } => None,
        }
    }
}
