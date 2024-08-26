use std::iter;

use rustc_hash::FxHashMap;
use same_file::is_same_file;
use tracing::debug;

use cache_key::CanonicalUrl;
use distribution_types::Verbatim;
use pep508_rs::VerbatimUrl;
use pypi_types::{ParsedDirectoryUrl, ParsedUrl, VerbatimParsedUrl};
use uv_git::GitResolver;
use uv_normalize::PackageName;

use crate::{DependencyMode, Manifest, ResolveError, ResolverMarkers};

/// The URLs that are allowed for packages.
///
/// These are the URLs used in the root package or by other URL dependencies (including path
/// dependencies). They take precedence over requirements by version (except for the special case
/// where we are in a fork that doesn't use any of the URL(s) used in other forks). Each fork may
/// only use a single URL.
///
/// This type contains all URLs without checking, the validation happens in
/// [`crate::fork_urls::ForkUrls`].
#[derive(Debug, Default)]
pub(crate) struct Urls {
    /// URL requirements in overrides. There can only be a single URL per package in overrides
    /// (since it replaces all other URLs), and an override URL replaces all requirements and
    /// constraints URLs.
    overrides: FxHashMap<PackageName, VerbatimParsedUrl>,
    /// URLs from regular requirements or from constraints. There can be multiple URLs for the same
    /// package as long as they are in different forks.
    regular: FxHashMap<PackageName, Vec<VerbatimParsedUrl>>,
}

impl Urls {
    pub(crate) fn from_manifest(
        manifest: &Manifest,
        markers: &ResolverMarkers,
        git: &GitResolver,
        dependencies: DependencyMode,
    ) -> Result<Self, ResolveError> {
        let mut urls: FxHashMap<PackageName, Vec<VerbatimParsedUrl>> = FxHashMap::default();
        let mut overrides: FxHashMap<PackageName, VerbatimParsedUrl> = FxHashMap::default();

        // Add all direct regular requirements and constraints URL.
        for requirement in manifest.requirements_no_overrides(markers, dependencies) {
            let Some(url) = requirement.source.to_verbatim_parsed_url() else {
                // Registry requirement
                continue;
            };

            let package_urls = urls.entry(requirement.name.clone()).or_default();
            if let Some(package_url) = package_urls
                .iter_mut()
                .find(|package_url| same_resource(&package_url.parsed_url, &url.parsed_url, git))
            {
                // Allow editables to override non-editables.
                let previous_editable = package_url.is_editable();
                *package_url = url;
                if previous_editable {
                    if let VerbatimParsedUrl {
                        parsed_url: ParsedUrl::Directory(ParsedDirectoryUrl { editable, .. }),
                        verbatim: _,
                    } = package_url
                    {
                        if !*editable {
                            debug!("Allowing an editable variant of {}", &package_url.verbatim);
                            *editable = true;
                        }
                    };
                }
            } else {
                package_urls.push(url);
            }
        }

        // Add all URLs from overrides. If there is an override URL, all other URLs from
        // requirements and constraints are moot and will be removed.
        for requirement in manifest.overrides(markers, dependencies) {
            let Some(url) = requirement.source.to_verbatim_parsed_url() else {
                // Registry requirement
                continue;
            };
            // We only clear for non-URL overrides, since e.g. with an override `anyio==0.0.0` and
            // a requirements.txt entry `./anyio`, we still use the URL. See
            // `allow_recursive_url_local_path_override_constraint`.
            urls.remove(&requirement.name);
            let previous = overrides.insert(requirement.name.clone(), url.clone());
            if let Some(previous) = previous {
                if !same_resource(&previous.parsed_url, &url.parsed_url, git) {
                    return Err(ResolveError::ConflictingOverrideUrls(
                        requirement.name.clone(),
                        previous.verbatim.verbatim().to_string(),
                        url.verbatim.verbatim().to_string(),
                    ));
                }
            }
        }

        Ok(Self {
            regular: urls,
            overrides,
        })
    }

    /// Check and canonicalize the URL of a requirement.
    ///
    /// If we have a URL override, apply it unconditionally for registry and URL requirements.
    /// Otherwise, there are two case: For a URL requirement (`url` isn't `None`), check that the
    /// URL is allowed and return its canonical form. For registry requirements, we return `None`
    /// if there is no override.
    pub(crate) fn get_url<'a>(
        &'a self,
        name: &'a PackageName,
        url: Option<&'a VerbatimParsedUrl>,
        git: &'a GitResolver,
    ) -> Result<Option<&'a VerbatimParsedUrl>, ResolveError> {
        if let Some(override_url) = self.get_override(name) {
            Ok(Some(override_url))
        } else if let Some(url) = url {
            Ok(Some(self.canonicalize_allowed_url(
                name,
                git,
                &url.verbatim,
                &url.parsed_url,
            )?))
        } else {
            Ok(None)
        }
    }

    pub(crate) fn any_url(&self, name: &PackageName) -> bool {
        self.get_override(name).is_some() || self.get_regular(name).is_some()
    }

    /// Return the [`VerbatimUrl`] override for the given package, if any.
    fn get_override(&self, package: &PackageName) -> Option<&VerbatimParsedUrl> {
        self.overrides.get(package)
    }

    /// Return the allowed [`VerbatimUrl`]s for given package from regular requirements and
    /// constraints (but not overrides), if any.
    ///
    /// It's more than one more URL if they are in different forks (or conflict after forking).
    fn get_regular(&self, package: &PackageName) -> Option<&[VerbatimParsedUrl]> {
        self.regular.get(package).map(Vec::as_slice)
    }

    /// Check if a URL is allowed (known), and if so, return its canonical form.
    fn canonicalize_allowed_url<'a>(
        &'a self,
        package_name: &'a PackageName,
        git: &'a GitResolver,
        verbatim_url: &'a VerbatimUrl,
        parsed_url: &'a ParsedUrl,
    ) -> Result<&'a VerbatimParsedUrl, ResolveError> {
        let Some(expected) = self.get_regular(package_name) else {
            return Err(ResolveError::DisallowedUrl(
                package_name.clone(),
                verbatim_url.to_string(),
            ));
        };

        let matching_urls: Vec<_> = expected
            .iter()
            .filter(|requirement| same_resource(&requirement.parsed_url, parsed_url, git))
            .collect();

        let [allowed_url] = matching_urls.as_slice() else {
            let mut conflicting_urls: Vec<_> = matching_urls
                .into_iter()
                .map(|parsed_url| parsed_url.verbatim.verbatim().to_string())
                .chain(iter::once(verbatim_url.verbatim().to_string()))
                .collect();
            conflicting_urls.sort();
            return Err(ResolveError::ConflictingUrlsUniversal(
                package_name.clone(),
                conflicting_urls,
            ));
        };
        Ok(*allowed_url)
    }
}

/// Returns `true` if the [`ParsedUrl`] instances point to the same resource.
fn same_resource(a: &ParsedUrl, b: &ParsedUrl, git: &GitResolver) -> bool {
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
