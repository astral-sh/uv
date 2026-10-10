use std::borrow::Cow;
#[cfg(any(test, feature = "testing"))]
use std::ops::BitOr;
use std::sync::{Mutex, OnceLock};
use std::{
    fmt::{Debug, Display, Formatter},
    str::FromStr,
};

use enumflags2::{BitFlags, bitflags};
use thiserror::Error;
use uv_macros::PreviewMetadata;
use uv_warnings::warn_user_once;

/// Indicates if the preview state has been finalized yet or not.
enum PreviewState {
    Provisional {
        preview: Preview,
        required: BitFlags<PreviewFeature>,
    },
    Final(Preview),
}

fn check_required(preview: Preview, feature: PreviewFeature) -> Result<(), PreviewError> {
    if preview.is_enabled(feature) {
        Ok(())
    } else {
        Err(PreviewError::FeatureRequired(feature))
    }
}

/// Indicates how the preview was initialised, to distinguish between normal
/// code and unit tests.
enum PreviewMode {
    /// Initialised by a call to [`init`].
    Normal(Mutex<PreviewState>),
    /// Initialised by a call to [`test::with_features`].
    #[cfg(feature = "testing")]
    Test(std::sync::RwLock<Option<Preview>>),
}

static PREVIEW: OnceLock<PreviewMode> = OnceLock::new();

/// Error type for global preview state initialization related errors
#[derive(Debug, Error)]
pub enum PreviewError {
    /// Returned when [`set`] or [`finalize`] are called on a finalized state.
    #[error("The preview configuration has already been finalized")]
    AlreadyFinalized,

    /// Returned when [`finalize`] is called on an uninitialized state.
    #[error("The preview configuration has not been initialized yet")]
    NotInitialized,

    /// A parsed input requires a preview feature that was not enabled.
    #[error("The `{0}` feature is experimental and requires `--preview-features {0}`")]
    FeatureRequired(PreviewFeature),

    /// Returned when [`set`] or [`finalize`] are called on a test state.
    #[cfg(feature = "testing")]
    #[error("The preview configuration is in test mode and {}::{} cannot be used", module_path!(), .0)]
    InTest(&'static str),
}

/// Set the global preview configuration before finalization.
///
/// Requirements recorded by [`require`] are retained when the configuration changes.
pub fn set(preview: Preview) -> Result<(), PreviewError> {
    let mode = PREVIEW.get_or_init(|| {
        PreviewMode::Normal(Mutex::new(PreviewState::Provisional {
            preview: Preview::default(),
            required: BitFlags::empty(),
        }))
    });
    match mode {
        PreviewMode::Normal(mutex) => {
            // Calling `set` in a test context is already disallowed, so a panic if
            // the mutex is poisoned is fine.
            let mut state = mutex.lock().unwrap();
            match &mut *state {
                PreviewState::Provisional {
                    preview: current, ..
                } => {
                    *current = preview;
                    Ok(())
                }
                PreviewState::Final(_) => Err(PreviewError::AlreadyFinalized),
            }
        }
        #[cfg(feature = "testing")]
        PreviewMode::Test(_) => Err(PreviewError::InTest("set")),
    }
}

/// Finalize the preview configuration, checking all requirements recorded during parsing.
pub fn finalize() -> Result<(), PreviewError> {
    match PREVIEW.get().ok_or(PreviewError::NotInitialized)? {
        PreviewMode::Normal(mutex) => {
            // Calling `set` in a test context is already disallowed, so a panic if
            // the mutex is poisoned is fine.
            let mut state = mutex.lock().unwrap();
            match &*state {
                PreviewState::Provisional { preview, required } => {
                    for feature in *required {
                        check_required(*preview, feature)?;
                    }
                    *state = PreviewState::Final(*preview);
                    Ok(())
                }
                PreviewState::Final(_) => Err(PreviewError::AlreadyFinalized),
            }
        }
        #[cfg(feature = "testing")]
        PreviewMode::Test(_) => Err(PreviewError::InTest("finalize")),
    }
}

/// Get the current global preview configuration.
///
/// # Panics
///
/// When called before [`init`] or (with the `testing` feature) when the
/// current thread does not hold a [`test::with_features`] guard.
fn get() -> Preview {
    match PREVIEW.get() {
        Some(PreviewMode::Normal(mutex)) => match *mutex.lock().unwrap() {
            PreviewState::Provisional { preview, .. } => preview,
            PreviewState::Final(preview) => preview,
        },
        #[cfg(feature = "testing")]
        Some(PreviewMode::Test(rwlock)) => {
            assert!(
                test::HELD.get(),
                "The preview configuration is in test mode but the current thread does not hold a `FeaturesGuard`\nHint: Use `{}::test::with_features` to get a `FeaturesGuard` and hold it when testing functions which rely on the global preview state",
                module_path!()
            );
            // The unwrap may panic only if the current thread had panicked
            // while attempting to write the value and then recovered with
            // `catch_unwind`. This seems unlikely.
            rwlock
                .read()
                .unwrap()
                .expect("FeaturesGuard is held but preview value is not set")
        }
        #[cfg(feature = "testing")]
        None => panic!(
            "The preview configuration has not been initialized\nHint: Use `{}::init` or `{}::test::with_features` to initialize it",
            module_path!(),
            module_path!()
        ),
        #[cfg(not(feature = "testing"))]
        None => panic!("The preview configuration has not been initialized"),
    }
}

/// Check if a specific preview feature is enabled globally.
pub fn is_enabled(flag: PreviewFeature) -> bool {
    get().is_enabled(flag)
}

/// Require a preview feature for an input parsed before or after configuration discovery.
///
/// Before [`finalize`], record the requirement so a feature enabled in the same configuration
/// file can satisfy it. After finalization, reject disabled features immediately.
pub fn require(feature: PreviewFeature) -> Result<(), PreviewError> {
    let mode = PREVIEW.get_or_init(|| {
        PreviewMode::Normal(Mutex::new(PreviewState::Provisional {
            preview: Preview::default(),
            required: BitFlags::empty(),
        }))
    });
    match mode {
        PreviewMode::Normal(mutex) => {
            let mut state = mutex.lock().expect("Preview state lock is not poisoned");
            match &mut *state {
                PreviewState::Provisional { required, .. } => {
                    required.insert(feature);
                    Ok(())
                }
                PreviewState::Final(preview) => check_required(*preview, feature),
            }
        }
        #[cfg(feature = "testing")]
        PreviewMode::Test(_) => check_required(get(), feature),
    }
}

/// Functions for unit tests, do not use from normal code!
#[cfg(feature = "testing")]
pub mod test {
    use super::{PREVIEW, Preview, PreviewMode};
    use std::cell::Cell;
    use std::sync::{Mutex, MutexGuard, RwLock};

    /// The global preview state test mutex. It does not guard any data but is
    /// simply used to ensure tests which rely on the global preview state are
    /// ran serially.
    static MUTEX: Mutex<()> = Mutex::new(());

    thread_local! {
        /// Whether the current thread holds the global mutex.
        ///
        /// This is used to catch situations where a test forgets to set the
        /// global test state but happens to work anyway because of another test
        /// setting the state.
        pub(crate) static HELD: Cell<bool> = const { Cell::new(false) };
    }

    /// A scope guard which ensures that the global preview state is configured
    /// and consistent for the duration of its lifetime.
    #[derive(Debug)]
    #[expect(unused)]
    pub struct FeaturesGuard(MutexGuard<'static, ()>);

    /// Temporarily set the state of preview features for the duration of the
    /// lifetime of the returned guard.
    ///
    /// Calls cannot be nested, and this function must be used to set the global
    /// preview features when testing functionality which uses it, otherwise
    /// that functionality will panic.
    ///
    /// The preview state will only be valid for the thread which calls this
    /// function, it will not be valid for any other thread. This is a
    /// consequence of how `HELD` is used to check for tests which are missing
    /// the guard.
    pub fn with_features(features: &[super::PreviewFeature]) -> FeaturesGuard {
        assert!(
            !HELD.get(),
            "Additional calls to `{}::with_features` are not allowed while holding a `FeaturesGuard`",
            module_path!()
        );

        let guard = match MUTEX.lock() {
            Ok(guard) => guard,
            // This is okay because the mutex isn't guarding any data, so when
            // it gets poisoned, it just means a test thread died while holding
            // it, so it's safe to just re-grab it from the PoisonError, there's
            // no chance of any corruption.
            Err(err) => err.into_inner(),
        };

        HELD.set(true);

        let state = PREVIEW.get_or_init(|| PreviewMode::Test(RwLock::new(None)));
        match state {
            PreviewMode::Test(rwlock) => {
                *rwlock.write().unwrap() = Some(Preview::new(features));
            }
            PreviewMode::Normal(_) => {
                panic!(
                    "Cannot use `{}::with_features` after `uv_preview::init` has been called",
                    module_path!()
                );
            }
        }
        FeaturesGuard(guard)
    }

    impl Drop for FeaturesGuard {
        fn drop(&mut self) {
            HELD.set(false);

            match PREVIEW.get().unwrap() {
                PreviewMode::Test(rwlock) => {
                    *rwlock.write().unwrap() = None;
                }
                PreviewMode::Normal(_) => {
                    unreachable!("FeaturesGuard should not exist when in Normal mode");
                }
            }
        }
    }
}

#[bitflags]
#[expect(
    clippy::use_self,
    reason = "enumflags2 refers to the enum by name when inferring bits"
)]
#[repr(u64)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PreviewMetadata)]
pub enum PreviewFeature {
    /// The `uv python install --default` option installs `python` and `python3` executables
    /// alongside the versioned executable, making a uv-managed Python available without specifying
    /// its minor version. When no version is requested and no `.python-version` file is found,
    /// enabling this feature also makes `uv python install` create these executables by default.
    /// See [installing Python executables](./python-versions.md#installing-python-executables) for
    /// details about the installation location and handling of existing executables.
    PythonInstallDefault,
    /// Commands that support `--output-format json`, such as `uv tree`, can produce
    /// machine-readable output for use by other tools. The JSON schemas are experimental and may
    /// change without warning; enabling this feature silences the preview warning.
    JsonOutput,
    /// The `uv pip install` and `uv pip sync` commands can install dependencies from `pylock.toml`
    /// files, the standardized Python lockfile format. For example, use
    /// `uv pip install -r pylock.toml` or `uv pip sync pylock.toml`; enabling this feature silences
    /// the preview warning.
    Pylock,
    /// The [`add-bounds`](../reference/settings.md#add-bounds) setting controls the default version
    /// bounds added by `uv add`. Instead of the default lower bound, dependencies can be
    /// constrained to a major or minor version range, or pinned to an exact version.
    AddBounds,
    /// Workspace members can declare [conflicting
    /// dependencies](./resolution.md#conflicting-dependencies) at the package level, in addition to
    /// conflicts between extras or dependency groups. This allows members with incompatible
    /// requirements to share a lockfile, provided they are not installed together.
    PackageConflicts,
    /// The [`extra-build-dependencies`](./projects/config.md#augmenting-build-dependencies) setting
    /// augments a package's declared build dependencies without disabling build isolation. It can
    /// supply a missing build dependency or, with `match-runtime = true`, ensure that a build
    /// dependency uses the same version as the project environment.
    ExtraBuildDependencies,
    /// uv warns when multiple packages install conflicting Python modules into the same
    /// environment. These conflicts can cause imports to depend on installation order, even when
    /// the packages have different distribution names.
    DetectModuleConflicts,
    /// The `uv format` command formats Python code with Ruff, downloading the formatter when
    /// needed. It can also check formatting with `--check` or show proposed changes with `--diff`;
    /// enabling this feature silences the preview warning.
    #[preview(alias = "format")]
    FormatCommand,
    /// The `uv auth` commands store credentials in a [system-native
    /// location](./authentication/http.md#the-uv-credentials-store), using Keychain Services on
    /// macOS, Credential Manager on Windows, or the Secret Service API on Linux. uv only retrieves
    /// credentials that it has stored itself, rather than credentials saved by other applications.
    NativeAuth,
    /// uv can authenticate requests to an S3-compatible storage endpoint using AWS Signature
    /// Version 4. Set [`UV_S3_ENDPOINT_URL`](../reference/environment.md#uv_s3_endpoint_url) to
    /// identify the endpoint; credentials are obtained from the configured AWS credential sources.
    S3Endpoint,
    /// The `uv cache size` command reports the total size of uv's cache. It supports human-readable
    /// output and a machine-readable byte count; enabling this feature silences the preview
    /// warning.
    CacheSize,
    /// Cache cleanup reports the physical disk space reclaimed, accounting for hardlinks and
    /// copy-on-write clones on macOS and Linux. If an entry's allocated size cannot be measured, uv
    /// reports a lower bound; other platforms continue to use a coarser estimate. See [clearing the
    /// cache](./cache.md#clearing-the-cache) for details.
    CachePhysicalSpace,
    /// The `uv init` command rejects the deprecated `--project` option. To choose where to create a
    /// project, pass the target directory as a positional argument, such as `uv init my-project`.
    InitProjectFlag,
    /// The `uv workspace metadata` command exposes structured information about a workspace and its
    /// resolved dependencies for use by other tools. Its output is experimental and may change; use
    /// `--sync` to include a mapping from importable modules to the packages that provide them.
    WorkspaceMetadata,
    /// The `uv workspace dir` command prints the path to the workspace root. Use `--package` to
    /// print the path to a specific workspace member, for example, with `--package my-package`.
    WorkspaceDir,
    /// The `uv workspace list` command lists the names of workspace members, with one name per
    /// line. Use `--paths` to display their paths instead, for example, when passing workspace
    /// directories to another tool.
    WorkspaceList,
    /// The `uv export --format cyclonedx1.5` command exports a software bill of materials in
    /// CycloneDX 1.5 JSON format. This describes the locked dependencies in a format that can be
    /// consumed by software inventory and security tools.
    SbomExport,
    /// The [`uv auth helper`](./authentication/cli.md#using-credentials-with-external-tools)
    /// command lets external tools retrieve HTTP credentials through uv. It currently supports the
    /// Bazel credential helper protocol, reading a JSON request from standard input and writing a
    /// JSON response containing authentication headers when credentials are available.
    AuthHelper,
    /// For a local `uv run` target, uv starts project and workspace discovery from the directory
    /// containing the target instead of the current working directory. This feature takes effect
    /// before configuration is loaded, so it must be enabled on the command line or through an
    /// environment variable.
    TargetWorkspaceDiscovery,
    /// The uv build backend includes `METADATA.json` and `WHEEL.json` files in built wheels
    /// alongside the standard `METADATA` and `WHEEL` files. These additional files expose package
    /// and wheel metadata in JSON format.
    MetadataJson,
    /// uv can authenticate requests to a Google Cloud Storage endpoint using Google Cloud
    /// credentials. Set [`UV_GCS_ENDPOINT_URL`](../reference/environment.md#uv_gcs_endpoint_url) to
    /// identify the endpoint; authentication uses `GOOGLE_APPLICATION_CREDENTIALS` or Application
    /// Default Credentials.
    GcsEndpoint,
    /// On Unix, uv raises the process's soft open-file limit at startup, up to the hard limit. This
    /// helps avoid "too many open files" errors during concurrent operations, and child processes
    /// inherit the raised limit.
    AdjustUlimit,
    /// Conda environments named `base` or `root` are classified using their paths, like other named
    /// environments. The name alone does not cause uv to treat a user-created environment as the
    /// base Conda installation.
    SpecialCondaEnvNames,
    /// uv creates relocatable virtual environments by default, using relative paths in their entry
    /// point and activation scripts. This allows the environment to be moved without invalidating
    /// those scripts, although arbitrary binaries and nonstandard scripts are not guaranteed to be
    /// relocatable. Use `uv venv --no-relocatable` to opt out.
    RelocatableEnvsDefault,
    /// The `uv publish` command requires distribution filenames to be normalized. Files with
    /// non-normalized names are skipped when selecting distributions to upload.
    PublishRequireNormalized,
    /// The `uv audit` and `uv tool audit` commands check project and installed-tool dependencies
    /// for known vulnerabilities. Tool audits use the lockfiles recorded by the
    /// [`tool-install-locks`](#tool-install-locks) feature; enabling both features silences the
    /// preview warning for `uv tool audit`.
    #[preview(alias = "audit")]
    AuditCommand,
    /// The `--project` option rejects invalid paths instead of warning and continuing in the
    /// current directory. Except for `uv init`, the path must already exist as a directory or point
    /// to a `pyproject.toml` file. This feature takes effect before configuration is loaded, so it
    /// must be enabled on the command line or through an environment variable.
    ProjectDirectoryMustExist,
    /// Each configured package index can set its own
    /// [`exclude-newer`](./indexes.md#configuring-exclude-newer-for-an-index) cutoff, overriding
    /// the global cutoff for packages served by that index. Set the value to `false` to disable the
    /// cutoff for an index; package-specific `exclude-newer-package` values still take precedence.
    IndexExcludeNewer,
    /// uv can authenticate requests to an Azure Blob Storage endpoint using Azure credentials. Set
    /// [`UV_AZURE_ENDPOINT_URL`](../reference/environment.md#uv_azure_endpoint_url) to identify the
    /// endpoint; authentication uses the default Azure credential chain, including Azure CLI
    /// credentials and workload identity.
    AzureEndpoint,
    /// When building source distributions, the uv build backend rewrites `pyproject.toml` as TOML
    /// 1.0 for compatibility with older build tools. The original file is included as
    /// `pyproject.toml.orig` in the source distribution.
    TomlBackwardsCompatibility,
    /// The `uv sync` command and other installation commands can check packages for malware using
    /// [OSV](https://osv.dev) before installing them. This checks for known malicious packages,
    /// while [`uv audit`](#audit-command) checks dependencies for known vulnerabilities.
    MalwareCheck,
    /// The `uv venv --clear` option refuses to clear a directory that does not contain a
    /// `pyvenv.cfg` file. This helps avoid removing unrelated files when the target is not a
    /// virtual environment; use `--force` to explicitly allow clearing such a directory.
    VenvSafeClear,
    /// The `uv check` command runs Python type checking with ty. It uses the project's environment
    /// to resolve imports and can synchronize that environment before checking; enabling this
    /// feature silences the preview warning.
    #[preview(alias = "check")]
    CheckCommand,
    /// The `uv init` command creates a packaged application by default, with a `src/` layout, a
    /// build system, and a script entry point. This gives new applications an installable package
    /// structure without requiring `--package`.
    PackagedInit,
    /// uv stores default [project virtual
    /// environments](./projects/layout.md#centralized-project-environments) in its cache and
    /// attempts to link `.venv` to the cached environment. Switching interpreters selects separate
    /// cached environments that can be reused later. Explicit environment paths and environments
    /// selected with `--active` are not centralized.
    CentralizedProjectEnvs,
    /// uv stores a `uv.lock` file alongside each installed tool and reuses it for subsequent
    /// installations and upgrades. The lockfile records the tool's resolved dependencies and also
    /// provides the dependency information used by `uv tool audit`.
    ToolInstallLocks,
    /// The `uv workspace list --scripts` command lists standalone Python scripts with inline
    /// metadata under the workspace root. It prints script paths relative to the workspace root,
    /// allowing tools to discover scripts separately from workspace members.
    WorkspaceListScripts,
    /// Each configured package index can [require a hash
    /// algorithm](./indexes.md#requiring-a-hash-algorithm) with the `hash-algorithm` setting. uv
    /// records that algorithm's hash in the lockfile and fails if a distribution does not advertise
    /// it, instead of selecting another available algorithm.
    IndexHashAlgorithm,
    /// Commands using `--locked` or `--check` reject non-canonical lockfile formatting, even if the
    /// lockfile can be parsed. This makes formatting part of the lockfile check in addition to
    /// checking whether the resolution is up to date.
    LockfileFormatCheck,
    /// uv combines equivalent dependency declarations when writing lockfiles. This reduces
    /// duplicated declarations in the recorded requirements, constraints, overrides, exclusions,
    /// and dependency groups.
    LockfileNormalization,
    /// uv omits the `package.metadata` tables from `uv.lock`, except for remote URL and Git
    /// dependencies. Their metadata is retained so uv can check whether those sources are requested
    /// or stale without network access.
    LockWithoutMetadata,
    /// The `--index` and `--default-index` options accept the names of configured package indexes
    /// as well as URLs. For example, `--index internal` selects the configured index named
    /// `internal`, so its URL does not need to be repeated on the command line.
    IndexByName,
    /// The `uv pip compile --generate-hashes` command restricts the generated hashes to artifacts
    /// allowed by the selected binary and build policies. For example, when source builds are
    /// disabled, hashes for source distributions are omitted.
    ArtifactHashFiltering,
    /// uv identifies cached wheel archives by a digest of their contents. Matching archive contents
    /// can share a cache entry, reducing duplication when the same wheel contents are encountered
    /// more than once.
    ContentAddressedCache,
    /// uv omits `exclude-newer-package` entries from the lockfile when the corresponding packages
    /// are absent from the resolved dependencies. This keeps cutoffs for unrelated packages out of
    /// `uv.lock`.
    MissingExcludeNewerPackageLock,
    /// uv omits redundant runtime constraints and unused overrides, exclusions, dependency
    /// metadata, and package-specific upload cutoffs from the lockfile. It records which settings
    /// were consulted during resolution, including backtracking, so settings that affected
    /// resolution can be retained even when their packages are absent from the final dependency
    /// graph.
    ResolutionInputs,
    /// The `uv export --batch` option exports multiple dependency selections from a TOML manifest
    /// containing `[[export]]` entries. Each entry specifies an `output-file` and its own package,
    /// extra, and dependency group selections; output paths are relative to the manifest.
    BatchExport,
    /// Frozen project commands can use `uv.lock` without the workspace's `pyproject.toml` file.
    /// Discovery requires a lockfile with revision 5 or later, which records the workspace
    /// information needed to select dependencies without the manifest.
    FrozenLockfile,
    /// The [`minimum-libc-version`](./resolution.md#minimum-libc-version) setting specifies the
    /// oldest glibc or musl versions that must be supported during universal resolution. Use it
    /// with `required-environments` to require compatible wheels for the selected Linux
    /// environments without excluding wheels for newer libc versions.
    MinimumLibcVersion,
    /// Before a nonisolated `uv build`, uv checks that the selected environment satisfies declared,
    /// backend-reported, and transitive build requirements. This reports missing or incompatible
    /// build dependencies before building; use `--skip-dependency-check` to skip the check.
    BuildDependencyCheck,
    /// uv enables lazy imports when invoking build backends on CPython 3.15 and later. This can
    /// reduce the work performed by imports during a build, but can also change import-time side
    /// effects in third-party build backends.
    BuildLazyImports,
    /// The `--require-build-hashes` option requires hashes for every build dependency, including
    /// transitive dependencies. It applies when build dependencies are downloaded during builds,
    /// project resolution, and installation. See [project build dependency
    /// hashes](./projects/build.md#project-build-dependency-hashes) for configuration and exceptions.
    BuildDependencyHashes,
}

impl Display for PreviewFeature {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Error, Clone)]
#[error("Unknown feature flag")]
pub struct PreviewFeatureParseError;

impl FromStr for PreviewFeature {
    type Err = PreviewFeatureParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::metadata()
            .iter()
            .find(|(feature, _, aliases)| feature.as_str() == s || aliases.contains(&s))
            .map(|(feature, _, _)| *feature)
            .ok_or(PreviewFeatureParseError)
    }
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
#[error("preview feature name cannot be empty")]
pub struct EmptyPreviewFeatureNameError;

/// A user-provided preview feature name, which may refer to an unknown feature.
#[derive(Debug, Clone)]
pub enum MaybePreviewFeature {
    Known(PreviewFeature),
    Unknown(String),
}

impl FromStr for MaybePreviewFeature {
    type Err = EmptyPreviewFeatureNameError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        if s.is_empty() {
            return Err(EmptyPreviewFeatureNameError);
        }

        Ok(match PreviewFeature::from_str(s) {
            Ok(feature) => Self::Known(feature),
            Err(_) => Self::Unknown(s.to_string()),
        })
    }
}

impl<'de> serde::Deserialize<'de> for MaybePreviewFeature {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let name: Cow<'de, str> = serde::Deserialize::deserialize(deserializer)?;
        Self::from_str(&name).map_err(serde::de::Error::custom)
    }
}

#[cfg(feature = "schemars")]
impl schemars::JsonSchema for MaybePreviewFeature {
    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("PreviewFeature")
    }

    fn json_schema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        // Advertise canonical names for editor completions, while accepting any nonempty name to
        // match the forwards-compatible runtime parsing behavior.
        let choices: Vec<&str> = BitFlags::<PreviewFeature>::all()
            .iter()
            .map(PreviewFeature::as_str)
            .collect();
        schemars::json_schema!({
            "type": "string",
            "anyOf": [
                {
                    "enum": choices,
                },
                {
                    "pattern": "\\S",
                },
            ],
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub struct Preview {
    flags: BitFlags<PreviewFeature>,
}

impl Debug for Preview {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let flags: Vec<_> = self.flags.iter().collect();
        f.debug_struct("Preview").field("flags", &flags).finish()
    }
}

impl Preview {
    #[cfg(any(test, feature = "testing"))]
    fn new(flags: &[PreviewFeature]) -> Self {
        Self {
            flags: flags.iter().copied().fold(BitFlags::empty(), BitOr::bitor),
        }
    }

    pub fn all() -> Self {
        Self {
            flags: BitFlags::all(),
        }
    }

    /// Check if a single feature is enabled.
    pub fn is_enabled(&self, flag: PreviewFeature) -> bool {
        self.flags.contains(flag)
    }

    /// Check if all preview feature rae enabled.
    pub fn all_enabled(&self) -> bool {
        self.flags.is_all()
    }

    /// Check if any preview feature is enabled.
    pub fn any_enabled(&self) -> bool {
        !self.flags.is_empty()
    }

    /// Resolve preview feature names, warning and ignoring unknown names.
    pub fn from_feature_names<'a>(
        feature_names: impl IntoIterator<Item = &'a MaybePreviewFeature>,
    ) -> Self {
        let mut flags = BitFlags::empty();

        for feature_name in feature_names {
            match feature_name {
                MaybePreviewFeature::Known(feature) => flags |= *feature,
                MaybePreviewFeature::Unknown(feature_name) => {
                    warn_user_once!("Unknown preview feature: `{feature_name}`");
                }
            }
        }

        Self { flags }
    }
}

impl Display for Preview {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if self.flags.is_empty() {
            write!(f, "disabled")
        } else if self.flags.is_all() {
            write!(f, "enabled")
        } else {
            write!(
                f,
                "{}",
                itertools::join(self.flags.iter().map(PreviewFeature::as_str), ",")
            )
        }
    }
}

impl FromStr for Preview {
    type Err = EmptyPreviewFeatureNameError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let feature_names = s
            .split(',')
            .map(MaybePreviewFeature::from_str)
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Self::from_feature_names(&feature_names))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_required_features() -> Result<(), PreviewError> {
        {
            let _guard = test::with_features(&[PreviewFeature::Pylock]);
            require(PreviewFeature::Pylock)?;
            assert!(matches!(
                require(PreviewFeature::JsonOutput),
                Err(PreviewError::FeatureRequired(PreviewFeature::JsonOutput))
            ));
        }
        {
            let _guard = test::with_features(&[PreviewFeature::JsonOutput]);
            require(PreviewFeature::JsonOutput)?;
            assert!(matches!(
                require(PreviewFeature::Pylock),
                Err(PreviewError::FeatureRequired(PreviewFeature::Pylock))
            ));
        }
        Ok(())
    }

    #[test]
    fn test_preview_feature_from_str() {
        for &(feature, _, aliases) in PreviewFeature::metadata() {
            assert_eq!(PreviewFeature::from_str(feature.as_str()).unwrap(), feature);

            for &alias in aliases {
                assert_eq!(PreviewFeature::from_str(alias).unwrap(), feature);
            }
        }
    }

    #[test]
    fn test_preview_from_str() {
        // Test single feature
        let preview = Preview::from_str("python-install-default").unwrap();
        assert_eq!(preview.flags, PreviewFeature::PythonInstallDefault);

        // Test multiple features
        let preview = Preview::from_str("json-output,pylock").unwrap();
        assert!(preview.is_enabled(PreviewFeature::JsonOutput));
        assert!(preview.is_enabled(PreviewFeature::Pylock));
        assert_eq!(preview.flags.bits().count_ones(), 2);

        let preview = Preview::from_str("tool-install-locks").unwrap();
        assert!(preview.is_enabled(PreviewFeature::ToolInstallLocks));

        // Test with whitespace
        let preview = Preview::from_str("pylock , add-bounds").unwrap();
        assert!(preview.is_enabled(PreviewFeature::Pylock));
        assert!(preview.is_enabled(PreviewFeature::AddBounds));

        // Test empty string error
        assert_eq!(Preview::from_str(""), Err(EmptyPreviewFeatureNameError));
        assert!(Preview::from_str("pylock,").is_err());
        assert!(Preview::from_str(",pylock").is_err());

        // Test unknown feature (should be ignored with warning)
        let preview = Preview::from_str("unknown-feature,pylock").unwrap();
        assert!(preview.is_enabled(PreviewFeature::Pylock));
        assert_eq!(preview.flags.bits().count_ones(), 1);
    }

    #[test]
    fn test_preview_display() {
        // Test disabled
        let preview = Preview::default();
        assert_eq!(preview.to_string(), "disabled");
        let preview = Preview::new(&[]);
        assert_eq!(preview.to_string(), "disabled");

        // Test enabled (all features)
        let preview = Preview::all();
        assert_eq!(preview.to_string(), "enabled");

        // Test single feature
        let preview = Preview::new(&[PreviewFeature::PythonInstallDefault]);
        assert_eq!(preview.to_string(), "python-install-default");

        // Test multiple features
        let preview = Preview::new(&[PreviewFeature::JsonOutput, PreviewFeature::Pylock]);
        assert_eq!(preview.to_string(), "json-output,pylock");
    }

    #[test]
    fn test_global_preview() {
        {
            let _guard =
                test::with_features(&[PreviewFeature::Pylock, PreviewFeature::WorkspaceMetadata]);
            assert!(!is_enabled(PreviewFeature::InitProjectFlag));
            assert!(is_enabled(PreviewFeature::Pylock));
            assert!(is_enabled(PreviewFeature::WorkspaceMetadata));
            assert!(!is_enabled(PreviewFeature::AuthHelper));
        }
        {
            let _guard =
                test::with_features(&[PreviewFeature::InitProjectFlag, PreviewFeature::AuthHelper]);
            assert!(is_enabled(PreviewFeature::InitProjectFlag));
            assert!(!is_enabled(PreviewFeature::Pylock));
            assert!(!is_enabled(PreviewFeature::WorkspaceMetadata));
            assert!(is_enabled(PreviewFeature::AuthHelper));
        }
    }

    #[test]
    #[should_panic(
        expected = "Additional calls to `uv_preview::test::with_features` are not allowed while holding a `FeaturesGuard`"
    )]
    fn test_global_preview_panic_nested() {
        let _guard =
            test::with_features(&[PreviewFeature::Pylock, PreviewFeature::WorkspaceMetadata]);
        let _guard2 =
            test::with_features(&[PreviewFeature::InitProjectFlag, PreviewFeature::AuthHelper]);
    }

    #[test]
    #[should_panic(expected = "uv_preview::test::with_features")]
    fn test_global_preview_panic_uninitialized() {
        let _preview = get();
    }
}
