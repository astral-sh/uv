use std::borrow::Cow;
use std::fmt::Debug;

use same_file::is_same_file;
use tracing::{debug, trace};
use url::Url;

use uv_cache_info::CacheInfo;
use uv_cache_key::{CanonicalUrl, RepositoryUrl};
use uv_distribution_filename::ExpandedTags;
use uv_distribution_types::{
    BuildInfo, BuildVariables, ConfigSettings, ExtraBuildRequirement, ExtraBuildRequires,
    ExtraBuildVariables, InstalledDirectUrlDist, InstalledDist, InstalledDistKind,
    PackageConfigSettings, RequirementSource,
};
use uv_git_types::{GitLfs, GitOid};
use uv_normalize::PackageName;
use uv_platform_tags::{IncompatibleTag, TagCompatibility, Tags};
use uv_pypi_types::{DirInfo, DirectUrl, VcsInfo, VcsKind};

use crate::InstallationStrategy;

#[derive(Debug, Copy, Clone)]
pub(crate) enum RequirementSatisfaction {
    Mismatch,
    Satisfied,
    OutOfDate,
    CacheInvalid,
}

impl RequirementSatisfaction {
    /// Returns true if a requirement is satisfied by an installed distribution.
    ///
    /// Returns an error if IO fails during a freshness check for a local path.
    pub(crate) fn check(
        name: &PackageName,
        distribution: &InstalledDist,
        source: &RequirementSource,
        installation: InstallationStrategy,
        tags: &Tags,
        config_settings: &ConfigSettings,
        config_settings_package: &PackageConfigSettings,
        extra_build_requires: &ExtraBuildRequires,
        extra_build_variables: &ExtraBuildVariables,
    ) -> Self {
        trace!(
            "Comparing installed with source: {:?} {:?}",
            distribution, source
        );

        // If the distribution was built with other settings, it is out of date.
        if distribution.build_info().is_some_and(|dist_build_info| {
            let config_settings =
                config_settings_for(name, config_settings, config_settings_package);
            let extra_build_requires = extra_build_requires_for(name, extra_build_requires);
            let extra_build_variables = extra_build_variables_for(name, extra_build_variables);
            let build_info = BuildInfo::from_settings(
                &config_settings,
                extra_build_requires,
                extra_build_variables,
            );
            dist_build_info != &build_info
        }) {
            debug!("Build info mismatch for {name}: {distribution}");
            return Self::OutOfDate;
        }

        // Filter out already-installed packages.
        match source {
            // If the requirement comes from a registry, check by name.
            RequirementSource::Registry { specifier, .. } => {
                // If the installed distribution is _not_ from a registry, reject it if and only if
                // we're in a stateless install.
                //
                // For example: the `uv pip` CLI is stateful, in that it "respects"
                // already-installed packages in the virtual environment. So if you run `uv pip
                // install ./path/to/idna`, and then `uv pip install anyio` (which depends on
                // `idna`), we'll "accept" the already-installed `idna` even though it is implicitly
                // being "required" as a registry package.
                //
                // The `uv sync` CLI is stateless, in that all requirements must be defined
                // declaratively ahead-of-time. So if you `uv sync` to install `./path/to/idna` and
                // later `uv sync` to install `anyio`, we'll know (during that second sync) if the
                // already-installed `idna` should come from the registry or not.
                if installation == InstallationStrategy::Strict {
                    if !matches!(distribution.kind, InstalledDistKind::Registry { .. }) {
                        debug!("Distribution type mismatch for {name}: {distribution:?}");
                        return Self::Mismatch;
                    }
                }

                if !specifier.contains(distribution.version()) {
                    return Self::Mismatch;
                }
            }
            RequirementSource::Url {
                // We use the location since `direct_url.json` also stores this URL, e.g.
                // `pip install git+https://github.com/tqdm/tqdm@cc372d09dcd5a5eabdc6ed4cf365bdb0be004d44#subdirectory=.`
                // records `"url": "https://github.com/tqdm/tqdm"` in `direct_url.json`.
                location: requested_url,
                subdirectory: requested_subdirectory,
                ext: _,
                url: _,
            } => {
                let InstalledDistKind::Url(InstalledDirectUrlDist {
                    direct_url,
                    editable,
                    cache_info,
                    ..
                }) = &distribution.kind
                else {
                    return Self::Mismatch;
                };
                let DirectUrl::ArchiveUrl {
                    url: installed_url,
                    archive_info: _,
                    subdirectory: installed_subdirectory,
                } = direct_url.as_ref()
                else {
                    return Self::Mismatch;
                };

                if *editable {
                    return Self::Mismatch;
                }

                if requested_subdirectory != installed_subdirectory {
                    return Self::Mismatch;
                }

                if !CanonicalUrl::parse(installed_url)
                    .is_ok_and(|installed_url| installed_url == CanonicalUrl::new(requested_url))
                {
                    return Self::Mismatch;
                }

                // If the requirement came from a local path, check freshness.
                if requested_url.scheme() == "file" {
                    if let Ok(archive) = requested_url.to_file_path() {
                        let Some(cache_info) = cache_info.as_ref() else {
                            return Self::OutOfDate;
                        };
                        match CacheInfo::from_path(&archive) {
                            Ok(read_cache_info) => {
                                if *cache_info != read_cache_info {
                                    return Self::OutOfDate;
                                }
                            }
                            Err(err) => {
                                debug!(
                                    "Failed to read cached requirement for: {distribution} ({err})"
                                );
                                return Self::CacheInvalid;
                            }
                        }
                    }
                }
            }
            RequirementSource::Git {
                url: _,
                git: requested_git,
                subdirectory: requested_subdirectory,
            } => {
                let InstalledDistKind::Url(InstalledDirectUrlDist { direct_url, .. }) =
                    &distribution.kind
                else {
                    return Self::Mismatch;
                };
                let DirectUrl::VcsUrl {
                    url: installed_url,
                    vcs_info:
                        VcsInfo {
                            vcs: VcsKind::Git,
                            requested_revision: _,
                            commit_id: installed_precise,
                            git_lfs: installed_git_lfs,
                        },
                    subdirectory: installed_subdirectory,
                } = direct_url.as_ref()
                else {
                    return Self::Mismatch;
                };

                if requested_subdirectory != installed_subdirectory {
                    debug!(
                        "Subdirectory mismatch: {:?} vs. {:?}",
                        installed_subdirectory, requested_subdirectory
                    );
                    return Self::Mismatch;
                }

                let requested_git_lfs = requested_git.lfs();
                let installed_git_lfs = installed_git_lfs.map(GitLfs::from).unwrap_or_default();
                if requested_git_lfs != installed_git_lfs {
                    debug!(
                        "Git LFS mismatch: {} (installed) vs. {} (requested)",
                        installed_git_lfs, requested_git_lfs,
                    );
                    return Self::Mismatch;
                }

                if !RepositoryUrl::parse(installed_url).is_ok_and(|installed_url| {
                    installed_url == RepositoryUrl::new(requested_git.repository())
                }) {
                    debug!(
                        "Repository mismatch: {:?} vs. {:?}",
                        installed_url,
                        requested_git.repository()
                    );
                    return Self::Mismatch;
                }

                // TODO(charlie): It would be more consistent for us to compare the requested
                // revisions here.
                if installed_precise.as_deref()
                    != requested_git.precise().as_ref().map(GitOid::as_str)
                {
                    debug!(
                        "Precise mismatch: {:?} vs. {:?}",
                        installed_precise,
                        requested_git.precise()
                    );
                    return Self::OutOfDate;
                }
            }
            RequirementSource::Path {
                install_path: requested_path,
                ext: _,
                url: _,
            } => {
                let InstalledDistKind::Url(InstalledDirectUrlDist {
                    direct_url,
                    cache_info,
                    ..
                }) = &distribution.kind
                else {
                    return Self::Mismatch;
                };
                let DirectUrl::ArchiveUrl {
                    url: installed_url,
                    archive_info: _,
                    subdirectory: None,
                } = direct_url.as_ref()
                else {
                    return Self::Mismatch;
                };

                let Some(installed_path) = Url::parse(installed_url)
                    .ok()
                    .and_then(|url| url.to_file_path().ok())
                else {
                    return Self::Mismatch;
                };

                if !(**requested_path == installed_path
                    || is_same_file(requested_path, &installed_path).unwrap_or(false))
                {
                    trace!(
                        "Path mismatch: {:?} vs. {:?}",
                        requested_path, installed_path,
                    );
                    return Self::Mismatch;
                }

                let Some(cache_info) = cache_info.as_ref() else {
                    return Self::OutOfDate;
                };
                match CacheInfo::from_path(requested_path) {
                    Ok(read_cache_info) => {
                        if *cache_info != read_cache_info {
                            return Self::OutOfDate;
                        }
                    }
                    Err(err) => {
                        debug!("Failed to read cached requirement for: {distribution} ({err})");
                        return Self::CacheInvalid;
                    }
                }
            }
            RequirementSource::Directory {
                install_path: requested_path,
                editable: requested_editable,
                r#virtual: _,
                url: _,
            } => {
                let InstalledDistKind::Url(InstalledDirectUrlDist {
                    direct_url,
                    cache_info,
                    ..
                }) = &distribution.kind
                else {
                    return Self::Mismatch;
                };
                let DirectUrl::LocalDirectory {
                    url: installed_url,
                    dir_info:
                        DirInfo {
                            editable: installed_editable,
                        },
                    subdirectory: None,
                } = direct_url.as_ref()
                else {
                    return Self::Mismatch;
                };

                if requested_editable != installed_editable {
                    trace!(
                        "Editable mismatch: {:?} vs. {:?}",
                        *requested_editable,
                        installed_editable.unwrap_or_default()
                    );
                    return Self::Mismatch;
                }

                let Some(installed_path) = Url::parse(installed_url)
                    .ok()
                    .and_then(|url| url.to_file_path().ok())
                else {
                    return Self::Mismatch;
                };

                if !(**requested_path == installed_path
                    || is_same_file(requested_path, &installed_path).unwrap_or(false))
                {
                    trace!(
                        "Path mismatch: {:?} vs. {:?}",
                        requested_path, installed_path,
                    );
                    return Self::Mismatch;
                }

                let Some(cache_info) = cache_info.as_ref() else {
                    return Self::OutOfDate;
                };
                match CacheInfo::from_path(requested_path) {
                    Ok(read_cache_info) => {
                        if *cache_info != read_cache_info {
                            return Self::OutOfDate;
                        }
                    }
                    Err(err) => {
                        debug!("Failed to read cached requirement for: {distribution} ({err})");
                        return Self::CacheInvalid;
                    }
                }
            }
        }

        // If the distribution isn't compatible with the current platform, it is a mismatch.
        if let Ok(Some(wheel_tags)) = distribution.read_tags() {
            if !wheel_tags.is_compatible(tags) {
                if let Some(hint) = generate_dist_compatibility_hint(wheel_tags, tags) {
                    debug!("Platform tags mismatch for {distribution}: {hint}");
                } else {
                    debug!("Platform tags mismatch for {distribution}");
                }
                return Self::Mismatch;
            }
        }

        // Otherwise, assume the requirement is up-to-date.
        Self::Satisfied
    }
}

/// Determine the [`ConfigSettings`] for the given package name.
fn config_settings_for<'settings>(
    name: &PackageName,
    config_settings: &'settings ConfigSettings,
    config_settings_package: &PackageConfigSettings,
) -> Cow<'settings, ConfigSettings> {
    if let Some(package_settings) = config_settings_package.get(name) {
        Cow::Owned(package_settings.clone().merge(config_settings.clone()))
    } else {
        Cow::Borrowed(config_settings)
    }
}

/// Determine the extra build requirements for the given package name.
fn extra_build_requires_for<'settings>(
    name: &PackageName,
    extra_build_requires: &'settings ExtraBuildRequires,
) -> &'settings [ExtraBuildRequirement] {
    extra_build_requires
        .get(name)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

/// Determine the extra build variables for the given package name.
fn extra_build_variables_for<'settings>(
    name: &PackageName,
    extra_build_variables: &'settings ExtraBuildVariables,
) -> Option<&'settings BuildVariables> {
    extra_build_variables.get(name)
}

/// Generate a hint for explaining tag compatibility issues.
// TODO(zanieb): We should refactor this to share logic with `generate_wheel_compatibility_hint`
fn generate_dist_compatibility_hint(wheel_tags: &ExpandedTags, tags: &Tags) -> Option<String> {
    let TagCompatibility::Incompatible(incompatible_tag) = wheel_tags.compatibility(tags) else {
        return None;
    };

    match incompatible_tag {
        IncompatibleTag::Python => {
            let wheel_tags = wheel_tags.python_tags();
            let current_tag = tags.python_tag();

            if let Some(current) = current_tag {
                let message = if let Some(pretty) = current.pretty() {
                    format!("{pretty} (`{current}`)")
                } else {
                    format!("`{current}`")
                };

                Some(format!(
                    "The distribution is compatible with {}, but you're using {}",
                    wheel_tags
                        .map(|tag| if let Some(pretty) = tag.pretty() {
                            format!("{pretty} (`{tag}`)")
                        } else {
                            format!("`{tag}`")
                        })
                        .collect::<Vec<_>>()
                        .join(", "),
                    message
                ))
            } else {
                Some(format!(
                    "The distribution requires {}",
                    wheel_tags
                        .map(|tag| if let Some(pretty) = tag.pretty() {
                            format!("{pretty} (`{tag}`)")
                        } else {
                            format!("`{tag}`")
                        })
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
            }
        }
        IncompatibleTag::Abi => {
            let wheel_tags = wheel_tags.abi_tags();
            let current_tag = tags.abi_tag();

            if let Some(current) = current_tag {
                let message = if let Some(pretty) = current.pretty() {
                    format!("{pretty} (`{current}`)")
                } else {
                    format!("`{current}`")
                };
                Some(format!(
                    "The distribution is compatible with {}, but you're using {}",
                    wheel_tags
                        .map(|tag| if let Some(pretty) = tag.pretty() {
                            format!("{pretty} (`{tag}`)")
                        } else {
                            format!("`{tag}`")
                        })
                        .collect::<Vec<_>>()
                        .join(", "),
                    message
                ))
            } else {
                Some(format!(
                    "The distribution requires {}",
                    wheel_tags
                        .map(|tag| if let Some(pretty) = tag.pretty() {
                            format!("{pretty} (`{tag}`)")
                        } else {
                            format!("`{tag}`")
                        })
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
            }
        }
        IncompatibleTag::Platform => {
            let wheel_tags = wheel_tags.platform_tags();
            let current_tag = tags.platform_tag();

            if let Some(current) = current_tag {
                let message = if let Some(pretty) = current.pretty() {
                    format!("{pretty} (`{current}`)")
                } else {
                    format!("`{current}`")
                };
                Some(format!(
                    "The distribution is compatible with {}, but you're on {}",
                    wheel_tags
                        .map(|tag| if let Some(pretty) = tag.pretty() {
                            format!("{pretty} (`{tag}`)")
                        } else {
                            format!("`{tag}`")
                        })
                        .collect::<Vec<_>>()
                        .join(", "),
                    message
                ))
            } else {
                Some(format!(
                    "The distribution requires {}",
                    wheel_tags
                        .map(|tag| if let Some(pretty) = tag.pretty() {
                            format!("{pretty} (`{tag}`)")
                        } else {
                            format!("`{tag}`")
                        })
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
            }
        }
        _ => None,
    }
}
