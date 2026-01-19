//! Data structures for the markdown test framework.

use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;

/// A complete markdown test file containing multiple tests.
#[derive(Debug)]
pub struct MarkdownTestFile {
    /// The source file path (for error reporting).
    pub path: PathBuf,
    /// All tests extracted from the file.
    pub tests: Vec<MarkdownTest>,
}

/// A single test extracted from a markdown file.
///
/// Each test corresponds to a leaf section (a section that contains code blocks
/// rather than subsections).
#[derive(Debug)]
pub struct MarkdownTest {
    /// Human-readable name derived from the header hierarchy.
    /// Example: "Lock - Basic locking"
    pub name: String,
    /// Configuration for this test (inherited from parent sections).
    pub config: TestConfig,
    /// Ordered sequence of steps to execute (files, trees, commands, snapshots in document order).
    pub steps: Vec<TestStep>,
    /// Line number in the source file where this test starts (for error reporting).
    pub line_number: usize,
}

impl MarkdownTest {
    /// Check if this test should run on the current platform with given enabled features.
    ///
    /// Returns `true` if:
    /// - `target-os` matches the current system
    /// - `target-family` matches the current system
    /// - All `required-features` are present in `enabled_features`
    #[must_use]
    pub fn should_run(&self, enabled_features: &[String]) -> bool {
        self.config.environment.target_os.matches_current()
            && self.config.environment.target_family.matches_current()
            && self
                .config
                .environment
                .required_features
                .all_enabled(enabled_features)
    }
}

/// A single step in test execution, preserving document order.
#[derive(Debug, Clone)]
pub enum TestStep {
    /// Create/write a file to the test directory.
    WriteFile(EmbeddedFile),
    /// Create a directory tree structure.
    CreateTree(TreeCreation),
    /// Execute a command and validate output.
    RunCommand(Command),
    /// Verify a file snapshot.
    CheckFileSnapshot(FileSnapshot),
    /// Check a content assertion.
    CheckContentAssertion(ContentAssertion),
    /// Verify a directory tree snapshot.
    CheckTreeSnapshot(TreeSnapshot),
}

/// An embedded file to be written to the test directory.
#[derive(Debug, Clone)]
pub struct EmbeddedFile {
    /// Relative path within the test directory.
    pub path: PathBuf,
    /// Content of the file.
    pub content: String,
    /// Line number in the markdown source where this file is defined.
    pub line_number: usize,
}

/// A command to execute during the test.
#[derive(Debug, Clone)]
pub struct Command {
    /// The full command line (without the `$ ` prefix).
    pub command: String,
    /// Expected output in the `uv_snapshot` format:
    /// - `success: true/false`
    /// - `exit_code: N`
    /// - `----- stdout -----` section
    /// - `----- stderr -----` section
    pub expected_output: String,
    /// Working directory relative to the test directory.
    /// If `None`, commands run in the test directory root.
    pub working_dir: Option<PathBuf>,
    /// Line number in the markdown source where this command is defined.
    pub line_number: usize,
}

/// A file to snapshot after test execution.
#[derive(Debug, Clone)]
pub struct FileSnapshot {
    /// Relative path within the test directory.
    pub path: PathBuf,
    /// Expected content of the file (to be compared against actual content).
    pub expected_content: String,
    /// Line number in the markdown source where this snapshot is defined.
    pub line_number: usize,
}

/// A content assertion to check after test execution.
#[derive(Debug, Clone)]
pub struct ContentAssertion {
    /// Relative path within the test directory.
    pub path: PathBuf,
    /// The kind of assertion.
    pub kind: AssertKind,
    /// The expected content to check against.
    pub expected: String,
    /// Line number in the markdown source where this assertion is defined.
    pub line_number: usize,
}

/// A directory tree snapshot to check after test execution.
#[derive(Debug, Clone)]
pub struct TreeSnapshot {
    /// Expected tree content (in `tree` command format).
    pub expected_content: String,
    /// Optional depth limit for tree generation.
    pub depth: Option<usize>,
    /// Line number in the markdown source where this snapshot is defined.
    pub line_number: usize,
}

/// An entry in a tree creation block.
#[derive(Debug, Clone)]
pub enum TreeEntry {
    /// A directory to create.
    Directory { path: PathBuf },
    /// A file to create (empty).
    File { path: PathBuf },
    /// A symbolic link to create.
    Symlink { path: PathBuf, target: PathBuf },
}

/// A directory tree structure to create before running commands.
#[derive(Debug, Clone)]
pub struct TreeCreation {
    /// Entries to create (directories, files, symlinks).
    pub entries: Vec<TreeEntry>,
    /// Line number in the markdown source where this tree is defined.
    pub line_number: usize,
}

/// Test configuration from `mdtest.toml` blocks.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct TestConfig {
    /// Environment configuration.
    #[serde(default)]
    pub environment: EnvironmentConfig,
    /// Filter configuration.
    #[serde(default)]
    pub filters: FilterConfig,
    /// Tree configuration.
    #[serde(default)]
    pub tree: TreeConfig,
}

/// Tree configuration for directory tree snapshots.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct TreeConfig {
    /// Patterns to exclude from tree output.
    /// Supports glob patterns like `*.pyc`, `__pycache__`, `cache`.
    #[serde(default)]
    pub exclude: Vec<String>,
    /// Whether to apply default filters to tree output (e.g., normalizing
    /// `Scripts` to `bin` on Windows). Defaults to true.
    #[serde(default = "default_true")]
    pub default_filters: bool,
}

impl Default for TreeConfig {
    fn default() -> Self {
        Self {
            exclude: Vec::new(),
            default_filters: true,
        }
    }
}

fn default_true() -> bool {
    true
}

/// Environment configuration for tests.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct EnvironmentConfig {
    /// Python versions to use for the test (e.g., `"3.12"` or `["3.11", "3.12"]`).
    /// Accepts both `python-version` (singular, for backwards compatibility)
    /// and `python-versions` (plural).
    #[serde(default, alias = "python-version", rename = "python-versions")]
    pub python_versions: PythonVersions,
    /// Exclude packages newer than this date.
    pub exclude_newer: Option<String>,
    /// HTTP timeout for requests.
    pub http_timeout: Option<String>,
    /// Number of concurrent installs.
    pub concurrent_installs: Option<String>,
    /// Target OS(es) for this test, matching Rust's `target_os` cfg values.
    /// If specified, the test only runs on matching operating systems.
    /// Valid values: "linux", "macos", "windows", "freebsd", "netbsd", "openbsd", etc.
    #[serde(default, rename = "target-os")]
    pub target_os: TargetOs,
    /// Target family for this test, matching Rust's `target_family` cfg values.
    /// If specified, the test only runs on matching OS families.
    /// Valid values: "unix", "windows"
    #[serde(default, rename = "target-family")]
    pub target_family: TargetFamily,
    /// Extra environment variables to set.
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// Environment variables to remove/unset.
    /// These are removed after `env` is applied, so a child section can use
    /// `env-remove` to unset a variable that was set by a parent section.
    #[serde(default)]
    pub env_remove: Vec<String>,
    /// Whether to create a virtual environment before running the test.
    /// Defaults to true if not specified.
    #[serde(default)]
    pub create_venv: Option<bool>,
    /// Required features for this test. If specified, the test only runs
    /// when all required features are enabled.
    /// Known features: "python-patch"
    #[serde(default, rename = "required-features")]
    pub required_features: RequiredFeatures,
}

/// Target OS specification for tests, using `uv_platform::Os` for matching.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum TargetOs {
    /// Run on all operating systems (default).
    #[default]
    All,
    /// Run only on specific operating systems.
    Only(Vec<uv_platform::Os>),
}

impl TargetOs {
    /// Check if the test should run on the current OS.
    #[must_use]
    pub fn matches_current(&self) -> bool {
        match self {
            Self::All => true,
            Self::Only(os_list) => {
                let current = uv_platform::Os::from_env();
                os_list.contains(&current)
            }
        }
    }
}

impl<'de> Deserialize<'de> for TargetOs {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::{self, Visitor};

        struct TargetOsVisitor;

        impl<'de> Visitor<'de> for TargetOsVisitor {
            type Value = TargetOs;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str(
                    "an OS string or array of OS strings (e.g., \"linux\", \"macos\", \"windows\")",
                )
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                let os = v
                    .parse::<uv_platform::Os>()
                    .map_err(|e| de::Error::custom(e.to_string()))?;
                Ok(TargetOs::Only(vec![os]))
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: de::SeqAccess<'de>,
            {
                let mut os_list = Vec::new();
                while let Some(s) = seq.next_element::<String>()? {
                    let os = s
                        .parse::<uv_platform::Os>()
                        .map_err(|e| de::Error::custom(e.to_string()))?;
                    os_list.push(os);
                }
                if os_list.is_empty() {
                    Ok(TargetOs::All)
                } else {
                    Ok(TargetOs::Only(os_list))
                }
            }
        }

        deserializer.deserialize_any(TargetOsVisitor)
    }
}

/// Target family specification for tests, matching Rust's `target_family` cfg values.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum TargetFamily {
    /// Run on all OS families (default).
    #[default]
    All,
    /// Run only on specific OS families.
    Only(Vec<String>),
}

impl TargetFamily {
    /// Check if the test should run on the current OS family.
    #[must_use]
    pub fn matches_current(&self) -> bool {
        match self {
            Self::All => true,
            Self::Only(families) => families.iter().any(|f| is_current_family(f)),
        }
    }
}

/// Check if the given family string matches the current OS family.
fn is_current_family(family: &str) -> bool {
    match family {
        "unix" => cfg!(target_family = "unix"),
        "windows" => cfg!(target_family = "windows"),
        "wasm" => cfg!(target_family = "wasm"),
        _ => false,
    }
}

impl<'de> Deserialize<'de> for TargetFamily {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::{self, Visitor};

        struct TargetFamilyVisitor;

        impl<'de> Visitor<'de> for TargetFamilyVisitor {
            type Value = TargetFamily;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str(
                    "a family string or array of family strings (\"unix\" or \"windows\")",
                )
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(TargetFamily::Only(vec![v.to_string()]))
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: de::SeqAccess<'de>,
            {
                let mut families = Vec::new();
                while let Some(s) = seq.next_element::<String>()? {
                    families.push(s);
                }
                if families.is_empty() {
                    Ok(TargetFamily::All)
                } else {
                    Ok(TargetFamily::Only(families))
                }
            }
        }

        deserializer.deserialize_any(TargetFamilyVisitor)
    }
}

/// Python versions specification for tests.
///
/// Controls which Python versions are available for a test.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum PythonVersions {
    /// Use the default Python version (currently "3.12").
    #[default]
    Default,
    /// No Python versions (empty array) - for tests that don't need Python.
    None,
    /// Run test with these specific versions.
    Only(Vec<String>),
}

impl PythonVersions {
    /// Check if this is the default value (for merge logic).
    #[must_use]
    pub fn is_default(&self) -> bool {
        matches!(self, Self::Default)
    }
}

impl<'de> Deserialize<'de> for PythonVersions {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::{self, Visitor};

        struct PythonVersionsVisitor;

        impl<'de> Visitor<'de> for PythonVersionsVisitor {
            type Value = PythonVersions;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str(
                    "a Python version string or array of version strings (e.g., \"3.12\", [\"3.11\", \"3.12\"])",
                )
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(PythonVersions::Only(vec![v.to_string()]))
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: de::SeqAccess<'de>,
            {
                let mut versions = Vec::new();
                while let Some(s) = seq.next_element::<String>()? {
                    versions.push(s);
                }
                if versions.is_empty() {
                    Ok(PythonVersions::None)
                } else {
                    Ok(PythonVersions::Only(versions))
                }
            }
        }

        deserializer.deserialize_any(PythonVersionsVisitor)
    }
}

/// Required features specification for tests.
///
/// Tests can specify features they require to run. If any required feature
/// is not enabled, the test will be skipped.
///
/// Known features:
/// - `python-patch`: Tests that depend on Python patch version behavior
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum RequiredFeatures {
    /// No features required (default) - test always runs.
    #[default]
    None,
    /// Test requires these specific features to be enabled.
    Some(Vec<String>),
}

impl RequiredFeatures {
    /// Check if this is the default value (no features required).
    #[must_use]
    pub fn is_default(&self) -> bool {
        matches!(self, Self::None)
    }

    /// Check if all required features are present in the enabled set.
    #[must_use]
    pub fn all_enabled(&self, enabled_features: &[String]) -> bool {
        match self {
            Self::None => true,
            Self::Some(required) => required.iter().all(|f| enabled_features.contains(f)),
        }
    }

    /// Get the list of required features, if any.
    #[must_use]
    pub fn as_slice(&self) -> &[String] {
        match self {
            Self::None => &[],
            Self::Some(features) => features,
        }
    }
}

impl<'de> Deserialize<'de> for RequiredFeatures {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::{self, Visitor};

        struct RequiredFeaturesVisitor;

        impl<'de> Visitor<'de> for RequiredFeaturesVisitor {
            type Value = RequiredFeatures;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str(
                    "a feature string or array of feature strings (e.g., \"python-patch\", [\"python-patch\"])",
                )
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(RequiredFeatures::Some(vec![v.to_string()]))
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: de::SeqAccess<'de>,
            {
                let mut features = Vec::new();
                while let Some(s) = seq.next_element::<String>()? {
                    features.push(s);
                }
                if features.is_empty() {
                    Ok(RequiredFeatures::None)
                } else {
                    Ok(RequiredFeatures::Some(features))
                }
            }
        }

        deserializer.deserialize_any(RequiredFeaturesVisitor)
    }
}

/// Filter configuration for tests.
///
/// These correspond to the `with_filtered_*` methods on `TestContext`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct FilterConfig {
    /// Filter package counts (e.g., "Resolved 5 packages" -> "Resolved [N] packages").
    #[serde(default)]
    pub counts: bool,
    /// Filter executable suffix (removes `.exe` on Windows).
    #[serde(default)]
    pub exe_suffix: bool,
    /// Filter Python executable names (e.g., `python3.12` -> `[PYTHON]`).
    #[serde(default)]
    pub python_names: bool,
    /// Filter virtualenv bin directory (e.g., `Scripts` or `bin` -> `[BIN]`).
    #[serde(default)]
    pub virtualenv_bin: bool,
    /// Filter Python installation bin directory.
    #[serde(default)]
    pub python_install_bin: bool,
    /// Filter Python source messages.
    #[serde(default)]
    pub python_sources: bool,
    /// Filter pyvenv.cfg file content.
    #[serde(default)]
    pub pyvenv_cfg: bool,
    /// Filter hardlink/copy mode warnings.
    #[serde(default)]
    pub link_mode_warning: bool,
    /// Filter "not executable" permission errors.
    #[serde(default)]
    pub not_executable: bool,
    /// Filter Python platform keys.
    #[serde(default)]
    pub python_keys: bool,
    /// Filter latest Python versions with `[LATEST]`.
    #[serde(default)]
    pub latest_python_versions: bool,
    /// Filter compiled file counts.
    #[serde(default)]
    pub compiled_file_count: bool,
    /// Filter `CycloneDX` UUIDs.
    #[serde(default)]
    pub cyclonedx: bool,
    /// Collapse whitespace (multiple spaces/tabs -> single space).
    #[serde(default)]
    pub collapse_whitespace: bool,
    /// Filter cache size output.
    #[serde(default)]
    pub cache_size: bool,
    /// Filter missing file errors (OS error 2/3).
    #[serde(default)]
    pub missing_file_error: bool,
}

impl TestConfig {
    /// Merge two configs, with `other` taking precedence.
    #[must_use]
    pub fn merge(&self, other: &Self) -> Self {
        Self {
            environment: EnvironmentConfig {
                python_versions: if other.environment.python_versions.is_default() {
                    self.environment.python_versions.clone()
                } else {
                    other.environment.python_versions.clone()
                },
                exclude_newer: other
                    .environment
                    .exclude_newer
                    .clone()
                    .or_else(|| self.environment.exclude_newer.clone()),
                http_timeout: other
                    .environment
                    .http_timeout
                    .clone()
                    .or_else(|| self.environment.http_timeout.clone()),
                concurrent_installs: other
                    .environment
                    .concurrent_installs
                    .clone()
                    .or_else(|| self.environment.concurrent_installs.clone()),
                target_os: match &other.environment.target_os {
                    TargetOs::All => self.environment.target_os.clone(),
                    other_os @ TargetOs::Only(_) => other_os.clone(),
                },
                target_family: match &other.environment.target_family {
                    TargetFamily::All => self.environment.target_family.clone(),
                    other_family @ TargetFamily::Only(_) => other_family.clone(),
                },
                env: {
                    let mut env = self.environment.env.clone();
                    env.extend(other.environment.env.clone());
                    env
                },
                env_remove: {
                    let mut env_remove = self.environment.env_remove.clone();
                    env_remove.extend(other.environment.env_remove.clone());
                    env_remove
                },
                // Child's explicit setting takes precedence over parent's
                create_venv: other
                    .environment
                    .create_venv
                    .or(self.environment.create_venv),
                // Combine required features from parent and child (union)
                required_features: {
                    let mut features: Vec<String> =
                        self.environment.required_features.as_slice().to_vec();
                    for f in other.environment.required_features.as_slice() {
                        if !features.contains(f) {
                            features.push(f.clone());
                        }
                    }
                    if features.is_empty() {
                        RequiredFeatures::None
                    } else {
                        RequiredFeatures::Some(features)
                    }
                },
            },
            filters: FilterConfig {
                counts: other.filters.counts || self.filters.counts,
                exe_suffix: other.filters.exe_suffix || self.filters.exe_suffix,
                python_names: other.filters.python_names || self.filters.python_names,
                virtualenv_bin: other.filters.virtualenv_bin || self.filters.virtualenv_bin,
                python_install_bin: other.filters.python_install_bin
                    || self.filters.python_install_bin,
                python_sources: other.filters.python_sources || self.filters.python_sources,
                pyvenv_cfg: other.filters.pyvenv_cfg || self.filters.pyvenv_cfg,
                link_mode_warning: other.filters.link_mode_warning
                    || self.filters.link_mode_warning,
                not_executable: other.filters.not_executable || self.filters.not_executable,
                python_keys: other.filters.python_keys || self.filters.python_keys,
                latest_python_versions: other.filters.latest_python_versions
                    || self.filters.latest_python_versions,
                compiled_file_count: other.filters.compiled_file_count
                    || self.filters.compiled_file_count,
                cyclonedx: other.filters.cyclonedx || self.filters.cyclonedx,
                collapse_whitespace: other.filters.collapse_whitespace
                    || self.filters.collapse_whitespace,
                cache_size: other.filters.cache_size || self.filters.cache_size,
                missing_file_error: other.filters.missing_file_error
                    || self.filters.missing_file_error,
            },
            tree: TreeConfig {
                exclude: {
                    let mut exclude = self.tree.exclude.clone();
                    exclude.extend(other.tree.exclude.clone());
                    exclude
                },
                // If other explicitly sets default_filters to false, use that
                // Otherwise, inherit from self (default is true)
                default_filters: if other.tree.exclude.is_empty() && other.tree.default_filters {
                    self.tree.default_filters
                } else {
                    other.tree.default_filters
                },
            },
        }
    }
}

/// Code block attributes parsed from the info string.
#[derive(Debug, Clone, Default)]
pub struct CodeBlockAttributes {
    /// The language of the code block (e.g., "toml", "python").
    pub language: Option<String>,
    /// The title/filename attribute (e.g., `title="pyproject.toml"`).
    pub title: Option<String>,
    /// Whether this is a snapshot block (e.g., `snapshot=true`).
    pub snapshot: bool,
    /// Whether this tree block creates the structure (e.g., `create=true`).
    pub create: bool,
    /// Assertion type for file content (e.g., `assert=contains`).
    pub assert: Option<AssertKind>,
    /// Depth for tree snapshots (e.g., `depth=2`).
    pub depth: Option<usize>,
    /// Working directory for command blocks (e.g., `working-dir="./subdir"`).
    pub working_dir: Option<String>,
}

/// Kind of assertion for file content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssertKind {
    /// Assert that the file contains the specified content.
    Contains,
}

impl CodeBlockAttributes {
    /// Parse attributes from a code block info string.
    ///
    /// Example info strings:
    /// - `toml title="pyproject.toml"`
    /// - `toml title="uv.lock" snapshot=true`
    /// - (empty string, for command blocks)
    pub fn parse(info_string: &str) -> Self {
        let mut attrs = Self::default();
        let mut parts = info_string.split_whitespace();

        // First part is the language (if any)
        if let Some(first) = parts.next() {
            if !first.contains('=') {
                attrs.language = Some(first.to_string());
            } else {
                // First part is an attribute, not a language
                Self::parse_attribute(&mut attrs, first);
            }
        }

        // Parse remaining attributes
        for part in parts {
            Self::parse_attribute(&mut attrs, part);
        }

        attrs
    }

    fn parse_attribute(attrs: &mut Self, attr: &str) {
        if let Some((key, value)) = attr.split_once('=') {
            let value = value.trim_matches('"');
            match key {
                "title" => attrs.title = Some(value.to_string()),
                "snapshot" => attrs.snapshot = value == "true",
                "create" => attrs.create = value == "true",
                "assert" => {
                    attrs.assert = match value {
                        "contains" => Some(AssertKind::Contains),
                        _ => None,
                    };
                }
                "depth" => {
                    attrs.depth = value.parse().ok();
                }
                "working-dir" => {
                    attrs.working_dir = Some(value.to_string());
                }
                _ => {} // Ignore unknown attributes
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_code_block_attributes_empty() {
        let attrs = CodeBlockAttributes::parse("");
        assert!(attrs.language.is_none());
        assert!(attrs.title.is_none());
        assert!(!attrs.snapshot);
    }

    #[test]
    fn test_code_block_attributes_language_only() {
        let attrs = CodeBlockAttributes::parse("toml");
        assert_eq!(attrs.language.as_deref(), Some("toml"));
        assert!(attrs.title.is_none());
        assert!(!attrs.snapshot);
    }

    #[test]
    fn test_code_block_attributes_with_title() {
        let attrs = CodeBlockAttributes::parse("toml title=\"pyproject.toml\"");
        assert_eq!(attrs.language.as_deref(), Some("toml"));
        assert_eq!(attrs.title.as_deref(), Some("pyproject.toml"));
        assert!(!attrs.snapshot);
    }

    #[test]
    fn test_code_block_attributes_with_snapshot() {
        let attrs = CodeBlockAttributes::parse("toml title=\"uv.lock\" snapshot=true");
        assert_eq!(attrs.language.as_deref(), Some("toml"));
        assert_eq!(attrs.title.as_deref(), Some("uv.lock"));
        assert!(attrs.snapshot);
    }

    #[test]
    fn test_code_block_attributes_with_working_dir() {
        let attrs = CodeBlockAttributes::parse("console working-dir=\"./subdir\"");
        assert_eq!(attrs.language.as_deref(), Some("console"));
        assert_eq!(attrs.working_dir.as_deref(), Some("./subdir"));
    }

    #[test]
    fn test_code_block_attributes_working_dir_no_language() {
        let attrs = CodeBlockAttributes::parse("working-dir=\"foo/bar\"");
        assert!(attrs.language.is_none());
        assert_eq!(attrs.working_dir.as_deref(), Some("foo/bar"));
    }

    #[test]
    fn test_config_merge() {
        let base = TestConfig {
            environment: EnvironmentConfig {
                python_versions: PythonVersions::Only(vec!["3.11".to_string()]),
                exclude_newer: Some("2024-01-01".to_string()),
                env: HashMap::from([("FOO".to_string(), "bar".to_string())]),
                ..Default::default()
            },
            ..Default::default()
        };

        let override_config = TestConfig {
            environment: EnvironmentConfig {
                python_versions: PythonVersions::Only(vec!["3.12".to_string()]),
                exclude_newer: None,
                env: HashMap::from([("BAZ".to_string(), "qux".to_string())]),
                ..Default::default()
            },
            ..Default::default()
        };

        let merged = base.merge(&override_config);
        assert_eq!(
            merged.environment.python_versions,
            PythonVersions::Only(vec!["3.12".to_string()])
        );
        assert_eq!(
            merged.environment.exclude_newer.as_deref(),
            Some("2024-01-01")
        );
        assert_eq!(
            merged.environment.env.get("FOO").map(String::as_str),
            Some("bar")
        );
        assert_eq!(
            merged.environment.env.get("BAZ").map(String::as_str),
            Some("qux")
        );
    }

    #[test]
    fn test_target_os_all_matches() {
        let target = TargetOs::All;
        assert!(target.matches_current());
    }

    #[test]
    fn test_target_os_current_matches() {
        let current_os = uv_platform::Os::from_env();
        let target = TargetOs::Only(vec![current_os]);
        assert!(target.matches_current());
    }

    #[test]
    fn test_target_os_other_does_not_match() {
        // Use an OS that's definitely not the current one
        let current = uv_platform::Os::from_env();
        let other_os: uv_platform::Os = if current.is_windows() {
            "linux".parse().unwrap()
        } else {
            "windows".parse().unwrap()
        };
        let target = TargetOs::Only(vec![other_os]);
        assert!(!target.matches_current());
    }

    #[test]
    fn test_target_os_list_matches_if_any() {
        let current_os = uv_platform::Os::from_env();
        let freebsd: uv_platform::Os = "freebsd".parse().unwrap();
        let target = TargetOs::Only(vec![freebsd, current_os]);
        assert!(target.matches_current());
    }

    #[test]
    fn test_target_family_all_matches() {
        let target = TargetFamily::All;
        assert!(target.matches_current());
    }

    #[test]
    fn test_target_family_unix_matches_on_unix() {
        let target = TargetFamily::Only(vec!["unix".to_string()]);
        // This test will pass on Unix, fail on Windows
        #[cfg(target_family = "unix")]
        assert!(target.matches_current());
        #[cfg(not(target_family = "unix"))]
        assert!(!target.matches_current());
    }

    #[test]
    fn test_target_family_windows_matches_on_windows() {
        let target = TargetFamily::Only(vec!["windows".to_string()]);
        // This test will pass on Windows, fail on Unix
        #[cfg(target_family = "windows")]
        assert!(target.matches_current());
        #[cfg(not(target_family = "windows"))]
        assert!(!target.matches_current());
    }

    #[test]
    fn test_target_os_deserialize_string() {
        let config: EnvironmentConfig = toml::from_str(r#"target-os = "linux""#).unwrap();
        let linux: uv_platform::Os = "linux".parse().unwrap();
        assert_eq!(config.target_os, TargetOs::Only(vec![linux]));
    }

    #[test]
    fn test_target_os_deserialize_array() {
        let config: EnvironmentConfig =
            toml::from_str(r#"target-os = ["linux", "macos"]"#).unwrap();
        let linux: uv_platform::Os = "linux".parse().unwrap();
        let macos: uv_platform::Os = "macos".parse().unwrap();
        assert_eq!(config.target_os, TargetOs::Only(vec![linux, macos]));
    }

    #[test]
    fn test_target_family_deserialize_string() {
        let config: EnvironmentConfig = toml::from_str(r#"target-family = "unix""#).unwrap();
        assert_eq!(
            config.target_family,
            TargetFamily::Only(vec!["unix".to_string()])
        );
    }

    #[test]
    fn test_target_family_deserialize_array() {
        let config: EnvironmentConfig =
            toml::from_str(r#"target-family = ["unix", "windows"]"#).unwrap();
        assert_eq!(
            config.target_family,
            TargetFamily::Only(vec!["unix".to_string(), "windows".to_string()])
        );
    }

    #[test]
    fn test_create_venv_deserialize() {
        // Test parsing create-venv = false
        let config: EnvironmentConfig = toml::from_str(r"create-venv = false").unwrap();
        assert_eq!(config.create_venv, Some(false));

        // Test parsing create-venv = true
        let config: EnvironmentConfig = toml::from_str(r"create-venv = true").unwrap();
        assert_eq!(config.create_venv, Some(true));

        // Test default (not specified)
        let config: EnvironmentConfig = toml::from_str(r"").unwrap();
        assert_eq!(config.create_venv, None);
    }

    #[test]
    fn test_create_venv_merge() {
        // Parent sets false, child doesn't set -> should be false
        let parent = TestConfig {
            environment: EnvironmentConfig {
                create_venv: Some(false),
                ..Default::default()
            },
            ..Default::default()
        };
        let child = TestConfig::default();
        let merged = parent.merge(&child);
        assert_eq!(merged.environment.create_venv, Some(false));

        // Parent sets false, child sets true -> child wins
        let child = TestConfig {
            environment: EnvironmentConfig {
                create_venv: Some(true),
                ..Default::default()
            },
            ..Default::default()
        };
        let merged = parent.merge(&child);
        assert_eq!(merged.environment.create_venv, Some(true));

        // Parent sets true, child sets false -> child wins
        let parent = TestConfig {
            environment: EnvironmentConfig {
                create_venv: Some(true),
                ..Default::default()
            },
            ..Default::default()
        };
        let child = TestConfig {
            environment: EnvironmentConfig {
                create_venv: Some(false),
                ..Default::default()
            },
            ..Default::default()
        };
        let merged = parent.merge(&child);
        assert_eq!(merged.environment.create_venv, Some(false));
    }

    #[test]
    fn test_python_versions_deserialize_string() {
        let config: EnvironmentConfig = toml::from_str(r#"python-versions = "3.11""#).unwrap();
        assert_eq!(
            config.python_versions,
            PythonVersions::Only(vec!["3.11".to_string()])
        );
    }

    #[test]
    fn test_python_versions_deserialize_array() {
        let config: EnvironmentConfig =
            toml::from_str(r#"python-versions = ["3.11", "3.12"]"#).unwrap();
        assert_eq!(
            config.python_versions,
            PythonVersions::Only(vec!["3.11".to_string(), "3.12".to_string()])
        );
    }

    #[test]
    fn test_python_versions_deserialize_empty_array() {
        let config: EnvironmentConfig = toml::from_str(r"python-versions = []").unwrap();
        assert_eq!(config.python_versions, PythonVersions::None);
    }

    #[test]
    fn test_python_versions_backwards_compat() {
        // python-version (singular) should work for backwards compatibility
        let config: EnvironmentConfig = toml::from_str(r#"python-version = "3.11""#).unwrap();
        assert_eq!(
            config.python_versions,
            PythonVersions::Only(vec!["3.11".to_string()])
        );
    }

    #[test]
    fn test_python_versions_default() {
        let config: EnvironmentConfig = toml::from_str(r"").unwrap();
        assert_eq!(config.python_versions, PythonVersions::Default);
    }

    #[test]
    fn test_python_versions_merge() {
        // Parent sets versions, child uses default -> parent wins
        let parent = TestConfig {
            environment: EnvironmentConfig {
                python_versions: PythonVersions::Only(vec!["3.11".to_string()]),
                ..Default::default()
            },
            ..Default::default()
        };
        let child = TestConfig::default();
        let merged = parent.merge(&child);
        assert_eq!(
            merged.environment.python_versions,
            PythonVersions::Only(vec!["3.11".to_string()])
        );

        // Parent sets versions, child sets different -> child wins
        let child = TestConfig {
            environment: EnvironmentConfig {
                python_versions: PythonVersions::Only(vec!["3.12".to_string()]),
                ..Default::default()
            },
            ..Default::default()
        };
        let merged = parent.merge(&child);
        assert_eq!(
            merged.environment.python_versions,
            PythonVersions::Only(vec!["3.12".to_string()])
        );

        // Parent sets versions, child sets None -> child wins
        let child = TestConfig {
            environment: EnvironmentConfig {
                python_versions: PythonVersions::None,
                ..Default::default()
            },
            ..Default::default()
        };
        let merged = parent.merge(&child);
        assert_eq!(merged.environment.python_versions, PythonVersions::None);
    }

    #[test]
    fn test_required_features_deserialize_string() {
        let config: EnvironmentConfig =
            toml::from_str(r#"required-features = "python-patch""#).unwrap();
        assert_eq!(
            config.required_features,
            RequiredFeatures::Some(vec!["python-patch".to_string()])
        );
    }

    #[test]
    fn test_required_features_deserialize_array() {
        let config: EnvironmentConfig =
            toml::from_str(r#"required-features = ["python-patch", "other-feature"]"#).unwrap();
        assert_eq!(
            config.required_features,
            RequiredFeatures::Some(vec![
                "python-patch".to_string(),
                "other-feature".to_string()
            ])
        );
    }

    #[test]
    fn test_required_features_deserialize_empty_array() {
        let config: EnvironmentConfig = toml::from_str(r"required-features = []").unwrap();
        assert_eq!(config.required_features, RequiredFeatures::None);
    }

    #[test]
    fn test_required_features_default() {
        let config: EnvironmentConfig = toml::from_str(r"").unwrap();
        assert_eq!(config.required_features, RequiredFeatures::None);
    }

    #[test]
    fn test_required_features_all_enabled() {
        // No features required - always passes
        let none = RequiredFeatures::None;
        assert!(none.all_enabled(&[]));
        assert!(none.all_enabled(&["python-patch".to_string()]));

        // Single feature required
        let single = RequiredFeatures::Some(vec!["python-patch".to_string()]);
        assert!(!single.all_enabled(&[]));
        assert!(single.all_enabled(&["python-patch".to_string()]));
        assert!(single.all_enabled(&["python-patch".to_string(), "other".to_string()]));

        // Multiple features required
        let multiple = RequiredFeatures::Some(vec![
            "python-patch".to_string(),
            "other-feature".to_string(),
        ]);
        assert!(!multiple.all_enabled(&[]));
        assert!(!multiple.all_enabled(&["python-patch".to_string()]));
        assert!(multiple.all_enabled(&["python-patch".to_string(), "other-feature".to_string()]));
    }

    #[test]
    fn test_required_features_merge() {
        // Parent has no features, child has no features -> no features
        let parent = TestConfig::default();
        let child = TestConfig::default();
        let merged = parent.merge(&child);
        assert_eq!(merged.environment.required_features, RequiredFeatures::None);

        // Parent has feature, child has none -> parent's feature
        let parent = TestConfig {
            environment: EnvironmentConfig {
                required_features: RequiredFeatures::Some(vec!["python-patch".to_string()]),
                ..Default::default()
            },
            ..Default::default()
        };
        let child = TestConfig::default();
        let merged = parent.merge(&child);
        assert_eq!(
            merged.environment.required_features,
            RequiredFeatures::Some(vec!["python-patch".to_string()])
        );

        // Parent has feature, child has different feature -> both combined
        let child = TestConfig {
            environment: EnvironmentConfig {
                required_features: RequiredFeatures::Some(vec!["other-feature".to_string()]),
                ..Default::default()
            },
            ..Default::default()
        };
        let merged = parent.merge(&child);
        assert_eq!(
            merged.environment.required_features,
            RequiredFeatures::Some(vec![
                "python-patch".to_string(),
                "other-feature".to_string()
            ])
        );

        // Parent and child have same feature -> deduplicated
        let child = TestConfig {
            environment: EnvironmentConfig {
                required_features: RequiredFeatures::Some(vec!["python-patch".to_string()]),
                ..Default::default()
            },
            ..Default::default()
        };
        let merged = parent.merge(&child);
        assert_eq!(
            merged.environment.required_features,
            RequiredFeatures::Some(vec!["python-patch".to_string()])
        );
    }

    #[test]
    fn test_env_remove_deserialize() {
        let config: EnvironmentConfig = toml::from_str(r#"env-remove = ["FOO", "BAR"]"#).unwrap();
        assert_eq!(config.env_remove, vec!["FOO", "BAR"]);
    }

    #[test]
    fn test_env_remove_merge() {
        // Parent sets env_remove, child adds more
        let parent = TestConfig {
            environment: EnvironmentConfig {
                env_remove: vec!["FOO".to_string()],
                ..Default::default()
            },
            ..Default::default()
        };
        let child = TestConfig {
            environment: EnvironmentConfig {
                env_remove: vec!["BAR".to_string()],
                ..Default::default()
            },
            ..Default::default()
        };
        let merged = parent.merge(&child);
        assert_eq!(merged.environment.env_remove, vec!["FOO", "BAR"]);
    }

    #[test]
    fn test_env_and_env_remove_together() {
        // Test that both env and env-remove can be set together
        let config: TestConfig = toml::from_str(
            r#"
            [environment]
            env = { FOO = "bar" }
            env-remove = ["BAZ"]
            "#,
        )
        .unwrap();
        assert_eq!(
            config.environment.env.get("FOO").map(String::as_str),
            Some("bar")
        );
        assert_eq!(config.environment.env_remove, vec!["BAZ"]);
    }
}
