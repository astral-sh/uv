//! Expand symbolic presets into concrete filesystem paths, env var names, etc.

use std::path::{Path, PathBuf};

use crate::options::{EnvEntry, EnvPreset, FsEntry, FsPreset};

/// Context needed to expand presets into concrete paths.
pub struct PresetContext {
    /// The project root directory (where pyproject.toml lives).
    pub project_root: PathBuf,
    /// The Python interpreter path (`sys.executable`).
    pub python: PathBuf,
    /// The base Python installation prefix (`sys.base_prefix`).
    ///
    /// For virtual environments, this points to the original CPython
    /// installation (e.g. `~/.local/share/uv/python/cpython-3.12.x-*/`),
    /// which contains shared libraries like `libpython3.12.dylib`.
    pub python_prefix: PathBuf,
    /// Python stdlib directory.
    pub stdlib: PathBuf,
    /// The virtual environment directory (`.venv` or similar).
    pub virtualenv: PathBuf,
    /// The user's home directory.
    pub home: PathBuf,
    /// The uv cache directory.
    pub cache_dir: PathBuf,
}

impl PresetContext {
    /// Expand a filesystem preset into concrete paths.
    pub fn expand_fs_preset(&self, preset: FsPreset) -> Vec<PathBuf> {
        match preset {
            FsPreset::Project => vec![self.project_root.clone()],
            FsPreset::Python => {
                let mut paths = vec![
                    self.python.clone(),
                    self.stdlib.clone(),
                    // The base Python installation prefix includes shared
                    // libraries (e.g. libpython3.12.dylib) that the
                    // interpreter needs at runtime.
                    self.python_prefix.clone(),
                ];
                // Include the site-packages from the virtualenv.
                if self.virtualenv.exists() {
                    paths.push(self.virtualenv.clone());
                }
                paths
            }
            FsPreset::Virtualenv => vec![self.virtualenv.clone()],
            FsPreset::System => system_read_paths(),
            FsPreset::Home => vec![self.home.clone()],
            FsPreset::UvCache => vec![self.cache_dir.clone()],
            FsPreset::Tmp => tmp_paths(),
            FsPreset::KnownSecrets => known_secret_paths(&self.home),
            FsPreset::ShellConfigs => shell_config_paths(&self.home),
            FsPreset::GitHooks => {
                // .git/hooks in the project root.
                vec![self.project_root.join(".git").join("hooks")]
            }
            FsPreset::IdeConfigs => ide_config_paths(&self.project_root),
        }
    }

    /// Expand a filesystem entry into concrete paths.
    pub fn expand_fs_entry(&self, entry: &FsEntry) -> Vec<PathBuf> {
        match entry {
            FsEntry::Path(path) => {
                let path = Path::new(path);
                if path.is_absolute() {
                    vec![path.to_path_buf()]
                } else {
                    // Relative paths are resolved against the project root.
                    vec![self.project_root.join(path)]
                }
            }
            FsEntry::Preset { preset } => self.expand_fs_preset(*preset),
        }
    }

    /// Expand a list of filesystem entries into concrete paths.
    pub fn expand_fs_entries(&self, entries: &[FsEntry]) -> Vec<PathBuf> {
        entries
            .iter()
            .flat_map(|entry| self.expand_fs_entry(entry))
            .collect()
    }
}

/// Standard environment variable names in the `standard` preset.
pub const STANDARD_ENV_VARS: &[&str] = &[
    "HOME",
    "USER",
    "LOGNAME",
    "SHELL",
    "PATH",
    "LANG",
    "LC_ALL",
    "LC_CTYPE",
    "LC_MESSAGES",
    "TERM",
    "COLORTERM",
    "TMPDIR",
    "TMP",
    "TEMP",
    "VIRTUAL_ENV",
    "PYTHONPATH",
    "PYTHONDONTWRITEBYTECODE",
    "PYTHONUNBUFFERED",
    "PYTHONHASHSEED",
    "PYTHONIOENCODING",
    "UV_CACHE_DIR",
    "UV_PYTHON",
    "XDG_CONFIG_HOME",
    "XDG_CACHE_HOME",
    "XDG_DATA_HOME",
    "XDG_RUNTIME_DIR",
    "NO_COLOR",
    "FORCE_COLOR",
    "CLICOLOR",
    "CLICOLOR_FORCE",
];

/// Known secret environment variable names/patterns.
pub const KNOWN_SECRET_ENV_PATTERNS: &[&str] = &[
    "AWS_SECRET_ACCESS_KEY",
    "AWS_SESSION_TOKEN",
    "GITHUB_TOKEN",
    "GH_TOKEN",
    "GITLAB_TOKEN",
    "NPM_TOKEN",
    "PYPI_TOKEN",
    "CARGO_REGISTRY_TOKEN",
    "DOCKER_PASSWORD",
    "DOCKER_TOKEN",
    "SSH_AUTH_SOCK",
    "GPG_AGENT_INFO",
    "OPENAI_API_KEY",
    "ANTHROPIC_API_KEY",
    "GOOGLE_API_KEY",
    // Azure — only known secret/credential variables, not all AZURE_* (which
    // includes non-secret config like AZURE_REGION, AZURE_SUBSCRIPTION_NAME).
    "AZURE_CLIENT_SECRET",
    "AZURE_CLIENT_CERTIFICATE_PASSWORD",
    "AZURE_FEDERATED_TOKEN",
    "AZURE_TENANT_ID",
    "AZURE_PASSWORD",
    // Heroku — only API key/token variables.
    "HEROKU_API_KEY",
    "HEROKU_API_TOKEN",
    // Stripe — secret keys only.
    "STRIPE_SECRET_KEY",
    "STRIPE_API_KEY",
    // Twilio — auth token only.
    "TWILIO_AUTH_TOKEN",
    // SendGrid — API key only.
    "SENDGRID_API_KEY",
    // Slack — tokens/secrets only.
    "SLACK_TOKEN",
    "SLACK_BOT_TOKEN",
    "SLACK_SIGNING_SECRET",
    // Discord — tokens only.
    "DISCORD_TOKEN",
    "DISCORD_BOT_TOKEN",
    // DigitalOcean — access token only.
    "DIGITALOCEAN_ACCESS_TOKEN",
    // Cloudflare — API tokens/keys only.
    "CLOUDFLARE_API_TOKEN",
    "CLOUDFLARE_API_KEY",
    "DATABASE_URL",
    "DATABASE_PASSWORD",
    "DB_PASSWORD",
    "REDIS_URL",
    "REDIS_PASSWORD",
    "MONGODB_URI",
    "MYSQL_PASSWORD",
    "POSTGRES_PASSWORD",
    "PGPASSWORD",
];

/// Check if an env var name matches a pattern (supports `*` suffix wildcard).
pub fn env_var_matches_pattern(var_name: &str, pattern: &str) -> bool {
    if let Some(prefix) = pattern.strip_suffix('*') {
        var_name.starts_with(prefix)
    } else {
        var_name == pattern
    }
}

/// Resolve which environment variables should be passed through.
///
/// Returns `None` if all variables should be passed (full env).
/// Returns `Some(vec)` with the allowed variable names.
pub fn resolve_env_filter(
    allow: &[EnvEntry],
    deny: &[EnvEntry],
    current_env: &[(String, String)],
) -> Vec<(String, String)> {
    let mut result = Vec::new();

    for (name, value) in current_env {
        let allowed = allow.iter().any(|entry| match entry {
            EnvEntry::Name(pattern) => env_var_matches_pattern(name, pattern),
            EnvEntry::Preset { preset } => match preset {
                EnvPreset::Standard => STANDARD_ENV_VARS.contains(&name.as_str()),
                EnvPreset::KnownSecrets => KNOWN_SECRET_ENV_PATTERNS
                    .iter()
                    .any(|p| env_var_matches_pattern(name, p)),
            },
        });

        if !allowed {
            continue;
        }

        let denied = deny.iter().any(|entry| match entry {
            EnvEntry::Name(pattern) => env_var_matches_pattern(name, pattern),
            EnvEntry::Preset { preset } => match preset {
                EnvPreset::Standard => STANDARD_ENV_VARS.contains(&name.as_str()),
                EnvPreset::KnownSecrets => KNOWN_SECRET_ENV_PATTERNS
                    .iter()
                    .any(|p| env_var_matches_pattern(name, p)),
            },
        });

        if !denied {
            result.push((name.clone(), value.clone()));
        }
    }

    result
}

/// Common system read paths.
fn system_read_paths() -> Vec<PathBuf> {
    let mut paths = vec![
        PathBuf::from("/usr/lib"),
        PathBuf::from("/usr/bin"),
        PathBuf::from("/usr/share"),
        PathBuf::from("/etc"),
    ];

    #[cfg(target_os = "linux")]
    {
        paths.push(PathBuf::from("/lib"));
        paths.push(PathBuf::from("/lib64"));
        paths.push(PathBuf::from("/usr/lib64"));
        paths.push(PathBuf::from("/bin"));
        paths.push(PathBuf::from("/sbin"));
        paths.push(PathBuf::from("/usr/sbin"));
        paths.push(PathBuf::from("/usr/local/bin"));
        paths.push(PathBuf::from("/proc/self"));
        paths.push(PathBuf::from("/dev/null"));
        paths.push(PathBuf::from("/dev/urandom"));
        paths.push(PathBuf::from("/dev/random"));
    }

    #[cfg(target_os = "macos")]
    {
        // Base system binaries (e.g., `/bin/sh`) are required to execute common
        // shebang wrappers generated by Python entry points.
        paths.push(PathBuf::from("/bin"));
        paths.push(PathBuf::from("/sbin"));
        paths.push(PathBuf::from("/usr/sbin"));

        paths.push(PathBuf::from("/usr/local/lib"));
        paths.push(PathBuf::from("/usr/local/bin"));
        paths.push(PathBuf::from("/Library/Frameworks"));
        paths.push(PathBuf::from("/System/Library"));
        paths.push(PathBuf::from("/dev/null"));
        paths.push(PathBuf::from("/dev/urandom"));
        paths.push(PathBuf::from("/dev/random"));
        // Homebrew paths.
        paths.push(PathBuf::from("/opt/homebrew/lib"));
        paths.push(PathBuf::from("/opt/homebrew/bin"));
        paths.push(PathBuf::from("/opt/homebrew/Cellar"));
    }

    paths
}

/// Temp directory paths.
///
/// Includes `/tmp` (or `/private/tmp` on macOS) and `$TMPDIR` if set.
/// On macOS, `$TMPDIR` is typically `/var/folders/.../T/` which is distinct
/// from `/tmp`.
fn tmp_paths() -> Vec<PathBuf> {
    let mut paths = Vec::with_capacity(3);

    paths.push(PathBuf::from("/tmp"));

    #[cfg(target_os = "macos")]
    {
        // `/tmp` is a symlink to `/private/tmp` on macOS; allow both so the
        // Seatbelt profile works regardless of whether the path is resolved.
        paths.push(PathBuf::from("/private/tmp"));
    }

    if let Ok(tmpdir) = std::env::var("TMPDIR") {
        let p = PathBuf::from(tmpdir);
        if !paths.contains(&p) {
            paths.push(p);
        }
    }

    paths
}

/// Known credential/secret file paths under home.
fn known_secret_paths(home: &Path) -> Vec<PathBuf> {
    let mut paths = vec![
        // SSH
        home.join(".ssh"),
        // GPG
        home.join(".gnupg"),
        home.join(".gpg"),
        // Cloud providers
        home.join(".aws"),
        home.join(".azure"),
        home.join(".gcloud"),
        home.join(".config/gcloud"),
        // Docker
        home.join(".docker/config.json"),
        // Package registries
        home.join(".npmrc"),
        home.join(".pypirc"),
        // General credentials
        home.join(".netrc"),
        home.join(".git-credentials"),
        home.join(".cargo/credentials"),
        home.join(".cargo/credentials.toml"),
        // uv's own credential store (typically ~/.local/share/uv/credentials/)
        home.join(".local/share/uv/credentials"),
    ];

    // Also include the XDG-based uv credential directory if it differs from
    // the default (~/.local/share/uv/credentials).
    if let Ok(xdg_data) = std::env::var("XDG_DATA_HOME") {
        let xdg_creds = PathBuf::from(xdg_data).join("uv/credentials");
        if !paths.contains(&xdg_creds) {
            paths.push(xdg_creds);
        }
    }

    paths
}

/// Shell config file paths.
fn shell_config_paths(home: &Path) -> Vec<PathBuf> {
    vec![
        home.join(".bashrc"),
        home.join(".bash_profile"),
        home.join(".bash_login"),
        home.join(".profile"),
        home.join(".zshrc"),
        home.join(".zshenv"),
        home.join(".zprofile"),
        home.join(".zlogin"),
        home.join(".config/fish/config.fish"),
    ]
}

/// IDE/editor config paths within a project.
fn ide_config_paths(project_root: &Path) -> Vec<PathBuf> {
    vec![
        project_root.join(".vscode"),
        project_root.join(".idea"),
        project_root.join(".vim"),
        project_root.join(".nvim"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_env_var_matches_pattern_exact() {
        assert!(env_var_matches_pattern("HOME", "HOME"));
        assert!(!env_var_matches_pattern("HOME", "PATH"));
    }

    #[test]
    fn test_env_var_matches_pattern_wildcard() {
        assert!(env_var_matches_pattern("AWS_ACCESS_KEY_ID", "AWS_*"));
        assert!(env_var_matches_pattern("AWS_SECRET_ACCESS_KEY", "AWS_*"));
        assert!(!env_var_matches_pattern("HOME", "AWS_*"));
    }

    #[test]
    fn test_resolve_env_filter_standard() {
        let allow = vec![EnvEntry::Preset {
            preset: EnvPreset::Standard,
        }];
        let deny = vec![];
        let env = vec![
            ("HOME".to_string(), "/home/user".to_string()),
            ("PATH".to_string(), "/usr/bin".to_string()),
            ("SECRET_KEY".to_string(), "hidden".to_string()),
        ];

        let result = resolve_env_filter(&allow, &deny, &env);
        assert_eq!(result.len(), 2);
        assert!(result.iter().any(|(k, _)| k == "HOME"));
        assert!(result.iter().any(|(k, _)| k == "PATH"));
        assert!(!result.iter().any(|(k, _)| k == "SECRET_KEY"));
    }

    #[test]
    fn test_resolve_env_filter_deny_overrides_allow() {
        let allow = vec![EnvEntry::Preset {
            preset: EnvPreset::Standard,
        }];
        let deny = vec![EnvEntry::Name("HOME".to_string())];
        let env = vec![
            ("HOME".to_string(), "/home/user".to_string()),
            ("PATH".to_string(), "/usr/bin".to_string()),
        ];

        let result = resolve_env_filter(&allow, &deny, &env);
        assert_eq!(result.len(), 1);
        assert!(result.iter().any(|(k, _)| k == "PATH"));
    }

    #[test]
    fn test_resolve_env_filter_deny_wildcard() {
        let _allow = [EnvEntry::Preset {
            preset: EnvPreset::Standard,
        }];
        // This won't match any standard vars, but let's test with full env.
        let allow_all = vec![
            EnvEntry::Preset {
                preset: EnvPreset::Standard,
            },
            EnvEntry::Name("AWS_*".to_string()),
        ];
        let deny = vec![EnvEntry::Name("AWS_*".to_string())];
        let env = vec![
            ("HOME".to_string(), "/home/user".to_string()),
            ("AWS_ACCESS_KEY_ID".to_string(), "AKIA".to_string()),
            ("AWS_SECRET_ACCESS_KEY".to_string(), "secret".to_string()),
        ];

        let result = resolve_env_filter(&allow_all, &deny, &env);
        assert_eq!(result.len(), 1);
        assert!(result.iter().any(|(k, _)| k == "HOME"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_system_paths_include_bin_for_shebang_wrappers() {
        let paths = system_read_paths();
        assert!(
            paths.iter().any(|path| path == &PathBuf::from("/bin")),
            "@system should include /bin so scripts like '#!/bin/sh' can run"
        );
    }
}
