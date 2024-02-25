use rustc_hash::FxHashMap;

use distribution_types::Verbatim;
use pep508_rs::{MarkerEnvironment, VerbatimUrl};
use uv_normalize::PackageName;

use crate::{Manifest, ResolveError};

#[derive(Debug, Default)]
pub(crate) struct Urls(FxHashMap<PackageName, VerbatimUrl>);

impl Urls {
    pub(crate) fn from_manifest(
        manifest: &Manifest,
        markers: &MarkerEnvironment,
    ) -> Result<Self, ResolveError> {
        let mut urls = FxHashMap::default();

        // Add all direct requirements and constraints. If there are any conflicts, return an error.
        for requirement in manifest
            .requirements
            .iter()
            .chain(manifest.constraints.iter())
        {
            if !requirement.evaluate_markers(markers, &[]) {
                continue;
            }

            if let Some(pep508_rs::VersionOrUrl::Url(url)) = &requirement.version_or_url {
                if let Some(previous) = urls.insert(requirement.name.clone(), url.clone()) {
                    if cache_key::CanonicalUrl::new(previous.raw())
                        != cache_key::CanonicalUrl::new(url.raw())
                    {
                        return Err(ResolveError::ConflictingUrlsDirect(
                            requirement.name.clone(),
                            previous.verbatim().to_string(),
                            url.verbatim().to_string(),
                        ));
                    }
                }
            }
        }

        // Add any editable requirements. If there are any conflicts, return an error.
        for (requirement, metadata) in &manifest.editables {
            if let Some(previous) = urls.insert(metadata.name.clone(), requirement.url.clone()) {
                if cache_key::CanonicalUrl::new(previous.raw())
                    != cache_key::CanonicalUrl::new(requirement.raw())
                {
                    return Err(ResolveError::ConflictingUrlsDirect(
                        metadata.name.clone(),
                        previous.verbatim().to_string(),
                        requirement.verbatim().to_string(),
                    ));
                }
            }

            for requirement in &metadata.requires_dist {
                if let Some(pep508_rs::VersionOrUrl::Url(url)) = &requirement.version_or_url {
                    if let Some(previous) = urls.insert(requirement.name.clone(), url.clone()) {
                        if cache_key::CanonicalUrl::new(previous.raw())
                            != cache_key::CanonicalUrl::new(url.raw())
                        {
                            return Err(ResolveError::ConflictingUrlsDirect(
                                requirement.name.clone(),
                                previous.verbatim().to_string(),
                                url.verbatim().to_string(),
                            ));
                        }
                    }
                }
            }
        }

        // Add any overrides. Conflicts here are fine, as the overrides are meant to be
        // authoritative.
        for requirement in &manifest.overrides {
            if !requirement.evaluate_markers(markers, &[]) {
                continue;
            }

            if let Some(pep508_rs::VersionOrUrl::Url(url)) = &requirement.version_or_url {
                urls.insert(requirement.name.clone(), url.clone());
            }
        }

        Ok(Self(urls))
    }

    pub(crate) fn get(&self, package: &PackageName) -> Option<&VerbatimUrl> {
        self.0.get(package)
    }
}
