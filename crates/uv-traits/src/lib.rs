//! Avoid cyclic crate dependencies between resolver, installer and builder.

use std::collections::btree_map::Entry;
use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::Result;

use distribution_types::{CachedDist, DistributionId, IndexLocations, Resolution, SourceDist};
use once_map::OnceMap;
use pep508_rs::Requirement;
use uv_cache::Cache;
use uv_interpreter::{Interpreter, PythonEnvironment};
use uv_normalize::PackageName;

/// Avoid cyclic crate dependencies between resolver, installer and builder.
///
/// To resolve the dependencies of a packages, we may need to build one or more source
/// distributions. To building a source distribution, we need to create a virtual environment from
/// the same base python as we use for the root resolution, resolve the build requirements
/// (potentially which nested source distributions, recursing a level deeper), installing
/// them and then build. The installer, the resolver and the source distribution builder are each in
/// their own crate. To avoid circular crate dependencies, this type dispatches between the three
/// crates with its three main methods ([`BuildContext::resolve`], [`BuildContext::install`] and
/// [`BuildContext::setup_build`]).
///
/// The overall main crate structure looks like this:
///
/// ```text
///                    ┌────────────────┐
///                    │       uv       │
///                    └───────▲────────┘
///                            │
///                            │
///                    ┌───────┴────────┐
///         ┌─────────►│  uv-dispatch   │◄─────────┐
///         │          └───────▲────────┘          │
///         │                  │                   │
///         │                  │                   │
/// ┌───────┴────────┐ ┌───────┴────────┐ ┌────────┴───────┐
/// │  uv-resolver   │ │  uv-installer  │ │    uv-build    │
/// └───────▲────────┘ └───────▲────────┘ └────────▲───────┘
///         │                  │                   │
///         └─────────────┐    │    ┌──────────────┘
///                    ┌──┴────┴────┴───┐
///                    │    uv-traits   │
///                    └────────────────┘
/// ```
///
/// Put in a different way, this trait allows `uv-resolver` to depend on `uv-build` and
/// `uv-build` to depend on `uv-resolver` which having actual crate dependencies between
/// them.

pub trait BuildContext: Sync {
    type SourceDistBuilder: SourceBuildTrait + Send + Sync;

    /// Return a reference to the cache.
    fn cache(&self) -> &Cache;

    /// All (potentially nested) source distribution builds use the same base python and can reuse
    /// it's metadata (e.g. wheel compatibility tags).
    fn interpreter(&self) -> &Interpreter;

    /// Whether to enforce build isolation when building source distributions.
    fn build_isolation(&self) -> BuildIsolation;

    /// Whether source distribution building is disabled. This [`BuildContext::setup_build`] calls
    /// will fail in this case. This method exists to avoid fetching source distributions if we know
    /// we can't build them
    fn no_build(&self) -> &NoBuild;

    /// Whether using pre-built wheels is disabled.
    fn no_binary(&self) -> &NoBinary;

    /// The index locations being searched.
    fn index_locations(&self) -> &IndexLocations;

    /// The strategy to use when building source distributions that lack a `pyproject.toml`.
    fn setup_py_strategy(&self) -> SetupPyStrategy;

    /// Resolve the given requirements into a ready-to-install set of package versions.
    fn resolve<'a>(
        &'a self,
        requirements: &'a [Requirement],
    ) -> impl Future<Output = Result<Resolution>> + Send + 'a;

    /// Install the given set of package versions into the virtual environment. The environment must
    /// use the same base Python as [`BuildContext::interpreter`]
    fn install<'a>(
        &'a self,
        resolution: &'a Resolution,
        venv: &'a PythonEnvironment,
    ) -> impl Future<Output = Result<()>> + Send + 'a;

    /// Setup a source distribution build by installing the required dependencies. A wrapper for
    /// `uv_build::SourceBuild::setup`.
    ///
    /// For PEP 517 builds, this calls `get_requires_for_build_wheel`.
    ///
    /// `package_id` is for error reporting only.
    /// `dist` is for safety checks and may be null for editable builds.
    fn setup_build<'a>(
        &'a self,
        source: &'a Path,
        subdirectory: Option<&'a Path>,
        package_id: &'a str,
        dist: Option<&'a SourceDist>,
        build_kind: BuildKind,
    ) -> impl Future<Output = Result<Self::SourceDistBuilder>> + Send + 'a;
}

/// A wrapper for `uv_build::SourceBuild` to avoid cyclical crate dependencies.
///
/// You can either call only `wheel()` to build the wheel directly, call only `metadata()` to get
/// the metadata without performing the actual or first call `metadata()` and then `wheel()`.
pub trait SourceBuildTrait {
    /// A wrapper for `uv_build::SourceBuild::get_metadata_without_build`.
    ///
    /// For PEP 517 builds, this calls `prepare_metadata_for_build_wheel`
    ///
    /// Returns the metadata directory if we're having a PEP 517 build and the
    /// `prepare_metadata_for_build_wheel` hook exists
    fn metadata(&mut self) -> impl Future<Output = Result<Option<PathBuf>>> + Send;

    /// A wrapper for `uv_build::SourceBuild::build`.
    ///
    /// For PEP 517 builds, this calls `build_wheel`.
    ///
    /// Returns the filename of the built wheel inside the given `wheel_dir`.
    fn wheel<'a>(&'a self, wheel_dir: &'a Path)
        -> impl Future<Output = Result<String>> + Send + 'a;
}

#[derive(Default)]
pub struct InFlight {
    /// The in-flight distribution downloads.
    pub downloads: OnceMap<DistributionId, Result<CachedDist, String>>,
}

/// Whether to enforce build isolation when building source distributions.
#[derive(Debug, Copy, Clone)]
pub enum BuildIsolation<'a> {
    Isolated,
    Shared(&'a PythonEnvironment),
}

impl<'a> BuildIsolation<'a> {
    /// Returns `true` if build isolation is enforced.
    pub fn is_isolated(&self) -> bool {
        matches!(self, Self::Isolated)
    }
}

/// The strategy to use when building source distributions that lack a `pyproject.toml`.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum SetupPyStrategy {
    /// Perform a PEP 517 build.
    #[default]
    Pep517,
    /// Perform a build by invoking `setuptools` directly.
    Setuptools,
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum BuildKind {
    /// A regular PEP 517 wheel build
    #[default]
    Wheel,
    /// A PEP 660 editable installation wheel build
    Editable,
}

impl Display for BuildKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Wheel => f.write_str("wheel"),
            Self::Editable => f.write_str("editable"),
        }
    }
}

#[derive(Debug, Clone)]
pub enum PackageNameSpecifier {
    All,
    None,
    Package(PackageName),
}

impl FromStr for PackageNameSpecifier {
    type Err = uv_normalize::InvalidNameError;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        match name {
            ":all:" => Ok(Self::All),
            ":none:" => Ok(Self::None),
            _ => Ok(Self::Package(PackageName::from_str(name)?)),
        }
    }
}

#[derive(Debug, Clone)]
pub enum PackageNameSpecifiers {
    All,
    None,
    Packages(Vec<PackageName>),
}

impl PackageNameSpecifiers {
    fn from_iter(specifiers: impl Iterator<Item = PackageNameSpecifier>) -> Self {
        let mut packages = Vec::new();
        let mut all: bool = false;

        for specifier in specifiers {
            match specifier {
                PackageNameSpecifier::None => {
                    packages.clear();
                    all = false;
                }
                PackageNameSpecifier::All => {
                    all = true;
                }
                PackageNameSpecifier::Package(name) => {
                    packages.push(name);
                }
            }
        }

        if all {
            Self::All
        } else if packages.is_empty() {
            Self::None
        } else {
            Self::Packages(packages)
        }
    }
}

#[derive(Debug, Clone)]
pub enum NoBinary {
    /// Allow installation of any wheel.
    None,

    /// Do not allow installation from any wheels.
    All,

    /// Do not allow installation from the specific wheels.
    Packages(Vec<PackageName>),
}

impl NoBinary {
    /// Determine the binary installation strategy to use.
    pub fn from_args(no_binary: Vec<PackageNameSpecifier>) -> Self {
        let combined = PackageNameSpecifiers::from_iter(no_binary.into_iter());
        match combined {
            PackageNameSpecifiers::All => Self::All,
            PackageNameSpecifiers::None => Self::None,
            PackageNameSpecifiers::Packages(packages) => Self::Packages(packages),
        }
    }
}

impl NoBinary {
    /// Returns `true` if all wheels are allowed.
    pub fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum NoBuild {
    /// Allow building wheels from any source distribution.
    None,

    /// Do not allow building wheels from any source distribution.
    All,

    /// Do not allow building wheels from the given package's source distributions.
    Packages(Vec<PackageName>),
}

impl NoBuild {
    /// Determine the build strategy to use.
    pub fn from_args(only_binary: Vec<PackageNameSpecifier>, no_build: bool) -> Self {
        if no_build {
            Self::All
        } else {
            let combined = PackageNameSpecifiers::from_iter(only_binary.into_iter());
            match combined {
                PackageNameSpecifiers::All => Self::All,
                PackageNameSpecifiers::None => Self::None,
                PackageNameSpecifiers::Packages(packages) => Self::Packages(packages),
            }
        }
    }
}

impl NoBuild {
    /// Returns `true` if all builds are allowed.
    pub fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }
}

#[derive(Debug, Clone)]
pub struct ConfigSettingEntry {
    /// The key of the setting. For example, given `key=value`, this would be `key`.
    key: String,
    /// The value of the setting. For example, given `key=value`, this would be `value`.
    value: String,
}

impl FromStr for ConfigSettingEntry {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let Some((key, value)) = s.split_once('=') else {
            return Err(anyhow::anyhow!(
                "Invalid config setting: {s} (expected `KEY=VALUE`)"
            ));
        };
        Ok(Self {
            key: key.trim().to_string(),
            value: value.trim().to_string(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ConfigSettingValue {
    /// The value consists of a single string.
    String(String),
    /// The value consists of a list of strings.
    List(Vec<String>),
}

/// Settings to pass to a PEP 517 build backend, structured as a map from (string) key to string or
/// list of strings.
///
/// See: <https://peps.python.org/pep-0517/#config-settings>
#[derive(Debug, Default, Clone)]
pub struct ConfigSettings(BTreeMap<String, ConfigSettingValue>);

impl FromIterator<ConfigSettingEntry> for ConfigSettings {
    fn from_iter<T: IntoIterator<Item = ConfigSettingEntry>>(iter: T) -> Self {
        let mut config = BTreeMap::default();
        for entry in iter {
            match config.entry(entry.key) {
                Entry::Vacant(vacant) => {
                    vacant.insert(ConfigSettingValue::String(entry.value));
                }
                Entry::Occupied(mut occupied) => match occupied.get_mut() {
                    ConfigSettingValue::String(existing) => {
                        let existing = existing.clone();
                        occupied.insert(ConfigSettingValue::List(vec![existing, entry.value]));
                    }
                    ConfigSettingValue::List(existing) => {
                        existing.push(entry.value);
                    }
                },
            }
        }
        Self(config)
    }
}

#[cfg(feature = "serde")]
impl ConfigSettings {
    /// Convert the settings to a string that can be passed directly to a PEP 517 build backend.
    pub fn escape_for_python(&self) -> String {
        serde_json::to_string(self).expect("Failed to serialize config settings")
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for ConfigSettings {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;

        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for (key, value) in &self.0 {
            match value {
                ConfigSettingValue::String(value) => {
                    map.serialize_entry(&key, &value)?;
                }
                ConfigSettingValue::List(values) => {
                    map.serialize_entry(&key, &values)?;
                }
            }
        }
        map.end()
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Error;

    use super::*;

    #[test]
    fn no_build_from_args() -> Result<(), Error> {
        assert_eq!(
            NoBuild::from_args(vec![PackageNameSpecifier::from_str(":all:")?], false),
            NoBuild::All,
        );
        assert_eq!(
            NoBuild::from_args(vec![PackageNameSpecifier::from_str(":all:")?], true),
            NoBuild::All,
        );
        assert_eq!(
            NoBuild::from_args(vec![PackageNameSpecifier::from_str(":none:")?], true),
            NoBuild::All,
        );
        assert_eq!(
            NoBuild::from_args(vec![PackageNameSpecifier::from_str(":none:")?], false),
            NoBuild::None,
        );
        assert_eq!(
            NoBuild::from_args(
                vec![
                    PackageNameSpecifier::from_str("foo")?,
                    PackageNameSpecifier::from_str("bar")?
                ],
                false
            ),
            NoBuild::Packages(vec![
                PackageName::from_str("foo")?,
                PackageName::from_str("bar")?
            ]),
        );
        assert_eq!(
            NoBuild::from_args(
                vec![
                    PackageNameSpecifier::from_str("test")?,
                    PackageNameSpecifier::All
                ],
                false
            ),
            NoBuild::All,
        );
        assert_eq!(
            NoBuild::from_args(
                vec![
                    PackageNameSpecifier::from_str("foo")?,
                    PackageNameSpecifier::from_str(":none:")?,
                    PackageNameSpecifier::from_str("bar")?
                ],
                false
            ),
            NoBuild::Packages(vec![PackageName::from_str("bar")?]),
        );

        Ok(())
    }

    #[test]
    fn collect_config_settings() {
        let settings: ConfigSettings = vec![
            ConfigSettingEntry {
                key: "key".to_string(),
                value: "value".to_string(),
            },
            ConfigSettingEntry {
                key: "key".to_string(),
                value: "value2".to_string(),
            },
            ConfigSettingEntry {
                key: "list".to_string(),
                value: "value3".to_string(),
            },
            ConfigSettingEntry {
                key: "list".to_string(),
                value: "value4".to_string(),
            },
        ]
        .into_iter()
        .collect();
        assert_eq!(
            settings.0.get("key"),
            Some(&ConfigSettingValue::List(vec![
                "value".to_string(),
                "value2".to_string()
            ]))
        );
        assert_eq!(
            settings.0.get("list"),
            Some(&ConfigSettingValue::List(vec![
                "value3".to_string(),
                "value4".to_string()
            ]))
        );
    }

    #[test]
    #[cfg(feature = "serde")]
    fn escape_for_python() {
        let mut settings = ConfigSettings::default();
        settings.0.insert(
            "key".to_string(),
            ConfigSettingValue::String("value".to_string()),
        );
        settings.0.insert(
            "list".to_string(),
            ConfigSettingValue::List(vec!["value1".to_string(), "value2".to_string()]),
        );
        assert_eq!(
            settings.escape_for_python(),
            r#"{"key":"value","list":["value1","value2"]}"#
        );

        let mut settings = ConfigSettings::default();
        settings.0.insert(
            "key".to_string(),
            ConfigSettingValue::String("Hello, \"world!\"".to_string()),
        );
        settings.0.insert(
            "list".to_string(),
            ConfigSettingValue::List(vec!["'value1'".to_string()]),
        );
        assert_eq!(
            settings.escape_for_python(),
            r#"{"key":"Hello, \"world!\"","list":["'value1'"]}"#
        );

        let mut settings = ConfigSettings::default();
        settings.0.insert(
            "key".to_string(),
            ConfigSettingValue::String("val\\1 {}ue".to_string()),
        );
        assert_eq!(settings.escape_for_python(), r#"{"key":"val\\1 {}ue"}"#);
    }
}
