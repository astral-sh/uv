use rustc_hash::FxHashMap;
use same_file::is_same_file;
use tracing::debug;

use cache_key::CanonicalUrl;
use distribution_types::Verbatim;
use pep508_rs::MarkerEnvironment;
use pypi_types::{
    ParsedArchiveUrl, ParsedDirectoryUrl, ParsedGitUrl, ParsedPathUrl, ParsedUrl,
    RequirementSource, VerbatimParsedUrl,
};
use uv_git::GitResolver;
use uv_normalize::PackageName;

use crate::{DependencyMode, Manifest, ResolveError};

/// A map of package names to their associated, required URLs.
#[derive(Debug, Default)]
pub(crate) struct Urls(FxHashMap<PackageName, VerbatimParsedUrl>);

impl Urls {
    pub(crate) fn from_manifest(
        manifest: &Manifest,
        markers: Option<&MarkerEnvironment>,
        git: &GitResolver,
        dependencies: DependencyMode,
    ) -> Result<Self, ResolveError> {
        let mut urls: FxHashMap<PackageName, VerbatimParsedUrl> = FxHashMap::default();

        // Add all direct requirements and constraints. If there are any conflicts, return an error.
        for requirement in manifest.requirements(markers, dependencies) {
            match &requirement.source {
                RequirementSource::Registry { .. } => {}
                RequirementSource::Url {
                    subdirectory,
                    location,
                    url,
                } => {
                    let url = VerbatimParsedUrl {
                        parsed_url: ParsedUrl::Archive(ParsedArchiveUrl::from_source(
                            location.clone(),
                            subdirectory.clone(),
                        )),
                        verbatim: url.clone(),
                    };
                    if let Some(previous) = urls.insert(requirement.name.clone(), url.clone()) {
                        if !Self::same_resource(&previous.parsed_url, &url.parsed_url, git) {
                            return Err(ResolveError::ConflictingUrlsDirect(
                                requirement.name.clone(),
                                previous.verbatim.verbatim().to_string(),
                                url.verbatim.verbatim().to_string(),
                            ));
                        }
                    }
                }
                RequirementSource::Path {
                    install_path,
                    lock_path,
                    url,
                } => {
                    let url = VerbatimParsedUrl {
                        parsed_url: ParsedUrl::Path(ParsedPathUrl::from_source(
                            install_path.clone(),
                            lock_path.clone(),
                            url.to_url(),
                        )),
                        verbatim: url.clone(),
                    };
                    if let Some(previous) = urls.insert(requirement.name.clone(), url.clone()) {
                        if !Self::same_resource(&previous.parsed_url, &url.parsed_url, git) {
                            return Err(ResolveError::ConflictingUrlsDirect(
                                requirement.name.clone(),
                                previous.verbatim.verbatim().to_string(),
                                url.verbatim.verbatim().to_string(),
                            ));
                        }
                    }
                }
                RequirementSource::Directory {
                    install_path,
                    lock_path,
                    editable,
                    url,
                } => {
                    let url = VerbatimParsedUrl {
                        parsed_url: ParsedUrl::Directory(ParsedDirectoryUrl::from_source(
                            install_path.clone(),
                            lock_path.clone(),
                            *editable,
                            url.to_url(),
                        )),
                        verbatim: url.clone(),
                    };
                    match urls.entry(requirement.name.clone()) {
                        std::collections::hash_map::Entry::Occupied(mut entry) => {
                            let previous = entry.get();
                            if Self::same_resource(&previous.parsed_url, &url.parsed_url, git) {
                                // Allow editables to override non-editables.
                                if previous.parsed_url.is_editable() && !editable {
                                    debug!(
                                        "Allowing {} as an editable variant of {}",
                                        &previous.verbatim, url.verbatim
                                    );
                                } else {
                                    entry.insert(url.clone());
                                }
                                continue;
                            }

                            return Err(ResolveError::ConflictingUrlsDirect(
                                requirement.name.clone(),
                                previous.verbatim.verbatim().to_string(),
                                url.verbatim.verbatim().to_string(),
                            ));
                        }
                        std::collections::hash_map::Entry::Vacant(entry) => {
                            entry.insert(url.clone());
                        }
                    }
                }
                RequirementSource::Git {
                    repository,
                    reference,
                    precise,
                    subdirectory,
                    url,
                } => {
                    let url = VerbatimParsedUrl {
                        parsed_url: ParsedUrl::Git(ParsedGitUrl::from_source(
                            repository.clone(),
                            reference.clone(),
                            *precise,
                            subdirectory.clone(),
                        )),
                        verbatim: url.clone(),
                    };
                    if let Some(previous) = urls.insert(requirement.name.clone(), url.clone()) {
                        if !Self::same_resource(&previous.parsed_url, &url.parsed_url, git) {
                            return Err(ResolveError::ConflictingUrlsDirect(
                                requirement.name.clone(),
                                previous.verbatim.verbatim().to_string(),
                                url.verbatim.verbatim().to_string(),
                            ));
                        }
                    }
                }
            }
        }

        Ok(Self(urls))
    }

    /// Return the [`VerbatimUrl`] associated with the given package name, if any.
    pub(crate) fn get(&self, package: &PackageName) -> Option<&VerbatimParsedUrl> {
        self.0.get(package)
    }

    /// Returns `true` if the [`ParsedUrl`] instances point to the same resource.
    pub(crate) fn same_resource(a: &ParsedUrl, b: &ParsedUrl, git: &GitResolver) -> bool {
        match (a, b) {
            (ParsedUrl::Archive(a), ParsedUrl::Archive(b)) => {
                a.subdirectory == b.subdirectory
                    && CanonicalUrl::new(&a.url) == CanonicalUrl::new(&b.url)
            }
            (ParsedUrl::Git(a), ParsedUrl::Git(b)) => {
                a.subdirectory == b.subdirectory && git.same_ref(&a.url, &b.url)
            }
            (ParsedUrl::Path(a), ParsedUrl::Path(b)) => {
                a.install_path == b.install_path
                    || is_same_file(&a.install_path, &b.install_path).unwrap_or(false)
            }
            (ParsedUrl::Directory(a), ParsedUrl::Directory(b)) => {
                a.install_path == b.install_path
                    || is_same_file(&a.install_path, &b.install_path).unwrap_or(false)
            }
            _ => false,
        }
    }
}
