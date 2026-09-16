use std::sync::{Arc, Mutex};

use rustc_hash::{FxHashMap, FxHashSet};
use same_file::is_same_file;

use uv_cache_key::CanonicalUrl;
use uv_distribution_types::Requirement;
use uv_git::GitResolver;
use uv_git_types::GitUrl;
use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_pep508::MarkerTree;
use uv_pypi_types::{ParsedUrl, VerbatimParsedUrl};

use crate::pubgrub::SourceId;
use crate::{DependencyMode, Manifest, ResolverEnvironment};

/// Root URL inputs and shared identities for direct resources discovered during solving.
#[derive(Debug, Default)]
pub(crate) struct Urls {
    /// Names whose root URL input may affect whether speculative registry requests are useful.
    root_names: FxHashSet<PackageName>,
    /// The concrete resources which root requirements and configuration might authorize.
    initial: Vec<Requirement>,
    /// Root-authored constraints and overrides may authorize a dependency of a registry package.
    configuration: FxHashMap<PackageName, Vec<ConfiguredUrl>>,
    /// Resource identities are shared between environmental forks and source-search retries.
    resources: Mutex<Vec<(PackageName, Arc<VerbatimParsedUrl>)>>,
}

#[derive(Debug)]
struct ConfiguredUrl {
    scope: Option<(PackageName, Option<Version>)>,
    requirement: Requirement,
}

impl Urls {
    pub(crate) fn from_manifest(
        manifest: &Manifest,
        env: &ResolverEnvironment,
        dependencies: DependencyMode,
    ) -> Self {
        let mut root_names = FxHashSet::default();
        let mut initial = Vec::new();
        for requirement in manifest
            .requirements_no_overrides(env, dependencies)
            .chain(manifest.overrides(env, dependencies))
        {
            if requirement.source.to_verbatim_parsed_url().is_some() {
                root_names.insert(requirement.name.clone());
                initial.push(requirement.into_owned());
            }
        }

        let mut configuration = FxHashMap::<_, Vec<_>>::default();
        for requirement in manifest
            .constraints
            .requirements()
            .chain(manifest.overrides.global_requirements())
        {
            if requirement.source.to_verbatim_parsed_url().is_some() {
                configuration
                    .entry(requirement.name.clone())
                    .or_default()
                    .push(ConfiguredUrl {
                        scope: None,
                        requirement: requirement.clone(),
                    });
            }
        }
        for (package, version, requirement) in manifest.overrides.scoped_requirements() {
            if requirement.source.to_verbatim_parsed_url().is_some() {
                configuration
                    .entry(requirement.name.clone())
                    .or_default()
                    .push(ConfiguredUrl {
                        scope: Some((package.clone(), version.cloned())),
                        requirement: requirement.clone(),
                    });
            }
        }

        root_names.extend(configuration.keys().cloned());
        initial.extend(
            configuration
                .values()
                .flatten()
                .map(|configured| configured.requirement.clone()),
        );
        initial.sort();
        initial.dedup();
        Self {
            root_names,
            initial,
            configuration,
            resources: Mutex::default(),
        }
    }

    /// Whether any root input can introduce a URL anywhere in the dependency graph.
    pub(crate) fn has_potential(&self) -> bool {
        !self.root_names.is_empty()
    }

    pub(crate) fn initial(&self) -> &[Requirement] {
        &self.initial
    }

    /// Give compatible resource spellings an immutable solver identity. A plain or virtual
    /// directory can share either installation mode; selected paths reconcile explicit choices.
    pub(crate) fn intern(
        &self,
        name: &PackageName,
        url: &VerbatimParsedUrl,
        git: &GitResolver,
        preferred: &[SourceId],
    ) -> SourceId {
        let mut resources = self
            .resources
            .lock()
            .expect("URL resource lock is not poisoned");
        if let Some(source) = preferred.iter().find(|source| {
            let (package, resource) = &resources[source.0];
            package == name && same_resource(&resource.parsed_url, &url.parsed_url, git)
        }) {
            return *source;
        }
        if let Some(index) = resources.iter().position(|(package, resource)| {
            package == name && same_resource(&resource.parsed_url, &url.parsed_url, git)
        }) {
            return SourceId(index);
        }
        let id = SourceId(resources.len());
        resources.push((name.clone(), Arc::new(url.clone())));
        id
    }

    /// Look up an already registered resource without adding URLs from inactive metadata.
    pub(crate) fn lookup(
        &self,
        name: &PackageName,
        url: &VerbatimParsedUrl,
        git: &GitResolver,
    ) -> Vec<SourceId> {
        self.resources
            .lock()
            .expect("URL resource lock is not poisoned")
            .iter()
            .enumerate()
            .filter_map(|(index, (package, resource))| {
                (package == name && same_resource(&resource.parsed_url, &url.parsed_url, git))
                    .then_some(SourceId(index))
            })
            .collect()
    }

    /// Retrieve the presentation originally used to register a resource identity.
    pub(crate) fn get(&self, source: SourceId) -> Arc<VerbatimParsedUrl> {
        self.resources
            .lock()
            .expect("URL resource lock is not poisoned")[source.0]
            .1
            .clone()
    }

    /// Whether this expanded URL requirement is independently authorized by root configuration.
    pub(crate) fn configuration_authorizes(
        &self,
        parent: Option<(&PackageName, &Version)>,
        requirement: &Requirement,
        git: &GitResolver,
    ) -> bool {
        let Some(url) = requirement.source.to_verbatim_parsed_url() else {
            return false;
        };
        self.configuration
            .get(&requirement.name)
            .is_some_and(|requirements| {
                requirements.iter().any(|configured| {
                    let matches_scope =
                        configured.scope.as_ref().is_none_or(|(package, version)| {
                            parent.is_some_and(|(parent, parent_version)| {
                                parent == package
                                    && version
                                        .as_ref()
                                        .is_none_or(|version| version == parent_version)
                            })
                        });
                    let marker = requirement.marker.without_extras();
                    let configured_marker = configured.requirement.marker.without_extras();
                    matches_scope
                        && (configured_marker == MarkerTree::TRUE
                            || marker.is_disjoint(configured_marker.negate()))
                        && configured
                            .requirement
                            .source
                            .to_verbatim_parsed_url()
                            .is_some_and(|configured| {
                                same_resource(&url.parsed_url, &configured.parsed_url, git)
                            })
                })
            })
    }

    /// Whether an initial requirement, override, or constraint names a URL for this package.
    pub(crate) fn any_url(&self, name: &PackageName) -> bool {
        self.root_names.contains(name)
    }
}

/// Returns `true` if the [`ParsedUrl`] instances point to the same resource.
pub(super) fn same_resource(a: &ParsedUrl, b: &ParsedUrl, git: &GitResolver) -> bool {
    match a {
        ParsedUrl::Archive(a) => {
            if let ParsedUrl::Archive(b) = b {
                a.subdirectory.as_deref().map(uv_fs::normalize_path)
                    == b.subdirectory.as_deref().map(uv_fs::normalize_path)
                    && CanonicalUrl::new(a.url.clone()) == CanonicalUrl::new(b.url.clone())
            } else {
                false
            }
        }
        ParsedUrl::GitDirectory(a) => {
            if let ParsedUrl::GitDirectory(b) = b {
                a.subdirectory.as_deref().map(uv_fs::normalize_path)
                    == b.subdirectory.as_deref().map(uv_fs::normalize_path)
                    && git.same_ref(&a.url, &b.url)
            } else {
                false
            }
        }
        ParsedUrl::GitPath(a) => {
            if let ParsedUrl::GitPath(b) = b {
                uv_fs::normalize_path(&a.install_path) == uv_fs::normalize_path(&b.install_path)
                    && git.same_ref(&a.url, &b.url)
            } else {
                false
            }
        }
        ParsedUrl::Path(a) => {
            if let ParsedUrl::Path(b) = b {
                a.install_path == b.install_path
                    || is_same_file(&a.install_path, &b.install_path).unwrap_or(false)
            } else {
                false
            }
        }
        ParsedUrl::Directory(a) => {
            if let ParsedUrl::Directory(b) = b {
                (a.install_path == b.install_path
                    || is_same_file(&a.install_path, &b.install_path).unwrap_or(false))
                    && (a.r#virtual == Some(true)
                        || b.r#virtual == Some(true)
                        || a.editable.is_none_or(|a| b.editable.is_none_or(|b| a == b)))
            } else {
                false
            }
        }
    }
}

/// Whether Git URLs could be aliases once their references have been resolved.
pub(super) fn could_be_same_git_resource(a: &ParsedUrl, b: &ParsedUrl) -> bool {
    match a {
        ParsedUrl::GitDirectory(a) => {
            if let ParsedUrl::GitDirectory(b) = b {
                a.subdirectory.as_deref().map(uv_fs::normalize_path)
                    == b.subdirectory.as_deref().map(uv_fs::normalize_path)
                    && a.url.repository() == b.url.repository()
            } else {
                false
            }
        }
        ParsedUrl::GitPath(a) => {
            if let ParsedUrl::GitPath(b) = b {
                uv_fs::normalize_path(&a.install_path) == uv_fs::normalize_path(&b.install_path)
                    && a.url.repository() == b.url.repository()
            } else {
                false
            }
        }
        ParsedUrl::Archive(_) | ParsedUrl::Path(_) | ParsedUrl::Directory(_) => false,
    }
}

/// Return the repository reference of a Git source, without its package path or subdirectory.
pub(super) fn git_url(url: &ParsedUrl) -> Option<&GitUrl> {
    match url {
        ParsedUrl::GitDirectory(url) => Some(&url.url),
        ParsedUrl::GitPath(url) => Some(&url.url),
        ParsedUrl::Archive(_) | ParsedUrl::Path(_) | ParsedUrl::Directory(_) => None,
    }
}
