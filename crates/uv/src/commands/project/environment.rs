use std::path::{Path, PathBuf};

use anyhow::Context;
use tracing::{debug, info_span};
use uv_warnings::warn_user;

use crate::commands::pip::loggers::{InstallLogger, ResolveLogger};
use crate::commands::pip::operations::Modifications;
use crate::commands::pip::{resolution_markers, resolution_tags};
use crate::commands::project::{
    EnvironmentSpecification, PlatformState, ProjectError, resolve_environment, sync_environment,
};
use crate::printer::Printer;
use crate::settings::ResolverInstallerSettings;

use uv_cache::{Cache, CacheBucket};
use uv_cache_info::CacheInfo;
use uv_cache_key::{cache_digest, hash_digest};
use uv_client::BaseClientBuilder;
use uv_configuration::{BuildOptions, Concurrency, Constraints, TargetTriple};
use uv_distribution_types::{
    BuiltDist, Dist, Identifier, Node, Resolution, ResolvedDist, SourceDist,
};
use uv_fs::{PythonExt, Simplified};
use uv_normalize::{ExtraName, GroupName};
use uv_preview::{Preview, PreviewFeature};
use uv_python::{Interpreter, PythonEnvironment, canonicalize_executable};
use uv_resolver::PylockToml;
use uv_types::SourceTreeEditablePolicy;
use uv_workspace::WorkspaceCache;

/// An ephemeral [`PythonEnvironment`] for running an individual command.
#[derive(Debug)]
pub(crate) struct EphemeralEnvironment(PythonEnvironment);

impl From<PythonEnvironment> for EphemeralEnvironment {
    fn from(environment: PythonEnvironment) -> Self {
        Self(environment)
    }
}

impl From<EphemeralEnvironment> for PythonEnvironment {
    fn from(environment: EphemeralEnvironment) -> Self {
        environment.0
    }
}

impl EphemeralEnvironment {
    /// Set the ephemeral overlay for a Python environment.
    pub(crate) fn set_overlay(&self, contents: impl AsRef<[u8]>) -> Result<(), ProjectError> {
        let site_packages = self
            .0
            .site_packages()
            .next()
            .ok_or(ProjectError::NoSitePackages)?;
        let overlay_path = site_packages.join("_uv_ephemeral_overlay.pth");
        fs_err::write(overlay_path, contents)?;
        Ok(())
    }

    /// Enable system site packages for a Python environment.
    pub(crate) fn set_system_site_packages(&self) -> Result<(), ProjectError> {
        self.0
            .set_pyvenv_cfg("include-system-site-packages", "true")?;
        Ok(())
    }

    /// Set the `extends-environment` key in the `pyvenv.cfg` file to the given path.
    ///
    /// Ephemeral environments created by `uv run --with` extend a parent (virtual or system)
    /// environment by adding a `.pth` file to the ephemeral environment's `site-packages`
    /// directory. The `pth` file contains Python code to dynamically add the parent
    /// environment's `site-packages` directory to Python's import search paths in addition to
    /// the ephemeral environment's `site-packages` directory. This works well at runtime, but
    /// is too dynamic for static analysis tools like ty to understand. As such, we
    /// additionally write the `sys.prefix` of the parent environment to the
    /// `extends-environment` key of the ephemeral environment's `pyvenv.cfg` file, making it
    /// easier for these tools to statically and reliably understand the relationship between
    /// the two environments.
    pub(crate) fn set_parent_environment(
        &self,
        parent_environment_sys_prefix: &Path,
    ) -> Result<(), ProjectError> {
        self.0.set_pyvenv_cfg(
            "extends-environment",
            &parent_environment_sys_prefix.escape_for_python(),
        )?;
        Ok(())
    }

    /// Returns the path to the environment's scripts directory.
    pub(crate) fn scripts(&self) -> &Path {
        self.0.scripts()
    }

    /// Returns the path to the environment's Python executable.
    pub(crate) fn sys_executable(&self) -> &Path {
        self.0.interpreter().sys_executable()
    }

    pub(crate) fn sys_prefix(&self) -> &Path {
        self.0.interpreter().sys_prefix()
    }
}

/// A [`PythonEnvironment`] stored in the cache.
#[derive(Debug)]
pub(crate) struct CachedEnvironment(PythonEnvironment);

impl From<CachedEnvironment> for PythonEnvironment {
    fn from(environment: CachedEnvironment) -> Self {
        environment.0
    }
}

#[derive(Debug, Clone, Hash)]
struct CachedEnvironmentDist {
    dist: ResolvedDist,
    hashes: uv_pypi_types::HashDigests,
    cache_info: Option<CacheInfo>,
}

impl CachedEnvironment {
    /// Get or create an [`CachedEnvironment`] based on a given set of requirements.
    pub(crate) async fn from_spec(
        spec: EnvironmentSpecification<'_>,
        build_constraints: Constraints,
        interpreter: &Interpreter,
        python_platform: Option<&TargetTriple>,
        settings: &ResolverInstallerSettings,
        client_builder: &BaseClientBuilder<'_>,
        state: &PlatformState,
        resolve: Box<dyn ResolveLogger>,
        install: Box<dyn InstallLogger>,
        installer_metadata: bool,
        concurrency: &Concurrency,
        cache: &Cache,
        workspace_cache: &WorkspaceCache,
        printer: Printer,
        preview: Preview,
    ) -> Result<Self, ProjectError> {
        let interpreter = Self::base_interpreter(interpreter, cache)?;

        // If a `pylock.toml` was provided, derive the [`Resolution`] from it directly, bypassing
        // the resolver; otherwise, resolve the requirements with the interpreter.
        let resolution = if let Some(pylock) = spec.requirements.pylock.clone() {
            if !preview.is_enabled(PreviewFeature::Pylock) {
                warn_user!(
                    "The `pylock.toml` format is experimental and may change without warning. Pass `--preview-features {}` to disable this warning.",
                    PreviewFeature::Pylock
                );
            }
            resolve_from_pylock(
                &pylock,
                &interpreter,
                python_platform,
                &settings.resolver.build_options,
                client_builder,
            )
            .await?
        } else {
            Resolution::from(
                resolve_environment(
                    spec,
                    &interpreter,
                    python_platform,
                    SourceTreeEditablePolicy::Project,
                    build_constraints.clone(),
                    &settings.resolver,
                    client_builder,
                    state,
                    resolve,
                    concurrency,
                    cache,
                    workspace_cache,
                    printer,
                    preview,
                )
                .await?,
            )
        };

        // Hash the resolution by hashing the generated lockfile.
        let resolution_hash = {
            let mut distributions = resolution
                .graph()
                .node_weights()
                .filter_map(|node| match node {
                    Node::Dist {
                        dist,
                        hashes,
                        install: true,
                    } => Some((dist, hashes)),
                    Node::Dist { install: false, .. } | Node::Root => None,
                })
                .map(|(dist, hashes)| {
                    Ok(CachedEnvironmentDist {
                        dist: dist.clone(),
                        hashes: hashes.clone(),
                        cache_info: Self::cache_info(dist).map_err(ProjectError::from)?,
                    })
                })
                .collect::<Result<Vec<_>, ProjectError>>()?;
            distributions.sort_unstable_by(|left, right| {
                left.dist
                    .distribution_id()
                    .cmp(&right.dist.distribution_id())
            });
            hash_digest(&distributions)
        };

        // Construct a hash for the environment.
        //
        // Use the canonicalized base interpreter path since that's the interpreter we performed the
        // resolution with and the interpreter the environment will be created with.
        //
        // We cache environments independent of the environment they'd be layered on top of. The
        // assumption is such that the environment will _not_ be modified by the user or uv;
        // otherwise, we risk cache poisoning. For example, if we were to write a `.pth` file to
        // the cached environment, it would be shared across all projects that use the same
        // interpreter and the same cached dependencies.
        //
        // TODO(zanieb): We should include the version of the base interpreter in the hash, so if
        // the interpreter at the canonicalized path changes versions we construct a new
        // environment.
        let interpreter_hash =
            cache_digest(&canonicalize_executable(interpreter.sys_executable())?);

        // Search in the content-addressed cache.
        let cache_entry = cache.entry(CacheBucket::Environments, interpreter_hash, resolution_hash);

        if let Ok(root) = cache.resolve_link(cache_entry.path()) {
            if let Ok(environment) = PythonEnvironment::from_root(root, cache) {
                return Ok(Self(environment));
            }
        }

        // Create the environment in the cache, then relocate it to its content-addressed location.
        let temp_dir = cache.venv_dir()?;
        let venv = uv_virtualenv::create_venv(
            temp_dir.path(),
            interpreter,
            uv_virtualenv::Prompt::None,
            false,
            uv_virtualenv::OnExisting::Remove(uv_virtualenv::RemovalReason::TemporaryEnvironment),
            true,
            false,
            false,
        )?;

        sync_environment(
            venv,
            &resolution,
            Modifications::Exact,
            build_constraints,
            settings.into(),
            client_builder,
            state,
            install,
            installer_metadata,
            concurrency,
            cache,
            printer,
            preview,
        )
        .await?;

        // Now that the environment is complete, sync it to its content-addressed location.
        let id = cache.persist(temp_dir.keep(), cache_entry.path()).await?;
        let root = cache.archive(&id);

        Ok(Self(PythonEnvironment::from_root(root, cache)?))
    }

    /// Return any mutable cache info that should invalidate a cached environment for a given
    /// distribution.
    fn cache_info(dist: &ResolvedDist) -> Result<Option<CacheInfo>, uv_cache_info::CacheInfoError> {
        let path = match dist {
            ResolvedDist::Installed { .. } => return Ok(None),
            ResolvedDist::Installable { dist, .. } => match dist.as_ref() {
                Dist::Built(BuiltDist::Path(wheel)) => wheel.install_path.as_ref(),
                Dist::Source(SourceDist::Path(sdist)) => sdist.install_path.as_ref(),
                Dist::Source(SourceDist::Directory(directory)) => directory.install_path.as_ref(),
                _ => return Ok(None),
            },
        };

        Ok(Some(CacheInfo::from_path(path)?))
    }

    /// Return the [`Interpreter`] to use for the cached environment, based on a given
    /// [`Interpreter`].
    ///
    /// When caching, always use the base interpreter, rather than that of the virtual
    /// environment.
    fn base_interpreter(
        interpreter: &Interpreter,
        cache: &Cache,
    ) -> Result<Interpreter, uv_python::Error> {
        let base_python = if cfg!(unix) {
            interpreter.find_base_python()?
        } else {
            interpreter.to_base_python()?
        };
        if base_python == interpreter.sys_executable() {
            debug!(
                "Caching via base interpreter: `{}`",
                interpreter.sys_executable().display()
            );
            Ok(interpreter.clone())
        } else {
            let base_interpreter = Interpreter::query(base_python, cache)?;
            debug!(
                "Caching via base interpreter: `{}`",
                base_interpreter.sys_executable().display()
            );
            Ok(base_interpreter)
        }
    }
}

/// Convert a `pylock.toml` file (from a local path or HTTP(S) URL) into a [`Resolution`], so it
/// can be installed into an ephemeral [`CachedEnvironment`] without a resolver run.
async fn resolve_from_pylock(
    pylock: &Path,
    interpreter: &Interpreter,
    python_platform: Option<&TargetTriple>,
    build_options: &BuildOptions,
    client_builder: &BaseClientBuilder<'_>,
) -> Result<Resolution, ProjectError> {
    let (install_path, content) = if pylock.starts_with("http://")
        || pylock.starts_with("https://")
    {
        let url = uv_redacted::DisplaySafeUrl::parse(&pylock.to_string_lossy())
            .map_err(|err| ProjectError::Anyhow(err.into()))?;
        let client = client_builder.build()?;
        let response = client
            .for_host(&url)
            .get(url::Url::from(url.clone()))
            .send()
            .await
            .map_err(|err| ProjectError::Anyhow(err.into()))?;
        response
            .error_for_status_ref()
            .map_err(|err| ProjectError::Anyhow(err.into()))?;
        let content = response
            .text()
            .await
            .map_err(|err| ProjectError::Anyhow(err.into()))?;
        (std::env::current_dir()?, content)
    } else {
        let absolute = std::path::absolute(pylock)?;
        let install_path = absolute
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(PathBuf::new);
        let content = fs_err::tokio::read_to_string(pylock).await?;
        (install_path, content)
    };

    let lock = info_span!("toml::from_str pylock.toml", path = %pylock.display())
        .in_scope(|| toml::from_str::<PylockToml>(&content))
        .with_context(|| format!("Not a valid `pylock.toml` file: {}", pylock.user_display()))
        .map_err(ProjectError::Anyhow)?;

    if let Some(requires_python) = lock.requires_python.as_ref() {
        if !requires_python.contains(interpreter.python_version()) {
            return Err(ProjectError::Anyhow(anyhow::anyhow!(
                "The requested interpreter resolved to Python {}, which is incompatible with the `pylock.toml`'s Python requirement: `{}`",
                interpreter.python_version(),
                requires_python,
            )));
        }
    }

    let tags = resolution_tags(None, python_platform, interpreter)?;
    let marker_env = resolution_markers(None, python_platform, interpreter);
    let extras: Vec<ExtraName> = Vec::new();
    let groups: Vec<GroupName> = Vec::new();

    lock.to_resolution(
        &install_path,
        marker_env.markers(),
        &extras,
        &groups,
        &tags,
        build_options,
    )
    .map_err(|err| ProjectError::Anyhow(err.into()))
}
