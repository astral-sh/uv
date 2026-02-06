#[cfg(feature = "schemars")]
use std::borrow::Cow;
use std::{
    fmt::{Debug, Display, Formatter},
    ops::BitOr,
    str::FromStr,
};

use enumflags2::{BitFlags, bitflags};
use thiserror::Error;
use uv_warnings::warn_user_once;

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
            _ => return Err(PreviewFeatureParseError),
        })
    }
}

#[cfg(feature = "schemars")]
impl schemars::JsonSchema for PreviewFeature {
    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("PreviewFeature")
    }

    fn json_schema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        let choices: Vec<&str> = BitFlags::<Self>::all().iter().map(Self::as_str).collect();
        schemars::json_schema!({
            "type": "string",
            "enum": choices,
        })
    }
}

impl<'de> serde::Deserialize<'de> for PreviewFeature {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct Visitor;

        impl serde::de::Visitor<'_> for Visitor {
            type Value = PreviewFeature;

            fn expecting(&self, f: &mut Formatter) -> std::fmt::Result {
                f.write_str("a string")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                PreviewFeature::from_str(v).map_err(serde::de::Error::custom)
            }
        }

        deserializer.deserialize_str(Visitor)
    }
}

impl serde::Serialize for PreviewFeature {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
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
    pub fn all() -> Self {
        Self {
            flags: BitFlags::all(),
        }
    }

    /// Check if a single feature is enabled
    pub fn is_enabled(&self, flag: PreviewFeature) -> bool {
        self.flags.contains(flag)
    }
}

impl<'flag> FromIterator<&'flag PreviewFeature> for Preview {
    fn from_iter<T: IntoIterator<Item = &'flag PreviewFeature>>(iter: T) -> Self {
        let flags = iter
            .into_iter()
            .copied()
            .fold(BitFlags::empty(), BitOr::bitor);
        Self { flags }
    }
}

impl From<bool> for Preview {
    fn from(value: bool) -> Self {
        if value { Self::all() } else { Self::default() }
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
        let preview = Preview::from_iter(&[]);
        assert_eq!(preview.to_string(), "disabled");

        // Test enabled (all features)
        let preview = Preview::all();
        assert_eq!(preview.to_string(), "enabled");

        // Test single feature
        let preview = Preview::from_iter(&[PreviewFeature::PythonInstallDefault]);
        assert_eq!(preview.to_string(), "python-install-default");

        // Test multiple features
        let preview = Preview::from_iter(&[PreviewFeature::PythonUpgrade, PreviewFeature::Pylock]);
        assert_eq!(preview.to_string(), "python-upgrade,pylock");
    }

    #[test]
    fn test_preview_from_args() {
        // Test no_preview
        let preview = Preview::default();
        assert_eq!(preview.to_string(), "disabled");

        // Test preview (all features)
        let preview = Preview::all();
        assert_eq!(preview.to_string(), "enabled");

        // Test specific features
        let preview: Preview = [PreviewFeature::PythonUpgrade, PreviewFeature::JsonOutput]
            .iter()
            .collect();
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
    }

    #[test]
    fn test_serde_roundtrip() {
        let input = r#"["python-upgrade", "format"]"#;

        let deserialized: Vec<PreviewFeature> = serde_json::from_str(input).unwrap();
        assert_eq!(deserialized.len(), 2);
        assert_eq!(deserialized[0], PreviewFeature::PythonUpgrade);
        assert_eq!(deserialized[1], PreviewFeature::Format);

        let serialized = serde_json::to_string(&deserialized).unwrap();
        insta::assert_snapshot!(serialized, @r#"["python-upgrade","format"]"#);

        let roundtrip: Vec<PreviewFeature> = serde_json::from_str(&serialized).unwrap();
        assert_eq!(roundtrip, deserialized);
    }
}
