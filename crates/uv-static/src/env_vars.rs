use uv_macros::{attr_env_var_pattern, attr_hidden, attribute_env_vars_metadata};

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
    pub const UV: &'static str = "UV";

    /// Equivalent to the `--offline` command-line argument. If set, uv will disable network access.
    pub const UV_OFFLINE: &'static str = "UV_OFFLINE";

    /// Equivalent to the `--default-index` command-line argument. If set, uv will use
    /// this URL as the default index when searching for packages.
    pub const UV_DEFAULT_INDEX: &'static str = "UV_DEFAULT_INDEX";

    /// Equivalent to the `--index` command-line argument. If set, uv will use this
    /// space-separated list of URLs as additional indexes when searching for packages.
    pub const UV_INDEX: &'static str = "UV_INDEX";

    /// Equivalent to the `--index-url` command-line argument. If set, uv will use this
    /// URL as the default index when searching for packages.
    /// (Deprecated: use `UV_DEFAULT_INDEX` instead.)
    pub const UV_INDEX_URL: &'static str = "UV_INDEX_URL";

    /// Equivalent to the `--extra-index-url` command-line argument. If set, uv will
    /// use this space-separated list of URLs as additional indexes when searching for packages.
    /// (Deprecated: use `UV_INDEX` instead.)
    pub const UV_EXTRA_INDEX_URL: &'static str = "UV_EXTRA_INDEX_URL";

    /// Equivalent to the `--find-links` command-line argument. If set, uv will use this
    /// comma-separated list of additional locations to search for packages.
    pub const UV_FIND_LINKS: &'static str = "UV_FIND_LINKS";

    /// Equivalent to the `--cache-dir` command-line argument. If set, uv will use this
    /// directory for caching instead of the default cache directory.
    pub const UV_CACHE_DIR: &'static str = "UV_CACHE_DIR";

    /// Equivalent to the `--no-cache` command-line argument. If set, uv will not use the
    /// cache for any operations.
    pub const UV_NO_CACHE: &'static str = "UV_NO_CACHE";

    /// Equivalent to the `--resolution` command-line argument. For example, if set to
    /// `lowest-direct`, uv will install the lowest compatible versions of all direct dependencies.
    pub const UV_RESOLUTION: &'static str = "UV_RESOLUTION";

    /// Equivalent to the `--prerelease` command-line argument. For example, if set to
    /// `allow`, uv will allow pre-release versions for all dependencies.
    pub const UV_PRERELEASE: &'static str = "UV_PRERELEASE";

    /// Equivalent to the `--fork-strategy` argument. Controls version selection during universal
    /// resolution.
    pub const UV_FORK_STRATEGY: &'static str = "UV_FORK_STRATEGY";

    /// Equivalent to the `--system` command-line argument. If set to `true`, uv will
    /// use the first Python interpreter found in the system `PATH`.
    ///
    /// WARNING: `UV_SYSTEM_PYTHON=true` is intended for use in continuous integration (CI)
    /// or containerized environments and should be used with caution, as modifying the system
    /// Python can lead to unexpected behavior.
    pub const UV_SYSTEM_PYTHON: &'static str = "UV_SYSTEM_PYTHON";

    /// Equivalent to the `--python` command-line argument. If set to a path, uv will use
    /// this Python interpreter for all operations.
    pub const UV_PYTHON: &'static str = "UV_PYTHON";

    /// Equivalent to the `--break-system-packages` command-line argument. If set to `true`,
    /// uv will allow the installation of packages that conflict with system-installed packages.
    ///
    /// WARNING: `UV_BREAK_SYSTEM_PACKAGES=true` is intended for use in continuous integration
    /// (CI) or containerized environments and should be used with caution, as modifying the system
    /// Python can lead to unexpected behavior.
    pub const UV_BREAK_SYSTEM_PACKAGES: &'static str = "UV_BREAK_SYSTEM_PACKAGES";

    /// Equivalent to the `--native-tls` command-line argument. If set to `true`, uv will
    /// use the system's trust store instead of the bundled `webpki-roots` crate.
    pub const UV_NATIVE_TLS: &'static str = "UV_NATIVE_TLS";

    /// Equivalent to the `--index-strategy` command-line argument.
    ///
    /// For example, if set to `unsafe-any-match`, uv will consider versions of a given package
    /// available across all index URLs, rather than limiting its search to the first index URL
    /// that contains the package.
    pub const UV_INDEX_STRATEGY: &'static str = "UV_INDEX_STRATEGY";

    /// Equivalent to the `--require-hashes` command-line argument. If set to `true`,
    /// uv will require that all dependencies have a hash specified in the requirements file.
    pub const UV_REQUIRE_HASHES: &'static str = "UV_REQUIRE_HASHES";

    /// Equivalent to the `--constraint` command-line argument. If set, uv will use this
    /// file as the constraints file. Uses space-separated list of files.
    pub const UV_CONSTRAINT: &'static str = "UV_CONSTRAINT";

    /// Equivalent to the `--build-constraint` command-line argument. If set, uv will use this file
    /// as constraints for any source distribution builds. Uses space-separated list of files.
    pub const UV_BUILD_CONSTRAINT: &'static str = "UV_BUILD_CONSTRAINT";

    /// Equivalent to the `--override` command-line argument. If set, uv will use this file
    /// as the overrides file. Uses space-separated list of files.
    pub const UV_OVERRIDE: &'static str = "UV_OVERRIDE";

    /// Equivalent to the `--link-mode` command-line argument. If set, uv will use this as
    /// a link mode.
    pub const UV_LINK_MODE: &'static str = "UV_LINK_MODE";

    /// Equivalent to the `--no-build-isolation` command-line argument. If set, uv will
    /// skip isolation when building source distributions.
    pub const UV_NO_BUILD_ISOLATION: &'static str = "UV_NO_BUILD_ISOLATION";

    /// Equivalent to the `--custom-compile-command` command-line argument.
    ///
    /// Used to override uv in the output header of the `requirements.txt` files generated by
    /// `uv pip compile`. Intended for use-cases in which `uv pip compile` is called from within a wrapper
    /// script, to include the name of the wrapper script in the output file.
    pub const UV_CUSTOM_COMPILE_COMMAND: &'static str = "UV_CUSTOM_COMPILE_COMMAND";

    /// Equivalent to the `--keyring-provider` command-line argument. If set, uv
    /// will use this value as the keyring provider.
    pub const UV_KEYRING_PROVIDER: &'static str = "UV_KEYRING_PROVIDER";

    /// Equivalent to the `--config-file` command-line argument. Expects a path to a
    /// local `uv.toml` file to use as the configuration file.
    pub const UV_CONFIG_FILE: &'static str = "UV_CONFIG_FILE";

    /// Equivalent to the `--no-config` command-line argument. If set, uv will not read
    /// any configuration files from the current directory, parent directories, or user configuration
    /// directories.
    pub const UV_NO_CONFIG: &'static str = "UV_NO_CONFIG";

    /// Equivalent to the `--exclude-newer` command-line argument. If set, uv will
    /// exclude distributions published after the specified date.
    pub const UV_EXCLUDE_NEWER: &'static str = "UV_EXCLUDE_NEWER";

    /// Equivalent to the `--python-preference` command-line argument. Whether uv
    /// should prefer system or managed Python versions.
    pub const UV_PYTHON_PREFERENCE: &'static str = "UV_PYTHON_PREFERENCE";

    /// Equivalent to the
    /// [`python-downloads`](../reference/settings.md#python-downloads) setting and, when disabled, the
    /// `--no-python-downloads` option. Whether uv should allow Python downloads.
    pub const UV_PYTHON_DOWNLOADS: &'static str = "UV_PYTHON_DOWNLOADS";

    /// Equivalent to the `--compile-bytecode` command-line argument. If set, uv
    /// will compile Python source files to bytecode after installation.
    pub const UV_COMPILE_BYTECODE: &'static str = "UV_COMPILE_BYTECODE";

    /// Equivalent to the `--no-binary` command-line argument. If set, uv will install
    /// all packages from source. The resolver will still use pre-built wheels to
    /// extract package metadata, if available.
    pub const UV_NO_BINARY: &'static str = "UV_NO_BINARY";

    /// Equivalent to the `--no-binary-package` command line argument. If set, uv will
    /// not use pre-built wheels for the given space-delimited list of packages.
    pub const UV_NO_BINARY_PACKAGE: &'static str = "UV_NO_BINARY_PACKAGE";

    /// Equivalent to the `--no-build` command-line argument. If set, uv will not build
    /// source distributions.
    pub const UV_NO_BUILD: &'static str = "UV_NO_BUILD";

    /// Equivalent to the `--no-build-package` command line argument. If set, uv will
    /// not build source distributions for the given space-delimited list of packages.
    pub const UV_NO_BUILD_PACKAGE: &'static str = "UV_NO_BUILD_PACKAGE";

    /// Equivalent to the `--publish-url` command-line argument. The URL of the upload
    /// endpoint of the index to use with `uv publish`.
    pub const UV_PUBLISH_URL: &'static str = "UV_PUBLISH_URL";

    /// Equivalent to the `--token` command-line argument in `uv publish`. If set, uv
    /// will use this token (with the username `__token__`) for publishing.
    pub const UV_PUBLISH_TOKEN: &'static str = "UV_PUBLISH_TOKEN";

    /// Equivalent to the `--index` command-line argument in `uv publish`. If
    /// set, uv the index with this name in the configuration for publishing.
    pub const UV_PUBLISH_INDEX: &'static str = "UV_PUBLISH_INDEX";

    /// Equivalent to the `--username` command-line argument in `uv publish`. If
    /// set, uv will use this username for publishing.
    pub const UV_PUBLISH_USERNAME: &'static str = "UV_PUBLISH_USERNAME";

    /// Equivalent to the `--password` command-line argument in `uv publish`. If
    /// set, uv will use this password for publishing.
    pub const UV_PUBLISH_PASSWORD: &'static str = "UV_PUBLISH_PASSWORD";

    /// Don't upload a file if it already exists on the index. The value is the URL of the index.
    pub const UV_PUBLISH_CHECK_URL: &'static str = "UV_PUBLISH_CHECK_URL";

    /// Equivalent to the `--no-sync` command-line argument. If set, uv will skip updating
    /// the environment.
    pub const UV_NO_SYNC: &'static str = "UV_NO_SYNC";

    /// Equivalent to the `--locked` command-line argument. If set, uv will assert that the
    /// `uv.lock` remains unchanged.
    pub const UV_LOCKED: &'static str = "UV_LOCKED";

    /// Equivalent to the `--frozen` command-line argument. If set, uv will run without
    /// updating the `uv.lock` file.
    pub const UV_FROZEN: &'static str = "UV_FROZEN";

    /// Equivalent to the `--preview` argument. Enables preview mode.
    pub const UV_PREVIEW: &'static str = "UV_PREVIEW";

    /// Equivalent to the `--token` argument for self update. A GitHub token for authentication.
    pub const UV_GITHUB_TOKEN: &'static str = "UV_GITHUB_TOKEN";

    /// Equivalent to the `--no-verify-hashes` argument. Disables hash verification for
    /// `requirements.txt` files.
    pub const UV_NO_VERIFY_HASHES: &'static str = "UV_NO_VERIFY_HASHES";

    /// Equivalent to the `--allow-insecure-host` argument.
    pub const UV_INSECURE_HOST: &'static str = "UV_INSECURE_HOST";

    /// Sets the maximum number of in-flight concurrent downloads that uv will
    /// perform at any given time.
    pub const UV_CONCURRENT_DOWNLOADS: &'static str = "UV_CONCURRENT_DOWNLOADS";

    /// Sets the maximum number of source distributions that uv will build
    /// concurrently at any given time.
    pub const UV_CONCURRENT_BUILDS: &'static str = "UV_CONCURRENT_BUILDS";

    /// Controls the number of threads used when installing and unzipping
    /// packages.
    pub const UV_CONCURRENT_INSTALLS: &'static str = "UV_CONCURRENT_INSTALLS";

    /// Equivalent to the `--no-progress` command-line argument. Disables all progress output. For
    /// example, spinners and progress bars.
    pub const UV_NO_PROGRESS: &'static str = "UV_NO_PROGRESS";

    /// Specifies the directory where uv stores managed tools.
    pub const UV_TOOL_DIR: &'static str = "UV_TOOL_DIR";

    /// Specifies the "bin" directory for installing tool executables.
    pub const UV_TOOL_BIN_DIR: &'static str = "UV_TOOL_BIN_DIR";

    /// Specifies the path to the directory to use for a project virtual environment.
    ///
    /// See the [project documentation](../concepts/projects/config.md#project-environment-path)
    /// for more details.
    pub const UV_PROJECT_ENVIRONMENT: &'static str = "UV_PROJECT_ENVIRONMENT";

    /// Specifies the directory to place links to installed, managed Python executables.
    pub const UV_PYTHON_BIN_DIR: &'static str = "UV_PYTHON_BIN_DIR";

    /// Specifies the directory for storing managed Python installations.
    pub const UV_PYTHON_INSTALL_DIR: &'static str = "UV_PYTHON_INSTALL_DIR";

    /// Managed Python installations are downloaded from the Astral
    /// [`python-build-standalone`](https://github.com/astral-sh/python-build-standalone) project.
    ///
    /// This variable can be set to a mirror URL to use a different source for Python installations.
    /// The provided URL will replace `https://github.com/astral-sh/python-build-standalone/releases/download` in, e.g.,
    /// `https://github.com/astral-sh/python-build-standalone/releases/download/20240713/cpython-3.12.4%2B20240713-aarch64-apple-darwin-install_only.tar.gz`.
    /// Distributions can be read from a local directory by using the `file://` URL scheme.
    pub const UV_PYTHON_INSTALL_MIRROR: &'static str = "UV_PYTHON_INSTALL_MIRROR";

    /// Managed PyPy installations are downloaded from [python.org](https://downloads.python.org/).
    ///
    /// This variable can be set to a mirror URL to use a
    /// different source for PyPy installations. The provided URL will replace
    /// `https://downloads.python.org/pypy` in, e.g.,
    /// `https://downloads.python.org/pypy/pypy3.8-v7.3.7-osx64.tar.bz2`.
    /// Distributions can be read from a local directory by using the `file://` URL scheme.
    pub const UV_PYPY_INSTALL_MIRROR: &'static str = "UV_PYPY_INSTALL_MIRROR";

    /// Install seed packages (one or more of: `pip`, `setuptools`, and `wheel`) into the virtual environment
    /// created by `uv venv`.
    ///
    /// Note that `setuptools` and `wheel` are not included in Python 3.12+ environments.
    pub const UV_VENV_SEED: &'static str = "UV_VENV_SEED";

    /// Used to override `PATH` to limit Python executable availability in the test suite.
    #[attr_hidden]
    pub const UV_TEST_PYTHON_PATH: &'static str = "UV_TEST_PYTHON_PATH";

    /// Include resolver and installer output related to environment modifications.
    #[attr_hidden]
    pub const UV_SHOW_RESOLUTION: &'static str = "UV_SHOW_RESOLUTION";

    /// Use to update the json schema files.
    #[attr_hidden]
    pub const UV_UPDATE_SCHEMA: &'static str = "UV_UPDATE_SCHEMA";

    /// Use to disable line wrapping for diagnostics.
    pub const UV_NO_WRAP: &'static str = "UV_NO_WRAP";

    /// Provides the HTTP Basic authentication username for a named index.
    ///
    /// The `name` parameter is the name of the index. For example, given an index named `foo`,
    /// the environment variable key would be `UV_INDEX_FOO_USERNAME`.
    #[attr_env_var_pattern("UV_INDEX_{name}_USERNAME")]
    pub fn index_username(name: &str) -> String {
        format!("UV_INDEX_{name}_USERNAME")
    }

    /// Provides the HTTP Basic authentication password for a named index.
    ///
    /// The `name` parameter is the name of the index. For example, given an index named `foo`,
    /// the environment variable key would be `UV_INDEX_FOO_PASSWORD`.
    #[attr_env_var_pattern("UV_INDEX_{name}_PASSWORD")]
    pub fn index_password(name: &str) -> String {
        format!("UV_INDEX_{name}_PASSWORD")
    }

    /// Used to set the uv commit hash at build time via `build.rs`.
    #[attr_hidden]
    pub const UV_COMMIT_HASH: &'static str = "UV_COMMIT_HASH";

    /// Used to set the uv commit short hash at build time via `build.rs`.
    #[attr_hidden]
    pub const UV_COMMIT_SHORT_HASH: &'static str = "UV_COMMIT_SHORT_HASH";

    /// Used to set the uv commit date at build time via `build.rs`.
    #[attr_hidden]
    pub const UV_COMMIT_DATE: &'static str = "UV_COMMIT_DATE";

    /// Used to set the uv tag at build time via `build.rs`.
    #[attr_hidden]
    pub const UV_LAST_TAG: &'static str = "UV_LAST_TAG";

    /// Used to set the uv tag distance from head at build time via `build.rs`.
    #[attr_hidden]
    pub const UV_LAST_TAG_DISTANCE: &'static str = "UV_LAST_TAG_DISTANCE";

    /// Used to set the spawning/parent interpreter when using --system in the test suite.
    #[attr_hidden]
    pub const UV_INTERNAL__PARENT_INTERPRETER: &'static str = "UV_INTERNAL__PARENT_INTERPRETER";

    /// Used to force showing the derivation tree during resolver error reporting.
    #[attr_hidden]
    pub const UV_INTERNAL__SHOW_DERIVATION_TREE: &'static str = "UV_INTERNAL__SHOW_DERIVATION_TREE";

    /// Used to set a temporary directory for some tests.
    #[attr_hidden]
    pub const UV_INTERNAL__TEST_DIR: &'static str = "UV_INTERNAL__TEST_DIR";

    /// Path to system-level configuration directory on Unix systems.
    pub const XDG_CONFIG_DIRS: &'static str = "XDG_CONFIG_DIRS";

    /// Path to system-level configuration directory on Windows systems.
    pub const SYSTEMDRIVE: &'static str = "SYSTEMDRIVE";

    /// Path to user-level configuration directory on Windows systems.
    pub const APPDATA: &'static str = "APPDATA";

    /// Path to root directory of user's profile on Windows systems.
    pub const USERPROFILE: &'static str = "USERPROFILE";

    /// Path to user-level configuration directory on Unix systems.
    pub const XDG_CONFIG_HOME: &'static str = "XDG_CONFIG_HOME";

    /// Path to cache directory on Unix systems.
    pub const XDG_CACHE_HOME: &'static str = "XDG_CACHE_HOME";

    /// Path to directory for storing managed Python installations and tools.
    pub const XDG_DATA_HOME: &'static str = "XDG_DATA_HOME";

    /// Path to directory where executables are installed.
    pub const XDG_BIN_HOME: &'static str = "XDG_BIN_HOME";

    /// Custom certificate bundle file path for SSL connections.
    pub const SSL_CERT_FILE: &'static str = "SSL_CERT_FILE";

    /// If set, uv will use this file for mTLS authentication.
    /// This should be a single file containing both the certificate and the private key in PEM format.
    pub const SSL_CLIENT_CERT: &'static str = "SSL_CLIENT_CERT";

    /// Proxy for HTTP requests.
    pub const HTTP_PROXY: &'static str = "HTTP_PROXY";

    /// Proxy for HTTPS requests.
    pub const HTTPS_PROXY: &'static str = "HTTPS_PROXY";

    /// General proxy for all network requests.
    pub const ALL_PROXY: &'static str = "ALL_PROXY";

    /// Timeout (in seconds) for HTTP requests. (default: 30 s)
    pub const UV_HTTP_TIMEOUT: &'static str = "UV_HTTP_TIMEOUT";

    /// Timeout (in seconds) for HTTP requests. Equivalent to `UV_HTTP_TIMEOUT`.
    pub const UV_REQUEST_TIMEOUT: &'static str = "UV_REQUEST_TIMEOUT";

    /// Timeout (in seconds) for HTTP requests. Equivalent to `UV_HTTP_TIMEOUT`.
    pub const HTTP_TIMEOUT: &'static str = "HTTP_TIMEOUT";

    /// The validation modes to use when run with `--compile`.
    ///
    /// See [`PycInvalidationMode`](https://docs.python.org/3/library/py_compile.html#py_compile.PycInvalidationMode).
    pub const PYC_INVALIDATION_MODE: &'static str = "PYC_INVALIDATION_MODE";

    /// Used to detect an activated virtual environment.
    pub const VIRTUAL_ENV: &'static str = "VIRTUAL_ENV";

    /// Used to detect an activated Conda environment.
    pub const CONDA_PREFIX: &'static str = "CONDA_PREFIX";

    /// Used to determine if an active Conda environment is the base environment or not.
    pub const CONDA_DEFAULT_ENV: &'static str = "CONDA_DEFAULT_ENV";

    /// If set to `1` before a virtual environment is activated, then the
    /// virtual environment name will not be prepended to the terminal prompt.
    pub const VIRTUAL_ENV_DISABLE_PROMPT: &'static str = "VIRTUAL_ENV_DISABLE_PROMPT";

    /// Used to detect the use of the Windows Command Prompt (as opposed to PowerShell).
    pub const PROMPT: &'static str = "PROMPT";

    /// Used to detect `NuShell` usage.
    pub const NU_VERSION: &'static str = "NU_VERSION";

    /// Used to detect Fish shell usage.
    pub const FISH_VERSION: &'static str = "FISH_VERSION";

    /// Used to detect Bash shell usage.
    pub const BASH_VERSION: &'static str = "BASH_VERSION";

    /// Used to detect Zsh shell usage.
    pub const ZSH_VERSION: &'static str = "ZSH_VERSION";

    /// Used to determine which `.zshenv` to use when Zsh is being used.
    pub const ZDOTDIR: &'static str = "ZDOTDIR";

    /// Used to detect Ksh shell usage.
    pub const KSH_VERSION: &'static str = "KSH_VERSION";

    /// Used with `--python-platform macos` and related variants to set the
    /// deployment target (i.e., the minimum supported macOS version).
    ///
    /// Defaults to `12.0`, the least-recent non-EOL macOS version at time of writing.
    pub const MACOSX_DEPLOYMENT_TARGET: &'static str = "MACOSX_DEPLOYMENT_TARGET";

    /// Disables colored output (takes precedence over `FORCE_COLOR`).
    ///
    /// See [no-color.org](https://no-color.org).
    pub const NO_COLOR: &'static str = "NO_COLOR";

    /// Forces colored output regardless of terminal support.
    ///
    /// See [force-color.org](https://force-color.org).
    pub const FORCE_COLOR: &'static str = "FORCE_COLOR";

    /// Use to control color via `anstyle`.
    pub const CLICOLOR_FORCE: &'static str = "CLICOLOR_FORCE";

    /// The standard `PATH` env var.
    pub const PATH: &'static str = "PATH";

    /// The standard `HOME` env var.
    pub const HOME: &'static str = "HOME";

    /// The standard `SHELL` posix env var.
    pub const SHELL: &'static str = "SHELL";

    /// The standard `PWD` posix env var.
    pub const PWD: &'static str = "PWD";

    /// Used to look for Microsoft Store Pythons installations.
    pub const LOCALAPPDATA: &'static str = "LOCALAPPDATA";

    /// Path to the `.git` directory. Ignored by `uv` when performing fetch.
    #[attr_hidden]
    pub const GIT_DIR: &'static str = "GIT_DIR";

    /// Path to the git working tree. Ignored by `uv` when performing fetch.
    #[attr_hidden]
    pub const GIT_WORK_TREE: &'static str = "GIT_WORK_TREE";

    /// Path to the index file for staged changes. Ignored by `uv` when performing fetch.
    #[attr_hidden]
    pub const GIT_INDEX_FILE: &'static str = "GIT_INDEX_FILE";

    /// Path to where git object files are located. Ignored by `uv` when performing fetch.
    #[attr_hidden]
    pub const GIT_OBJECT_DIRECTORY: &'static str = "GIT_OBJECT_DIRECTORY";

    /// Alternate locations for git objects. Ignored by `uv` when performing fetch.
    #[attr_hidden]
    pub const GIT_ALTERNATE_OBJECT_DIRECTORIES: &'static str = "GIT_ALTERNATE_OBJECT_DIRECTORIES";

    /// Disables SSL verification for git operations.
    #[attr_hidden]
    pub const GIT_SSL_NO_VERIFY: &'static str = "GIT_SSL_NO_VERIFY";

    /// Disable interactive git prompts in terminals, e.g., for credentials. Does not disable
    /// GUI prompts.
    #[attr_hidden]
    pub const GIT_TERMINAL_PROMPT: &'static str = "GIT_TERMINAL_PROMPT";

    /// Used in tests for better git isolation.
    ///
    /// For example, we run some tests in ~/.local/share/uv/tests.
    /// And if the user's `$HOME` directory is a git repository,
    /// this will change the behavior of some tests. Setting
    /// `GIT_CEILING_DIRECTORIES=/home/andrew/.local/share/uv/tests` will
    /// prevent git from crawling up the directory tree past that point to find
    /// parent git repositories.
    #[attr_hidden]
    pub const GIT_CEILING_DIRECTORIES: &'static str = "GIT_CEILING_DIRECTORIES";

    /// Used for trusted publishing via `uv publish`.
    pub const GITHUB_ACTIONS: &'static str = "GITHUB_ACTIONS";

    /// Used for trusted publishing via `uv publish`. Contains the oidc token url.
    pub const ACTIONS_ID_TOKEN_REQUEST_URL: &'static str = "ACTIONS_ID_TOKEN_REQUEST_URL";

    /// Used for trusted publishing via `uv publish`. Contains the oidc request token.
    pub const ACTIONS_ID_TOKEN_REQUEST_TOKEN: &'static str = "ACTIONS_ID_TOKEN_REQUEST_TOKEN";

    /// Sets the encoding for standard I/O streams (e.g., PYTHONIOENCODING=utf-8).
    #[attr_hidden]
    pub const PYTHONIOENCODING: &'static str = "PYTHONIOENCODING";

    /// Forces unbuffered I/O streams, equivalent to `-u` in Python.
    #[attr_hidden]
    pub const PYTHONUNBUFFERED: &'static str = "PYTHONUNBUFFERED";

    /// Enables UTF-8 mode for Python, equivalent to `-X utf8`.
    #[attr_hidden]
    pub const PYTHONUTF8: &'static str = "PYTHONUTF8";

    /// Adds directories to Python module search path (e.g., `PYTHONPATH=/path/to/modules`).
    pub const PYTHONPATH: &'static str = "PYTHONPATH";

    /// Used in tests to enforce a consistent locale setting.
    #[attr_hidden]
    pub const LC_ALL: &'static str = "LC_ALL";

    /// Typically set by CI runners, used to detect a CI runner.
    #[attr_hidden]
    pub const CI: &'static str = "CI";

    /// Use to set the .netrc file location.
    pub const NETRC: &'static str = "NETRC";

    /// The standard `PAGER` posix env var. Used by `uv` to configure the appropriate pager.
    pub const PAGER: &'static str = "PAGER";

    /// Used to detect when running inside a Jupyter notebook.
    pub const JPY_SESSION_NAME: &'static str = "JPY_SESSION_NAME";

    /// Use to create the tracing root directory via the `tracing-durations-export` feature.
    #[attr_hidden]
    pub const TRACING_DURATIONS_TEST_ROOT: &'static str = "TRACING_DURATIONS_TEST_ROOT";

    /// Use to create the tracing durations file via the `tracing-durations-export` feature.
    pub const TRACING_DURATIONS_FILE: &'static str = "TRACING_DURATIONS_FILE";

    /// Used to set `RUST_HOST_TARGET` at build time via `build.rs`.
    #[attr_hidden]
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
    pub const RUST_LOG: &'static str = "RUST_LOG";

    /// Add additional context and structure to log messages.
    ///
    /// If logging is not enabled, e.g., with `RUST_LOG` or `-v`, this has no effect.
    pub const UV_LOG_CONTEXT: &'static str = "UV_LOG_CONTEXT";

    /// Use to set the stack size used by uv.
    ///
    /// The value is in bytes, and the default is typically 2MB (2097152).
    /// Unlike the normal `RUST_MIN_STACK` semantics, this can affect main thread
    /// stack size, because we actually spawn our own main2 thread to work around
    /// the fact that Windows' real main thread is only 1MB. That thread has size
    /// `max(RUST_MIN_STACK, 4MB)`.
    pub const RUST_MIN_STACK: &'static str = "RUST_MIN_STACK";

    /// The directory containing the `Cargo.toml` manifest for a package.
    #[attr_hidden]
    pub const CARGO_MANIFEST_DIR: &'static str = "CARGO_MANIFEST_DIR";

    /// Specifies the directory where Cargo stores build artifacts (target directory).
    #[attr_hidden]
    pub const CARGO_TARGET_DIR: &'static str = "CARGO_TARGET_DIR";

    /// Used in tests for environment substitution testing in `requirements.in`.
    #[attr_hidden]
    pub const URL: &'static str = "URL";

    /// Used in tests for environment substitution testing in `requirements.in`.
    #[attr_hidden]
    pub const FILE_PATH: &'static str = "FILE_PATH";

    /// Used in tests for environment substitution testing in `requirements.in`.
    #[attr_hidden]
    pub const HATCH_PATH: &'static str = "HATCH_PATH";

    /// Used in tests for environment substitution testing in `requirements.in`.
    #[attr_hidden]
    pub const BLACK_PATH: &'static str = "BLACK_PATH";

    /// Used in testing Hatch's root.uri feature
    ///
    /// See: <https://hatch.pypa.io/dev/config/dependency/#local>.
    #[attr_hidden]
    pub const ROOT_PATH: &'static str = "ROOT_PATH";

    /// Used to set test credentials for keyring tests.
    #[attr_hidden]
    pub const KEYRING_TEST_CREDENTIALS: &'static str = "KEYRING_TEST_CREDENTIALS";

    /// Used to set the vendor links url for tests.
    #[attr_hidden]
    pub const UV_TEST_VENDOR_LINKS_URL: &'static str = "UV_TEST_VENDOR_LINKS_URL";

    /// Used to set an index url for tests.
    #[attr_hidden]
    pub const UV_TEST_INDEX_URL: &'static str = "UV_TEST_INDEX_URL";

    /// Hide progress messages with non-deterministic order in tests.
    #[attr_hidden]
    pub const UV_TEST_NO_CLI_PROGRESS: &'static str = "UV_TEST_NO_CLI_PROGRESS";

    /// `.env` files from which to load environment variables when executing `uv run` commands.
    pub const UV_ENV_FILE: &'static str = "UV_ENV_FILE";

    /// Ignore `.env` files when executing `uv run` commands.
    pub const UV_NO_ENV_FILE: &'static str = "UV_NO_ENV_FILE";

    /// The URL from which to download uv using the standalone installer and `self update` feature,
    /// in lieu of the default GitHub URL.
    pub const UV_INSTALLER_GITHUB_BASE_URL: &'static str = "UV_INSTALLER_GITHUB_BASE_URL";

    /// The URL from which to download uv using the standalone installer and `self update` feature,
    /// in lieu of the default GitHub Enterprise URL.
    pub const UV_INSTALLER_GHE_BASE_URL: &'static str = "UV_INSTALLER_GHE_BASE_URL";

    /// The directory in which to install uv using the standalone installer and `self update` feature.
    /// Defaults to `~/.local/bin`.
    pub const UV_INSTALL_DIR: &'static str = "UV_INSTALL_DIR";

    /// Used ephemeral environments like CI to install uv to a specific path while preventing
    /// the installer from modifying shell profiles or environment variables.
    pub const UV_UNMANAGED_INSTALL: &'static str = "UV_UNMANAGED_INSTALL";

    /// Avoid modifying the `PATH` environment variable when installing uv using the standalone
    /// installer and `self update` feature.
    pub const INSTALLER_NO_MODIFY_PATH: &'static str = "INSTALLER_NO_MODIFY_PATH";

    /// Skip writing `uv` installer metadata files (e.g., `INSTALLER`, `REQUESTED`, and `direct_url.json`) to site-packages `.dist-info` directories.
    pub const UV_NO_INSTALLER_METADATA: &'static str = "UV_NO_INSTALLER_METADATA";

    /// Enables fetching files stored in Git LFS when installing a package from a Git repository.
    pub const UV_GIT_LFS: &'static str = "UV_GIT_LFS";

    /// Number of times that `uv run` has been recursively invoked. Used to guard against infinite
    /// recursion, e.g., when `uv run`` is used in a script shebang.
    #[attr_hidden]
    pub const UV_RUN_RECURSION_DEPTH: &'static str = "UV_RUN_RECURSION_DEPTH";

    /// Number of times that `uv run` will allow recursive invocations, before exiting with an
    /// error.
    #[attr_hidden]
    pub const UV_RUN_MAX_RECURSION_DEPTH: &'static str = "UV_RUN_MAX_RECURSION_DEPTH";

    /// Overrides terminal width used for wrapping. This variable is not read by uv directly.
    ///
    /// This is a quasi-standard variable, described, e.g., in `ncurses(3x)`.
    pub const COLUMNS: &'static str = "COLUMNS";
}
