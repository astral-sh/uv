use std::collections::hash_map::Entry;

use rustc_hash::FxHashMap;

use distribution_types::Verbatim;
use pypi_types::VerbatimParsedUrl;
use uv_normalize::PackageName;

use crate::resolver::ResolverMarkers;
use crate::ResolveError;

/// See [`crate::resolver::SolveState`].
#[derive(Default, Debug, Clone)]
pub(crate) struct ForkUrls(FxHashMap<PackageName, VerbatimParsedUrl>);

impl ForkUrls {
    /// Get the URL previously used for a package in this fork.
    pub(crate) fn get(&self, package_name: &PackageName) -> Option<&VerbatimParsedUrl> {
        self.0.get(package_name)
    }

    /// Whether we use a URL for this package.
    pub(crate) fn contains_key(&self, package_name: &PackageName) -> bool {
        self.0.contains_key(package_name)
    }

    /// Check that this is the only URL used for this package in this fork.
    pub(crate) fn insert(
        &mut self,
        package_name: &PackageName,
        url: &VerbatimParsedUrl,
        fork_markers: &ResolverMarkers,
    ) -> Result<(), ResolveError> {
        match self.0.entry(package_name.clone()) {
            Entry::Occupied(previous) => {
                if previous.get() != url {
                    let mut conflicting_url = vec![
                        previous.get().verbatim.verbatim().to_string(),
                        url.verbatim.verbatim().to_string(),
                    ];
                    conflicting_url.sort();
                    return match fork_markers {
                        ResolverMarkers::Universal { .. }
                        | ResolverMarkers::SpecificEnvironment(_) => {
                            Err(ResolveError::ConflictingUrlsUniversal(
                                package_name.clone(),
                                conflicting_url,
                            ))
                        }
                        ResolverMarkers::Fork(fork_markers) => {
                            Err(ResolveError::ConflictingUrlsFork {
                                package_name: package_name.clone(),
                                urls: conflicting_url,
                                fork_markers: fork_markers.clone(),
                            })
                        }
                    };
                }
            }
            Entry::Vacant(vacant) => {
                vacant.insert(url.clone());
            }
        }
        Ok(())
    }
}
