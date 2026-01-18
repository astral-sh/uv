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
    /// Files to create before running commands.
    pub files: Vec<EmbeddedFile>,
    /// Commands to execute in order.
    pub commands: Vec<Command>,
    /// Files to snapshot after commands run.
    pub file_snapshots: Vec<FileSnapshot>,
    /// Line number in the source file where this test starts (for error reporting).
    pub line_number: usize,
}

impl MarkdownTest {
    /// Check if this test should run on the current platform.
    ///
    /// Returns `true` if both `target-os` and `target-family` match the current system.
    #[must_use]
    pub fn should_run(&self) -> bool {
        self.config.environment.target_os.matches_current()
            && self.config.environment.target_family.matches_current()
    }
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
}

/// Environment configuration for tests.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct EnvironmentConfig {
    /// Python version to use (e.g., "3.12").
    pub python_version: Option<String>,
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
            TargetOs::All => true,
            TargetOs::Only(os_list) => {
                let current = uv_platform::Os::from_env();
                os_list.iter().any(|os| *os == current)
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
            TargetFamily::All => true,
            TargetFamily::Only(families) => families.iter().any(|f| is_current_family(f)),
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
                python_version: other
                    .environment
                    .python_version
                    .clone()
                    .or_else(|| self.environment.python_version.clone()),
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
                    other_os => other_os.clone(),
                },
                target_family: match &other.environment.target_family {
                    TargetFamily::All => self.environment.target_family.clone(),
                    other_family => other_family.clone(),
                },
                env: {
                    let mut env = self.environment.env.clone();
                    env.extend(other.environment.env.clone());
                    env
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
    fn test_config_merge() {
        let base = TestConfig {
            environment: EnvironmentConfig {
                python_version: Some("3.11".to_string()),
                exclude_newer: Some("2024-01-01".to_string()),
                env: HashMap::from([("FOO".to_string(), "bar".to_string())]),
                ..Default::default()
            },
            ..Default::default()
        };

        let override_config = TestConfig {
            environment: EnvironmentConfig {
                python_version: Some("3.12".to_string()),
                exclude_newer: None,
                env: HashMap::from([("BAZ".to_string(), "qux".to_string())]),
                ..Default::default()
            },
            ..Default::default()
        };

        let merged = base.merge(&override_config);
        assert_eq!(merged.environment.python_version.as_deref(), Some("3.12"));
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
}
