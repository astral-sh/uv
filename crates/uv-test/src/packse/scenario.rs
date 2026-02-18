//! Packse scenario types – a Rust equivalent of `packse/scenario.py`.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;

/// A complete packse scenario definition.
#[derive(Debug, Deserialize)]
pub struct Scenario {
    /// The scenario name (e.g., `"fork-basic"`).
    pub name: String,

    /// Human-readable description.
    #[serde(default)]
    pub description: Option<String>,

    /// The packages available in this scenario.
    #[serde(default)]
    pub packages: BTreeMap<String, Package>,

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
    pub fn from_path(path: &Path) -> Self {
        let contents = fs_err::read_to_string(path).expect("failed to read scenario file");
        toml::from_str(&contents).expect("failed to parse scenario file")
    }
}

/// A package with one or more versions.
#[derive(Debug, Deserialize)]
pub struct Package {
    pub versions: BTreeMap<String, PackageMetadata>,
}

/// Metadata for a single version of a package.
#[derive(Debug, Deserialize, Default)]
pub struct PackageMetadata {
    /// The `Requires-Python` specifier. Defaults to `">=3.12"`.
    #[serde(default = "default_requires_python")]
    pub requires_python: Option<String>,

    /// Dependency requirements (PEP 508 strings).
    #[serde(default)]
    pub requires: Vec<String>,

    /// Optional-dependency groups (extras).
    #[serde(default)]
    pub extras: BTreeMap<String, Vec<String>>,

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
    pub wheel_tags: Vec<String>,

    /// A description for the package version.
    #[serde(default)]
    pub description: String,
}

/// The root/entrypoint package.
#[derive(Debug, Deserialize)]
pub struct RootPackage {
    /// `Requires-Python` for the root.
    #[serde(default = "default_requires_python")]
    pub requires_python: Option<String>,

    /// Top-level requirements (PEP 508 strings).
    #[serde(default)]
    pub requires: Vec<String>,
}

/// Expected resolution outcome.
#[derive(Debug, Deserialize)]
pub struct Expected {
    /// Whether the scenario is satisfiable.
    pub satisfiable: bool,

    /// Expected installed packages and their versions.
    #[serde(default)]
    pub packages: BTreeMap<String, String>,

    /// Optional explanation.
    #[serde(default)]
    pub explanation: Option<String>,
}

/// Python environment metadata.
#[derive(Debug, Deserialize)]
pub struct Environment {
    /// Active Python version.
    #[serde(default = "default_python")]
    pub python: String,

    /// Additional Python versions available on the system.
    #[serde(default)]
    pub additional_python: Vec<String>,
}

impl Default for Environment {
    fn default() -> Self {
        Self {
            python: "3.12".to_string(),
            additional_python: Vec::new(),
        }
    }
}

/// Additional resolver options.
#[derive(Debug, Default, Deserialize)]
pub struct ResolverOptions {
    /// Python version override for resolution.
    #[serde(default)]
    pub python: Option<String>,

    /// Enable pre-release selection.
    #[serde(default)]
    pub prereleases: bool,

    /// Packages that must use pre-built wheels (no building from source).
    #[serde(default)]
    pub no_build: Vec<String>,

    /// Packages that must NOT use pre-built wheels (must build from source).
    #[serde(default)]
    pub no_binary: Vec<String>,

    /// Universal (multi-platform) resolution mode.
    #[serde(default)]
    pub universal: bool,

    /// Python platform to resolve for.
    #[serde(default)]
    pub python_platform: Option<String>,

    /// Required environments (platform markers).
    #[serde(default)]
    pub required_environments: Option<Vec<String>>,
}

#[expect(clippy::unnecessary_wraps)] // Must return `Option` for serde `default`
fn default_requires_python() -> Option<String> {
    Some(">=3.12".to_string())
}

fn default_true() -> bool {
    true
}

fn default_python() -> String {
    "3.12".to_string()
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
        let scenario: Scenario = toml::from_str(toml).unwrap();
        assert_eq!(scenario.name, "fork-basic");
        assert!(scenario.resolver_options.universal);
        assert_eq!(scenario.packages.len(), 1);
        assert_eq!(scenario.packages["a"].versions.len(), 2);
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
        let scenario: Scenario = toml::from_str(toml).unwrap();
        assert_eq!(scenario.name, "all-extras-required");
        let a_meta = &scenario.packages["a"].versions["1.0.0"];
        assert_eq!(a_meta.extras.len(), 3);
        assert_eq!(a_meta.extras["extra_b"], vec!["b"]);
    }
}
