use serde::{Deserialize, Serialize};
use uv_macros::OptionsMetadata;

use crate::{AllowEnv, AllowNet, EnvEntry, EnvPreset, FsEntry, FsPreset, NetEntry};

/// The `[tool.uv.sandbox]` / `[sandbox]` configuration.
///
/// When this section is present and the `sandbox` preview feature is enabled,
/// `uv run` will execute the command in a sandboxed environment that denies
/// all access except what is explicitly permitted.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize, OptionsMetadata)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct SandboxOptions {
    /// Filesystem paths the sandboxed process can read.
    ///
    /// Each entry is either a literal path (string) or a uv-defined preset (object).
    ///
    /// Presets: `project`, `python`, `virtualenv`, `system`, `home`, `uv-cache`, `tmp`.
    #[option(
        default = r#"["@project", "@python", "@system"]"#,
        value_type = r#"list[str | @preset]"#,
        example = r#"
            allow-read = ["@project", "@python", "@system"]
        "#
    )]
    pub allow_read: Option<Vec<FsEntry>>,

    /// Filesystem paths to deny reading, even within allowed paths.
    ///
    /// Deny entries take precedence over allow entries.
    ///
    /// Presets: `known-secrets`.
    #[option(
        default = r#"["@known-secrets"]"#,
        value_type = r#"list[str | @preset]"#,
        example = r#"
            deny-read = ["@known-secrets"]
        "#
    )]
    pub deny_read: Option<Vec<FsEntry>>,

    /// Filesystem paths the sandboxed process can write to. Write implies read.
    ///
    /// Presets: `project`, `virtualenv`, `home`, `tmp`.
    #[option(
        default = r#"["@project", "@tmp"]"#,
        value_type = r#"list[str | @preset]"#,
        example = r#"
            allow-write = ["@project", "@tmp"]
        "#
    )]
    pub allow_write: Option<Vec<FsEntry>>,

    /// Filesystem paths to deny writing, even within allowed paths.
    ///
    /// Presets: `known-secrets`, `shell-configs`, `git-hooks`, `ide-configs`.
    #[option(
        default = r#"["@known-secrets", "@shell-configs", "@git-hooks", "@ide-configs"]"#,
        value_type = r#"list[str | @preset]"#,
        example = r#"
            deny-write = ["@shell-configs", "@git-hooks", ".env"]
        "#
    )]
    pub deny_write: Option<Vec<FsEntry>>,

    /// Filesystem paths the sandboxed process can execute binaries from. Execute implies read.
    ///
    /// Presets: `python`, `system`.
    #[option(
        default = r#"["@python"]"#,
        value_type = r#"list[str | @preset]"#,
        example = r#"
            allow-execute = ["@python"]
        "#
    )]
    pub allow_execute: Option<Vec<FsEntry>>,

    /// Filesystem paths to deny executing from, even within allowed paths.
    #[option(
        default = "[]",
        value_type = r#"list[str | @preset]"#,
        example = r#"
            deny-execute = ["./untrusted"]
        "#
    )]
    pub deny_execute: Option<Vec<FsEntry>>,

    /// Network access control.
    ///
    /// - `false` — deny all network access (default)
    /// - `true` — allow all network access
    /// - List of hosts/presets — allow specific hosts (phase 2)
    #[option(
        default = "false",
        value_type = r#"bool | list[str | @preset]"#,
        example = r#"
            allow-net = false
        "#
    )]
    pub allow_net: Option<AllowNet>,

    /// Network hosts to deny, even if allowed by `allow-net`.
    #[option(
        default = "[]",
        value_type = r#"list[str | @preset]"#,
        example = r#"
            deny-net = ["evil.example.com"]
        "#
    )]
    pub deny_net: Option<Vec<NetEntry>>,

    /// Environment variables visible to the sandboxed process.
    ///
    /// - `false` — deny all env vars
    /// - `true` — pass all env vars
    /// - List of names/presets — pass specific variables
    #[option(
        default = r#"["@standard"]"#,
        value_type = r#"bool | list[str | @preset]"#,
        example = r#"
            allow-env = ["@standard", "DATABASE_URL"]
        "#
    )]
    pub allow_env: Option<AllowEnv>,

    /// Environment variables to hide, even if allowed by `allow-env`.
    ///
    /// Supports prefix wildcards (e.g., `"AWS_*"`).
    ///
    /// Presets: `known-secrets`.
    #[option(
        default = r#"["@known-secrets"]"#,
        value_type = r#"list[str | @preset]"#,
        example = r#"
            deny-env = ["@known-secrets", "AWS_*"]
        "#
    )]
    pub deny_env: Option<Vec<EnvEntry>>,

    /// Fail on unsupported platforms instead of warning and running unsandboxed.
    #[option(
        default = "false",
        value_type = "bool",
        example = r#"
            required = true
        "#
    )]
    pub required: Option<bool>,
}

impl SandboxOptions {
    /// Returns `true` if this configuration is "empty" — i.e., no fields are set.
    pub fn is_empty(&self) -> bool {
        self.allow_read.is_none()
            && self.deny_read.is_none()
            && self.allow_write.is_none()
            && self.deny_write.is_none()
            && self.allow_execute.is_none()
            && self.deny_execute.is_none()
            && self.allow_net.is_none()
            && self.deny_net.is_none()
            && self.allow_env.is_none()
            && self.deny_env.is_none()
            && self.required.is_none()
    }

    /// The default `allow-read` presets, used when `allow-read` is not set.
    ///
    /// Grants read access to the project, the Python installation, and system
    /// libraries — the minimum required to start Python and import project code.
    pub fn default_allow_read() -> Vec<FsEntry> {
        vec![
            FsEntry::Preset {
                preset: FsPreset::Project,
            },
            FsEntry::Preset {
                preset: FsPreset::Python,
            },
            FsEntry::Preset {
                preset: FsPreset::System,
            },
        ]
    }

    /// The default `deny-read` presets, used when `deny-read` is not set.
    ///
    /// Denies read access to known credential stores (`~/.ssh/`, `~/.aws/`,
    /// etc.) to protect secrets from accidental or malicious access.
    pub fn default_deny_read() -> Vec<FsEntry> {
        vec![FsEntry::Preset {
            preset: FsPreset::KnownSecrets,
        }]
    }

    /// The default `allow-write` presets, used when `allow-write` is not set.
    ///
    /// Grants write access to the project directory and temporary directories.
    pub fn default_allow_write() -> Vec<FsEntry> {
        vec![
            FsEntry::Preset {
                preset: FsPreset::Project,
            },
            FsEntry::Preset {
                preset: FsPreset::Tmp,
            },
        ]
    }

    /// The default `deny-write` presets, used when `deny-write` is not set.
    ///
    /// Denies writes to shell configs, git hooks, and IDE configs to prevent
    /// supply-chain persistence attacks.
    pub fn default_deny_write() -> Vec<FsEntry> {
        vec![
            FsEntry::Preset {
                preset: FsPreset::KnownSecrets,
            },
            FsEntry::Preset {
                preset: FsPreset::ShellConfigs,
            },
            FsEntry::Preset {
                preset: FsPreset::GitHooks,
            },
            FsEntry::Preset {
                preset: FsPreset::IdeConfigs,
            },
        ]
    }

    /// The default `allow-execute` presets, used when `allow-execute` is not set.
    ///
    /// Grants execute access to the Python installation and common system
    /// paths (e.g., `/usr/bin`, `/bin`) needed for standard toolchains.
    pub fn default_allow_execute() -> Vec<FsEntry> {
        vec![
            FsEntry::Preset {
                preset: FsPreset::Python,
            },
            FsEntry::Preset {
                preset: FsPreset::System,
            },
        ]
    }

    /// The default `allow-env`, used when `allow-env` is not set.
    ///
    /// Passes through common safe environment variables (PATH, HOME, LANG,
    /// etc.) via the `standard` preset.
    pub fn default_allow_env() -> AllowEnv {
        AllowEnv::List(vec![EnvEntry::Preset {
            preset: EnvPreset::Standard,
        }])
    }

    /// The default `deny-env`, used when `deny-env` is not set.
    ///
    /// Hides known secret environment variables (`AWS_SECRET_ACCESS_KEY`,
    /// `GITHUB_TOKEN`, etc.).
    pub fn default_deny_env() -> Vec<EnvEntry> {
        vec![EnvEntry::Preset {
            preset: EnvPreset::KnownSecrets,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_sandbox_options_toml() {
        let toml_str = r#"
            allow-read = ["@project", "@python", "@system"]
            allow-write = ["@project", "@tmp"]
            deny-write = ["@known-secrets", "@shell-configs", ".env"]
            allow-execute = ["@python", "@system"]
            allow-net = false
            allow-env = ["@standard", "DATABASE_URL"]
            deny-env = ["@known-secrets"]
        "#;

        let options: SandboxOptions = toml::from_str(toml_str).unwrap();

        assert_eq!(options.allow_read.as_ref().unwrap().len(), 3);
        assert_eq!(options.allow_write.as_ref().unwrap().len(), 2);
        assert_eq!(options.deny_write.as_ref().unwrap().len(), 3);
        assert_eq!(options.allow_execute.as_ref().unwrap().len(), 2);
        assert!(matches!(options.allow_net, Some(AllowNet::Bool(false))));
        assert!(matches!(options.allow_env, Some(AllowEnv::List(ref l)) if l.len() == 2));
        assert_eq!(options.deny_env.as_ref().unwrap().len(), 1);
        assert!(options.required.is_none());
    }

    #[test]
    fn deserialize_sandbox_options_allow_net_true() {
        let toml_str = r#"allow-net = true"#;
        let options: SandboxOptions = toml::from_str(toml_str).unwrap();
        assert!(matches!(options.allow_net, Some(AllowNet::Bool(true))));
    }

    #[test]
    fn deserialize_sandbox_options_allow_env_true() {
        let toml_str = r#"
            allow-env = true
            deny-env = ["SECRET_KEY"]
        "#;
        let options: SandboxOptions = toml::from_str(toml_str).unwrap();
        assert!(matches!(options.allow_env, Some(AllowEnv::Bool(true))));
        assert_eq!(options.deny_env.as_ref().unwrap().len(), 1);
    }

    #[test]
    fn deserialize_sandbox_options_required() {
        let toml_str = r#"
            allow-read = ["@project"]
            required = true
        "#;
        let options: SandboxOptions = toml::from_str(toml_str).unwrap();
        assert_eq!(options.required, Some(true));
    }

    #[test]
    fn deserialize_sandbox_options_unknown_field() {
        let toml_str = r#"
            allow-read = [{ preset = "system" }]
            allow-frobulate = true
        "#;
        let result: Result<SandboxOptions, _> = toml::from_str(toml_str);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("unknown field"), "error was: {err}");
    }

    #[test]
    fn deserialize_sandbox_options_unknown_preset() {
        let toml_str = r#"
            allow-read = ["@nonexistent"]
        "#;
        let result: Result<SandboxOptions, _> = toml::from_str(toml_str);
        assert!(result.is_err());
    }

    #[test]
    fn deserialize_sandbox_options_legacy_object_form() {
        let toml_str = r#"
            allow-read = [{ preset = "project" }, { preset = "python" }]
            allow-env = [{ preset = "standard" }, "DATABASE_URL"]
        "#;
        let options: SandboxOptions = toml::from_str(toml_str).unwrap();
        assert_eq!(options.allow_read.as_ref().unwrap().len(), 2);
        assert_eq!(
            options.allow_read.as_ref().unwrap()[0],
            FsEntry::Preset {
                preset: FsPreset::Project,
            }
        );
        if let Some(AllowEnv::List(ref entries)) = options.allow_env {
            assert_eq!(entries.len(), 2);
            assert_eq!(
                entries[0],
                EnvEntry::Preset {
                    preset: EnvPreset::Standard,
                }
            );
        } else {
            panic!("expected AllowEnv::List");
        }
    }

    #[test]
    fn sandbox_options_is_empty() {
        let options = SandboxOptions::default();
        assert!(options.is_empty());

        let options: SandboxOptions = toml::from_str(r#"allow-net = false"#).unwrap();
        assert!(!options.is_empty());
    }
}
