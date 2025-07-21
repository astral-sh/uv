use std::ops::Deref;
use std::path::{Path, PathBuf};

use uv_dirs::{system_config_file, user_config_dir};
use uv_fs::Simplified;
use uv_static::EnvVars;
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
        constraint_dependencies: _,
        build_constraint_dependencies: _,
        environments: _,
        required_environments: _,
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
    Ok(())
}

/// Validate that an [`Options`] contains no fields that `uv.toml` would mask
///
/// This is essentially the inverse of [`validated_uv_toml`][].
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
            },
        top_level:
            ResolverInstallerOptions {
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
                exclude_newer,
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
        constraint_dependencies,
        build_constraint_dependencies,
        environments,
        required_environments,
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
    if exclude_newer.is_some() {
        masked_fields.push("exclude-newer");
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
    if constraint_dependencies.is_some() {
        masked_fields.push("constraint-dependencies");
    }
    if build_constraint_dependencies.is_some() {
        masked_fields.push("build-constraint-dependencies");
    }
    if environments.is_some() {
        masked_fields.push("environments");
    }
    if required_environments.is_some() {
        masked_fields.push("required-environments");
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

    #[error("Failed to parse: `{}`. The `{}` field is not allowed in a `uv.toml` file. `{}` is only applicable in the context of a project, and should be placed in a `pyproject.toml` file instead.", _0.user_display(), _1, _1)]
    PyprojectOnlyField(PathBuf, &'static str),

    #[error("Failed to parse environment variable `{name}` with invalid value `{value}`: {err}")]
    InvalidEnvironmentVariable {
        name: String,
        value: String,
        err: String,
    },
}

/// Options loaded from environment variables.
///
/// This is currently a subset of all respected environment variables, most are parsed via Clap at
/// the CLI level, however there are limited semantics in that context.
#[derive(Debug, Clone)]
pub struct EnvironmentOptions {
    pub python_install_bin: Option<bool>,
    pub python_install_registry: Option<bool>,
}

impl EnvironmentOptions {
    /// Create a new [`EnvironmentOptions`] from environment variables.
    pub fn new() -> Result<Self, Error> {
        Ok(Self {
            python_install_bin: parse_boolish_environment_variable(EnvVars::UV_PYTHON_INSTALL_BIN)?,
            python_install_registry: parse_boolish_environment_variable(
                EnvVars::UV_PYTHON_INSTALL_REGISTRY,
            )?,
        })
    }
}

/// Parse a boolean environment variable.
///
/// Adapted from Clap's `BoolishValueParser` which is dual licensed under the MIT and Apache-2.0.
fn parse_boolish_environment_variable(name: &'static str) -> Result<Option<bool>, Error> {
    // See `clap_builder/src/util/str_to_bool.rs`
    // We want to match Clap's accepted values

    // True values are `y`, `yes`, `t`, `true`, `on`, and `1`.
    const TRUE_LITERALS: [&str; 6] = ["y", "yes", "t", "true", "on", "1"];

    // False values are `n`, `no`, `f`, `false`, `off`, and `0`.
    const FALSE_LITERALS: [&str; 6] = ["n", "no", "f", "false", "off", "0"];

    // Converts a string literal representation of truth to true or false.
    //
    // `false` values are `n`, `no`, `f`, `false`, `off`, and `0` (case insensitive).
    //
    // Any other value will be considered as `true`.
    fn str_to_bool(val: impl AsRef<str>) -> Option<bool> {
        let pat: &str = &val.as_ref().to_lowercase();
        if TRUE_LITERALS.contains(&pat) {
            Some(true)
        } else if FALSE_LITERALS.contains(&pat) {
            Some(false)
        } else {
            None
        }
    }

    let Some(value) = std::env::var_os(name) else {
        return Ok(None);
    };

    let Some(value) = value.to_str() else {
        return Err(Error::InvalidEnvironmentVariable {
            name: name.to_string(),
            value: value.to_string_lossy().to_string(),
            err: "expected a valid UTF-8 string".to_string(),
        });
    };

    let Some(value) = str_to_bool(value) else {
        return Err(Error::InvalidEnvironmentVariable {
            name: name.to_string(),
            value: value.to_string(),
            err: "expected a boolish value".to_string(),
        });
    };

    Ok(Some(value))
}
