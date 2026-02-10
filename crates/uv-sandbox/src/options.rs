use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize};

/// A filesystem permission entry: literal path or preset.
///
/// In TOML:
/// - `"/opt/data"` — literal path
/// - `"@system"` — uv-defined preset (`@`-prefixed)
/// - `{ preset = "system" }` — uv-defined preset (legacy object form)
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum FsEntry {
    /// A literal filesystem path.
    Path(String),
    /// A uv-defined preset that expands to one or more paths.
    Preset { preset: FsPreset },
}

impl<'de> Deserialize<'de> for FsEntry {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            String(String),
            Preset { preset: FsPreset },
        }
        let raw = Raw::deserialize(deserializer)?;
        match raw {
            Raw::String(s) => {
                if let Some(name) = s.strip_prefix('@') {
                    let preset = FsPreset::from_str(name).ok_or_else(|| {
                        de::Error::custom(format!("unknown filesystem preset `@{name}`"))
                    })?;
                    Ok(FsEntry::Preset { preset })
                } else {
                    Ok(FsEntry::Path(s))
                }
            }
            Raw::Preset { preset } => Ok(FsEntry::Preset { preset }),
        }
    }
}

/// Named filesystem presets.
///
/// Some presets are intended for `allow-*` fields (like `project`, `python`, `system`),
/// while others are intended for `deny-*` fields (like `known-secrets`, `shell-configs`).
/// The type system does not enforce this distinction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "kebab-case")]
pub enum FsPreset {
    /// The project root directory.
    Project,
    /// Python interpreter, stdlib, and site-packages.
    Python,
    /// The project virtual environment.
    Virtualenv,
    /// Common system libraries and resources.
    System,
    /// The user's home directory.
    Home,
    /// The uv cache directory.
    UvCache,
    /// Temporary directories.
    Tmp,
    /// Credential files: `~/.ssh/`, `~/.gnupg/`, `~/.aws/`, etc.
    KnownSecrets,
    /// Shell config files: `.bashrc`, `.zshrc`, `.profile`, etc.
    ShellConfigs,
    /// Git hook and config files: `.git/hooks/`, `.git/config`, etc.
    GitHooks,
    /// IDE/editor config directories: `.vscode/`, `.idea/`.
    IdeConfigs,
}

impl FsPreset {
    /// Parse a kebab-case preset name, returning `None` for unknown names.
    fn from_str(s: &str) -> Option<Self> {
        match s {
            "project" => Some(Self::Project),
            "python" => Some(Self::Python),
            "virtualenv" => Some(Self::Virtualenv),
            "system" => Some(Self::System),
            "home" => Some(Self::Home),
            "uv-cache" => Some(Self::UvCache),
            "tmp" => Some(Self::Tmp),
            "known-secrets" => Some(Self::KnownSecrets),
            "shell-configs" => Some(Self::ShellConfigs),
            "git-hooks" => Some(Self::GitHooks),
            "ide-configs" => Some(Self::IdeConfigs),
            _ => None,
        }
    }
}

/// An environment variable permission entry: literal name or preset.
///
/// In TOML:
/// - `"DATABASE_URL"` — literal variable name
/// - `"AWS_*"` — prefix wildcard
/// - `"@standard"` — uv-defined preset (`@`-prefixed)
/// - `{ preset = "standard" }` — uv-defined preset (legacy object form)
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum EnvEntry {
    /// A literal environment variable name. Supports `*` suffix wildcards.
    Name(String),
    /// A uv-defined preset that expands to a set of variable names.
    Preset { preset: EnvPreset },
}

impl<'de> Deserialize<'de> for EnvEntry {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            String(String),
            Preset { preset: EnvPreset },
        }
        let raw = Raw::deserialize(deserializer)?;
        match raw {
            Raw::String(s) => {
                if let Some(name) = s.strip_prefix('@') {
                    let preset = EnvPreset::from_str(name).ok_or_else(|| {
                        de::Error::custom(format!("unknown environment preset `@{name}`"))
                    })?;
                    Ok(EnvEntry::Preset { preset })
                } else {
                    Ok(EnvEntry::Name(s))
                }
            }
            Raw::Preset { preset } => Ok(EnvEntry::Preset { preset }),
        }
    }
}

/// Named environment variable presets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "kebab-case")]
pub enum EnvPreset {
    /// Common safe variables: `PATH`, `HOME`, `LANG`, `TERM`, `VIRTUAL_ENV`, etc.
    Standard,
    /// Common secret variable patterns: `AWS_SECRET_ACCESS_KEY`, `GITHUB_TOKEN`, etc.
    KnownSecrets,
}

impl EnvPreset {
    /// Parse a kebab-case preset name, returning `None` for unknown names.
    fn from_str(s: &str) -> Option<Self> {
        match s {
            "standard" => Some(Self::Standard),
            "known-secrets" => Some(Self::KnownSecrets),
            _ => None,
        }
    }
}

/// A network host permission entry: literal hostname or preset.
///
/// In TOML:
/// - `"example.com"` — literal hostname
/// - `"*.github.com"` — wildcard domain
/// - `"@pypi"` — uv-defined preset (`@`-prefixed)
/// - `{ preset = "pypi" }` — uv-defined preset (legacy object form)
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum NetEntry {
    /// A literal hostname, optionally with port. Supports `*` prefix wildcards.
    Host(String),
    /// A uv-defined preset that expands to a set of hostnames.
    Preset { preset: NetPreset },
}

impl<'de> Deserialize<'de> for NetEntry {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            String(String),
            Preset { preset: NetPreset },
        }
        let raw = Raw::deserialize(deserializer)?;
        match raw {
            Raw::String(s) => {
                if let Some(name) = s.strip_prefix('@') {
                    let preset = NetPreset::from_str(name).ok_or_else(|| {
                        de::Error::custom(format!("unknown network preset `@{name}`"))
                    })?;
                    Ok(NetEntry::Preset { preset })
                } else {
                    Ok(NetEntry::Host(s))
                }
            }
            Raw::Preset { preset } => Ok(NetEntry::Preset { preset }),
        }
    }
}

/// Named network presets (phase 2: domain-level filtering).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "kebab-case")]
pub enum NetPreset {
    /// `pypi.org`, `files.pythonhosted.org`.
    Pypi,
    /// `github.com`, `*.github.com`, `api.github.com`, etc.
    Github,
    /// `registry.npmjs.org`, `*.npmjs.org`.
    Npm,
    /// `jsr.io`, `*.jsr.io`.
    Jsr,
}

impl NetPreset {
    /// Parse a kebab-case preset name, returning `None` for unknown names.
    fn from_str(s: &str) -> Option<Self> {
        match s {
            "pypi" => Some(Self::Pypi),
            "github" => Some(Self::Github),
            "npm" => Some(Self::Npm),
            "jsr" => Some(Self::Jsr),
            _ => None,
        }
    }
}

/// Network access control.
///
/// - `false` — deny all network access (default)
/// - `true` — allow all network access
/// - `["example.com", { preset = "pypi" }]` — allow specific hosts (phase 2)
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(untagged)]
pub enum AllowNet {
    /// `true` = allow all, `false` = deny all.
    Bool(bool),
    /// Allow specific hosts/presets (phase 2).
    Hosts(Vec<NetEntry>),
}

impl Default for AllowNet {
    fn default() -> Self {
        Self::Bool(false)
    }
}

/// Environment variable access control.
///
/// - `false` — pass no environment variables (default)
/// - `true` — pass all environment variables
/// - `[{ preset = "standard" }, "DATABASE_URL"]` — pass specific variables/presets
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(untagged)]
pub enum AllowEnv {
    /// `true` = pass all, `false` = pass none.
    Bool(bool),
    /// Pass specific variables/presets.
    List(Vec<EnvEntry>),
}

impl Default for AllowEnv {
    fn default() -> Self {
        Self::Bool(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- FsEntry ---

    #[test]
    fn deserialize_fs_entry_path() {
        let entry: FsEntry = serde_json::from_str(r#""/opt/data""#).unwrap();
        assert_eq!(entry, FsEntry::Path("/opt/data".to_string()));
    }

    #[test]
    fn deserialize_fs_entry_at_preset() {
        let entry: FsEntry = serde_json::from_str(r#""@system""#).unwrap();
        assert_eq!(
            entry,
            FsEntry::Preset {
                preset: FsPreset::System
            }
        );
    }

    #[test]
    fn deserialize_fs_entry_at_preset_kebab_case() {
        let entry: FsEntry = serde_json::from_str(r#""@known-secrets""#).unwrap();
        assert_eq!(
            entry,
            FsEntry::Preset {
                preset: FsPreset::KnownSecrets
            }
        );
    }

    #[test]
    fn deserialize_fs_entry_at_preset_unknown() {
        let result: Result<FsEntry, _> = serde_json::from_str(r#""@nonexistent""#);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("unknown filesystem preset"),
            "error was: {err}"
        );
    }

    #[test]
    fn deserialize_fs_entry_legacy_object() {
        let entry: FsEntry = serde_json::from_str(r#"{"preset": "system"}"#).unwrap();
        assert_eq!(
            entry,
            FsEntry::Preset {
                preset: FsPreset::System
            }
        );
    }

    #[test]
    fn deserialize_fs_entry_legacy_object_invalid() {
        let result: Result<FsEntry, _> =
            serde_json::from_str(r#"{"preset": "nonexistent-preset"}"#);
        assert!(result.is_err());
    }

    // --- EnvEntry ---

    #[test]
    fn deserialize_env_entry_name() {
        let entry: EnvEntry = serde_json::from_str(r#""DATABASE_URL""#).unwrap();
        assert_eq!(entry, EnvEntry::Name("DATABASE_URL".to_string()));
    }

    #[test]
    fn deserialize_env_entry_at_preset() {
        let entry: EnvEntry = serde_json::from_str(r#""@standard""#).unwrap();
        assert_eq!(
            entry,
            EnvEntry::Preset {
                preset: EnvPreset::Standard
            }
        );
    }

    #[test]
    fn deserialize_env_entry_at_preset_unknown() {
        let result: Result<EnvEntry, _> = serde_json::from_str(r#""@nonexistent""#);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("unknown environment preset"),
            "error was: {err}"
        );
    }

    #[test]
    fn deserialize_env_entry_legacy_object() {
        let entry: EnvEntry = serde_json::from_str(r#"{"preset": "standard"}"#).unwrap();
        assert_eq!(
            entry,
            EnvEntry::Preset {
                preset: EnvPreset::Standard
            }
        );
    }

    // --- NetEntry ---

    #[test]
    fn deserialize_net_entry_host() {
        let entry: NetEntry = serde_json::from_str(r#""example.com""#).unwrap();
        assert_eq!(entry, NetEntry::Host("example.com".to_string()));
    }

    #[test]
    fn deserialize_net_entry_at_preset() {
        let entry: NetEntry = serde_json::from_str(r#""@pypi""#).unwrap();
        assert_eq!(
            entry,
            NetEntry::Preset {
                preset: NetPreset::Pypi
            }
        );
    }

    #[test]
    fn deserialize_net_entry_at_preset_unknown() {
        let result: Result<NetEntry, _> = serde_json::from_str(r#""@nonexistent""#);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("unknown network preset"), "error was: {err}");
    }

    #[test]
    fn deserialize_net_entry_legacy_object() {
        let entry: NetEntry = serde_json::from_str(r#"{"preset": "pypi"}"#).unwrap();
        assert_eq!(
            entry,
            NetEntry::Preset {
                preset: NetPreset::Pypi
            }
        );
    }

    // --- AllowNet / AllowEnv ---

    #[test]
    fn deserialize_allow_net_bool() {
        let net: AllowNet = serde_json::from_str("false").unwrap();
        assert!(matches!(net, AllowNet::Bool(false)));

        let net: AllowNet = serde_json::from_str("true").unwrap();
        assert!(matches!(net, AllowNet::Bool(true)));
    }

    #[test]
    fn deserialize_allow_net_hosts_with_at_preset() {
        let net: AllowNet = serde_json::from_str(r#"["example.com", "@pypi"]"#).unwrap();
        if let AllowNet::Hosts(hosts) = net {
            assert_eq!(hosts.len(), 2);
            assert_eq!(hosts[0], NetEntry::Host("example.com".to_string()));
            assert_eq!(
                hosts[1],
                NetEntry::Preset {
                    preset: NetPreset::Pypi
                }
            );
        } else {
            panic!("expected Hosts variant");
        }
    }

    #[test]
    fn deserialize_allow_env_bool() {
        let env: AllowEnv = serde_json::from_str("true").unwrap();
        assert!(matches!(env, AllowEnv::Bool(true)));
    }

    #[test]
    fn deserialize_allow_env_list_with_at_preset() {
        let env: AllowEnv = serde_json::from_str(r#"["@standard", "DATABASE_URL"]"#).unwrap();
        if let AllowEnv::List(entries) = env {
            assert_eq!(entries.len(), 2);
            assert_eq!(
                entries[0],
                EnvEntry::Preset {
                    preset: EnvPreset::Standard
                }
            );
            assert_eq!(entries[1], EnvEntry::Name("DATABASE_URL".to_string()));
        } else {
            panic!("expected List variant");
        }
    }
}
