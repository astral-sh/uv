use std::sync::OnceLock;
use std::{
    fmt::{Debug, Display, Formatter},
    ops::BitOr,
    str::FromStr,
};

use enumflags2::{BitFlags, bitflags};
use thiserror::Error;
use uv_warnings::warn_user_once;

/// Indicates how the preview was initialised, to distinguish between normal
/// code and unit tests.
#[cfg(feature = "testing")]
enum PreviewMode {
    /// Initialised by a call to `uv_preview::init`
    Normal(Preview),
    /// Initialised by a call to `uv_preview::test::with_features`
    Test(std::sync::RwLock<Option<Preview>>),
}

#[cfg(feature = "testing")]
static PREVIEW: OnceLock<PreviewMode> = OnceLock::new();

#[cfg(not(feature = "testing"))]
static PREVIEW: OnceLock<Preview> = OnceLock::new();

/// Initialize the global preview configuration.
///
/// This should be called once at startup with the resolved preview settings.
#[cfg(feature = "testing")]
#[expect(clippy::result_unit_err)]
pub fn init(preview: Preview) -> Result<(), ()> {
    PREVIEW.set(PreviewMode::Normal(preview)).map_err(|_| ())
}

/// Initialize the global preview configuration.
///
/// This should be called once at startup with the resolved preview settings.
#[cfg(not(feature = "testing"))]
#[expect(clippy::result_unit_err)]
pub fn init(preview: Preview) -> Result<(), ()> {
    PREVIEW.set(preview).map_err(|_| ())
}

/// Get the current global preview configuration.
///
/// # Panics
///
/// When called before [`init`] or when the current thread does not hold a
/// [`test::with_features`] guard.
#[cfg(feature = "testing")]
pub fn get() -> Preview {
    match PREVIEW.get() {
        Some(PreviewMode::Normal(preview)) => *preview,
        Some(PreviewMode::Test(rwlock)) => {
            let panic_message = "The preview configuration is in test mode but the current thread does not hold a `FeaturesGuard`\nHint: Use `uv_preview::test::with_features` to get a `FeaturesGuard` and hold it when testing functions which rely on the global preview state";
            assert!(test::HELD.get(), "{}", panic_message);
            // The unwrap may panic only if the current thread had panicked
            // while attempting to write the value and then recovered with
            // `catch_unwind`. This seems unlikely.
            rwlock.read().unwrap().expect(panic_message)
        }
        None => panic!(
            "The preview configuration has not been initialized\nHint: Use `uv_preview::init` or `uv_preview::test::with_features` to initialize it"
        ),
    }
}

/// Get the current global preview configuration.
///
/// # Panics
///
/// When called before [`init`].
#[cfg(not(feature = "testing"))]
pub fn get() -> Preview {
    match PREVIEW.get() {
        Some(preview) => *preview,
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
            "Additional calls to `uv_preview::test::with_features` are not allowed while holding a `FeaturesGuard`"
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
                    "Cannot use `uv_preview::test::with_features` after `uv_preview::init` has been called"
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
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewFeature {
    PythonInstallDefault = 1 << 0,
    PythonUpgrade = 1 << 1,
    JsonOutput = 1 << 2,
    Pylock = 1 << 3,
    AddBounds = 1 << 4,
    PackageConflicts = 1 << 5,
    ExtraBuildDependencies = 1 << 6,
    DetectModuleConflicts = 1 << 7,
    Format = 1 << 8,
    NativeAuth = 1 << 9,
    S3Endpoint = 1 << 10,
    CacheSize = 1 << 11,
    InitProjectFlag = 1 << 12,
    WorkspaceMetadata = 1 << 13,
    WorkspaceDir = 1 << 14,
    WorkspaceList = 1 << 15,
    SbomExport = 1 << 16,
    AuthHelper = 1 << 17,
    DirectPublish = 1 << 18,
    TargetWorkspaceDiscovery = 1 << 19,
    MetadataJson = 1 << 20,
    GcsEndpoint = 1 << 21,
    AdjustUlimit = 1 << 22,
    SpecialCondaEnvNames = 1 << 23,
    RelocatableEnvsDefault = 1 << 24,
    PublishRequireNormalized = 1 << 25,
}

impl PreviewFeature {
    /// Returns the string representation of a single preview feature flag.
    fn as_str(self) -> &'static str {
        match self {
            Self::PythonInstallDefault => "python-install-default",
            Self::PythonUpgrade => "python-upgrade",
            Self::JsonOutput => "json-output",
            Self::Pylock => "pylock",
            Self::AddBounds => "add-bounds",
            Self::PackageConflicts => "package-conflicts",
            Self::ExtraBuildDependencies => "extra-build-dependencies",
            Self::DetectModuleConflicts => "detect-module-conflicts",
            Self::Format => "format",
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
            "python-upgrade" => Self::PythonUpgrade,
            "json-output" => Self::JsonOutput,
            "pylock" => Self::Pylock,
            "add-bounds" => Self::AddBounds,
            "package-conflicts" => Self::PackageConflicts,
            "extra-build-dependencies" => Self::ExtraBuildDependencies,
            "detect-module-conflicts" => Self::DetectModuleConflicts,
            "format" => Self::Format,
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
            _ => return Err(PreviewFeatureParseError),
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
    pub fn new(flags: &[PreviewFeature]) -> Self {
        Self {
            flags: flags.iter().copied().fold(BitFlags::empty(), BitOr::bitor),
        }
    }

    pub fn all() -> Self {
        Self {
            flags: BitFlags::all(),
        }
    }

    pub fn from_args(preview: bool, no_preview: bool, preview_features: &[PreviewFeature]) -> Self {
        if no_preview {
            return Self::default();
        }

        if preview {
            return Self::all();
        }

        Self::new(preview_features)
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

#[derive(Debug, Error, Clone)]
pub enum PreviewParseError {
    #[error("Empty string in preview features: {0}")]
    Empty(String),
}

impl FromStr for Preview {
    type Err = PreviewParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut flags = BitFlags::empty();

        for part in s.split(',') {
            let part = part.trim();
            if part.is_empty() {
                return Err(PreviewParseError::Empty(
                    "Empty string in preview features".to_string(),
                ));
            }

            match PreviewFeature::from_str(part) {
                Ok(flag) => flags |= flag,
                Err(_) => {
                    warn_user_once!("Unknown preview feature: `{part}`");
                }
            }
        }

        Ok(Self { flags })
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
        let preview = Preview::from_str("python-upgrade,json-output").unwrap();
        assert!(preview.is_enabled(PreviewFeature::PythonUpgrade));
        assert!(preview.is_enabled(PreviewFeature::JsonOutput));
        assert_eq!(preview.flags.bits().count_ones(), 2);

        // Test with whitespace
        let preview = Preview::from_str("pylock , add-bounds").unwrap();
        assert!(preview.is_enabled(PreviewFeature::Pylock));
        assert!(preview.is_enabled(PreviewFeature::AddBounds));

        // Test empty string error
        assert!(Preview::from_str("").is_err());
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
        let preview = Preview::new(&[PreviewFeature::PythonUpgrade, PreviewFeature::Pylock]);
        assert_eq!(preview.to_string(), "python-upgrade,pylock");
    }

    #[test]
    fn test_preview_from_args() {
        // Test no preview and no no_preview, and no features
        let preview = Preview::from_args(false, false, &[]);
        assert_eq!(preview.to_string(), "disabled");

        // Test no_preview
        let preview = Preview::from_args(true, true, &[]);
        assert_eq!(preview.to_string(), "disabled");

        // Test preview (all features)
        let preview = Preview::from_args(true, false, &[]);
        assert_eq!(preview.to_string(), "enabled");

        // Test specific features
        let features = vec![PreviewFeature::PythonUpgrade, PreviewFeature::JsonOutput];
        let preview = Preview::from_args(false, false, &features);
        assert!(preview.is_enabled(PreviewFeature::PythonUpgrade));
        assert!(preview.is_enabled(PreviewFeature::JsonOutput));
        assert!(!preview.is_enabled(PreviewFeature::Pylock));
    }

    #[test]
    fn test_preview_feature_as_str() {
        assert_eq!(
            PreviewFeature::PythonInstallDefault.as_str(),
            "python-install-default"
        );
        assert_eq!(PreviewFeature::PythonUpgrade.as_str(), "python-upgrade");
        assert_eq!(PreviewFeature::JsonOutput.as_str(), "json-output");
        assert_eq!(PreviewFeature::Pylock.as_str(), "pylock");
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
        assert_eq!(PreviewFeature::Format.as_str(), "format");
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
