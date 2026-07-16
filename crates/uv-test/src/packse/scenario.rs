//! Typed representation of the vendored Packse scenario TOML files.
//!
//! The nested TOML tables map directly onto [`Scenario::packages`]:
//! `[packages.<name>.versions.<version>]` becomes a [`PackageName`] key, then a [`Version`] key.

use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;
use std::str::FromStr;

use anyhow::{Context, Result};
use serde::Deserialize;

use uv_configuration::TargetTriple;
use uv_distribution_filename::WheelFilename;
use uv_normalize::{ExtraName, PackageName};
use uv_pep440::{Version, VersionSpecifiers};
use uv_pep508::{MarkerTree, Requirement};
use uv_python::PythonVersion;

/// A complete packse scenario definition.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scenario {
    /// The scenario name (e.g., `"fork-basic"`).
    pub name: String,

    /// Human-readable description.
    #[serde(default)]
    pub description: Option<String>,

    /// Packages keyed by the TOML segment in `[packages.<name>]`.
    #[serde(default)]
    pub packages: BTreeMap<PackageName, Package>,

    /// The root (entrypoint) requirements.
    pub root: RootPackage,

    /// What we expect the resolver to produce.
    pub expected: Expected,

    /// Metadata about the Python environment.
    #[serde(default)]
    pub environment: Environment,

    /// Additional resolver options.
    #[serde(default)]
    pub resolver_options: ResolverOptions,
}

impl Scenario {
    /// Parse a single scenario from a TOML file path.
    pub fn from_path(path: &Path) -> Result<Self> {
        let contents = fs_err::read_to_string(path)
            .with_context(|| format!("failed to read scenario file `{}`", path.display()))?;
        toml::from_str(&contents)
            .with_context(|| format!("failed to parse scenario file `{}`", path.display()))
    }

    /// Construct an otherwise-empty scenario for indexes that should only expose vendored files.
    pub fn empty() -> Self {
        Self {
            name: String::new(),
            description: None,
            packages: BTreeMap::new(),
            root: RootPackage {
                requires_python: None,
                requires: Vec::new(),
            },
            expected: Expected {
                satisfiable: true,
                packages: BTreeMap::new(),
                explanation: None,
            },
            environment: Environment::default(),
            resolver_options: ResolverOptions::default(),
        }
    }
}

/// A package with one or more versions.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Package {
    pub versions: BTreeMap<Version, PackageMetadata>,
}

/// Metadata for a single version of a package.
#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct PackageMetadata {
    /// The `Requires-Python` specifier. Defaults to `">=3.12"`.
    #[serde(default = "default_requires_python")]
    pub requires_python: Option<VersionSpecifiers>,

    /// Dependency requirements.
    #[serde(default)]
    pub requires: Vec<Requirement>,

    /// Extra names mapped to their optional dependency requirements.
    #[serde(default)]
    pub extras: BTreeMap<ExtraName, Vec<Requirement>>,

    /// Whether to produce a source distribution.
    #[serde(default = "default_true")]
    pub sdist: bool,

    /// Whether to produce a wheel.
    #[serde(default = "default_true")]
    pub wheel: bool,

    /// Whether this version is yanked.
    #[serde(default)]
    pub yanked: bool,

    /// Specific wheel tags to produce (e.g., `["cp312-abi3-win_amd64"]`).
    /// An empty list means produce only the default `py3-none-any` wheel.
    #[serde(default)]
    pub wheel_tags: Vec<WheelTag>,
}

/// A validated three-component compatibility tag for generated wheels.
#[derive(Clone, Debug)]
pub struct WheelTag(String);

impl WheelTag {
    /// Return the compatibility tag as it should appear in a wheel filename.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for WheelTag {
    type Err = String;

    fn from_str(tag: &str) -> Result<Self, Self::Err> {
        if tag.split('-').count() != 3 {
            return Err(format!(
                "wheel tag `{tag}` must have exactly three components"
            ));
        }
        WheelFilename::from_str(&format!("package-0-{tag}.whl"))
            .map_err(|error| format!("wheel tag `{tag}` is invalid: {error}"))?;
        Ok(Self(tag.to_string()))
    }
}

impl<'de> Deserialize<'de> for WheelTag {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let tag = String::deserialize(deserializer)?;
        Self::from_str(&tag).map_err(serde::de::Error::custom)
    }
}

/// The root/entrypoint package.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RootPackage {
    /// `Requires-Python` for the root.
    #[serde(default = "default_requires_python")]
    pub requires_python: Option<VersionSpecifiers>,

    /// Top-level requirements.
    #[serde(default)]
    pub requires: Vec<Requirement>,
}

/// Expected resolution outcome.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Expected {
    /// Whether the scenario is satisfiable.
    pub satisfiable: bool,

    /// Expected installed package names mapped to resolved versions.
    #[serde(default)]
    pub packages: BTreeMap<PackageName, Version>,

    /// Optional explanation.
    #[serde(default)]
    pub explanation: Option<String>,
}

/// Python environment metadata.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Environment {
    /// Active Python version.
    #[serde(default = "default_python")]
    pub python: PythonVersion,

    /// Additional Python versions available on the system.
    #[serde(default)]
    pub additional_python: Vec<PythonVersion>,
}

impl Default for Environment {
    fn default() -> Self {
        Self {
            python: default_python(),
            additional_python: Vec::new(),
        }
    }
}

/// Additional resolver options.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolverOptions {
    /// Select the generated test command for this scenario.
    #[serde(default)]
    pub test: Option<ScenarioTest>,

    /// Version selection strategy.
    #[serde(default)]
    pub resolution: Option<Resolution>,

    /// Python version override for resolution.
    #[serde(default)]
    pub python: Option<PythonVersion>,

    /// Enable pre-release selection.
    #[serde(default)]
    pub prereleases: bool,

    /// Packages that must use pre-built wheels (no building from source).
    #[serde(default)]
    pub no_build: Vec<PackageName>,

    /// Packages that must NOT use pre-built wheels (must build from source).
    #[serde(default)]
    pub no_binary: Vec<PackageName>,

    /// Universal (multi-platform) resolution mode.
    #[serde(default)]
    pub universal: bool,

    /// Python platform to resolve for.
    #[serde(default)]
    pub python_platform: Option<TargetTriple>,

    /// Required environments (platform markers).
    #[serde(default)]
    pub required_environments: Vec<MarkerTree>,
}

/// The command template used to generate a scenario test.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum ScenarioTest {
    /// Do not generate a test for this scenario.
    None,
    Install,
    Compile,
    Lock,
}

/// The version selection strategy used by the resolver.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum Resolution {
    Highest,
    Lowest,
    LowestDirect,
}

impl fmt::Display for Resolution {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Highest => formatter.write_str("highest"),
            Self::Lowest => formatter.write_str("lowest"),
            Self::LowestDirect => formatter.write_str("lowest-direct"),
        }
    }
}

#[expect(clippy::unnecessary_wraps)] // Must return `Option` for serde `default`
fn default_requires_python() -> Option<VersionSpecifiers> {
    Some(VersionSpecifiers::from_str(">=3.12").expect("default requires-python should be valid"))
}

fn default_true() -> bool {
    true
}

fn default_python() -> PythonVersion {
    PythonVersion::from_str("3.12").expect("default Python version should be valid")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_basic_scenario() {
        let toml = r#"
name = "fork-basic"
description = "An extremely basic test."

[resolver_options]
universal = true

[expected]
satisfiable = true

[root]
requires = ["a>=2 ; sys_platform == 'linux'", "a<2 ; sys_platform == 'darwin'"]

[packages.a.versions."1.0.0"]
[packages.a.versions."2.0.0"]
"#;
        let scenario: Scenario = toml::from_str(toml).expect("scenario should parse");
        let package_name = PackageName::from_str("a").expect("valid package name");
        assert_eq!(scenario.name, "fork-basic");
        assert!(scenario.resolver_options.universal);
        assert_eq!(scenario.packages.len(), 1);
        assert_eq!(scenario.packages[&package_name].versions.len(), 2);
    }

    #[test]
    fn parse_extras_scenario() {
        let toml = r#"
name = "all-extras-required"
description = "Multiple optional dependencies."

[root]
requires = ["a[all]"]

[expected]
satisfiable = true

[expected.packages]
a = "1.0.0"
b = "1.0.0"
c = "1.0.0"

[packages.b.versions."1.0.0"]
[packages.c.versions."1.0.0"]

[packages.a.versions."1.0.0".extras]
all = ["a[extra_b]", "a[extra_c]"]
extra_b = ["b"]
extra_c = ["c"]
"#;
        let scenario: Scenario = toml::from_str(toml).expect("scenario should parse");
        let package_name = PackageName::from_str("a").expect("valid package name");
        let version = Version::from_str("1.0.0").expect("valid version");
        let extra_name = ExtraName::from_str("extra_b").expect("valid extra name");
        assert_eq!(scenario.name, "all-extras-required");
        let a_meta = &scenario.packages[&package_name].versions[&version];
        assert_eq!(a_meta.extras.len(), 3);
        assert_eq!(
            a_meta.extras[&extra_name],
            vec![Requirement::from_str("b").expect("valid requirement")]
        );
    }

    #[test]
    fn parse_test_and_resolution() {
        let toml = r#"
name = "lowest-direct"

[root]
requires = ["a"]

[expected]
satisfiable = true

[resolver_options]
test = "compile"
resolution = "lowest-direct"
"#;
        let scenario: Scenario = toml::from_str(toml).expect("scenario should parse");
        assert_eq!(scenario.resolver_options.test, Some(ScenarioTest::Compile));
        assert_eq!(
            scenario.resolver_options.resolution,
            Some(Resolution::LowestDirect)
        );
    }

    #[test]
    fn reject_invalid_requires_python() {
        let toml = r#"
name = "invalid-requires-python"

[root]
requires = []

[expected]
satisfiable = true

[packages.a.versions."1.0.0"]
requires_python = "not a specifier"
"#;

        assert!(toml::from_str::<Scenario>(toml).is_err());
    }

    #[test]
    fn reject_unknown_metadata_field() {
        let toml = r#"
name = "unknown-metadata-field"

[root]
requires = ["a"]

[expected]
satisfiable = true

[packages.a.versions."1.0.0"]
wheels = false
"#;

        assert!(toml::from_str::<Scenario>(toml).is_err());
    }

    #[test]
    fn reject_invalid_wheel_tag() {
        let toml = r#"
name = "invalid-wheel-tag"

[root]
requires = ["a"]

[expected]
satisfiable = true

[packages.a.versions."1.0.0"]
wheel_tags = ["1-py3-none-any"]
"#;

        assert!(toml::from_str::<Scenario>(toml).is_err());
    }

    #[test]
    fn path_is_included_in_parse_errors() {
        let temporary_directory =
            tempfile::tempdir().expect("temporary directory should be created");
        let path = temporary_directory.path().join("invalid.toml");
        fs_err::write(&path, "not valid TOML = [").expect("invalid scenario should be written");

        let error = Scenario::from_path(&path).expect_err("scenario should fail to parse");
        insta::assert_snapshot!(
            error
                .to_string()
                .replace(temporary_directory.path().to_string_lossy().as_ref(), "[TEMP_DIR]")
                .replace('\\', "/"),
            @"failed to parse scenario file `[TEMP_DIR]/invalid.toml`"
        );
    }
}
