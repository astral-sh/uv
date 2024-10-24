/// Declares all environment variable used throughout `uv` and its crates.
pub struct EnvVars;

impl EnvVars {
    /// Equivalent to the `--default-index` argument. Base index URL for searching packages.
    pub const UV_DEFAULT_INDEX: &'static str = "UV_DEFAULT_INDEX";

    /// Equivalent to the `--index` argument. Additional indexes for searching packages.
    pub const UV_INDEX: &'static str = "UV_INDEX";

    /// Equivalent to the `--index-url` argument. Base index URL for searching packages.
    ///
    /// Deprecated: use `UV_DEFAULT_INDEX` instead.
    pub const UV_INDEX_URL: &'static str = "UV_INDEX_URL";

    /// Equivalent to the `--extra-index-url` argument. Additional indexes for searching packages.
    ///
    /// Deprecated: use `UV_INDEX` instead.
    pub const UV_EXTRA_INDEX_URL: &'static str = "UV_EXTRA_INDEX_URL";

    /// Equivalent to the `--find-links` argument. Additional package search locations.
    pub const UV_FIND_LINKS: &'static str = "UV_FIND_LINKS";

    /// Equivalent to the `--cache-dir` argument. Custom directory for caching.
    pub const UV_CACHE_DIR: &'static str = "UV_CACHE_DIR";

    /// Equivalent to the `--no-cache` argument. Disables cache usage.
    pub const UV_NO_CACHE: &'static str = "UV_NO_CACHE";

    /// Equivalent to the `--resolution` argument. Controls dependency resolution strategy.
    pub const UV_RESOLUTION: &'static str = "UV_RESOLUTION";

    /// Equivalent to the `--prerelease` argument. Allows or disallows pre-release versions.
    pub const UV_PRERELEASE: &'static str = "UV_PRERELEASE";

    /// Equivalent to the `--system` argument. Use system Python interpreter.
    pub const UV_SYSTEM_PYTHON: &'static str = "UV_SYSTEM_PYTHON";

    /// Equivalent to the `--python` argument. Path to a specific Python interpreter.
    pub const UV_PYTHON: &'static str = "UV_PYTHON";

    /// Equivalent to the `--break-system-packages` argument. Allows breaking system packages.
    pub const UV_BREAK_SYSTEM_PACKAGES: &'static str = "UV_BREAK_SYSTEM_PACKAGES";

    /// Equivalent to the `--native-tls` argument. Uses system's trust store for TLS.
    pub const UV_NATIVE_TLS: &'static str = "UV_NATIVE_TLS";

    /// Equivalent to the `--index-strategy` argument. Defines strategy for searching index URLs.
    pub const UV_INDEX_STRATEGY: &'static str = "UV_INDEX_STRATEGY";

    /// Equivalent to the `--require-hashes` argument. Requires hashes for all dependencies.
    pub const UV_REQUIRE_HASHES: &'static str = "UV_REQUIRE_HASHES";

    /// Equivalent to the `--constraint` argument. Path to constraints file.
    pub const UV_CONSTRAINT: &'static str = "UV_CONSTRAINT";

    /// Equivalent to the `--build-constraint` argument. Path to build constraints file.
    pub const UV_BUILD_CONSTRAINT: &'static str = "UV_BUILD_CONSTRAINT";

    /// Equivalent to the `--override` argument. Path to overrides file.
    pub const UV_OVERRIDE: &'static str = "UV_OVERRIDE";

    /// Equivalent to the `--link-mode` argument. Specifies link mode for the installation.
    pub const UV_LINK_MODE: &'static str = "UV_LINK_MODE";

    /// Equivalent to the `--no-build-isolation` argument. Skips build isolation.
    pub const UV_NO_BUILD_ISOLATION: &'static str = "UV_NO_BUILD_ISOLATION";

    /// Equivalent to the `--custom-compile-command` argument. Overrides the command in `requirements.txt`.
    pub const UV_CUSTOM_COMPILE_COMMAND: &'static str = "UV_CUSTOM_COMPILE_COMMAND";

    /// Equivalent to the `--keyring-provider` argument. Specifies keyring provider.
    pub const UV_KEYRING_PROVIDER: &'static str = "UV_KEYRING_PROVIDER";

    /// Equivalent to the `--config-file` argument. Path to configuration file.
    pub const UV_CONFIG_FILE: &'static str = "UV_CONFIG_FILE";

    /// Equivalent to the `--no-config` argument. Prevents reading configuration files.
    pub const UV_NO_CONFIG: &'static str = "UV_NO_CONFIG";

    /// Equivalent to the `--exclude-newer` argument. Excludes newer distributions after a date.
    pub const UV_EXCLUDE_NEWER: &'static str = "UV_EXCLUDE_NEWER";

    /// Equivalent to the `--python-preference` argument. Controls preference for Python versions.
    pub const UV_PYTHON_PREFERENCE: &'static str = "UV_PYTHON_PREFERENCE";

    /// Equivalent to the `--no-python-downloads` argument. Disables Python downloads.
    pub const UV_PYTHON_DOWNLOADS: &'static str = "UV_PYTHON_DOWNLOADS";

    /// Equivalent to the `--compile-bytecode` argument. Compiles Python source to bytecode.
    pub const UV_COMPILE_BYTECODE: &'static str = "UV_COMPILE_BYTECODE";

    /// Equivalent to the `--publish-url` argument. URL for publishing packages.
    pub const UV_PUBLISH_URL: &'static str = "UV_PUBLISH_URL";

    /// Equivalent to the `--token` argument in `uv publish`. Token for publishing.
    pub const UV_PUBLISH_TOKEN: &'static str = "UV_PUBLISH_TOKEN";

    /// Equivalent to the `--username` argument in `uv publish`. Username for publishing.
    pub const UV_PUBLISH_USERNAME: &'static str = "UV_PUBLISH_USERNAME";

    /// Equivalent to the `--password` argument in `uv publish`. Password for publishing.
    pub const UV_PUBLISH_PASSWORD: &'static str = "UV_PUBLISH_PASSWORD";

    /// Equivalent to the `--no-sync` argument. Skips syncing the environment.
    pub const UV_NO_SYNC: &'static str = "UV_NO_SYNC";

    /// Equivalent to the `--locked` argument. Assert that the `uv.lock` will remain unchanged.
    pub const UV_LOCKED: &'static str = "UV_LOCKED";

    /// Equivalent to the `--frozen` argument. Run without updating the `uv.lock` file.
    pub const UV_FROZEN: &'static str = "UV_FROZEN";

    /// Equivalent to the `--preview` argument. Enables preview mode.
    pub const UV_PREVIEW: &'static str = "UV_PREVIEW";

    /// Equivalent to the `--token` argument for self update. A GitHub token for authentication.
    pub const UV_GITHUB_TOKEN: &'static str = "UV_GITHUB_TOKEN";

    /// Equivalent to the `--verify-hashes` argument. Verifies included hashes.
    pub const UV_VERIFY_HASHES: &'static str = "UV_VERIFY_HASHES";

    /// Equivalent to the `--allow-insecure-host` argument.
    pub const UV_INSECURE_HOST: &'static str = "UV_INSECURE_HOST";

    /// Sets the maximum number of in-flight concurrent downloads.
    pub const UV_CONCURRENT_DOWNLOADS: &'static str = "UV_CONCURRENT_DOWNLOADS";

    /// Sets the maximum number of concurrent builds for source distributions.
    pub const UV_CONCURRENT_BUILDS: &'static str = "UV_CONCURRENT_BUILDS";

    /// Controls the number of threads used for concurrent installations.
    pub const UV_CONCURRENT_INSTALLS: &'static str = "UV_CONCURRENT_INSTALLS";

    /// Specifies the directory where `uv` stores managed tools.
    pub const UV_TOOL_DIR: &'static str = "UV_TOOL_DIR";

    /// Specifies the "bin" directory for installing tool executables.
    pub const UV_TOOL_BIN_DIR: &'static str = "UV_TOOL_BIN_DIR";

    /// Specifies the path to the project virtual environment.
    pub const UV_PROJECT_ENVIRONMENT: &'static str = "UV_PROJECT_ENVIRONMENT";

    /// Specifies the directory to place links to installed, managed Python executables.
    pub const UV_PYTHON_BIN_DIR: &'static str = "UV_PYTHON_BIN_DIR";

    /// Specifies the directory for storing managed Python installations.
    pub const UV_PYTHON_INSTALL_DIR: &'static str = "UV_PYTHON_INSTALL_DIR";

    /// Mirror URL for downloading managed Python installations.
    pub const UV_PYTHON_INSTALL_MIRROR: &'static str = "UV_PYTHON_INSTALL_MIRROR";

    /// Mirror URL for downloading managed PyPy installations.
    pub const UV_PYPY_INSTALL_MIRROR: &'static str = "UV_PYPY_INSTALL_MIRROR";

    /// Used to override `PATH` to limit Python executable availability in the test suite.
    pub const UV_TEST_PYTHON_PATH: &'static str = "UV_TEST_PYTHON_PATH";

    /// Include resolver and installer output related to environment modifications.
    pub const UV_SHOW_RESOLUTION: &'static str = "UV_SHOW_RESOLUTION";

    /// Use to update the json schema files.
    pub const UV_UPDATE_SCHEMA: &'static str = "UV_UPDATE_SCHEMA";

    /// Use to disable line wrapping for diagnostics.
    pub const UV_NO_WRAP: &'static str = "UV_NO_WRAP";

    /// Use to control the stack size used by uv. Typically more relevant for Windows in debug mode.
    pub const UV_STACK_SIZE: &'static str = "UV_STACK_SIZE";

    /// Generates the environment variable key for the HTTP Basic authentication username.
    pub fn index_username(name: &str) -> String {
        format!("UV_INDEX_{name}_USERNAME")
    }

    /// Generates the environment variable key for the HTTP Basic authentication password.
    pub fn index_password(name: &str) -> String {
        format!("UV_INDEX_{name}_PASSWORD")
    }

    /// Used to set the uv commit hash at build time via `build.rs`.
    pub const UV_COMMIT_HASH: &'static str = "UV_COMMIT_HASH";

    /// Used to set the uv commit short hash at build time via `build.rs`.
    pub const UV_COMMIT_SHORT_HASH: &'static str = "UV_COMMIT_SHORT_HASH";

    /// Used to set the uv commit date at build time via `build.rs`.
    pub const UV_COMMIT_DATE: &'static str = "UV_COMMIT_DATE";

    /// Used to set the uv tag at build time via `build.rs`.
    pub const UV_LAST_TAG: &'static str = "UV_LAST_TAG";

    /// Used to set the uv tag distance from head at build time via `build.rs`.
    pub const UV_LAST_TAG_DISTANCE: &'static str = "UV_LAST_TAG_DISTANCE";

    /// Used to set the spawning/parent interpreter when using --system in the test suite.
    pub const UV_INTERNAL__PARENT_INTERPRETER: &'static str = "UV_INTERNAL__PARENT_INTERPRETER";

    /// Used to force showing the derivation tree during resolver error reporting.
    pub const UV_INTERNAL__SHOW_DERIVATION_TREE: &'static str = "UV_INTERNAL__SHOW_DERIVATION_TREE";

    /// Used to set a temporary directory for some tests.
    pub const UV_INTERNAL__TEST_DIR: &'static str = "UV_INTERNAL__TEST_DIR";

    /// Path to system-level configuration directory on Unix systems.
    pub const XDG_CONFIG_DIRS: &'static str = "XDG_CONFIG_DIRS";

    /// Path to system-level configuration directory on Windows systems.
    pub const SYSTEMDRIVE: &'static str = "SYSTEMDRIVE";

    /// Path to user-level configuration directory on Unix systems.
    pub const XDG_CONFIG_HOME: &'static str = "XDG_CONFIG_HOME";

    /// Path to cache directory on Unix systems.
    pub const XDG_CACHE_HOME: &'static str = "XDG_CACHE_HOME";

    /// Path to directory for storing managed Python installations and tools.
    pub const XDG_DATA_HOME: &'static str = "XDG_DATA_HOME";

    /// Path to directory where executables are installed.
    pub const XDG_BIN_HOME: &'static str = "XDG_BIN_HOME";

    /// Timeout (in seconds) for HTTP requests.
    pub const UV_HTTP_TIMEOUT: &'static str = "UV_HTTP_TIMEOUT";

    /// Timeout (in seconds) for HTTP requests.
    pub const UV_REQUEST_TIMEOUT: &'static str = "UV_REQUEST_TIMEOUT";

    /// Timeout (in seconds) for HTTP requests.
    pub const HTTP_TIMEOUT: &'static str = "HTTP_TIMEOUT";

    /// Custom certificate bundle file path for SSL connections.
    pub const SSL_CERT_FILE: &'static str = "SSL_CERT_FILE";

    /// File for mTLS authentication (contains certificate and private key).
    pub const SSL_CLIENT_CERT: &'static str = "SSL_CLIENT_CERT";

    /// Proxy for HTTP requests.
    pub const HTTP_PROXY: &'static str = "HTTP_PROXY";

    /// Proxy for HTTPS requests.
    pub const HTTPS_PROXY: &'static str = "HTTPS_PROXY";

    /// General proxy for all network requests.
    pub const ALL_PROXY: &'static str = "ALL_PROXY";

    /// Used to detect an activated virtual environment.
    pub const VIRTUAL_ENV: &'static str = "VIRTUAL_ENV";

    /// Used to detect an activated Conda environment.
    pub const CONDA_PREFIX: &'static str = "CONDA_PREFIX";

    /// Used to determine if an active Conda environment is the base environment or not.
    pub const CONDA_DEFAULT_ENV: &'static str = "CONDA_DEFAULT_ENV";

    /// Disables prepending virtual environment name to the terminal prompt.
    pub const VIRTUAL_ENV_DISABLE_PROMPT: &'static str = "VIRTUAL_ENV_DISABLE_PROMPT";

    /// Used to detect Windows Command Prompt usage.
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

    /// Sets macOS deployment target when using `--python-platform macos`.
    pub const MACOSX_DEPLOYMENT_TARGET: &'static str = "MACOSX_DEPLOYMENT_TARGET";

    /// Disables colored output (takes precedence over `FORCE_COLOR`).
    pub const NO_COLOR: &'static str = "NO_COLOR";

    /// Forces colored output regardless of terminal support.
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
    pub const GIT_DIR: &'static str = "GIT_DIR";

    /// Path to the git working tree. Ignored by `uv` when performing fetch.
    pub const GIT_WORK_TREE: &'static str = "GIT_WORK_TREE";

    /// Path to the index file for staged changes. Ignored by `uv` when performing fetch.
    pub const GIT_INDEX_FILE: &'static str = "GIT_INDEX_FILE";

    /// Path to where git object files are located. Ignored by `uv` when performing fetch.
    pub const GIT_OBJECT_DIRECTORY: &'static str = "GIT_OBJECT_DIRECTORY";

    /// Alternate locations for git objects. Ignored by `uv` when performing fetch.
    pub const GIT_ALTERNATE_OBJECT_DIRECTORIES: &'static str = "GIT_ALTERNATE_OBJECT_DIRECTORIES";

    /// Used for trusted publishing via `uv publish`.
    pub const GITHUB_ACTIONS: &'static str = "GITHUB_ACTIONS";

    /// Used for trusted publishing via `uv publish`. Contains the oidc token url.
    pub const ACTIONS_ID_TOKEN_REQUEST_URL: &'static str = "ACTIONS_ID_TOKEN_REQUEST_URL";

    /// Used for trusted publishing via `uv publish`. Contains the oidc request token.
    pub const ACTIONS_ID_TOKEN_REQUEST_TOKEN: &'static str = "ACTIONS_ID_TOKEN_REQUEST_TOKEN";

    /// Sets the encoding for standard I/O streams (e.g., PYTHONIOENCODING=utf-8).
    pub const PYTHONIOENCODING: &'static str = "PYTHONIOENCODING";

    /// Forces unbuffered I/O streams, equivalent to `-u` in Python.
    pub const PYTHONUNBUFFERED: &'static str = "PYTHONUNBUFFERED";

    /// Enables UTF-8 mode for Python, equivalent to `-X utf8`.
    pub const PYTHONUTF8: &'static str = "PYTHONUTF8";

    /// Adds directories to Python module search path (e.g., PYTHONPATH=/path/to/modules).
    pub const PYTHONPATH: &'static str = "PYTHONPATH";

    /// Typically set by CI runners, used to detect a CI runner.
    pub const CI: &'static str = "CI";

    /// Use to set the .netrc file location.
    pub const NETRC: &'static str = "NETRC";

    /// The standard `PAGER` posix env var. Used by `uv` to configure the appropriate pager.
    pub const PAGER: &'static str = "PAGER";

    /// Used to detect when running inside a Jupyter notebook.
    pub const JPY_SESSION_NAME: &'static str = "JPY_SESSION_NAME";

    /// Use to create the tracing root directory via the `tracing-durations-export` feature.
    pub const TRACING_DURATIONS_TEST_ROOT: &'static str = "TRACING_DURATIONS_TEST_ROOT";

    /// Use to create the tracing durations file via the `tracing-durations-export` feature.
    pub const TRACING_DURATIONS_FILE: &'static str = "TRACING_DURATIONS_FILE";

    /// Used to set `RUST_HOST_TARGET` at build time via `build.rs`.
    pub const TARGET: &'static str = "TARGET";

    /// Custom log level for verbose output, compatible with `tracing_subscriber`.
    pub const RUST_LOG: &'static str = "RUST_LOG";

    /// The directory containing the `Cargo.toml` manifest for a package.
    pub const CARGO_MANIFEST_DIR: &'static str = "CARGO_MANIFEST_DIR";

    /// Specifies the directory where Cargo stores build artifacts (target directory).
    pub const CARGO_TARGET_DIR: &'static str = "CARGO_TARGET_DIR";

    /// Used in tests for environment substitution testing in `requirements.in`.
    pub const URL: &'static str = "URL";

    /// Used in tests for environment substitution testing in `requirements.in`.
    pub const FILE_PATH: &'static str = "FILE_PATH";

    /// Used in tests for environment substitution testing in `requirements.in`.
    pub const HATCH_PATH: &'static str = "HATCH_PATH";

    /// Used in tests for environment substitution testing in `requirements.in`.
    pub const BLACK_PATH: &'static str = "BLACK_PATH";

    /// Used in testing Hatch's root.uri feature
    ///
    /// See: <https://hatch.pypa.io/dev/config/dependency/#local>.
    pub const ROOT_PATH: &'static str = "ROOT_PATH";

    /// Used to set test credentials for keyring tests.
    pub const KEYRING_TEST_CREDENTIALS: &'static str = "KEYRING_TEST_CREDENTIALS";

    /// Used to overwrite path for loading `.env` files when executing `uv run` commands.
    pub const UV_ENV_FILE: &'static str = "UV_ENV_FILE";

    /// Used to ignore `.env` files when executing `uv run` commands.
    pub const UV_NO_ENV_FILE: &'static str = "UV_NO_ENV_FILE";
}
