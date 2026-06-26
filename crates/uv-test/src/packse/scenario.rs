//! Typed representation of the vendored Packse scenario TOML files.
//!
//! The nested TOML tables map directly onto [`Scenario::packages`]:
//! `[packages.<name>.versions.<version>]` becomes a [`PackageName`] key, then a [`Version`] key.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;
use std::str::FromStr;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use uv_configuration::TargetTriple;
use uv_distribution_filename::WheelFilename;
use uv_normalize::{ExtraName, PackageName};
use uv_pep440::{Version, VersionSpecifiers};
use uv_pep508::{MarkerTree, Requirement};
use uv_python::PythonVersion;

use super::scenarios_dir;

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
    /// Parse a vendored scenario by its path relative to `test/scenarios`.
    pub fn load(path: &str) -> Result<Self> {
        Self::from_path(&scenarios_dir().join(path))
    }

    /// Parse a single scenario from a TOML file path.
    pub fn from_path(path: &Path) -> Result<Self> {
        let contents = fs_err::read_to_string(path)
            .with_context(|| format!("failed to read scenario file `{}`", path.display()))?;
        let scenario: Self = toml::from_str(&contents)
            .with_context(|| format!("failed to parse scenario file `{}`", path.display()))?;
        scenario
            .validate()
            .with_context(|| format!("invalid scenario file `{}`", path.display()))?;
        Ok(scenario)
    }

    fn validate(&self) -> Result<()> {
        if self.root.workspace.is_some() && !self.resolver_options.universal {
            bail!("root workspaces are only supported for universal lock scenarios");
        }
        Ok(())
    }

    /// Render this scenario's root workspace as a `pyproject.toml` document.
    fn pyproject_toml(&self) -> Result<String> {
        let workspace = self
            .root
            .workspace
            .as_ref()
            .context("scenario root does not define a workspace")?;
        let required_environments = self
            .resolver_options
            .required_environments
            .iter()
            .map(|environment| {
                environment
                    .contents()
                    .context("required environment markers should not be empty")
                    .map(|contents| contents.to_string())
            })
            .collect::<Result<Vec<_>>>()?;
        let tool = (!required_environments.is_empty()).then_some(PyProjectTool {
            uv: PyProjectUv {
                required_environments,
            },
        });
        let pyproject = PyProjectToml {
            project: PyProjectProject {
                name: workspace.project.name.to_string(),
                version: workspace.project.version.to_string(),
                requires_python: self.root.requires_python.as_ref().map(ToString::to_string),
                dependencies: self.root.requires.iter().map(ToString::to_string).collect(),
            },
            tool,
        };
        toml::to_string_pretty(&pyproject).context("failed to serialize scenario workspace")
    }

    /// Materialize this scenario's root workspace into `destination`.
    pub fn materialize_workspace(&self, destination: impl AsRef<Path>) -> Result<()> {
        let destination = destination.as_ref();
        fs_err::create_dir_all(destination).with_context(|| {
            format!(
                "failed to create workspace directory `{}`",
                destination.display()
            )
        })?;
        let pyproject_toml = destination.join("pyproject.toml");
        let mut file = fs_err::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&pyproject_toml)
            .with_context(|| format!("failed to create `{}`", pyproject_toml.display()))?;
        file.write_all(self.pyproject_toml()?.as_bytes())
            .with_context(|| format!("failed to write `{}`", pyproject_toml.display()))
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
                workspace: None,
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

    /// Project metadata to use when materializing this root as a workspace.
    #[serde(default)]
    pub workspace: Option<RootWorkspace>,
}

/// An inline workspace definition for a scenario root.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RootWorkspace {
    /// The project at the workspace root.
    project: RootWorkspaceProject,
}

/// The single project supported by an inline root workspace.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RootWorkspaceProject {
    name: PackageName,
    version: Version,
}

#[derive(Serialize)]
struct PyProjectToml {
    project: PyProjectProject,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool: Option<PyProjectTool>,
}

#[derive(Serialize)]
struct PyProjectProject {
    name: String,
    version: String,
    #[serde(rename = "requires-python", skip_serializing_if = "Option::is_none")]
    requires_python: Option<String>,
    dependencies: Vec<String>,
}

#[derive(Serialize)]
struct PyProjectTool {
    uv: PyProjectUv,
}

#[derive(Serialize)]
struct PyProjectUv {
    #[serde(rename = "required-environments")]
    required_environments: Vec<String>,
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
    fn parse_root_workspace() {
        let toml = r#"
name = "workspace"

[root]
requires = []

[root.workspace.project]
name = "project"
version = "0.1.0"

[expected]
satisfiable = true
"#;
        let scenario: Scenario = toml::from_str(toml).expect("scenario should parse");
        let project = scenario
            .root
            .workspace
            .expect("root should define a workspace")
            .project;
        assert_eq!(project.name.as_ref(), "project");
        assert_eq!(
            project.version,
            Version::from_str("0.1.0").expect("valid version")
        );
    }

    #[test]
    fn materialize_root_workspace() {
        let toml = r#"
name = "workspace"

[root]
requires_python = ">=3.9"
requires = ["a ; python_version < '3.10'"]

[root.workspace.project]
name = "project"
version = "0.1.0"

[resolver_options]
required_environments = ["sys_platform == 'darwin'"]

[expected]
satisfiable = true
"#;
        let scenario: Scenario = toml::from_str(toml).expect("scenario should parse");
        let temporary_directory =
            tempfile::tempdir().expect("temporary directory should be created");
        scenario
            .materialize_workspace(temporary_directory.path())
            .expect("workspace should materialize");

        let pyproject_toml =
            fs_err::read_to_string(temporary_directory.path().join("pyproject.toml"))
                .expect("pyproject.toml should be readable");
        let pyproject: toml::Value =
            toml::from_str(&pyproject_toml).expect("pyproject.toml should parse");
        assert_eq!(pyproject["project"]["name"].as_str(), Some("project"));
        assert_eq!(pyproject["project"]["version"].as_str(), Some("0.1.0"));
        assert_eq!(
            pyproject["project"]["requires-python"].as_str(),
            Some(">=3.9")
        );
        assert_eq!(
            pyproject["project"]["dependencies"][0].as_str(),
            Some("a ; python_full_version < '3.10'")
        );
        assert_eq!(
            pyproject["tool"]["uv"]["required-environments"][0].as_str(),
            Some("sys_platform == 'darwin'")
        );
    }

    #[test]
    fn materialize_root_workspace_requires_configuration() {
        let scenario = Scenario::empty();
        let temporary_directory =
            tempfile::tempdir().expect("temporary directory should be created");
        assert!(
            scenario
                .materialize_workspace(temporary_directory.path())
                .is_err()
        );
    }

    #[test]
    fn materialize_root_workspace_does_not_overwrite() {
        let toml = r#"
name = "workspace"

[root]
requires = []

[root.workspace.project]
name = "project"
version = "0.1.0"

[expected]
satisfiable = true
"#;
        let scenario: Scenario = toml::from_str(toml).expect("scenario should parse");
        let temporary_directory =
            tempfile::tempdir().expect("temporary directory should be created");
        fs_err::write(
            temporary_directory.path().join("pyproject.toml"),
            "existing contents",
        )
        .expect("existing pyproject.toml should be written");

        assert!(
            scenario
                .materialize_workspace(temporary_directory.path())
                .is_err()
        );
        assert_eq!(
            fs_err::read_to_string(temporary_directory.path().join("pyproject.toml"))
                .expect("existing pyproject.toml should be readable"),
            "existing contents"
        );
    }

    #[test]
    fn reject_unknown_root_workspace_field() {
        let toml = r#"
name = "workspace"

[root]
requires = []

[root.workspace]
members = []

[root.workspace.project]
name = "project"
version = "0.1.0"

[expected]
satisfiable = true
"#;
        assert!(toml::from_str::<Scenario>(toml).is_err());
    }

    #[test]
    fn reject_root_workspace_for_non_universal_scenario() {
        let temporary_directory =
            tempfile::tempdir().expect("temporary directory should be created");
        let path = temporary_directory.path().join("workspace.toml");
        fs_err::write(
            &path,
            r#"
name = "workspace"

[root]
requires = []

[root.workspace.project]
name = "project"
version = "0.1.0"

[expected]
satisfiable = true
"#,
        )
        .expect("scenario should be written");

        assert!(Scenario::from_path(&path).is_err());
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
