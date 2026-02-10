//! Resolve sandbox options from config into a concrete [`SandboxSpec`].

use std::fmt;

use crate::options::{AllowEnv, AllowNet};
use crate::presets::{PresetContext, resolve_env_filter};
use crate::settings::SandboxOptions;
use crate::spec::SandboxSpec;

/// Error returned when sandbox options cannot be resolved.
#[derive(Debug)]
pub enum ResolveError {
    /// A feature is configured but not yet implemented.
    Unsupported(String),
}

impl fmt::Display for ResolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for ResolveError {}

/// Build a [`SandboxSpec`] from [`SandboxOptions`] and runtime context.
///
/// This expands all presets, resolves relative paths, filters environment
/// variables, and produces a fully concrete spec ready for the platform
/// sandbox implementation.
///
/// When a field is `None` (not set by the user), sensible defaults are applied:
///
/// - `allow-read`: `[project, python, system]`
/// - `deny-read`: `[known-secrets]`
/// - `allow-write`: `[project, tmp]`
/// - `deny-write`: `[known-secrets, shell-configs, git-hooks, ide-configs]`
/// - `allow-execute`: `[python]`
/// - `deny-execute`: `[]`
/// - `allow-net`: `false`
/// - `allow-env`: `[standard]`
/// - `deny-env`: `[known-secrets]`
///
/// This means an empty `[tool.uv.sandbox]` section produces a usable sandbox
/// that can run Python with project access, while blocking credential access,
/// network, and writes to sensitive paths.
///
/// Returns an error if the options use features that are not yet implemented
/// (e.g., per-host network filtering), rather than silently granting more
/// access than requested.
pub fn resolve_sandbox_spec(
    options: &SandboxOptions,
    context: &PresetContext,
) -> Result<SandboxSpec, ResolveError> {
    let default_allow_read = SandboxOptions::default_allow_read();
    let allow_read =
        context.expand_fs_entries(options.allow_read.as_deref().unwrap_or(&default_allow_read));

    let default_deny_read = SandboxOptions::default_deny_read();
    let deny_read =
        context.expand_fs_entries(options.deny_read.as_deref().unwrap_or(&default_deny_read));

    let default_allow_write = SandboxOptions::default_allow_write();
    let allow_write = context.expand_fs_entries(
        options
            .allow_write
            .as_deref()
            .unwrap_or(&default_allow_write),
    );

    let default_deny_write = SandboxOptions::default_deny_write();
    let deny_write =
        context.expand_fs_entries(options.deny_write.as_deref().unwrap_or(&default_deny_write));

    let default_allow_execute = SandboxOptions::default_allow_execute();
    let allow_execute = context.expand_fs_entries(
        options
            .allow_execute
            .as_deref()
            .unwrap_or(&default_allow_execute),
    );

    let deny_execute = options
        .deny_execute
        .as_deref()
        .map(|entries| context.expand_fs_entries(entries))
        .unwrap_or_default();

    // Per-host network filtering is not yet implemented (phase 2). Reject
    // host lists rather than silently granting unrestricted network access.
    if let Some(AllowNet::Hosts(_)) = &options.allow_net {
        return Err(ResolveError::Unsupported(
            "Per-host network filtering (`allow-net` with a host list) is not yet implemented. \
             Use `allow-net = true` or `allow-net = false` instead."
                .to_string(),
        ));
    }

    // `deny-net` is not yet implemented. Reject it rather than silently
    // ignoring the user's intent to block specific hosts.
    if options.deny_net.is_some() {
        return Err(ResolveError::Unsupported(
            "`deny-net` is not yet implemented. \
             Use `allow-net = false` to deny all network access instead."
                .to_string(),
        ));
    }

    let allow_net = match &options.allow_net {
        Some(AllowNet::Bool(val)) => *val,
        Some(AllowNet::Hosts(_)) => unreachable!("handled above"),
        None => false,
    };

    let env = resolve_env(options);

    Ok(SandboxSpec {
        allow_read,
        deny_read,
        allow_write,
        deny_write,
        allow_execute,
        deny_execute,
        allow_net,
        env,
    })
}

/// Resolve environment variable filtering using the parent's environment.
///
/// Returns `None` if all env vars should be passed through.
/// Returns `Some(vec)` with the filtered list.
fn resolve_env(options: &SandboxOptions) -> Option<Vec<(String, String)>> {
    let current_env: Vec<_> = std::env::vars().collect();
    resolve_env_impl(options, &current_env)
}

/// Resolve environment variable filtering against a caller-provided environment.
///
/// This allows the caller to pass in the effective environment of the child
/// process (which may differ from the parent's `std::env::vars()` due to
/// `Command::env()` calls made after initial sandbox resolution).
///
/// Returns `None` if all env vars should be passed through (e.g.,
/// `allow-env = true` with no deny list). Returns `Some(vec)` with the
/// filtered key-value pairs otherwise.
pub fn resolve_env_with_vars(
    options: &SandboxOptions,
    current_env: &[(String, String)],
) -> Option<Vec<(String, String)>> {
    resolve_env_impl(options, current_env)
}

/// Shared implementation for environment variable filtering.
///
/// Returns `None` if all env vars should be passed through.
/// Returns `Some(vec)` with the filtered list.
fn resolve_env_impl(
    options: &SandboxOptions,
    current_env: &[(String, String)],
) -> Option<Vec<(String, String)>> {
    match (&options.allow_env, &options.deny_env) {
        // No config at all → use defaults: allow standard, deny known-secrets.
        (None, None) => {
            let default_allow_env = SandboxOptions::default_allow_env();
            let default_deny_env = SandboxOptions::default_deny_env();
            let allow = match &default_allow_env {
                AllowEnv::List(entries) => entries.as_slice(),
                AllowEnv::Bool(_) => &[],
            };
            Some(resolve_env_filter(allow, &default_deny_env, current_env))
        }
        // allow-env = true, no deny → pass all.
        (Some(AllowEnv::Bool(true)), None) => None,
        // allow-env = true, with deny list.
        (Some(AllowEnv::Bool(true)), Some(deny)) => {
            let allow_all: Vec<_> = current_env
                .iter()
                .map(|(k, _)| crate::options::EnvEntry::Name(k.clone()))
                .collect();
            Some(resolve_env_filter(&allow_all, deny, current_env))
        }
        // allow-env = false → deny all (deny list is irrelevant).
        (Some(AllowEnv::Bool(false)), _) => Some(Vec::new()),
        // deny-env without allow-env → allow all, then apply deny list.
        // The user's intent is "pass everything except these".
        (None, Some(deny)) => {
            let allow_all: Vec<_> = current_env
                .iter()
                .map(|(k, _)| crate::options::EnvEntry::Name(k.clone()))
                .collect();
            Some(resolve_env_filter(&allow_all, deny, current_env))
        }
        // allow-env = list, optional deny.
        (Some(AllowEnv::List(allow)), deny_opt) => {
            let deny = deny_opt.as_deref().unwrap_or_default();
            Some(resolve_env_filter(allow, deny, current_env))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::options::{AllowNet, EnvEntry, EnvPreset, NetEntry, NetPreset};

    fn dummy_context() -> PresetContext {
        PresetContext {
            project_root: PathBuf::from("/tmp/project"),
            python: PathBuf::from("/usr/bin/python3"),
            python_prefix: PathBuf::from("/usr"),
            stdlib: PathBuf::from("/usr/lib/python3.12"),
            virtualenv: PathBuf::from("/tmp/project/.venv"),
            home: PathBuf::from("/home/user"),
            cache_dir: PathBuf::from("/tmp/uv-cache"),
        }
    }

    #[test]
    fn resolve_rejects_allow_net_hosts() {
        let options = SandboxOptions {
            allow_net: Some(AllowNet::Hosts(vec![NetEntry::Preset {
                preset: NetPreset::Pypi,
            }])),
            ..SandboxOptions::default()
        };
        let result = resolve_sandbox_spec(&options, &dummy_context());
        assert!(result.is_err(), "host-list allow-net should be rejected");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("not yet implemented"),
            "error should mention not yet implemented: {err}"
        );
    }

    #[test]
    fn resolve_rejects_deny_net() {
        let options = SandboxOptions {
            deny_net: Some(vec![NetEntry::Host("evil.example.com".to_string())]),
            ..SandboxOptions::default()
        };
        let result = resolve_sandbox_spec(&options, &dummy_context());
        assert!(result.is_err(), "deny-net should be rejected");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("not yet implemented"),
            "error should mention not yet implemented: {err}"
        );
    }

    #[test]
    fn resolve_allow_net_bool_works() {
        let options = SandboxOptions {
            allow_net: Some(AllowNet::Bool(true)),
            ..SandboxOptions::default()
        };
        let spec = resolve_sandbox_spec(&options, &dummy_context()).unwrap();
        assert!(spec.allow_net);

        let options = SandboxOptions {
            allow_net: Some(AllowNet::Bool(false)),
            ..SandboxOptions::default()
        };
        let spec = resolve_sandbox_spec(&options, &dummy_context()).unwrap();
        assert!(!spec.allow_net);
    }

    #[test]
    fn resolve_defaults_none_none_env_uses_standard_and_denies_secrets() {
        // When both allow_env and deny_env are None, the default is:
        // allow = [standard], deny = [known-secrets].
        let options = SandboxOptions::default();
        let env = vec![
            ("HOME".to_string(), "/home/user".to_string()),
            ("PATH".to_string(), "/usr/bin".to_string()),
            ("AWS_SECRET_ACCESS_KEY".to_string(), "secret".to_string()),
            ("CUSTOM_VAR".to_string(), "value".to_string()),
        ];
        let result = resolve_env_impl(&options, &env);
        let filtered = result.expect("should return Some");
        assert!(
            filtered.iter().any(|(k, _)| k == "HOME"),
            "HOME should be included (standard preset)"
        );
        assert!(
            filtered.iter().any(|(k, _)| k == "PATH"),
            "PATH should be included (standard preset)"
        );
        assert!(
            !filtered.iter().any(|(k, _)| k == "AWS_SECRET_ACCESS_KEY"),
            "AWS_SECRET_ACCESS_KEY should be denied (known-secrets preset)"
        );
        assert!(
            !filtered.iter().any(|(k, _)| k == "CUSTOM_VAR"),
            "CUSTOM_VAR should not be included (not in standard preset)"
        );
    }

    #[test]
    fn resolve_defaults_populates_all_fields() {
        // An empty SandboxOptions should produce a usable spec with default
        // allow/deny paths for read, write, and execute.
        let options = SandboxOptions::default();
        let context = dummy_context();
        let spec = resolve_sandbox_spec(&options, &context).unwrap();

        // allow-read should include project, python, system paths.
        assert!(
            !spec.allow_read.is_empty(),
            "default allow_read should not be empty"
        );
        assert!(
            spec.allow_read.iter().any(|p| p == &context.project_root),
            "allow_read should include project root"
        );

        // deny-read should include known-secrets paths.
        assert!(
            !spec.deny_read.is_empty(),
            "default deny_read should not be empty"
        );

        // allow-write should include project + tmp.
        assert!(
            !spec.allow_write.is_empty(),
            "default allow_write should not be empty"
        );

        // deny-write should include shell-configs, git-hooks, ide-configs.
        assert!(
            !spec.deny_write.is_empty(),
            "default deny_write should not be empty"
        );

        // allow-execute should include python.
        assert!(
            !spec.allow_execute.is_empty(),
            "default allow_execute should not be empty"
        );
        assert!(
            spec.allow_execute.iter().any(|p| p == &context.python),
            "allow_execute should include python interpreter"
        );

        // deny-execute should be empty (no default).
        assert!(
            spec.deny_execute.is_empty(),
            "default deny_execute should be empty"
        );

        // Network should be denied.
        assert!(!spec.allow_net, "default should deny network access");
    }

    #[test]
    fn resolve_deny_env_without_allow_env_filters() {
        // deny-env without allow-env should allow everything except the denied vars
        let options = SandboxOptions {
            deny_env: Some(vec![EnvEntry::Name("SECRET_KEY".to_string())]),
            ..SandboxOptions::default()
        };
        let env = vec![
            ("HOME".to_string(), "/home/user".to_string()),
            ("SECRET_KEY".to_string(), "hidden".to_string()),
        ];
        let result = resolve_env_impl(&options, &env);
        let filtered = result.expect("should return Some");
        assert_eq!(filtered.len(), 1, "SECRET_KEY should be filtered out");
        assert_eq!(filtered[0].0, "HOME");
    }

    #[test]
    fn resolve_deny_env_with_preset_without_allow_env() {
        let options = SandboxOptions {
            deny_env: Some(vec![EnvEntry::Preset {
                preset: EnvPreset::KnownSecrets,
            }]),
            ..SandboxOptions::default()
        };
        let env = vec![
            ("HOME".to_string(), "/home/user".to_string()),
            ("AWS_SECRET_ACCESS_KEY".to_string(), "secret".to_string()),
            ("GITHUB_TOKEN".to_string(), "ghp_xxx".to_string()),
        ];
        let result = resolve_env_impl(&options, &env);
        let filtered = result.expect("should return Some");
        assert_eq!(filtered.len(), 1, "secrets should be filtered out");
        assert_eq!(filtered[0].0, "HOME");
    }

    #[test]
    fn resolve_env_with_vars_uses_provided_env() {
        let options = SandboxOptions {
            allow_env: Some(AllowEnv::List(vec![EnvEntry::Preset {
                preset: EnvPreset::Standard,
            }])),
            ..SandboxOptions::default()
        };
        // Simulate Command having added VIRTUAL_ENV
        let env = vec![
            ("HOME".to_string(), "/home/user".to_string()),
            ("VIRTUAL_ENV".to_string(), "/tmp/project/.venv".to_string()),
            ("SECRET".to_string(), "hidden".to_string()),
        ];
        let filtered = resolve_env_with_vars(&options, &env).expect("should return Some");
        assert!(
            filtered.iter().any(|(k, _)| k == "VIRTUAL_ENV"),
            "VIRTUAL_ENV should be included via standard preset"
        );
        assert!(
            !filtered.iter().any(|(k, _)| k == "SECRET"),
            "SECRET should not be included"
        );
    }

    #[test]
    fn resolve_env_with_vars_allow_true_no_deny_returns_none() {
        let options = SandboxOptions {
            allow_env: Some(AllowEnv::Bool(true)),
            ..SandboxOptions::default()
        };
        let env = vec![
            ("HOME".to_string(), "/home/user".to_string()),
            ("SECRET".to_string(), "hidden".to_string()),
        ];
        let result = resolve_env_with_vars(&options, &env);
        assert!(
            result.is_none(),
            "allow-env = true with no deny list should return None (pass all)"
        );
    }
}
