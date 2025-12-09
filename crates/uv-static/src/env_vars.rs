//! Environment variables used or supported by uv.
//! Used to generate `docs/reference/environment.md`.
use uv_macros::{attr_added_in, attr_env_var_pattern, attr_hidden, attribute_env_vars_metadata};

/// Declares all environment variable used throughout `uv` and its crates.
pub struct EnvVars;

#[attribute_env_vars_metadata]
impl EnvVars {
    /// The path to the binary that was used to invoke uv.
    ///
    /// This is propagated to all subprocesses spawned by uv.
    ///
    /// If the executable was invoked through a symbolic link, some platforms will return the path
    /// of the symbolic link and other platforms will return the path of the symbolic link’s target.
    ///
    /// See <https://doc.rust-lang.org/std/env/fn.current_exe.html#security> for security
    /// considerations.
    #[attr_added_in("0.6.0")]
    pub const UV: &'static str = "UV";

    /// Equivalent to the `--offline` command-line argument. If set, uv will disable network access.
    #[attr_added_in("0.5.9")]
    pub const UV_OFFLINE: &'static str = "UV_OFFLINE";

    /// Equivalent to the `--default-index` command-line argument. If set, uv will use
    /// this URL as the default index when searching for packages.
    #[attr_added_in("0.4.23")]
    pub const UV_DEFAULT_INDEX: &'static str = "UV_DEFAULT_INDEX";

    /// Equivalent to the `--index` command-line argument. If set, uv will use this
    /// space-separated list of URLs as additional indexes when searching for packages.
    #[attr_added_in("0.4.23")]
    pub const UV_INDEX: &'static str = "UV_INDEX";

    /// Equivalent to the `--index-url` command-line argument. If set, uv will use this
    /// URL as the default index when searching for packages.
    /// (Deprecated: use `UV_DEFAULT_INDEX` instead.)
    #[attr_added_in("0.0.5")]
    pub const UV_INDEX_URL: &'static str = "UV_INDEX_URL";

    /// Equivalent to the `--extra-index-url` command-line argument. If set, uv will
    /// use this space-separated list of URLs as additional indexes when searching for packages.
    /// (Deprecated: use `UV_INDEX` instead.)
    #[attr_added_in("0.1.3")]
    pub const UV_EXTRA_INDEX_URL: &'static str = "UV_EXTRA_INDEX_URL";

    /// Equivalent to the `--find-links` command-line argument. If set, uv will use this
    /// comma-separated list of additional locations to search for packages.
    #[attr_added_in("0.4.19")]
    pub const UV_FIND_LINKS: &'static str = "UV_FIND_LINKS";

    /// Equivalent to the `--no-sources` command-line argument. If set, uv will ignore
    /// `[tool.uv.sources]` annotations when resolving dependencies.
    #[attr_added_in("0.9.8")]
    pub const UV_NO_SOURCES: &'static str = "UV_NO_SOURCES";

    /// Equivalent to the `--cache-dir` command-line argument. If set, uv will use this
    /// directory for caching instead of the default cache directory.
    #[attr_added_in("0.0.5")]
    pub const UV_CACHE_DIR: &'static str = "UV_CACHE_DIR";

    /// The directory for storage of credentials when using a plain text backend.
    #[attr_added_in("0.8.15")]
    pub const UV_CREDENTIALS_DIR: &'static str = "UV_CREDENTIALS_DIR";

    /// Equivalent to the `--no-cache` command-line argument. If set, uv will not use the
    /// cache for any operations.
    #[attr_added_in("0.1.2")]
    pub const UV_NO_CACHE: &'static str = "UV_NO_CACHE";

    /// Equivalent to the `--resolution` command-line argument. For example, if set to
    /// `lowest-direct`, uv will install the lowest compatible versions of all direct dependencies.
    #[attr_added_in("0.1.27")]
    pub const UV_RESOLUTION: &'static str = "UV_RESOLUTION";

    /// Equivalent to the `--prerelease` command-line argument. For example, if set to
    /// `allow`, uv will allow pre-release versions for all dependencies.
    #[attr_added_in("0.1.16")]
    pub const UV_PRERELEASE: &'static str = "UV_PRERELEASE";

    /// Equivalent to the `--fork-strategy` argument. Controls version selection during universal
    /// resolution.
    #[attr_added_in("0.5.9")]
    pub const UV_FORK_STRATEGY: &'static str = "UV_FORK_STRATEGY";

    /// Equivalent to the `--system` command-line argument. If set to `true`, uv will
    /// use the first Python interpreter found in the system `PATH`.
    ///
    /// WARNING: `UV_SYSTEM_PYTHON=true` is intended for use in continuous integration (CI)
    /// or containerized environments and should be used with caution, as modifying the system
    /// Python can lead to unexpected behavior.
    #[attr_added_in("0.1.18")]
    pub const UV_SYSTEM_PYTHON: &'static str = "UV_SYSTEM_PYTHON";

    /// Equivalent to the `--python` command-line argument. If set to a path, uv will use
    /// this Python interpreter for all operations.
    #[attr_added_in("0.1.40")]
    pub const UV_PYTHON: &'static str = "UV_PYTHON";

    /// Equivalent to the `--break-system-packages` command-line argument. If set to `true`,
    /// uv will allow the installation of packages that conflict with system-installed packages.
    ///
    /// WARNING: `UV_BREAK_SYSTEM_PACKAGES=true` is intended for use in continuous integration
    /// (CI) or containerized environments and should be used with caution, as modifying the system
    /// Python can lead to unexpected behavior.
    #[attr_added_in("0.1.32")]
    pub const UV_BREAK_SYSTEM_PACKAGES: &'static str = "UV_BREAK_SYSTEM_PACKAGES";

    /// Equivalent to the `--native-tls` command-line argument. If set to `true`, uv will
    /// use the system's trust store instead of the bundled `webpki-roots` crate.
    #[attr_added_in("0.1.19")]
    pub const UV_NATIVE_TLS: &'static str = "UV_NATIVE_TLS";

    /// Equivalent to the `--index-strategy` command-line argument.
    ///
    /// For example, if set to `unsafe-best-match`, uv will consider versions of a given package
    /// available across all index URLs, rather than limiting its search to the first index URL
    /// that contains the package.
    #[attr_added_in("0.1.29")]
    pub const UV_INDEX_STRATEGY: &'static str = "UV_INDEX_STRATEGY";

    /// Equivalent to the `--require-hashes` command-line argument. If set to `true`,
    /// uv will require that all dependencies have a hash specified in the requirements file.
    #[attr_added_in("0.1.34")]
    pub const UV_REQUIRE_HASHES: &'static str = "UV_REQUIRE_HASHES";

    /// Equivalent to the `--constraints` command-line argument. If set, uv will use this
    /// file as the constraints file. Uses space-separated list of files.
    #[attr_added_in("0.1.36")]
    pub const UV_CONSTRAINT: &'static str = "UV_CONSTRAINT";

    /// Equivalent to the `--build-constraints` command-line argument. If set, uv will use this file
    /// as constraints for any source distribution builds. Uses space-separated list of files.
    #[attr_added_in("0.2.34")]
    pub const UV_BUILD_CONSTRAINT: &'static str = "UV_BUILD_CONSTRAINT";

    /// Equivalent to the `--overrides` command-line argument. If set, uv will use this file
    /// as the overrides file. Uses space-separated list of files.
    #[attr_added_in("0.2.22")]
    pub const UV_OVERRIDE: &'static str = "UV_OVERRIDE";

    /// Equivalent to the `--excludes` command-line argument. If set, uv will use this
    /// as the excludes file. Uses space-separated list of files.
    #[attr_added_in("0.9.8")]
    pub const UV_EXCLUDE: &'static str = "UV_EXCLUDE";

    /// Equivalent to the `--link-mode` command-line argument. If set, uv will use this as
    /// a link mode.
    #[attr_added_in("0.1.40")]
    pub const UV_LINK_MODE: &'static str = "UV_LINK_MODE";

    /// Equivalent to the `--no-build-isolation` command-line argument. If set, uv will
    /// skip isolation when building source distributions.
    #[attr_added_in("0.1.40")]
    pub const UV_NO_BUILD_ISOLATION: &'static str = "UV_NO_BUILD_ISOLATION";

    /// Equivalent to the `--custom-compile-command` command-line argument.
    ///
    /// Used to override uv in the output header of the `requirements.txt` files generated by
    /// `uv pip compile`. Intended for use-cases in which `uv pip compile` is called from within a wrapper
    /// script, to include the name of the wrapper script in the output file.
    #[attr_added_in("0.1.23")]
    pub const UV_CUSTOM_COMPILE_COMMAND: &'static str = "UV_CUSTOM_COMPILE_COMMAND";

    /// Equivalent to the `--keyring-provider` command-line argument. If set, uv
    /// will use this value as the keyring provider.
    #[attr_added_in("0.1.19")]
    pub const UV_KEYRING_PROVIDER: &'static str = "UV_KEYRING_PROVIDER";

    /// Equivalent to the `--config-file` command-line argument. Expects a path to a
    /// local `uv.toml` file to use as the configuration file.
    #[attr_added_in("0.1.34")]
    pub const UV_CONFIG_FILE: &'static str = "UV_CONFIG_FILE";

    /// Equivalent to the `--no-config` command-line argument. If set, uv will not read
    /// any configuration files from the current directory, parent directories, or user configuration
    /// directories.
    #[attr_added_in("0.2.30")]
    pub const UV_NO_CONFIG: &'static str = "UV_NO_CONFIG";

    /// Equivalent to the `--isolated` command-line argument. If set, uv will avoid discovering
    /// a `pyproject.toml` or `uv.toml` file.
    #[attr_added_in("0.8.14")]
    pub const UV_ISOLATED: &'static str = "UV_ISOLATED";

    /// Equivalent to the `--exclude-newer` command-line argument. If set, uv will
    /// exclude distributions published after the specified date.
    #[attr_added_in("0.2.12")]
    pub const UV_EXCLUDE_NEWER: &'static str = "UV_EXCLUDE_NEWER";

    /// Whether uv should prefer system or managed Python versions.
    #[attr_added_in("0.3.2")]
    pub const UV_PYTHON_PREFERENCE: &'static str = "UV_PYTHON_PREFERENCE";

    /// Require use of uv-managed Python versions.
    #[attr_added_in("0.6.8")]
    pub const UV_MANAGED_PYTHON: &'static str = "UV_MANAGED_PYTHON";

    /// Disable use of uv-managed Python versions.
    #[attr_added_in("0.6.8")]
    pub const UV_NO_MANAGED_PYTHON: &'static str = "UV_NO_MANAGED_PYTHON";

    /// Equivalent to the
    /// [`python-downloads`](../reference/settings.md#python-downloads) setting and, when disabled, the
    /// `--no-python-downloads` option. Whether uv should allow Python downloads.
    #[attr_added_in("0.3.2")]
    pub const UV_PYTHON_DOWNLOADS: &'static str = "UV_PYTHON_DOWNLOADS";

    /// Overrides the environment-determined libc on linux systems when filling in the current platform
    /// within Python version requests. Options are: `gnu`, `gnueabi`, `gnueabihf`, `musl`, and `none`.
    #[attr_added_in("0.7.22")]
    pub const UV_LIBC: &'static str = "UV_LIBC";

    /// Equivalent to the `--compile-bytecode` command-line argument. If set, uv
    /// will compile Python source files to bytecode after installation.
    #[attr_added_in("0.3.3")]
    pub const UV_COMPILE_BYTECODE: &'static str = "UV_COMPILE_BYTECODE";

    /// Timeout (in seconds) for bytecode compilation.
    #[attr_added_in("0.7.22")]
    pub const UV_COMPILE_BYTECODE_TIMEOUT: &'static str = "UV_COMPILE_BYTECODE_TIMEOUT";

    /// Equivalent to the `--no-editable` command-line argument. If set, uv
    /// installs or exports any editable dependencies, including the project and any workspace
    /// members, as non-editable.
    #[attr_added_in("0.6.15")]
    pub const UV_NO_EDITABLE: &'static str = "UV_NO_EDITABLE";

    /// Equivalent to the `--dev` command-line argument. If set, uv will include
    /// development dependencies.
    #[attr_added_in("0.8.7")]
    pub const UV_DEV: &'static str = "UV_DEV";

    /// Equivalent to the `--no-dev` command-line argument. If set, uv will exclude
    /// development dependencies.
    #[attr_added_in("0.8.7")]
    pub const UV_NO_DEV: &'static str = "UV_NO_DEV";

    /// Equivalent to the `--no-group` command-line argument. If set, uv will disable
    /// the specified dependency groups for the given space-delimited list of packages.
    #[attr_added_in("0.9.8")]
    pub const UV_NO_GROUP: &'static str = "UV_NO_GROUP";

    /// Equivalent to the `--no-default-groups` command-line argument. If set, uv will
    /// not select the default dependency groups defined in `tool.uv.default-groups`.
    #[attr_added_in("0.9.9")]
    pub const UV_NO_DEFAULT_GROUPS: &'static str = "UV_NO_DEFAULT_GROUPS";

    /// Equivalent to the `--no-binary` command-line argument. If set, uv will install
    /// all packages from source. The resolver will still use pre-built wheels to
    /// extract package metadata, if available.
    #[attr_added_in("0.5.30")]
    pub const UV_NO_BINARY: &'static str = "UV_NO_BINARY";

    /// Equivalent to the `--no-binary-package` command line argument. If set, uv will
    /// not use pre-built wheels for the given space-delimited list of packages.
    #[attr_added_in("0.5.30")]
    pub const UV_NO_BINARY_PACKAGE: &'static str = "UV_NO_BINARY_PACKAGE";

    /// Equivalent to the `--no-build` command-line argument. If set, uv will not build
    /// source distributions.
    #[attr_added_in("0.1.40")]
    pub const UV_NO_BUILD: &'static str = "UV_NO_BUILD";

    /// Equivalent to the `--no-build-package` command line argument. If set, uv will
    /// not build source distributions for the given space-delimited list of packages.
    #[attr_added_in("0.6.5")]
    pub const UV_NO_BUILD_PACKAGE: &'static str = "UV_NO_BUILD_PACKAGE";

    /// Equivalent to the `--publish-url` command-line argument. The URL of the upload
    /// endpoint of the index to use with `uv publish`.
    #[attr_added_in("0.4.16")]
    pub const UV_PUBLISH_URL: &'static str = "UV_PUBLISH_URL";

    /// Equivalent to the `--token` command-line argument in `uv publish`. If set, uv
    /// will use this token (with the username `__token__`) for publishing.
    #[attr_added_in("0.4.16")]
    pub const UV_PUBLISH_TOKEN: &'static str = "UV_PUBLISH_TOKEN";

    /// Equivalent to the `--index` command-line argument in `uv publish`. If
    /// set, uv the index with this name in the configuration for publishing.
    #[attr_added_in("0.5.8")]
    pub const UV_PUBLISH_INDEX: &'static str = "UV_PUBLISH_INDEX";

    /// Equivalent to the `--username` command-line argument in `uv publish`. If
    /// set, uv will use this username for publishing.
    #[attr_added_in("0.4.16")]
    pub const UV_PUBLISH_USERNAME: &'static str = "UV_PUBLISH_USERNAME";

    /// Equivalent to the `--password` command-line argument in `uv publish`. If
    /// set, uv will use this password for publishing.
    #[attr_added_in("0.4.16")]
    pub const UV_PUBLISH_PASSWORD: &'static str = "UV_PUBLISH_PASSWORD";

    /// Don't upload a file if it already exists on the index. The value is the URL of the index.
    #[attr_added_in("0.4.30")]
    pub const UV_PUBLISH_CHECK_URL: &'static str = "UV_PUBLISH_CHECK_URL";

    /// Equivalent to the `--no-attestations` command-line argument in `uv publish`. If set,
    /// uv will skip uploading any collected attestations for the published distributions.
    #[attr_added_in("0.9.12")]
    pub const UV_PUBLISH_NO_ATTESTATIONS: &'static str = "UV_PUBLISH_NO_ATTESTATIONS";

    /// Equivalent to the `--no-sync` command-line argument. If set, uv will skip updating
    /// the environment.
    #[attr_added_in("0.4.18")]
    pub const UV_NO_SYNC: &'static str = "UV_NO_SYNC";

    /// Equivalent to the `--locked` command-line argument. If set, uv will assert that the
    /// `uv.lock` remains unchanged.
    #[attr_added_in("0.4.25")]
    pub const UV_LOCKED: &'static str = "UV_LOCKED";

    /// Equivalent to the `--frozen` command-line argument. If set, uv will run without
    /// updating the `uv.lock` file.
    #[attr_added_in("0.4.25")]
    pub const UV_FROZEN: &'static str = "UV_FROZEN";

    /// Equivalent to the `--preview` argument. Enables preview mode.
    #[attr_added_in("0.1.37")]
    pub const UV_PREVIEW: &'static str = "UV_PREVIEW";

    /// Equivalent to the `--preview-features` argument. Enables specific preview features.
    #[attr_added_in("0.8.4")]
    pub const UV_PREVIEW_FEATURES: &'static str = "UV_PREVIEW_FEATURES";

    /// Equivalent to the `--token` argument for self update. A GitHub token for authentication.
    #[attr_added_in("0.4.10")]
    pub const UV_GITHUB_TOKEN: &'static str = "UV_GITHUB_TOKEN";

    /// Equivalent to the `--no-verify-hashes` argument. Disables hash verification for
    /// `requirements.txt` files.
    #[attr_added_in("0.5.3")]
    pub const UV_NO_VERIFY_HASHES: &'static str = "UV_NO_VERIFY_HASHES";

    /// Equivalent to the `--allow-insecure-host` argument.
    #[attr_added_in("0.3.5")]
    pub const UV_INSECURE_HOST: &'static str = "UV_INSECURE_HOST";

    /// Disable ZIP validation for streamed wheels and ZIP-based source distributions.
    ///
    /// WARNING: Disabling ZIP validation can expose your system to security risks by bypassing
    /// integrity checks and allowing uv to install potentially malicious ZIP files. If uv rejects
    /// a ZIP file due to failing validation, it is likely that the file is malformed; consider
    /// filing an issue with the package maintainer.
    #[attr_added_in("0.8.6")]
    pub const UV_INSECURE_NO_ZIP_VALIDATION: &'static str = "UV_INSECURE_NO_ZIP_VALIDATION";

    /// Sets the maximum number of in-flight concurrent downloads that uv will
    /// perform at any given time.
    #[attr_added_in("0.1.43")]
    pub const UV_CONCURRENT_DOWNLOADS: &'static str = "UV_CONCURRENT_DOWNLOADS";

    /// Sets the maximum number of source distributions that uv will build
    /// concurrently at any given time.
    #[attr_added_in("0.1.43")]
    pub const UV_CONCURRENT_BUILDS: &'static str = "UV_CONCURRENT_BUILDS";

    /// Controls the number of threads used when installing and unzipping
    /// packages.
    #[attr_added_in("0.1.45")]
    pub const UV_CONCURRENT_INSTALLS: &'static str = "UV_CONCURRENT_INSTALLS";

    /// Equivalent to the `--no-progress` command-line argument. Disables all progress output. For
    /// example, spinners and progress bars.
    #[attr_added_in("0.2.28")]
    pub const UV_NO_PROGRESS: &'static str = "UV_NO_PROGRESS";

    /// Specifies the directory where uv stores managed tools.
    #[attr_added_in("0.2.16")]
    pub const UV_TOOL_DIR: &'static str = "UV_TOOL_DIR";

    /// Specifies the "bin" directory for installing tool executables.
    #[attr_added_in("0.3.0")]
    pub const UV_TOOL_BIN_DIR: &'static str = "UV_TOOL_BIN_DIR";

    /// Equivalent to the `--build-backend` argument for `uv init`. Determines the default backend
    /// to use when creating a new project.
    #[attr_added_in("0.8.2")]
    pub const UV_INIT_BUILD_BACKEND: &'static str = "UV_INIT_BUILD_BACKEND";

    /// Specifies the path to the directory to use for a project virtual environment.
    ///
    /// See the [project documentation](../concepts/projects/config.md#project-environment-path)
    /// for more details.
    #[attr_added_in("0.4.4")]
    pub const UV_PROJECT_ENVIRONMENT: &'static str = "UV_PROJECT_ENVIRONMENT";

    /// Specifies the directory to place links to installed, managed Python executables.
    #[attr_added_in("0.4.29")]
    pub const UV_PYTHON_BIN_DIR: &'static str = "UV_PYTHON_BIN_DIR";

    /// Specifies the directory for storing managed Python installations.
    #[attr_added_in("0.2.22")]
    pub const UV_PYTHON_INSTALL_DIR: &'static str = "UV_PYTHON_INSTALL_DIR";

    /// Whether to install the Python executable into the `UV_PYTHON_BIN_DIR` directory.
    #[attr_added_in("0.8.0")]
    pub const UV_PYTHON_INSTALL_BIN: &'static str = "UV_PYTHON_INSTALL_BIN";

    /// Whether to install the Python executable into the Windows registry.
    #[attr_added_in("0.8.0")]
    pub const UV_PYTHON_INSTALL_REGISTRY: &'static str = "UV_PYTHON_INSTALL_REGISTRY";

    /// Managed Python installations information is hardcoded in the `uv` binary.
    ///
    /// This variable can be set to a local path or URL pointing to
    /// a JSON list of Python installations to override the hardcoded list.
    ///
    /// This allows customizing the URLs for downloads or using slightly older or newer versions
    /// of Python than the ones hardcoded into this build of `uv`.
    #[attr_added_in("0.6.13")]
    pub const UV_PYTHON_DOWNLOADS_JSON_URL: &'static str = "UV_PYTHON_DOWNLOADS_JSON_URL";

    /// Specifies the directory for caching the archives of managed Python installations before
    /// installation.
    #[attr_added_in("0.7.0")]
    pub const UV_PYTHON_CACHE_DIR: &'static str = "UV_PYTHON_CACHE_DIR";

    /// Managed Python installations are downloaded from the Astral
    /// [`python-build-standalone`](https://github.com/astral-sh/python-build-standalone) project.
    ///
    /// This variable can be set to a mirror URL to use a different source for Python installations.
    /// The provided URL will replace `https://github.com/astral-sh/python-build-standalone/releases/download` in, e.g.,
    /// `https://github.com/astral-sh/python-build-standalone/releases/download/20240713/cpython-3.12.4%2B20240713-aarch64-apple-darwin-install_only.tar.gz`.
    /// Distributions can be read from a local directory by using the `file://` URL scheme.
    #[attr_added_in("0.2.35")]
    pub const UV_PYTHON_INSTALL_MIRROR: &'static str = "UV_PYTHON_INSTALL_MIRROR";

    /// Managed PyPy installations are downloaded from [python.org](https://downloads.python.org/).
    ///
    /// This variable can be set to a mirror URL to use a
    /// different source for PyPy installations. The provided URL will replace
    /// `https://downloads.python.org/pypy` in, e.g.,
    /// `https://downloads.python.org/pypy/pypy3.8-v7.3.7-osx64.tar.bz2`.
    /// Distributions can be read from a local directory by using the `file://` URL scheme.
    #[attr_added_in("0.2.35")]
    pub const UV_PYPY_INSTALL_MIRROR: &'static str = "UV_PYPY_INSTALL_MIRROR";

    /// Pin managed CPython versions to a specific build version.
    ///
    /// For CPython, this should be the build date (e.g., "20250814").
    #[attr_added_in("0.8.14")]
    pub const UV_PYTHON_CPYTHON_BUILD: &'static str = "UV_PYTHON_CPYTHON_BUILD";

    /// Pin managed PyPy versions to a specific build version.
    ///
    /// For PyPy, this should be the PyPy version (e.g., "7.3.20").
    #[attr_added_in("0.8.14")]
    pub const UV_PYTHON_PYPY_BUILD: &'static str = "UV_PYTHON_PYPY_BUILD";

    /// Pin managed GraalPy versions to a specific build version.
    ///
    /// For GraalPy, this should be the GraalPy version (e.g., "24.2.2").
    #[attr_added_in("0.8.14")]
    pub const UV_PYTHON_GRAALPY_BUILD: &'static str = "UV_PYTHON_GRAALPY_BUILD";

    /// Pin managed Pyodide versions to a specific build version.
    ///
    /// For Pyodide, this should be the Pyodide version (e.g., "0.28.1").
    #[attr_added_in("0.8.14")]
    pub const UV_PYTHON_PYODIDE_BUILD: &'static str = "UV_PYTHON_PYODIDE_BUILD";

    /// Equivalent to the `--clear` command-line argument. If set, uv will remove any
    /// existing files or directories at the target path.
    #[attr_added_in("0.8.0")]
    pub const UV_VENV_CLEAR: &'static str = "UV_VENV_CLEAR";

    /// Install seed packages (one or more of: `pip`, `setuptools`, and `wheel`) into the virtual environment
    /// created by `uv venv`.
    ///
    /// Note that `setuptools` and `wheel` are not included in Python 3.12+ environments.
    #[attr_added_in("0.5.21")]
    pub const UV_VENV_SEED: &'static str = "UV_VENV_SEED";

    /// Used to override `PATH` to limit Python executable availability in the test suite.
    #[attr_hidden]
    #[attr_added_in("0.0.5")]
    pub const UV_TEST_PYTHON_PATH: &'static str = "UV_TEST_PYTHON_PATH";

    /// Include resolver and installer output related to environment modifications.
    #[attr_hidden]
    #[attr_added_in("0.2.32")]
    pub const UV_SHOW_RESOLUTION: &'static str = "UV_SHOW_RESOLUTION";

    /// Use to update the json schema files.
    #[attr_hidden]
    #[attr_added_in("0.1.34")]
    pub const UV_UPDATE_SCHEMA: &'static str = "UV_UPDATE_SCHEMA";

    /// Use to disable line wrapping for diagnostics.
    #[attr_added_in("0.0.5")]
    pub const UV_NO_WRAP: &'static str = "UV_NO_WRAP";

    /// Provides the HTTP Basic authentication username for a named index.
    ///
    /// The `name` parameter is the name of the index. For example, given an index named `foo`,
    /// the environment variable key would be `UV_INDEX_FOO_USERNAME`.
    #[attr_added_in("0.4.23")]
    #[attr_env_var_pattern("UV_INDEX_{name}_USERNAME")]
    pub fn index_username(name: &str) -> String {
        format!("UV_INDEX_{name}_USERNAME")
    }

    /// Provides the HTTP Basic authentication password for a named index.
    ///
    /// The `name` parameter is the name of the index. For example, given an index named `foo`,
    /// the environment variable key would be `UV_INDEX_FOO_PASSWORD`.
    #[attr_added_in("0.4.23")]
    #[attr_env_var_pattern("UV_INDEX_{name}_PASSWORD")]
    pub fn index_password(name: &str) -> String {
        format!("UV_INDEX_{name}_PASSWORD")
    }

    /// Used to set the uv commit hash at build time via `build.rs`.
    #[attr_hidden]
    #[attr_added_in("0.1.11")]
    pub const UV_COMMIT_HASH: &'static str = "UV_COMMIT_HASH";

    /// Used to set the uv commit short hash at build time via `build.rs`.
    #[attr_hidden]
    #[attr_added_in("0.1.11")]
    pub const UV_COMMIT_SHORT_HASH: &'static str = "UV_COMMIT_SHORT_HASH";

    /// Used to set the uv commit date at build time via `build.rs`.
    #[attr_hidden]
    #[attr_added_in("0.1.11")]
    pub const UV_COMMIT_DATE: &'static str = "UV_COMMIT_DATE";

    /// Used to set the uv tag at build time via `build.rs`.
    #[attr_hidden]
    #[attr_added_in("0.1.11")]
    pub const UV_LAST_TAG: &'static str = "UV_LAST_TAG";

    /// Used to set the uv tag distance from head at build time via `build.rs`.
    #[attr_hidden]
    #[attr_added_in("0.1.11")]
    pub const UV_LAST_TAG_DISTANCE: &'static str = "UV_LAST_TAG_DISTANCE";

    /// Used to set the spawning/parent interpreter when using --system in the test suite.
    #[attr_hidden]
    #[attr_added_in("0.2.0")]
    pub const UV_INTERNAL__PARENT_INTERPRETER: &'static str = "UV_INTERNAL__PARENT_INTERPRETER";

    /// Used to force showing the derivation tree during resolver error reporting.
    #[attr_hidden]
    #[attr_added_in("0.3.0")]
    pub const UV_INTERNAL__SHOW_DERIVATION_TREE: &'static str = "UV_INTERNAL__SHOW_DERIVATION_TREE";

    /// Used to set a temporary directory for some tests.
    #[attr_hidden]
    #[attr_added_in("0.3.4")]
    pub const UV_INTERNAL__TEST_DIR: &'static str = "UV_INTERNAL__TEST_DIR";

    /// Used to force treating an interpreter as "managed" during tests.
    #[attr_hidden]
    #[attr_added_in("0.8.0")]
    pub const UV_INTERNAL__TEST_PYTHON_MANAGED: &'static str = "UV_INTERNAL__TEST_PYTHON_MANAGED";

    /// Used to force ignoring Git LFS commands as `git-lfs` detection cannot be overridden via PATH.
    #[attr_hidden]
    #[attr_added_in("0.9.15")]
    pub const UV_INTERNAL__TEST_LFS_DISABLED: &'static str = "UV_INTERNAL__TEST_LFS_DISABLED";

    /// Path to system-level configuration directory on Unix systems.
    #[attr_added_in("0.4.26")]
    pub const XDG_CONFIG_DIRS: &'static str = "XDG_CONFIG_DIRS";

    /// Path to system-level configuration directory on Windows systems.
    #[attr_added_in("0.4.26")]
    pub const SYSTEMDRIVE: &'static str = "SYSTEMDRIVE";

    /// Path to user-level configuration directory on Windows systems.
    #[attr_added_in("0.1.42")]
    pub const APPDATA: &'static str = "APPDATA";

    /// Path to root directory of user's profile on Windows systems.
    #[attr_added_in("0.0.5")]
    pub const USERPROFILE: &'static str = "USERPROFILE";

    /// Path to user-level configuration directory on Unix systems.
    #[attr_added_in("0.1.34")]
    pub const XDG_CONFIG_HOME: &'static str = "XDG_CONFIG_HOME";

    /// Path to cache directory on Unix systems.
    #[attr_added_in("0.1.17")]
    pub const XDG_CACHE_HOME: &'static str = "XDG_CACHE_HOME";

    /// Path to directory for storing managed Python installations and tools.
    #[attr_added_in("0.2.16")]
    pub const XDG_DATA_HOME: &'static str = "XDG_DATA_HOME";

    /// Path to directory where executables are installed.
    #[attr_added_in("0.2.16")]
    pub const XDG_BIN_HOME: &'static str = "XDG_BIN_HOME";

    /// Custom certificate bundle file path for SSL connections.
    ///
    /// Takes precedence over `UV_NATIVE_TLS` when set.
    #[attr_added_in("0.1.14")]
    pub const SSL_CERT_FILE: &'static str = "SSL_CERT_FILE";

    /// Custom path for certificate bundles for SSL connections.
    /// Multiple entries are supported separated using a platform-specific
    /// delimiter (`:` on Unix, `;` on Windows).
    ///
    /// Takes precedence over `UV_NATIVE_TLS` when set.
    #[attr_added_in("0.9.10")]
    pub const SSL_CERT_DIR: &'static str = "SSL_CERT_DIR";

    /// If set, uv will use this file for mTLS authentication.
    /// This should be a single file containing both the certificate and the private key in PEM format.
    #[attr_added_in("0.2.11")]
    pub const SSL_CLIENT_CERT: &'static str = "SSL_CLIENT_CERT";

    /// Proxy for HTTP requests.
    #[attr_added_in("0.1.38")]
    pub const HTTP_PROXY: &'static str = "HTTP_PROXY";

    /// Proxy for HTTPS requests.
    #[attr_added_in("0.1.38")]
    pub const HTTPS_PROXY: &'static str = "HTTPS_PROXY";

    /// General proxy for all network requests.
    #[attr_added_in("0.1.38")]
    pub const ALL_PROXY: &'static str = "ALL_PROXY";

    /// Comma-separated list of hostnames (e.g., `example.com`) and/or patterns (e.g., `192.168.1.0/24`) that should bypass the proxy.
    #[attr_added_in("0.1.38")]
    pub const NO_PROXY: &'static str = "NO_PROXY";

    /// Timeout (in seconds) for only upload HTTP requests. (default: 900 s)
    #[attr_added_in("0.9.1")]
    pub const UV_UPLOAD_HTTP_TIMEOUT: &'static str = "UV_UPLOAD_HTTP_TIMEOUT";

    /// Timeout (in seconds) for HTTP requests. (default: 30 s)
    #[attr_added_in("0.1.7")]
    pub const UV_HTTP_TIMEOUT: &'static str = "UV_HTTP_TIMEOUT";

    /// The number of retries for HTTP requests. (default: 3)
    #[attr_added_in("0.7.21")]
    pub const UV_HTTP_RETRIES: &'static str = "UV_HTTP_RETRIES";

    /// Timeout (in seconds) for HTTP requests. Equivalent to `UV_HTTP_TIMEOUT`.
    #[attr_added_in("0.1.6")]
    pub const UV_REQUEST_TIMEOUT: &'static str = "UV_REQUEST_TIMEOUT";

    /// Timeout (in seconds) for HTTP requests. Equivalent to `UV_HTTP_TIMEOUT`.
    #[attr_added_in("0.1.7")]
    pub const HTTP_TIMEOUT: &'static str = "HTTP_TIMEOUT";

    /// The validation modes to use when run with `--compile`.
    ///
    /// See [`PycInvalidationMode`](https://docs.python.org/3/library/py_compile.html#py_compile.PycInvalidationMode).
    #[attr_added_in("0.1.7")]
    pub const PYC_INVALIDATION_MODE: &'static str = "PYC_INVALIDATION_MODE";

    /// Used to detect an activated virtual environment.
    #[attr_added_in("0.0.5")]
    pub const VIRTUAL_ENV: &'static str = "VIRTUAL_ENV";

    /// Used to detect the path of an active Conda environment.
    #[attr_added_in("0.0.5")]
    pub const CONDA_PREFIX: &'static str = "CONDA_PREFIX";

    /// Used to determine the name of the active Conda environment.
    #[attr_added_in("0.5.0")]
    pub const CONDA_DEFAULT_ENV: &'static str = "CONDA_DEFAULT_ENV";

    /// Used to determine the root install path of Conda.
    #[attr_added_in("0.8.18")]
    pub const CONDA_ROOT: &'static str = "_CONDA_ROOT";

    /// Used to determine if we're running in Dependabot.
    #[attr_added_in("0.9.11")]
    pub const DEPENDABOT: &'static str = "DEPENDABOT";

    /// If set to `1` before a virtual environment is activated, then the
    /// virtual environment name will not be prepended to the terminal prompt.
    #[attr_added_in("0.0.5")]
    pub const VIRTUAL_ENV_DISABLE_PROMPT: &'static str = "VIRTUAL_ENV_DISABLE_PROMPT";

    /// Used to detect the use of the Windows Command Prompt (as opposed to PowerShell).
    #[attr_added_in("0.1.16")]
    pub const PROMPT: &'static str = "PROMPT";

    /// Used to detect `NuShell` usage.
    #[attr_added_in("0.1.16")]
    pub const NU_VERSION: &'static str = "NU_VERSION";

    /// Used to detect Fish shell usage.
    #[attr_added_in("0.1.28")]
    pub const FISH_VERSION: &'static str = "FISH_VERSION";

    /// Used to detect Bash shell usage.
    #[attr_added_in("0.1.28")]
    pub const BASH_VERSION: &'static str = "BASH_VERSION";

    /// Used to detect Zsh shell usage.
    #[attr_added_in("0.1.28")]
    pub const ZSH_VERSION: &'static str = "ZSH_VERSION";

    /// Used to determine which `.zshenv` to use when Zsh is being used.
    #[attr_added_in("0.2.25")]
    pub const ZDOTDIR: &'static str = "ZDOTDIR";

    /// Used to detect Ksh shell usage.
    #[attr_added_in("0.2.33")]
    pub const KSH_VERSION: &'static str = "KSH_VERSION";

    /// Used with `--python-platform macos` and related variants to set the
    /// deployment target (i.e., the minimum supported macOS version).
    ///
    /// Defaults to `13.0`, the least-recent non-EOL macOS version at time of writing.
    #[attr_added_in("0.1.42")]
    pub const MACOSX_DEPLOYMENT_TARGET: &'static str = "MACOSX_DEPLOYMENT_TARGET";

    /// Used with `--python-platform arm64-apple-ios` and related variants to set the
    /// deployment target (i.e., the minimum supported iOS version).
    ///
    /// Defaults to `13.0`.
    #[attr_added_in("0.8.16")]
    pub const IPHONEOS_DEPLOYMENT_TARGET: &'static str = "IPHONEOS_DEPLOYMENT_TARGET";

    /// Used with `--python-platform aarch64-linux-android` and related variants to set the
    /// Android API level. (i.e., the minimum supported Android API level).
    ///
    /// Defaults to `24`.
    #[attr_added_in("0.8.16")]
    pub const ANDROID_API_LEVEL: &'static str = "ANDROID_API_LEVEL";

    /// Disables colored output (takes precedence over `FORCE_COLOR`).
    ///
    /// See [no-color.org](https://no-color.org).
    #[attr_added_in("0.2.7")]
    pub const NO_COLOR: &'static str = "NO_COLOR";

    /// Forces colored output regardless of terminal support.
    ///
    /// See [force-color.org](https://force-color.org).
    #[attr_added_in("0.2.7")]
    pub const FORCE_COLOR: &'static str = "FORCE_COLOR";

    /// Use to control color via `anstyle`.
    #[attr_added_in("0.1.32")]
    pub const CLICOLOR_FORCE: &'static str = "CLICOLOR_FORCE";

    /// The standard `PATH` env var.
    #[attr_added_in("0.0.5")]
    pub const PATH: &'static str = "PATH";

    /// The standard `HOME` env var.
    #[attr_added_in("0.0.5")]
    pub const HOME: &'static str = "HOME";

    /// The standard `SHELL` posix env var.
    #[attr_added_in("0.1.16")]
    pub const SHELL: &'static str = "SHELL";

    /// The standard `PWD` posix env var.
    #[attr_added_in("0.0.5")]
    pub const PWD: &'static str = "PWD";

    /// Used to look for Microsoft Store Pythons installations.
    #[attr_added_in("0.3.3")]
    pub const LOCALAPPDATA: &'static str = "LOCALAPPDATA";

    /// Path to the `.git` directory. Ignored by `uv` when performing fetch.
    #[attr_hidden]
    #[attr_added_in("0.0.5")]
    pub const GIT_DIR: &'static str = "GIT_DIR";

    /// Path to the git working tree. Ignored by `uv` when performing fetch.
    #[attr_hidden]
    #[attr_added_in("0.0.5")]
    pub const GIT_WORK_TREE: &'static str = "GIT_WORK_TREE";

    /// Path to the index file for staged changes. Ignored by `uv` when performing fetch.
    #[attr_hidden]
    #[attr_added_in("0.0.5")]
    pub const GIT_INDEX_FILE: &'static str = "GIT_INDEX_FILE";

    /// Path to where git object files are located. Ignored by `uv` when performing fetch.
    #[attr_hidden]
    #[attr_added_in("0.0.5")]
    pub const GIT_OBJECT_DIRECTORY: &'static str = "GIT_OBJECT_DIRECTORY";

    /// Alternate locations for git objects. Ignored by `uv` when performing fetch.
    #[attr_hidden]
    #[attr_added_in("0.0.5")]
    pub const GIT_ALTERNATE_OBJECT_DIRECTORIES: &'static str = "GIT_ALTERNATE_OBJECT_DIRECTORIES";

    /// Disables SSL verification for git operations.
    #[attr_hidden]
    #[attr_added_in("0.5.28")]
    pub const GIT_SSL_NO_VERIFY: &'static str = "GIT_SSL_NO_VERIFY";

    /// Sets allowed protocols for git operations.
    ///
    /// When uv is in "offline" mode, only the "file" protocol is allowed.
    #[attr_hidden]
    #[attr_added_in("0.6.13")]
    pub const GIT_ALLOW_PROTOCOL: &'static str = "GIT_ALLOW_PROTOCOL";

    /// Sets the SSH command used when Git tries to establish a connection using SSH.
    #[attr_hidden]
    #[attr_added_in("0.7.11")]
    pub const GIT_SSH_COMMAND: &'static str = "GIT_SSH_COMMAND";

    /// Disable interactive git prompts in terminals, e.g., for credentials. Does not disable
    /// GUI prompts.
    #[attr_hidden]
    #[attr_added_in("0.6.4")]
    pub const GIT_TERMINAL_PROMPT: &'static str = "GIT_TERMINAL_PROMPT";

    /// Skip Smudge LFS Filter.
    #[attr_hidden]
    #[attr_added_in("0.9.15")]
    pub const GIT_LFS_SKIP_SMUDGE: &'static str = "GIT_LFS_SKIP_SMUDGE";

    /// Used in tests to set the user global git config location.
    #[attr_hidden]
    #[attr_added_in("0.9.15")]
    pub const GIT_CONFIG_GLOBAL: &'static str = "GIT_CONFIG_GLOBAL";

    /// Used in tests for better git isolation.
    ///
    /// For example, we run some tests in ~/.local/share/uv/tests.
    /// And if the user's `$HOME` directory is a git repository,
    /// this will change the behavior of some tests. Setting
    /// `GIT_CEILING_DIRECTORIES=/home/andrew/.local/share/uv/tests` will
    /// prevent git from crawling up the directory tree past that point to find
    /// parent git repositories.
    #[attr_hidden]
    #[attr_added_in("0.4.29")]
    pub const GIT_CEILING_DIRECTORIES: &'static str = "GIT_CEILING_DIRECTORIES";

    /// Indicates that the current process is running in GitHub Actions.
    ///
    /// `uv publish` may attempt trusted publishing flows when set
    /// to `true`.
    #[attr_added_in("0.4.16")]
    pub const GITHUB_ACTIONS: &'static str = "GITHUB_ACTIONS";

    /// Indicates that the current process is running in GitLab CI.
    ///
    /// `uv publish` may attempt trusted publishing flows when set
    /// to `true`.
    #[attr_added_in("0.8.18")]
    pub const GITLAB_CI: &'static str = "GITLAB_CI";

    /// Used for testing GitLab CI trusted publishing.
    #[attr_hidden]
    #[attr_added_in("0.8.18")]
    pub const PYPI_ID_TOKEN: &'static str = "PYPI_ID_TOKEN";

    /// Used for testing GitLab CI trusted publishing.
    #[attr_hidden]
    #[attr_added_in("0.8.18")]
    pub const TESTPYPI_ID_TOKEN: &'static str = "TESTPYPI_ID_TOKEN";

    /// Sets the encoding for standard I/O streams (e.g., PYTHONIOENCODING=utf-8).
    #[attr_hidden]
    #[attr_added_in("0.4.18")]
    pub const PYTHONIOENCODING: &'static str = "PYTHONIOENCODING";

    /// Forces unbuffered I/O streams, equivalent to `-u` in Python.
    #[attr_hidden]
    #[attr_added_in("0.1.15")]
    pub const PYTHONUNBUFFERED: &'static str = "PYTHONUNBUFFERED";

    /// Enables UTF-8 mode for Python, equivalent to `-X utf8`.
    #[attr_hidden]
    #[attr_added_in("0.4.19")]
    pub const PYTHONUTF8: &'static str = "PYTHONUTF8";

    /// Adds directories to Python module search path (e.g., `PYTHONPATH=/path/to/modules`).
    #[attr_added_in("0.1.22")]
    pub const PYTHONPATH: &'static str = "PYTHONPATH";

    /// Used to set the location of Python stdlib when using trampolines.
    #[attr_hidden]
    #[attr_added_in("0.7.13")]
    pub const PYTHONHOME: &'static str = "PYTHONHOME";

    /// Used to correctly detect virtual environments when using trampolines.
    #[attr_hidden]
    #[attr_added_in("0.7.13")]
    pub const PYVENV_LAUNCHER: &'static str = "__PYVENV_LAUNCHER__";

    /// Used in tests to enforce a consistent locale setting.
    #[attr_hidden]
    #[attr_added_in("0.4.28")]
    pub const LC_ALL: &'static str = "LC_ALL";

    /// Typically set by CI runners, used to detect a CI runner.
    #[attr_hidden]
    #[attr_added_in("0.0.5")]
    pub const CI: &'static str = "CI";

    /// Azure DevOps build identifier, used to detect CI environments.
    #[attr_hidden]
    #[attr_added_in("0.1.22")]
    pub const BUILD_BUILDID: &'static str = "BUILD_BUILDID";

    /// Generic build identifier, used to detect CI environments.
    #[attr_hidden]
    #[attr_added_in("0.1.22")]
    pub const BUILD_ID: &'static str = "BUILD_ID";

    /// Pip environment variable to indicate CI environment.
    #[attr_hidden]
    #[attr_added_in("0.1.22")]
    pub const PIP_IS_CI: &'static str = "PIP_IS_CI";

    /// Use to set the .netrc file location.
    #[attr_added_in("0.1.16")]
    pub const NETRC: &'static str = "NETRC";

    /// The standard `PAGER` posix env var. Used by `uv` to configure the appropriate pager.
    #[attr_added_in("0.4.18")]
    pub const PAGER: &'static str = "PAGER";

    /// Used to detect when running inside a Jupyter notebook.
    #[attr_added_in("0.2.6")]
    pub const JPY_SESSION_NAME: &'static str = "JPY_SESSION_NAME";

    /// Use to create the tracing root directory via the `tracing-durations-export` feature.
    #[attr_hidden]
    #[attr_added_in("0.1.32")]
    pub const TRACING_DURATIONS_TEST_ROOT: &'static str = "TRACING_DURATIONS_TEST_ROOT";

    /// Use to create the tracing durations file via the `tracing-durations-export` feature.
    #[attr_added_in("0.0.5")]
    pub const TRACING_DURATIONS_FILE: &'static str = "TRACING_DURATIONS_FILE";

    /// Used to set `RUST_HOST_TARGET` at build time via `build.rs`.
    #[attr_hidden]
    #[attr_added_in("0.1.11")]
    pub const TARGET: &'static str = "TARGET";

    /// If set, uv will use this value as the log level for its `--verbose` output. Accepts
    /// any filter compatible with the `tracing_subscriber` crate.
    ///
    /// For example:
    ///
    /// * `RUST_LOG=uv=debug` is the equivalent of adding `--verbose` to the command line
    /// * `RUST_LOG=trace` will enable trace-level logging.
    ///
    /// See the [tracing documentation](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#example-syntax)
    /// for more.
    #[attr_added_in("0.0.5")]
    pub const RUST_LOG: &'static str = "RUST_LOG";

    /// If set, it can be used to display more stack trace details when a panic occurs.
    /// This is used by uv particularly on windows to show more details during a platform exception.
    ///
    /// For example:
    ///
    /// * `RUST_BACKTRACE=1` will print a short backtrace.
    /// * `RUST_BACKTRACE=full` will print a full backtrace.
    ///
    /// See the [Rust backtrace documentation](https://doc.rust-lang.org/std/backtrace/index.html)
    /// for more.
    #[attr_added_in("0.7.22")]
    pub const RUST_BACKTRACE: &'static str = "RUST_BACKTRACE";

    /// Add additional context and structure to log messages.
    ///
    /// If logging is not enabled, e.g., with `RUST_LOG` or `-v`, this has no effect.
    #[attr_added_in("0.6.4")]
    pub const UV_LOG_CONTEXT: &'static str = "UV_LOG_CONTEXT";

    /// Use to set the stack size used by uv.
    ///
    /// The value is in bytes, and if both `UV_STACK_SIZE` are `RUST_MIN_STACK` unset, uv uses a 4MB
    /// (4194304) stack. `UV_STACK_SIZE` takes precedence over `RUST_MIN_STACK`.
    ///
    /// Unlike the normal `RUST_MIN_STACK` semantics, this can affect main thread
    /// stack size, because we actually spawn our own main2 thread to work around
    /// the fact that Windows' real main thread is only 1MB. That thread has size
    /// `max(UV_STACK_SIZE, 1MB)`.
    #[attr_added_in("0.0.5")]
    pub const UV_STACK_SIZE: &'static str = "UV_STACK_SIZE";

    /// Use to set the stack size used by uv.
    ///
    /// The value is in bytes, and if both `UV_STACK_SIZE` are `RUST_MIN_STACK` unset, uv uses a 4MB
    /// (4194304) stack. `UV_STACK_SIZE` takes precedence over `RUST_MIN_STACK`.
    ///
    /// Prefer setting `UV_STACK_SIZE`, since `RUST_MIN_STACK` also affects subprocesses, such as
    /// build backends that use Rust code.
    ///
    /// Unlike the normal `RUST_MIN_STACK` semantics, this can affect main thread
    /// stack size, because we actually spawn our own main2 thread to work around
    /// the fact that Windows' real main thread is only 1MB. That thread has size
    /// `max(RUST_MIN_STACK, 1MB)`.
    #[attr_added_in("0.5.19")]
    pub const RUST_MIN_STACK: &'static str = "RUST_MIN_STACK";

    /// The directory containing the `Cargo.toml` manifest for a package.
    #[attr_hidden]
    #[attr_added_in("0.1.11")]
    pub const CARGO_MANIFEST_DIR: &'static str = "CARGO_MANIFEST_DIR";

    /// Specifies the directory where Cargo stores build artifacts (target directory).
    #[attr_hidden]
    #[attr_added_in("0.0.5")]
    pub const CARGO_TARGET_DIR: &'static str = "CARGO_TARGET_DIR";

    /// Set by cargo when compiling for Windows-like platforms.
    #[attr_hidden]
    #[attr_added_in("0.0.5")]
    pub const CARGO_CFG_WINDOWS: &'static str = "CARGO_CFG_WINDOWS";

    /// Specifies the directory where Cargo stores intermediate build artifacts.
    #[attr_hidden]
    #[attr_added_in("0.8.25")]
    pub const OUT_DIR: &'static str = "OUT_DIR";

    /// Used in tests for environment substitution testing in `requirements.in`.
    #[attr_hidden]
    #[attr_added_in("0.1.18")]
    pub const URL: &'static str = "URL";

    /// Used in tests for environment substitution testing in `requirements.in`.
    #[attr_hidden]
    #[attr_added_in("0.1.18")]
    pub const FILE_PATH: &'static str = "FILE_PATH";

    /// Used in tests for environment substitution testing in `requirements.in`.
    #[attr_hidden]
    #[attr_added_in("0.1.25")]
    pub const HATCH_PATH: &'static str = "HATCH_PATH";

    /// Used in tests for environment substitution testing in `requirements.in`.
    #[attr_hidden]
    #[attr_added_in("0.1.25")]
    pub const BLACK_PATH: &'static str = "BLACK_PATH";

    /// Used in testing Hatch's root.uri feature
    ///
    /// See: <https://hatch.pypa.io/dev/config/dependency/#local>.
    #[attr_hidden]
    #[attr_added_in("0.1.22")]
    pub const ROOT_PATH: &'static str = "ROOT_PATH";

    /// Used in testing extra build dependencies.
    #[attr_hidden]
    #[attr_added_in("0.8.5")]
    pub const EXPECTED_ANYIO_VERSION: &'static str = "EXPECTED_ANYIO_VERSION";

    /// Used to set test credentials for keyring tests.
    #[attr_hidden]
    #[attr_added_in("0.1.34")]
    pub const KEYRING_TEST_CREDENTIALS: &'static str = "KEYRING_TEST_CREDENTIALS";

    /// Used to disable delay for HTTP retries in tests.
    #[attr_added_in("0.7.21")]
    pub const UV_TEST_NO_HTTP_RETRY_DELAY: &'static str = "UV_TEST_NO_HTTP_RETRY_DELAY";

    /// Used to set a packse index url for tests.
    #[attr_hidden]
    #[attr_added_in("0.2.12")]
    pub const UV_TEST_PACKSE_INDEX: &'static str = "UV_TEST_PACKSE_INDEX";

    /// Used for testing named indexes in tests.
    #[attr_hidden]
    #[attr_added_in("0.5.21")]
    pub const UV_INDEX_MY_INDEX_USERNAME: &'static str = "UV_INDEX_MY_INDEX_USERNAME";

    /// Used for testing named indexes in tests.
    #[attr_hidden]
    #[attr_added_in("0.5.21")]
    pub const UV_INDEX_MY_INDEX_PASSWORD: &'static str = "UV_INDEX_MY_INDEX_PASSWORD";

    /// Used to set the GitHub fast-path url for tests.
    #[attr_hidden]
    #[attr_added_in("0.7.15")]
    pub const UV_GITHUB_FAST_PATH_URL: &'static str = "UV_GITHUB_FAST_PATH_URL";

    /// Hide progress messages with non-deterministic order in tests.
    #[attr_hidden]
    #[attr_added_in("0.5.29")]
    pub const UV_TEST_NO_CLI_PROGRESS: &'static str = "UV_TEST_NO_CLI_PROGRESS";

    /// Used to mock the current timestamp for relative `--exclude-newer` times in tests.
    /// Should be set to an RFC 3339 timestamp (e.g., `2025-11-21T12:00:00Z`).
    #[attr_hidden]
    #[attr_added_in("0.9.8")]
    pub const UV_TEST_CURRENT_TIMESTAMP: &'static str = "UV_TEST_CURRENT_TIMESTAMP";

    /// `.env` files from which to load environment variables when executing `uv run` commands.
    #[attr_added_in("0.4.30")]
    pub const UV_ENV_FILE: &'static str = "UV_ENV_FILE";

    /// Ignore `.env` files when executing `uv run` commands.
    #[attr_added_in("0.4.30")]
    pub const UV_NO_ENV_FILE: &'static str = "UV_NO_ENV_FILE";

    /// The URL from which to download uv using the standalone installer and `self update` feature,
    /// in lieu of the default GitHub URL.
    #[attr_added_in("0.5.0")]
    pub const UV_INSTALLER_GITHUB_BASE_URL: &'static str = "UV_INSTALLER_GITHUB_BASE_URL";

    /// The URL from which to download uv using the standalone installer and `self update` feature,
    /// in lieu of the default GitHub Enterprise URL.
    #[attr_added_in("0.5.0")]
    pub const UV_INSTALLER_GHE_BASE_URL: &'static str = "UV_INSTALLER_GHE_BASE_URL";

    /// The directory in which to install uv using the standalone installer and `self update` feature.
    /// Defaults to `~/.local/bin`.
    #[attr_added_in("0.5.0")]
    pub const UV_INSTALL_DIR: &'static str = "UV_INSTALL_DIR";

    /// Used ephemeral environments like CI to install uv to a specific path while preventing
    /// the installer from modifying shell profiles or environment variables.
    #[attr_added_in("0.5.0")]
    pub const UV_UNMANAGED_INSTALL: &'static str = "UV_UNMANAGED_INSTALL";

    /// The URL from which to download uv using the standalone installer. By default, installs from
    /// uv's GitHub Releases. `INSTALLER_DOWNLOAD_URL` is also supported as an alias, for backwards
    /// compatibility.
    #[attr_added_in("0.8.4")]
    pub const UV_DOWNLOAD_URL: &'static str = "UV_DOWNLOAD_URL";

    /// Avoid modifying the `PATH` environment variable when installing uv using the standalone
    /// installer and `self update` feature. `INSTALLER_NO_MODIFY_PATH` is also supported as an
    /// alias, for backwards compatibility.
    #[attr_added_in("0.8.4")]
    pub const UV_NO_MODIFY_PATH: &'static str = "UV_NO_MODIFY_PATH";

    /// Skip writing `uv` installer metadata files (e.g., `INSTALLER`, `REQUESTED`, and `direct_url.json`) to site-packages `.dist-info` directories.
    #[attr_added_in("0.5.7")]
    pub const UV_NO_INSTALLER_METADATA: &'static str = "UV_NO_INSTALLER_METADATA";

    /// Enables fetching files stored in Git LFS when installing a package from a Git repository.
    #[attr_added_in("0.5.19")]
    pub const UV_GIT_LFS: &'static str = "UV_GIT_LFS";

    /// Number of times that `uv run` has been recursively invoked. Used to guard against infinite
    /// recursion, e.g., when `uv run`` is used in a script shebang.
    #[attr_hidden]
    #[attr_added_in("0.5.31")]
    pub const UV_RUN_RECURSION_DEPTH: &'static str = "UV_RUN_RECURSION_DEPTH";

    /// Number of times that `uv run` will allow recursive invocations, before exiting with an
    /// error.
    #[attr_hidden]
    #[attr_added_in("0.5.31")]
    pub const UV_RUN_MAX_RECURSION_DEPTH: &'static str = "UV_RUN_MAX_RECURSION_DEPTH";

    /// Overrides terminal width used for wrapping. This variable is not read by uv directly.
    ///
    /// This is a quasi-standard variable, described, e.g., in `ncurses(3x)`.
    #[attr_added_in("0.6.2")]
    pub const COLUMNS: &'static str = "COLUMNS";

    /// The CUDA driver version to assume when inferring the PyTorch backend (e.g., `550.144.03`).
    #[attr_hidden]
    #[attr_added_in("0.6.9")]
    pub const UV_CUDA_DRIVER_VERSION: &'static str = "UV_CUDA_DRIVER_VERSION";

    /// The AMD GPU architecture to assume when inferring the PyTorch backend (e.g., `gfx1100`).
    #[attr_hidden]
    #[attr_added_in("0.7.14")]
    pub const UV_AMD_GPU_ARCHITECTURE: &'static str = "UV_AMD_GPU_ARCHITECTURE";

    /// Equivalent to the `--torch-backend` command-line argument (e.g., `cpu`, `cu126`, or `auto`).
    #[attr_added_in("0.6.9")]
    pub const UV_TORCH_BACKEND: &'static str = "UV_TORCH_BACKEND";

    /// Equivalent to the `--project` command-line argument.
    #[attr_added_in("0.4.4")]
    pub const UV_PROJECT: &'static str = "UV_PROJECT";

    /// Equivalent to the `--directory` command-line argument. `UV_WORKING_DIRECTORY` (added in
    /// v0.9.1) is also supported for backwards compatibility.
    #[attr_added_in("next version")]
    pub const UV_WORKING_DIR: &'static str = "UV_WORKING_DIR";

    /// Equivalent to the `--directory` command-line argument.
    #[attr_hidden]
    #[attr_added_in("0.9.1")]
    pub const UV_WORKING_DIRECTORY: &'static str = "UV_WORKING_DIRECTORY";

    /// Disable GitHub-specific requests that allow uv to skip `git fetch` in some circumstances.
    #[attr_added_in("0.7.13")]
    pub const UV_NO_GITHUB_FAST_PATH: &'static str = "UV_NO_GITHUB_FAST_PATH";

    /// Authentication token for Hugging Face requests. When set, uv will use this token
    /// when making requests to `https://huggingface.co/` and any subdomains.
    #[attr_added_in("0.8.1")]
    pub const HF_TOKEN: &'static str = "HF_TOKEN";

    /// Disable Hugging Face authentication, even if `HF_TOKEN` is set.
    #[attr_added_in("0.8.1")]
    pub const UV_NO_HF_TOKEN: &'static str = "UV_NO_HF_TOKEN";

    /// The URL to treat as an S3-compatible storage endpoint. Requests to this endpoint
    /// will be signed using AWS Signature Version 4 based on the `AWS_ACCESS_KEY_ID`,
    /// `AWS_SECRET_ACCESS_KEY`, `AWS_PROFILE`, and `AWS_CONFIG_FILE` environment variables.
    #[attr_added_in("0.8.21")]
    pub const UV_S3_ENDPOINT_URL: &'static str = "UV_S3_ENDPOINT_URL";

    /// The URL of the pyx Simple API server.
    #[attr_added_in("0.8.15")]
    pub const PYX_API_URL: &'static str = "PYX_API_URL";

    /// The domain of the pyx CDN.
    #[attr_added_in("0.8.15")]
    pub const PYX_CDN_DOMAIN: &'static str = "PYX_CDN_DOMAIN";

    /// The pyx API key (e.g., `sk-pyx-...`).
    #[attr_added_in("0.8.15")]
    pub const PYX_API_KEY: &'static str = "PYX_API_KEY";

    /// The pyx API key, for backwards compatibility.
    #[attr_hidden]
    #[attr_added_in("0.8.15")]
    pub const UV_API_KEY: &'static str = "UV_API_KEY";

    /// The pyx authentication token (e.g., `eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9...`), as output by `uv auth token`.
    #[attr_added_in("0.8.15")]
    pub const PYX_AUTH_TOKEN: &'static str = "PYX_AUTH_TOKEN";

    /// The pyx authentication token, for backwards compatibility.
    #[attr_hidden]
    #[attr_added_in("0.8.15")]
    pub const UV_AUTH_TOKEN: &'static str = "UV_AUTH_TOKEN";

    /// Specifies the directory where uv stores pyx credentials.
    #[attr_added_in("0.8.15")]
    pub const PYX_CREDENTIALS_DIR: &'static str = "PYX_CREDENTIALS_DIR";

    /// The AWS region to use when signing S3 requests.
    #[attr_added_in("0.8.21")]
    pub const AWS_REGION: &'static str = "AWS_REGION";

    /// The default AWS region to use when signing S3 requests, if `AWS_REGION` is not set.
    #[attr_added_in("0.8.21")]
    pub const AWS_DEFAULT_REGION: &'static str = "AWS_DEFAULT_REGION";

    /// The AWS access key ID to use when signing S3 requests.
    #[attr_added_in("0.8.21")]
    pub const AWS_ACCESS_KEY_ID: &'static str = "AWS_ACCESS_KEY_ID";

    /// The AWS secret access key to use when signing S3 requests.
    #[attr_added_in("0.8.21")]
    pub const AWS_SECRET_ACCESS_KEY: &'static str = "AWS_SECRET_ACCESS_KEY";

    /// The AWS session token to use when signing S3 requests.
    #[attr_added_in("0.8.21")]
    pub const AWS_SESSION_TOKEN: &'static str = "AWS_SESSION_TOKEN";

    /// The AWS profile to use when signing S3 requests.
    #[attr_added_in("0.8.21")]
    pub const AWS_PROFILE: &'static str = "AWS_PROFILE";

    /// The AWS config file to use when signing S3 requests.
    #[attr_added_in("0.8.21")]
    pub const AWS_CONFIG_FILE: &'static str = "AWS_CONFIG_FILE";

    /// The AWS shared credentials file to use when signing S3 requests.
    #[attr_added_in("0.8.21")]
    pub const AWS_SHARED_CREDENTIALS_FILE: &'static str = "AWS_SHARED_CREDENTIALS_FILE";

    /// Avoid verifying that wheel filenames match their contents when installing wheels. This
    /// is not recommended, as wheels with inconsistent filenames should be considered invalid and
    /// corrected by the relevant package maintainers; however, this option can be used to work
    /// around invalid artifacts in rare cases.
    #[attr_added_in("0.8.23")]
    pub const UV_SKIP_WHEEL_FILENAME_CHECK: &'static str = "UV_SKIP_WHEEL_FILENAME_CHECK";

    /// Suppress output from the build backend when building source distributions, even in the event
    /// of build failures.
    #[attr_added_in("0.9.15")]
    pub const UV_HIDE_BUILD_OUTPUT: &'static str = "UV_HIDE_BUILD_OUTPUT";

    /// The time in seconds uv waits for a file lock to become available.
    ///
    /// Defaults to 300s (5 min).
    #[attr_added_in("0.9.4")]
    pub const UV_LOCK_TIMEOUT: &'static str = "UV_LOCK_TIMEOUT";
}
