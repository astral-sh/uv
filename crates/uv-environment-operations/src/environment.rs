use tracing::debug;

use crate::{
    EnvironmentError, EnvironmentResolution, EnvironmentSpecification, resolve_environment,
    sync_environment,
};
use uv_command_support::Printer;
use uv_configuration::{Concurrency, Constraints, HashCheckingMode, Modifications, TargetTriple};
use uv_dispatch::PlatformState;
use uv_install_operations::loggers::InstallLogger;
use uv_resolve_operations::loggers::ResolveLogger;
use uv_settings::ResolverInstallerSettings;
use uv_virtualenv::UpgradePolicy;

use uv_cache::{Cache, CacheBucket};
use uv_cache_info::CacheInfo;
use uv_cache_key::{cache_digest, hash_digest};
use uv_client::BaseClientBuilder;
use uv_distribution_types::{
    BuiltDist, Dist, Identifier, Node, Resolution, ResolvedDist, SourceDist,
};
use uv_preview::Preview;
use uv_python_interpreter::{Interpreter, PythonEnvironment, canonicalize_executable};
use uv_settings::MalwareCheckSettings;
use uv_types::{HashStrategy, HashVerification, SourceTreeEditablePolicy};
use uv_workspace::WorkspaceCache;

/// A [`PythonEnvironment`] stored in the cache.
#[derive(Debug)]
pub struct CachedEnvironment(PythonEnvironment);

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

fn cached_environment_resolution_hash(
    resolution_hash: String,
    hash_strategy: &HashStrategy,
) -> String {
    match hash_strategy.verification() {
        // Preserve existing cache identities for environments materialized without verification.
        HashVerification::None => resolution_hash,
        // Never reuse an environment materialized without hash verification for a lock-backed
        // resolution with the same distributions and expected hashes.
        HashVerification::IfPresent(_) | HashVerification::Required(_) => {
            hash_digest(&("verify", resolution_hash))
        }
    }
}

impl CachedEnvironment {
    /// Get or create an [`CachedEnvironment`] based on a given set of requirements.
    pub async fn from_spec(
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
    ) -> Result<Self, EnvironmentError> {
        let interpreter = Self::base_interpreter(interpreter, cache)?;

        // Resolve the requirements with the interpreter.
        let resolution = Resolution::from(
            resolve_environment(
                spec,
                EnvironmentResolution::Specific,
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
        );

        Self::from_resolution(
            &resolution,
            HashStrategy::default(),
            build_constraints,
            &interpreter,
            settings,
            client_builder,
            state,
            install,
            installer_metadata,
            concurrency,
            cache,
            printer,
            preview,
        )
        .await
    }

    /// Get or create a [`CachedEnvironment`] from a lock-backed [`Resolution`].
    ///
    /// Prefer [`Self::from_spec`] when starting from unresolved requirements; it selects the base
    /// interpreter and resolves the requirements for that interpreter before delegating here.
    ///
    /// This method checks `resolution` for malware when enabled and verifies its recorded hashes.
    /// Both checks run before cache lookup. `interpreter` must be the base interpreter for which
    /// `resolution` was produced. In particular, callers materializing a universal lock must derive
    /// its markers and tags from the same interpreter.
    pub async fn from_locked_resolution(
        resolution: &Resolution,
        build_constraints: Constraints,
        interpreter: &Interpreter,
        settings: &ResolverInstallerSettings,
        malware_settings: &MalwareCheckSettings,
        client_builder: &BaseClientBuilder<'_>,
        state: &PlatformState,
        install: Box<dyn InstallLogger>,
        installer_metadata: bool,
        concurrency: &Concurrency,
        cache: &Cache,
        printer: Printer,
        preview: Preview,
    ) -> Result<Self, EnvironmentError> {
        let malware_check_client_builder = client_builder
            .clone()
            .keyring(settings.resolver.keyring_provider);
        crate::malware::check_resolution_malware(
            resolution,
            &malware_check_client_builder,
            concurrency,
            malware_settings,
            cache,
            preview,
        )
        .await?;

        let hash_strategy = HashStrategy::from_resolution(resolution, HashCheckingMode::Verify)?;
        Self::from_resolution(
            resolution,
            hash_strategy,
            build_constraints,
            interpreter,
            settings,
            client_builder,
            state,
            install,
            installer_metadata,
            concurrency,
            cache,
            printer,
            preview,
        )
        .await
    }

    async fn from_resolution(
        resolution: &Resolution,
        hash_strategy: HashStrategy,
        build_constraints: Constraints,
        interpreter: &Interpreter,
        settings: &ResolverInstallerSettings,
        client_builder: &BaseClientBuilder<'_>,
        state: &PlatformState,
        install: Box<dyn InstallLogger>,
        installer_metadata: bool,
        concurrency: &Concurrency,
        cache: &Cache,
        printer: Printer,
        preview: Preview,
    ) -> Result<Self, EnvironmentError> {
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
                        cache_info: Self::cache_info(dist).map_err(EnvironmentError::from)?,
                    })
                })
                .collect::<Result<Vec<_>, EnvironmentError>>()?;
            distributions.sort_unstable_by(|left, right| {
                left.dist
                    .distribution_id()
                    .cmp(&right.dist.distribution_id())
            });
            cached_environment_resolution_hash(hash_digest(&distributions), &hash_strategy)
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
            interpreter.clone(),
            uv_virtualenv::Prompt::None,
            false,
            uv_virtualenv::OnExisting::Remove(uv_virtualenv::RemovalReason::TemporaryEnvironment),
            true,
            uv_virtualenv::Seed::Disabled,
            UpgradePolicy::Fixed,
        )?;

        sync_environment(
            venv,
            resolution,
            hash_strategy,
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
    pub fn base_interpreter(
        interpreter: &Interpreter,
        cache: &Cache,
    ) -> Result<Interpreter, uv_python_discovery::Error> {
        let base_python = if cfg!(unix) {
            interpreter.find_base_python()?
        } else {
            interpreter.to_base_python()?
        };
        if base_python == interpreter.sys_executable() {
            debug!(
                "Caching via base interpreter: {}",
                interpreter.sys_executable().display()
            );
            Ok(interpreter.clone())
        } else {
            let base_interpreter = Interpreter::query(base_python, cache)?;
            debug!(
                "Caching via base interpreter: {}",
                base_interpreter.sys_executable().display()
            );
            Ok(base_interpreter)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use uv_types::HashStrategy;

    use super::{cached_environment_resolution_hash, hash_digest};

    #[test]
    fn verified_cached_environment_uses_separate_resolution_hash() {
        let resolution_hash = hash_digest(&["ty==0.0.17"]);
        let unverified =
            cached_environment_resolution_hash(resolution_hash.clone(), &HashStrategy::default());
        let verified = cached_environment_resolution_hash(
            resolution_hash.clone(),
            &HashStrategy::verify(Arc::default()),
        );

        assert_eq!(unverified, resolution_hash);
        assert_ne!(verified, unverified);
    }
}
