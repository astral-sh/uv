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
    Provisional(Preview),
    Final(Preview),
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

    /// Returned when [`set`] or [`finalize`] are called on a test state.
    #[cfg(feature = "testing")]
    #[error("The preview configuration is in test mode and {}::{} cannot be used", module_path!(), .0)]
    InTest(&'static str),
}

/// Initialize the global preview configuration.
///
/// This should be called once at startup with the resolved preview settings.
pub fn set(preview: Preview) -> Result<(), PreviewError> {
    let mode = PREVIEW.get_or_init(|| {
        PreviewMode::Normal(Mutex::new(PreviewState::Provisional(Preview::default())))
    });
    match mode {
        PreviewMode::Normal(mutex) => {
            // Calling `set` in a test context is already disallowed, so a panic if
            // the mutex is poisoned is fine.
            let mut state = mutex.lock().unwrap();
            match &*state {
                PreviewState::Provisional(_) => {
                    *state = PreviewState::Provisional(preview);
                    Ok(())
                }
                PreviewState::Final(_) => Err(PreviewError::AlreadyFinalized),
            }
        }
        #[cfg(feature = "testing")]
        PreviewMode::Test(_) => Err(PreviewError::InTest("set")),
    }
}

pub fn finalize() -> Result<(), PreviewError> {
    match PREVIEW.get().ok_or(PreviewError::NotInitialized)? {
        PreviewMode::Normal(mutex) => {
            // Calling `set` in a test context is already disallowed, so a panic if
            // the mutex is poisoned is fine.
            let mut state = mutex.lock().unwrap();
            match &*state {
                PreviewState::Provisional(preview) => {
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
            PreviewState::Provisional(preview) => preview,
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
#[repr(u64)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PreviewMetadata)]
pub enum PreviewFeature {
    /// Allows [installing `python` and `python3` executables](./python-versions.md#installing-python-executables).
    PythonInstallDefault = 1 << 0,
    /// Allows `--output-format json` for various uv commands.
    JsonOutput = 1 << 2,
    /// Allows installing from `pylock.toml` files.
    Pylock = 1 << 3,
    /// Allows configuring the [default bounds for `uv add`](../reference/settings.md#add-bounds) invocations.
    AddBounds = 1 << 4,
    /// Allows defining workspace conflicts at the package level.
    PackageConflicts = 1 << 5,
    /// Allows specifying additional dependencies for package builds.
    ExtraBuildDependencies = 1 << 6,
    /// Warns when multiple packages would install conflicting Python modules into the same
    /// environment.
    DetectModuleConflicts = 1 << 7,
    /// Allows using `uv format`.
    Format = 1 << 8,
    /// Enables storage of credentials in a [system-native location](../concepts/authentication/http.md#the-uv-credentials-store).
    NativeAuth = 1 << 9,
    /// Allows signing requests to configured S3-compatible endpoints.
    S3Endpoint = 1 << 10,
    /// Allows using `uv cache size`.
    CacheSize = 1 << 11,
    /// Rejects the deprecated `--project` option in `uv init`.
    InitProjectFlag = 1 << 12,
    /// Allows using `uv workspace metadata`.
    WorkspaceMetadata = 1 << 13,
    /// Allows using `uv workspace dir`.
    WorkspaceDir = 1 << 14,
    /// Allows using `uv workspace list`.
    WorkspaceList = 1 << 15,
    /// Allows using `uv export --format=cyclonedx1.5`.
    SbomExport = 1 << 16,
    /// Allows using `uv auth helper` as a credential helper for external tools.
    AuthHelper = 1 << 17,
    /// Allows publishing directly to a package index.
    DirectPublish = 1 << 18,
    /// Uses the directory containing a local `uv run` target, rather than the current working
    /// directory, as the starting point for project and workspace discovery. This feature takes
    /// effect before configuration is loaded.
    TargetWorkspaceDiscovery = 1 << 19,
    /// Includes JSON metadata files in built wheels.
    MetadataJson = 1 << 20,
    /// Allows signing requests to configured Google Cloud Storage endpoints.
    GcsEndpoint = 1 << 21,
    /// On Unix, raises the process's soft open-file limit at startup, up to the hard limit.
    AdjustUlimit = 1 << 22,
    /// Stops treating Conda environments named `base` or `root` as special.
    SpecialCondaEnvNames = 1 << 23,
    /// Creates relocatable virtual environments by default.
    RelocatableEnvsDefault = 1 << 24,
    /// Requires normalized distribution filenames when publishing, skipping files whose names are
    /// not normalized.
    PublishRequireNormalized = 1 << 25,
    /// Allows using `uv audit`.
    Audit = 1 << 26,
    /// Rejects an invalid `--project` path instead of warning and continuing. Except for `uv init`,
    /// the path must already exist as a directory or point to a `pyproject.toml` file. This feature
    /// takes effect before configuration is loaded.
    ProjectDirectoryMustExist = 1 << 27,
    /// Allows setting `exclude-newer` on configured package indexes.
    IndexExcludeNewer = 1 << 28,
    /// Allows signing requests to Azure Blob Storage endpoints with Azure credentials.
    AzureEndpoint = 1 << 29,
    /// Rewrites `pyproject.toml` as TOML 1.0 when building source distributions, preserving the
    /// original as `pyproject.toml.orig` to ensure compatibility with older build tools.
    TomlBackwardsCompatibility = 1 << 30,
    /// Allows `uv sync` and other commands to check for malware using [OSV](https://osv.dev) before
    /// installing packages.
    MalwareCheck = 1 << 31,
    /// Prevents `uv venv --clear` from clearing a directory that does not contain a `pyvenv.cfg` file
    /// unless `--force` is provided.
    VenvSafeClear = 1 << 32,
    /// Allows using `uv check`.
    Check = 1 << 33,
    /// Makes `uv init` create a packaged application with a `src/` layout, build system, and script
    /// entry point by default.
    PackagedInit = 1 << 34,
    /// Stores [project virtual environments](./projects/layout.md#centralized-project-environments)
    /// in the uv cache.
    CentralizedProjectEnvs = 1 << 35,
    /// Stores a `uv.lock` alongside each installed tool and reuses it for reproducible installations
    /// and upgrades.
    ToolInstallLocks = 1 << 36,
    /// Allows using `uv workspace list --scripts`.
    WorkspaceListScripts = 1 << 37,
    /// Stops installing the `_virtualenv.py` / `_virtualenv.pth` distutils configuration monkeypatch
    /// in virtual environments for Python 3.10 and later.
    NoDistutilsPatch = 1 << 38,
    /// Allows requiring a hash algorithm for configured package indexes.
    IndexHashAlgorithm = 1 << 39,
    /// Rejects non-canonical lockfile formatting when using `--locked` or `--check`.
    LockfileFormatCheck = 1 << 40,
}

impl PreviewFeature {
    /// Returns the string representation of a single preview feature flag.
    fn as_str(self) -> &'static str {
        match self {
            Self::PythonInstallDefault => "python-install-default",
            Self::JsonOutput => "json-output",
            Self::Pylock => "pylock",
            Self::AddBounds => "add-bounds",
            Self::PackageConflicts => "package-conflicts",
            Self::ExtraBuildDependencies => "extra-build-dependencies",
            Self::DetectModuleConflicts => "detect-module-conflicts",
            Self::Format => "format-command",
            Self::NativeAuth => "native-auth",
            Self::S3Endpoint => "s3-endpoint",
            Self::CacheSize => "cache-size",
            Self::InitProjectFlag => "init-project-flag",
            Self::WorkspaceMetadata => "workspace-metadata",
            Self::WorkspaceDir => "workspace-dir",
            Self::WorkspaceList => "workspace-list",
            Self::SbomExport => "sbom-export",
            Self::AuthHelper => "auth-helper",
            Self::DirectPublish => "direct-publish",
            Self::TargetWorkspaceDiscovery => "target-workspace-discovery",
            Self::MetadataJson => "metadata-json",
            Self::GcsEndpoint => "gcs-endpoint",
            Self::AdjustUlimit => "adjust-ulimit",
            Self::SpecialCondaEnvNames => "special-conda-env-names",
            Self::RelocatableEnvsDefault => "relocatable-envs-default",
            Self::PublishRequireNormalized => "publish-require-normalized",
            Self::Audit => "audit-command",
            Self::ProjectDirectoryMustExist => "project-directory-must-exist",
            Self::IndexExcludeNewer => "index-exclude-newer",
            Self::AzureEndpoint => "azure-endpoint",
            Self::TomlBackwardsCompatibility => "toml-backwards-compatibility",
            Self::MalwareCheck => "malware-check",
            Self::VenvSafeClear => "venv-safe-clear",
            Self::Check => "check-command",
            Self::PackagedInit => "packaged-init",
            Self::CentralizedProjectEnvs => "centralized-project-envs",
            Self::ToolInstallLocks => "tool-install-locks",
            Self::WorkspaceListScripts => "workspace-list-scripts",
            Self::NoDistutilsPatch => "no-distutils-patch",
            Self::IndexHashAlgorithm => "index-hash-algorithm",
            Self::LockfileFormatCheck => "lockfile-format-check",
        }
    }
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
        Ok(match s {
            "python-install-default" => Self::PythonInstallDefault,
            "json-output" => Self::JsonOutput,
            "pylock" => Self::Pylock,
            "add-bounds" => Self::AddBounds,
            "package-conflicts" => Self::PackageConflicts,
            "extra-build-dependencies" => Self::ExtraBuildDependencies,
            "detect-module-conflicts" => Self::DetectModuleConflicts,
            "format" | "format-command" => Self::Format,
            "native-auth" => Self::NativeAuth,
            "s3-endpoint" => Self::S3Endpoint,
            "gcs-endpoint" => Self::GcsEndpoint,
            "cache-size" => Self::CacheSize,
            "init-project-flag" => Self::InitProjectFlag,
            "workspace-metadata" => Self::WorkspaceMetadata,
            "workspace-dir" => Self::WorkspaceDir,
            "workspace-list" => Self::WorkspaceList,
            "sbom-export" => Self::SbomExport,
            "auth-helper" => Self::AuthHelper,
            "direct-publish" => Self::DirectPublish,
            "target-workspace-discovery" => Self::TargetWorkspaceDiscovery,
            "metadata-json" => Self::MetadataJson,
            "adjust-ulimit" => Self::AdjustUlimit,
            "special-conda-env-names" => Self::SpecialCondaEnvNames,
            "relocatable-envs-default" => Self::RelocatableEnvsDefault,
            "publish-require-normalized" => Self::PublishRequireNormalized,
            "audit" | "audit-command" => Self::Audit,
            "project-directory-must-exist" => Self::ProjectDirectoryMustExist,
            "index-exclude-newer" => Self::IndexExcludeNewer,
            "azure-endpoint" => Self::AzureEndpoint,
            "toml-backwards-compatibility" => Self::TomlBackwardsCompatibility,
            "malware-check" => Self::MalwareCheck,
            "venv-safe-clear" => Self::VenvSafeClear,
            "check" | "check-command" => Self::Check,
            "packaged-init" => Self::PackagedInit,
            "centralized-project-envs" => Self::CentralizedProjectEnvs,
            "tool-install-locks" => Self::ToolInstallLocks,
            "workspace-list-scripts" => Self::WorkspaceListScripts,
            "no-distutils-patch" => Self::NoDistutilsPatch,
            "index-hash-algorithm" => Self::IndexHashAlgorithm,
            "lockfile-format-check" => Self::LockfileFormatCheck,
            _ => return Err(PreviewFeatureParseError),
        })
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
    fn test_preview_feature_from_str() {
        let features = PreviewFeature::from_str("python-install-default").unwrap();
        assert_eq!(features, PreviewFeature::PythonInstallDefault);
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
    fn test_preview_feature_as_str() {
        assert_eq!(
            PreviewFeature::PythonInstallDefault.as_str(),
            "python-install-default"
        );
        assert_eq!(PreviewFeature::JsonOutput.as_str(), "json-output");
        assert_eq!(PreviewFeature::Pylock.as_str(), "pylock");
        assert_eq!(
            PreviewFeature::ToolInstallLocks.as_str(),
            "tool-install-locks"
        );
        assert_eq!(PreviewFeature::AddBounds.as_str(), "add-bounds");
        assert_eq!(
            PreviewFeature::PackageConflicts.as_str(),
            "package-conflicts"
        );
        assert_eq!(
            PreviewFeature::ExtraBuildDependencies.as_str(),
            "extra-build-dependencies"
        );
        assert_eq!(
            PreviewFeature::DetectModuleConflicts.as_str(),
            "detect-module-conflicts"
        );
        assert_eq!(PreviewFeature::Format.as_str(), "format-command");
        assert_eq!(PreviewFeature::NativeAuth.as_str(), "native-auth");
        assert_eq!(PreviewFeature::S3Endpoint.as_str(), "s3-endpoint");
        assert_eq!(PreviewFeature::CacheSize.as_str(), "cache-size");
        assert_eq!(
            PreviewFeature::InitProjectFlag.as_str(),
            "init-project-flag"
        );
        assert_eq!(
            PreviewFeature::WorkspaceMetadata.as_str(),
            "workspace-metadata"
        );
        assert_eq!(PreviewFeature::WorkspaceDir.as_str(), "workspace-dir");
        assert_eq!(PreviewFeature::WorkspaceList.as_str(), "workspace-list");
        assert_eq!(PreviewFeature::SbomExport.as_str(), "sbom-export");
        assert_eq!(PreviewFeature::AuthHelper.as_str(), "auth-helper");
        assert_eq!(PreviewFeature::DirectPublish.as_str(), "direct-publish");
        assert_eq!(
            PreviewFeature::TargetWorkspaceDiscovery.as_str(),
            "target-workspace-discovery"
        );
        assert_eq!(PreviewFeature::MetadataJson.as_str(), "metadata-json");
        assert_eq!(PreviewFeature::GcsEndpoint.as_str(), "gcs-endpoint");
        assert_eq!(PreviewFeature::AdjustUlimit.as_str(), "adjust-ulimit");
        assert_eq!(
            PreviewFeature::SpecialCondaEnvNames.as_str(),
            "special-conda-env-names"
        );
        assert_eq!(
            PreviewFeature::RelocatableEnvsDefault.as_str(),
            "relocatable-envs-default"
        );
        assert_eq!(
            PreviewFeature::PublishRequireNormalized.as_str(),
            "publish-require-normalized"
        );
        assert_eq!(
            PreviewFeature::ProjectDirectoryMustExist.as_str(),
            "project-directory-must-exist"
        );
        assert_eq!(
            PreviewFeature::IndexExcludeNewer.as_str(),
            "index-exclude-newer"
        );
        assert_eq!(PreviewFeature::AzureEndpoint.as_str(), "azure-endpoint");
        assert_eq!(
            PreviewFeature::TomlBackwardsCompatibility.as_str(),
            "toml-backwards-compatibility"
        );
        assert_eq!(PreviewFeature::MalwareCheck.as_str(), "malware-check");
        assert_eq!(PreviewFeature::VenvSafeClear.as_str(), "venv-safe-clear");
        assert_eq!(PreviewFeature::Audit.as_str(), "audit-command");
        assert_eq!(PreviewFeature::Check.as_str(), "check-command");
        assert_eq!(
            PreviewFeature::CentralizedProjectEnvs.as_str(),
            "centralized-project-envs"
        );
        assert_eq!(
            PreviewFeature::WorkspaceListScripts.as_str(),
            "workspace-list-scripts"
        );
        assert_eq!(
            PreviewFeature::NoDistutilsPatch.as_str(),
            "no-distutils-patch"
        );
        assert_eq!(
            PreviewFeature::IndexHashAlgorithm.as_str(),
            "index-hash-algorithm"
        );
        assert_eq!(
            PreviewFeature::LockfileFormatCheck.as_str(),
            "lockfile-format-check"
        );
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
