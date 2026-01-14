use std::num::NonZeroUsize;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::time::Duration;

use uv_dirs::{system_config_file, user_config_dir};
use uv_flags::EnvironmentFlags;
use uv_fs::Simplified;
use uv_static::{EnvVars, InvalidEnvironmentVariable, parse_boolish_environment_variable};
use uv_warnings::warn_user;

pub use crate::combine::*;
pub use crate::settings::*;

mod combine;
mod settings;

/// The [`Options`] as loaded from a configuration file on disk.
#[derive(Debug, Clone)]
pub struct FilesystemOptions(Options);

impl FilesystemOptions {
    /// Convert the [`FilesystemOptions`] into [`Options`].
    pub fn into_options(self) -> Options {
        self.0
    }
}

impl Deref for FilesystemOptions {
    type Target = Options;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl FilesystemOptions {
    /// Load the user [`FilesystemOptions`].
    pub fn user() -> Result<Option<Self>, Error> {
        let Some(dir) = user_config_dir() else {
            return Ok(None);
        };
        let root = dir.join("uv");
        let file = root.join("uv.toml");

        tracing::debug!("Searching for user configuration in: `{}`", file.display());
        match read_file(&file) {
            Ok(options) => {
                tracing::debug!("Found user configuration in: `{}`", file.display());
                validate_uv_toml(&file, &options)?;
                Ok(Some(Self(options)))
            }
            Err(Error::Io(err))
                if matches!(
                    err.kind(),
                    std::io::ErrorKind::NotFound
                        | std::io::ErrorKind::NotADirectory
                        | std::io::ErrorKind::PermissionDenied
                ) =>
            {
                Ok(None)
            }
            Err(err) => Err(err),
        }
    }

    pub fn system() -> Result<Option<Self>, Error> {
        let Some(file) = system_config_file() else {
            return Ok(None);
        };

        tracing::debug!("Found system configuration in: `{}`", file.display());
        let options = read_file(&file)?;
        validate_uv_toml(&file, &options)?;
        Ok(Some(Self(options)))
    }

    /// Find the [`FilesystemOptions`] for the given path.
    ///
    /// The search starts at the given path and goes up the directory tree until a `uv.toml` file or
    /// `pyproject.toml` file is found.
    pub fn find(path: &Path) -> Result<Option<Self>, Error> {
        for ancestor in path.ancestors() {
            match Self::from_directory(ancestor) {
                Ok(Some(options)) => {
                    return Ok(Some(options));
                }
                Ok(None) => {
                    // Continue traversing the directory tree.
                }
                Err(Error::PyprojectToml(path, err)) => {
                    // If we see an invalid `pyproject.toml`, warn but continue.
                    warn_user!(
                        "Failed to parse `{}` during settings discovery:\n{}",
                        path.user_display().cyan(),
                        textwrap::indent(&err.to_string(), "  ")
                    );
                }
                Err(err) => {
                    // Otherwise, warn and stop.
                    return Err(err);
                }
            }
        }
        Ok(None)
    }

    /// Load a [`FilesystemOptions`] from a directory, preferring a `uv.toml` file over a
    /// `pyproject.toml` file.
    pub fn from_directory(dir: &Path) -> Result<Option<Self>, Error> {
        // Read a `uv.toml` file in the current directory.
        let path = dir.join("uv.toml");
        match fs_err::read_to_string(&path) {
            Ok(content) => {
                let options = toml::from_str::<Options>(&content)
                    .map_err(|err| Error::UvToml(path.clone(), Box::new(err)))?
                    .relative_to(&std::path::absolute(dir)?)?;

                // If the directory also contains a `[tool.uv]` table in a `pyproject.toml` file,
                // warn.
                let pyproject = dir.join("pyproject.toml");
                if let Some(pyproject) = fs_err::read_to_string(pyproject)
                    .ok()
                    .and_then(|content| toml::from_str::<PyProjectToml>(&content).ok())
                {
                    if let Some(options) = pyproject.tool.as_ref().and_then(|tool| tool.uv.as_ref())
                    {
                        warn_uv_toml_masked_fields(options);
                    }
                }

                tracing::debug!("Found workspace configuration at `{}`", path.display());
                validate_uv_toml(&path, &options)?;
                return Ok(Some(Self(options)));
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => return Err(err.into()),
        }

        // Read a `pyproject.toml` file in the current directory.
        let path = dir.join("pyproject.toml");
        match fs_err::read_to_string(&path) {
            Ok(content) => {
                // Parse, but skip any `pyproject.toml` that doesn't have a `[tool.uv]` section.
                let pyproject: PyProjectToml = toml::from_str(&content)
                    .map_err(|err| Error::PyprojectToml(path.clone(), Box::new(err)))?;
                let Some(tool) = pyproject.tool else {
                    tracing::debug!(
                        "Skipping `pyproject.toml` in `{}` (no `[tool]` section)",
                        dir.display()
                    );
                    return Ok(None);
                };
                let Some(options) = tool.uv else {
                    tracing::debug!(
                        "Skipping `pyproject.toml` in `{}` (no `[tool.uv]` section)",
                        dir.display()
                    );
                    return Ok(None);
                };

                let options = options.relative_to(&std::path::absolute(dir)?)?;

                tracing::debug!("Found workspace configuration at `{}`", path.display());
                return Ok(Some(Self(options)));
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => return Err(err.into()),
        }

        Ok(None)
    }

    /// Load a [`FilesystemOptions`] from a `uv.toml` file.
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, Error> {
        let path = path.as_ref();
        tracing::debug!("Reading user configuration from: `{}`", path.display());

        let options = read_file(path)?;
        validate_uv_toml(path, &options)?;
        Ok(Self(options))
    }
}

impl From<Options> for FilesystemOptions {
    fn from(options: Options) -> Self {
        Self(options)
    }
}

/// Load [`Options`] from a `uv.toml` file.
fn read_file(path: &Path) -> Result<Options, Error> {
    let content = fs_err::read_to_string(path)?;
    let options = toml::from_str::<Options>(&content)
        .map_err(|err| Error::UvToml(path.to_path_buf(), Box::new(err)))?;
    let options = if let Some(parent) = std::path::absolute(path)?.parent() {
        options.relative_to(parent)?
    } else {
        options
    };
    Ok(options)
}

/// Validate that an [`Options`] schema is compatible with `uv.toml`.
fn validate_uv_toml(path: &Path, options: &Options) -> Result<(), Error> {
    let Options {
        globals: _,
        top_level: _,
        install_mirrors: _,
        publish: _,
        add: _,
        pip: _,
        cache_keys: _,
        override_dependencies: _,
        exclude_dependencies: _,
        constraint_dependencies: _,
        build_constraint_dependencies: _,
        environments,
        required_environments,
        conflicts,
        workspace,
        sources,
        dev_dependencies,
        default_groups,
        dependency_groups,
        managed,
        package,
        build_backend,
    } = options;
    // The `uv.toml` format is not allowed to include any of the following, which are
    // permitted by the schema since they _can_ be included in `pyproject.toml` files
    // (and we want to use `deny_unknown_fields`).
    if conflicts.is_some() {
        return Err(Error::PyprojectOnlyField(path.to_path_buf(), "conflicts"));
    }
    if workspace.is_some() {
        return Err(Error::PyprojectOnlyField(path.to_path_buf(), "workspace"));
    }
    if sources.is_some() {
        return Err(Error::PyprojectOnlyField(path.to_path_buf(), "sources"));
    }
    if dev_dependencies.is_some() {
        return Err(Error::PyprojectOnlyField(
            path.to_path_buf(),
            "dev-dependencies",
        ));
    }
    if default_groups.is_some() {
        return Err(Error::PyprojectOnlyField(
            path.to_path_buf(),
            "default-groups",
        ));
    }
    if dependency_groups.is_some() {
        return Err(Error::PyprojectOnlyField(
            path.to_path_buf(),
            "dependency-groups",
        ));
    }
    if managed.is_some() {
        return Err(Error::PyprojectOnlyField(path.to_path_buf(), "managed"));
    }
    if package.is_some() {
        return Err(Error::PyprojectOnlyField(path.to_path_buf(), "package"));
    }
    if build_backend.is_some() {
        return Err(Error::PyprojectOnlyField(
            path.to_path_buf(),
            "build-backend",
        ));
    }
    if environments.is_some() {
        return Err(Error::PyprojectOnlyField(
            path.to_path_buf(),
            "environments",
        ));
    }
    if required_environments.is_some() {
        return Err(Error::PyprojectOnlyField(
            path.to_path_buf(),
            "required-environments",
        ));
    }
    Ok(())
}

/// Validate that an [`Options`] contains no fields that `uv.toml` would mask
///
/// This is essentially the inverse of [`validate_uv_toml`].
fn warn_uv_toml_masked_fields(options: &Options) {
    let Options {
        globals:
            GlobalOptions {
                required_version,
                native_tls,
                offline,
                no_cache,
                cache_dir,
                preview,
                python_preference,
                python_downloads,
                concurrent_downloads,
                concurrent_builds,
                concurrent_installs,
                allow_insecure_host,
                http_proxy,
                https_proxy,
                no_proxy,
            },
        top_level:
            ResolverInstallerSchema {
                index,
                index_url,
                extra_index_url,
                no_index,
                find_links,
                index_strategy,
                keyring_provider,
                resolution,
                prerelease,
                fork_strategy,
                dependency_metadata,
                config_settings,
                config_settings_package,
                no_build_isolation,
                no_build_isolation_package,
                extra_build_dependencies,
                extra_build_variables,
                exclude_newer,
                exclude_newer_package,
                link_mode,
                compile_bytecode,
                no_sources,
                upgrade,
                upgrade_package,
                reinstall,
                reinstall_package,
                no_build,
                no_build_package,
                no_binary,
                no_binary_package,
                torch_backend,
            },
        install_mirrors:
            PythonInstallMirrors {
                python_install_mirror,
                pypy_install_mirror,
                python_downloads_json_url,
            },
        publish:
            PublishOptions {
                publish_url,
                trusted_publishing,
                check_url,
            },
        add: AddOptions { add_bounds },
        pip,
        cache_keys,
        override_dependencies,
        exclude_dependencies,
        constraint_dependencies,
        build_constraint_dependencies,
        environments: _,
        required_environments: _,
        conflicts: _,
        workspace: _,
        sources: _,
        dev_dependencies: _,
        default_groups: _,
        dependency_groups: _,
        managed: _,
        package: _,
        build_backend: _,
    } = options;

    let mut masked_fields = vec![];

    if required_version.is_some() {
        masked_fields.push("required-version");
    }
    if native_tls.is_some() {
        masked_fields.push("native-tls");
    }
    if offline.is_some() {
        masked_fields.push("offline");
    }
    if no_cache.is_some() {
        masked_fields.push("no-cache");
    }
    if cache_dir.is_some() {
        masked_fields.push("cache-dir");
    }
    if preview.is_some() {
        masked_fields.push("preview");
    }
    if python_preference.is_some() {
        masked_fields.push("python-preference");
    }
    if python_downloads.is_some() {
        masked_fields.push("python-downloads");
    }
    if concurrent_downloads.is_some() {
        masked_fields.push("concurrent-downloads");
    }
    if concurrent_builds.is_some() {
        masked_fields.push("concurrent-builds");
    }
    if concurrent_installs.is_some() {
        masked_fields.push("concurrent-installs");
    }
    if allow_insecure_host.is_some() {
        masked_fields.push("allow-insecure-host");
    }
    if http_proxy.is_some() {
        masked_fields.push("http-proxy");
    }
    if https_proxy.is_some() {
        masked_fields.push("https-proxy");
    }
    if no_proxy.is_some() {
        masked_fields.push("no-proxy");
    }
    if index.is_some() {
        masked_fields.push("index");
    }
    if index_url.is_some() {
        masked_fields.push("index-url");
    }
    if extra_index_url.is_some() {
        masked_fields.push("extra-index-url");
    }
    if no_index.is_some() {
        masked_fields.push("no-index");
    }
    if find_links.is_some() {
        masked_fields.push("find-links");
    }
    if index_strategy.is_some() {
        masked_fields.push("index-strategy");
    }
    if keyring_provider.is_some() {
        masked_fields.push("keyring-provider");
    }
    if resolution.is_some() {
        masked_fields.push("resolution");
    }
    if prerelease.is_some() {
        masked_fields.push("prerelease");
    }
    if fork_strategy.is_some() {
        masked_fields.push("fork-strategy");
    }
    if dependency_metadata.is_some() {
        masked_fields.push("dependency-metadata");
    }
    if config_settings.is_some() {
        masked_fields.push("config-settings");
    }
    if config_settings_package.is_some() {
        masked_fields.push("config-settings-package");
    }
    if no_build_isolation.is_some() {
        masked_fields.push("no-build-isolation");
    }
    if no_build_isolation_package.is_some() {
        masked_fields.push("no-build-isolation-package");
    }
    if extra_build_dependencies.is_some() {
        masked_fields.push("extra-build-dependencies");
    }
    if extra_build_variables.is_some() {
        masked_fields.push("extra-build-variables");
    }
    if exclude_newer.is_some() {
        masked_fields.push("exclude-newer");
    }
    if exclude_newer_package.is_some() {
        masked_fields.push("exclude-newer-package");
    }
    if link_mode.is_some() {
        masked_fields.push("link-mode");
    }
    if compile_bytecode.is_some() {
        masked_fields.push("compile-bytecode");
    }
    if no_sources.is_some() {
        masked_fields.push("no-sources");
    }
    if upgrade.is_some() {
        masked_fields.push("upgrade");
    }
    if upgrade_package.is_some() {
        masked_fields.push("upgrade-package");
    }
    if reinstall.is_some() {
        masked_fields.push("reinstall");
    }
    if reinstall_package.is_some() {
        masked_fields.push("reinstall-package");
    }
    if no_build.is_some() {
        masked_fields.push("no-build");
    }
    if no_build_package.is_some() {
        masked_fields.push("no-build-package");
    }
    if no_binary.is_some() {
        masked_fields.push("no-binary");
    }
    if no_binary_package.is_some() {
        masked_fields.push("no-binary-package");
    }
    if torch_backend.is_some() {
        masked_fields.push("torch-backend");
    }
    if python_install_mirror.is_some() {
        masked_fields.push("python-install-mirror");
    }
    if pypy_install_mirror.is_some() {
        masked_fields.push("pypy-install-mirror");
    }
    if python_downloads_json_url.is_some() {
        masked_fields.push("python-downloads-json-url");
    }
    if publish_url.is_some() {
        masked_fields.push("publish-url");
    }
    if trusted_publishing.is_some() {
        masked_fields.push("trusted-publishing");
    }
    if check_url.is_some() {
        masked_fields.push("check-url");
    }
    if add_bounds.is_some() {
        masked_fields.push("add-bounds");
    }
    if pip.is_some() {
        masked_fields.push("pip");
    }
    if cache_keys.is_some() {
        masked_fields.push("cache_keys");
    }
    if override_dependencies.is_some() {
        masked_fields.push("override-dependencies");
    }
    if exclude_dependencies.is_some() {
        masked_fields.push("exclude-dependencies");
    }
    if constraint_dependencies.is_some() {
        masked_fields.push("constraint-dependencies");
    }
    if build_constraint_dependencies.is_some() {
        masked_fields.push("build-constraint-dependencies");
    }
    if !masked_fields.is_empty() {
        let field_listing = masked_fields.join("\n- ");
        warn_user!(
            "Found both a `uv.toml` file and a `[tool.uv]` section in an adjacent `pyproject.toml`. The following fields from `[tool.uv]` will be ignored in favor of the `uv.toml` file:\n- {}",
            field_listing,
        );
    }
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Index(#[from] uv_distribution_types::IndexUrlError),

    #[error("Failed to parse: `{}`", _0.user_display())]
    PyprojectToml(PathBuf, #[source] Box<toml::de::Error>),

    #[error("Failed to parse: `{}`", _0.user_display())]
    UvToml(PathBuf, #[source] Box<toml::de::Error>),

    #[error("Failed to parse: `{}`. The `{}` field is not allowed in a `uv.toml` file. `{}` is only applicable in the context of a project, and should be placed in a `pyproject.toml` file instead.", _0.user_display(), _1, _1
    )]
    PyprojectOnlyField(PathBuf, &'static str),

    #[error(transparent)]
    InvalidEnvironmentVariable(#[from] InvalidEnvironmentVariable),
}

#[derive(Copy, Clone, Debug)]
pub struct Concurrency {
    pub downloads: Option<NonZeroUsize>,
    pub builds: Option<NonZeroUsize>,
    pub installs: Option<NonZeroUsize>,
}

/// A boolean flag parsed from an environment variable.
///
/// Stores both the value and the environment variable name for use in error messages.
#[derive(Debug, Clone, Copy)]
pub struct EnvFlag {
    pub value: Option<bool>,
    pub env_var: &'static str,
}

impl EnvFlag {
    /// Create a new [`EnvFlag`] by parsing the given environment variable.
    pub fn new(env_var: &'static str) -> Result<Self, Error> {
        Ok(Self {
            value: parse_boolish_environment_variable(env_var)?,
            env_var,
        })
    }
}

/// Options loaded from environment variables.
///
/// This is currently a subset of all respected environment variables, most are parsed via Clap at
/// the CLI level, however there are limited semantics in that context.
#[derive(Debug, Clone)]
pub struct EnvironmentOptions {
    pub skip_wheel_filename_check: Option<bool>,
    pub hide_build_output: Option<bool>,
    pub python_install_bin: Option<bool>,
    pub python_install_registry: Option<bool>,
    pub install_mirrors: PythonInstallMirrors,
    pub log_context: Option<bool>,
    pub lfs: Option<bool>,
    pub http_timeout: Duration,
    pub http_retries: u32,
    pub upload_http_timeout: Duration,
    pub concurrency: Concurrency,
    #[cfg(feature = "tracing-durations-export")]
    pub tracing_durations_file: Option<PathBuf>,
    pub frozen: EnvFlag,
    pub locked: EnvFlag,
    pub offline: EnvFlag,
    pub no_sync: EnvFlag,
    pub managed_python: EnvFlag,
    pub no_managed_python: EnvFlag,
    pub native_tls: EnvFlag,
    pub preview: EnvFlag,
    pub isolated: EnvFlag,
    pub no_progress: EnvFlag,
    pub no_installer_metadata: EnvFlag,
    pub dev: EnvFlag,
    pub no_dev: EnvFlag,
    pub show_resolution: EnvFlag,
    pub no_editable: EnvFlag,
    pub no_env_file: EnvFlag,
    pub venv_seed: EnvFlag,
    pub venv_clear: EnvFlag,
}

impl EnvironmentOptions {
    /// Create a new [`EnvironmentOptions`] from environment variables.
    pub fn new() -> Result<Self, Error> {
        // Timeout options, matching https://doc.rust-lang.org/nightly/cargo/reference/config.html#httptimeout
        // `UV_REQUEST_TIMEOUT` is provided for backwards compatibility with v0.1.6
        let http_timeout = parse_integer_environment_variable(EnvVars::UV_HTTP_TIMEOUT)?
            .or(parse_integer_environment_variable(
                EnvVars::UV_REQUEST_TIMEOUT,
            )?)
            .or(parse_integer_environment_variable(EnvVars::HTTP_TIMEOUT)?)
            .map(Duration::from_secs);

        Ok(Self {
            skip_wheel_filename_check: parse_boolish_environment_variable(
                EnvVars::UV_SKIP_WHEEL_FILENAME_CHECK,
            )?,
            hide_build_output: parse_boolish_environment_variable(EnvVars::UV_HIDE_BUILD_OUTPUT)?,
            python_install_bin: parse_boolish_environment_variable(EnvVars::UV_PYTHON_INSTALL_BIN)?,
            python_install_registry: parse_boolish_environment_variable(
                EnvVars::UV_PYTHON_INSTALL_REGISTRY,
            )?,
            concurrency: Concurrency {
                downloads: parse_integer_environment_variable(EnvVars::UV_CONCURRENT_DOWNLOADS)?,
                builds: parse_integer_environment_variable(EnvVars::UV_CONCURRENT_BUILDS)?,
                installs: parse_integer_environment_variable(EnvVars::UV_CONCURRENT_INSTALLS)?,
            },
            install_mirrors: PythonInstallMirrors {
                python_install_mirror: parse_string_environment_variable(
                    EnvVars::UV_PYTHON_INSTALL_MIRROR,
                )?,
                pypy_install_mirror: parse_string_environment_variable(
                    EnvVars::UV_PYPY_INSTALL_MIRROR,
                )?,
                python_downloads_json_url: parse_string_environment_variable(
                    EnvVars::UV_PYTHON_DOWNLOADS_JSON_URL,
                )?,
            },
            log_context: parse_boolish_environment_variable(EnvVars::UV_LOG_CONTEXT)?,
            lfs: parse_boolish_environment_variable(EnvVars::UV_GIT_LFS)?,
            upload_http_timeout: parse_integer_environment_variable(
                EnvVars::UV_UPLOAD_HTTP_TIMEOUT,
            )?
            .map(Duration::from_secs)
            .or(http_timeout)
            .unwrap_or(Duration::from_secs(15 * 60)),
            http_timeout: http_timeout.unwrap_or(Duration::from_secs(30)),
            http_retries: parse_integer_environment_variable(EnvVars::UV_HTTP_RETRIES)?
                .unwrap_or(uv_client::DEFAULT_RETRIES),
            #[cfg(feature = "tracing-durations-export")]
            tracing_durations_file: parse_path_environment_variable(
                EnvVars::TRACING_DURATIONS_FILE,
            ),
            frozen: EnvFlag::new(EnvVars::UV_FROZEN)?,
            locked: EnvFlag::new(EnvVars::UV_LOCKED)?,
            offline: EnvFlag::new(EnvVars::UV_OFFLINE)?,
            no_sync: EnvFlag::new(EnvVars::UV_NO_SYNC)?,
            managed_python: EnvFlag::new(EnvVars::UV_MANAGED_PYTHON)?,
            no_managed_python: EnvFlag::new(EnvVars::UV_NO_MANAGED_PYTHON)?,
            native_tls: EnvFlag::new(EnvVars::UV_NATIVE_TLS)?,
            preview: EnvFlag::new(EnvVars::UV_PREVIEW)?,
            isolated: EnvFlag::new(EnvVars::UV_ISOLATED)?,
            no_progress: EnvFlag::new(EnvVars::UV_NO_PROGRESS)?,
            no_installer_metadata: EnvFlag::new(EnvVars::UV_NO_INSTALLER_METADATA)?,
            dev: EnvFlag::new(EnvVars::UV_DEV)?,
            no_dev: EnvFlag::new(EnvVars::UV_NO_DEV)?,
            show_resolution: EnvFlag::new(EnvVars::UV_SHOW_RESOLUTION)?,
            no_editable: EnvFlag::new(EnvVars::UV_NO_EDITABLE)?,
            no_env_file: EnvFlag::new(EnvVars::UV_NO_ENV_FILE)?,
            venv_seed: EnvFlag::new(EnvVars::UV_VENV_SEED)?,
            venv_clear: EnvFlag::new(EnvVars::UV_VENV_CLEAR)?,
        })
    }
}

/// Parse a string environment variable.
fn parse_string_environment_variable(name: &'static str) -> Result<Option<String>, Error> {
    match std::env::var(name) {
        Ok(v) => {
            if v.is_empty() {
                Ok(None)
            } else {
                Ok(Some(v))
            }
        }
        Err(e) => match e {
            std::env::VarError::NotPresent => Ok(None),
            std::env::VarError::NotUnicode(err) => Err(Error::InvalidEnvironmentVariable(
                InvalidEnvironmentVariable {
                    name: name.to_string(),
                    value: err.to_string_lossy().to_string(),
                    err: "expected a valid UTF-8 string".to_string(),
                },
            )),
        },
    }
}

fn parse_integer_environment_variable<T>(name: &'static str) -> Result<Option<T>, Error>
where
    T: std::str::FromStr + Copy,
    <T as std::str::FromStr>::Err: std::fmt::Display,
{
    let value = match std::env::var(name) {
        Ok(v) => v,
        Err(e) => {
            return match e {
                std::env::VarError::NotPresent => Ok(None),
                std::env::VarError::NotUnicode(err) => Err(Error::InvalidEnvironmentVariable(
                    InvalidEnvironmentVariable {
                        name: name.to_string(),
                        value: err.to_string_lossy().to_string(),
                        err: "expected a valid UTF-8 string".to_string(),
                    },
                )),
            };
        }
    };
    if value.is_empty() {
        return Ok(None);
    }

    match value.parse::<T>() {
        Ok(v) => Ok(Some(v)),
        Err(err) => Err(Error::InvalidEnvironmentVariable(
            InvalidEnvironmentVariable {
                name: name.to_string(),
                value,
                err: err.to_string(),
            },
        )),
    }
}

#[cfg(feature = "tracing-durations-export")]
/// Parse a path environment variable.
fn parse_path_environment_variable(name: &'static str) -> Option<PathBuf> {
    let value = std::env::var_os(name)?;

    if value.is_empty() {
        return None;
    }

    Some(PathBuf::from(value))
}

/// Populate the [`EnvironmentFlags`] from the given [`EnvironmentOptions`].
impl From<&EnvironmentOptions> for EnvironmentFlags {
    fn from(options: &EnvironmentOptions) -> Self {
        let mut flags = Self::empty();
        if options.skip_wheel_filename_check == Some(true) {
            flags.insert(Self::SKIP_WHEEL_FILENAME_CHECK);
        }
        if options.hide_build_output == Some(true) {
            flags.insert(Self::HIDE_BUILD_OUTPUT);
        }
        flags
    }
}
