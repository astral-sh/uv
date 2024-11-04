# Environment variables

uv respects the following environment variables:

- <a id="UV_DEFAULT_INDEX"></a> [`UV_DEFAULT_INDEX`](#UV_DEFAULT_INDEX): Equivalent to the `--default-index` command-line argument. If set, uv will use
  this URL as the default index when searching for packages.
- <a id="UV_INDEX"></a> [`UV_INDEX`](#UV_INDEX): Equivalent to the `--index` command-line argument. If set, uv will use this
  space-separated list of URLs as additional indexes when searching for packages.
- <a id="UV_INDEX_URL"></a> [`UV_INDEX_URL`](#UV_INDEX_URL): Equivalent to the `--index-url` command-line argument. If set, uv will use this
  URL as the default index when searching for packages.
  (Deprecated: use `UV_DEFAULT_INDEX` instead.)
- <a id="UV_EXTRA_INDEX_URL"></a> [`UV_EXTRA_INDEX_URL`](#UV_EXTRA_INDEX_URL): Equivalent to the `--extra-index-url` command-line argument. If set, uv will
  use this space-separated list of URLs as additional indexes when searching for packages.
  (Deprecated: use `UV_INDEX` instead.)
- <a id="UV_FIND_LINKS"></a> [`UV_FIND_LINKS`](#UV_FIND_LINKS): Equivalent to the `--find-links` command-line argument. If set, uv will use this
  comma-separated list of additional locations to search for packages.
- <a id="UV_CACHE_DIR"></a> [`UV_CACHE_DIR`](#UV_CACHE_DIR): Equivalent to the `--cache-dir` command-line argument. If set, uv will use this
  directory for caching instead of the default cache directory.
- <a id="UV_NO_CACHE"></a> [`UV_NO_CACHE`](#UV_NO_CACHE): Equivalent to the `--no-cache` command-line argument. If set, uv will not use the
  cache for any operations.
- <a id="UV_RESOLUTION"></a> [`UV_RESOLUTION`](#UV_RESOLUTION): Equivalent to the `--resolution` command-line argument. For example, if set to
  `lowest-direct`, uv will install the lowest compatible versions of all direct dependencies.
- <a id="UV_PRERELEASE"></a> [`UV_PRERELEASE`](#UV_PRERELEASE): Equivalent to the `--prerelease` command-line argument. For example, if set to
  `allow`, uv will allow pre-release versions for all dependencies.
- <a id="UV_SYSTEM_PYTHON"></a> [`UV_SYSTEM_PYTHON`](#UV_SYSTEM_PYTHON): Equivalent to the `--system` command-line argument. If set to `true`, uv will
  use the first Python interpreter found in the system `PATH`.
  WARNING: `UV_SYSTEM_PYTHON=true` is intended for use in continuous integration (CI)
  or containerized environments and should be used with caution, as modifying the system
  Python can lead to unexpected behavior.
- <a id="UV_PYTHON"></a> [`UV_PYTHON`](#UV_PYTHON): Equivalent to the `--python` command-line argument. If set to a path, uv will use
  this Python interpreter for all operations.
- <a id="UV_BREAK_SYSTEM_PACKAGES"></a> [`UV_BREAK_SYSTEM_PACKAGES`](#UV_BREAK_SYSTEM_PACKAGES): Equivalent to the `--break-system-packages` command-line argument. If set to `true`,
  uv will allow the installation of packages that conflict with system-installed packages.
  WARNING: `UV_BREAK_SYSTEM_PACKAGES=true` is intended for use in continuous integration
  (CI) or containerized environments and should be used with caution, as modifying the system
  Python can lead to unexpected behavior.
- <a id="UV_NATIVE_TLS"></a> [`UV_NATIVE_TLS`](#UV_NATIVE_TLS): Equivalent to the `--native-tls` command-line argument. If set to `true`, uv will
  use the system's trust store instead of the bundled `webpki-roots` crate.
- <a id="UV_INDEX_STRATEGY"></a> [`UV_INDEX_STRATEGY`](#UV_INDEX_STRATEGY): Equivalent to the `--index-strategy` command-line argument. For example, if
  set to `unsafe-any-match`, uv will consider versions of a given package available across all index
  URLs, rather than limiting its search to the first index URL that contains the package.
- <a id="UV_REQUIRE_HASHES"></a> [`UV_REQUIRE_HASHES`](#UV_REQUIRE_HASHES): Equivalent to the `--require-hashes` command-line argument. If set to `true`,
  uv will require that all dependencies have a hash specified in the requirements file.
- <a id="UV_CONSTRAINT"></a> [`UV_CONSTRAINT`](#UV_CONSTRAINT): Equivalent to the `--constraint` command-line argument. If set, uv will use this
  file as the constraints file. Uses space-separated list of files.
- <a id="UV_BUILD_CONSTRAINT"></a> [`UV_BUILD_CONSTRAINT`](#UV_BUILD_CONSTRAINT): Equivalent to the `--build-constraint` command-line argument. If set, uv will use this file
  as constraints for any source distribution builds. Uses space-separated list of files.
- <a id="UV_OVERRIDE"></a> [`UV_OVERRIDE`](#UV_OVERRIDE): Equivalent to the `--override` command-line argument. If set, uv will use this file
  as the overrides file. Uses space-separated list of files.
- <a id="UV_LINK_MODE"></a> [`UV_LINK_MODE`](#UV_LINK_MODE): Equivalent to the `--link-mode` command-line argument. If set, uv will use this as
  a link mode.
- <a id="UV_NO_BUILD_ISOLATION"></a> [`UV_NO_BUILD_ISOLATION`](#UV_NO_BUILD_ISOLATION): Equivalent to the `--no-build-isolation` command-line argument. If set, uv will
  skip isolation when building source distributions.
- <a id="UV_CUSTOM_COMPILE_COMMAND"></a> [`UV_CUSTOM_COMPILE_COMMAND`](#UV_CUSTOM_COMPILE_COMMAND): Equivalent to the `--custom-compile-command` command-line argument.
  Used to override uv in the output header of the `requirements.txt` files generated by
  `uv pip compile`. Intended for use-cases in which `uv pip compile` is called from within a wrapper
  script, to include the name of the wrapper script in the output file.
- <a id="UV_KEYRING_PROVIDER"></a> [`UV_KEYRING_PROVIDER`](#UV_KEYRING_PROVIDER): Equivalent to the `--keyring-provider` command-line argument. If set, uv
  will use this value as the keyring provider.
- <a id="UV_CONFIG_FILE"></a> [`UV_CONFIG_FILE`](#UV_CONFIG_FILE): Equivalent to the `--config-file` command-line argument. Expects a path to a
  local `uv.toml` file to use as the configuration file.
- <a id="UV_NO_CONFIG"></a> [`UV_NO_CONFIG`](#UV_NO_CONFIG): Equivalent to the `--no-config` command-line argument. If set, uv will not read
  any configuration files from the current directory, parent directories, or user configuration
  directories.
- <a id="UV_EXCLUDE_NEWER"></a> [`UV_EXCLUDE_NEWER`](#UV_EXCLUDE_NEWER): Equivalent to the `--exclude-newer` command-line argument. If set, uv will
  exclude distributions published after the specified date.
- <a id="UV_PYTHON_PREFERENCE"></a> [`UV_PYTHON_PREFERENCE`](#UV_PYTHON_PREFERENCE): Equivalent to the `--python-preference` command-line argument. Whether uv
  should prefer system or managed Python versions.
- <a id="UV_PYTHON_DOWNLOADS"></a> [`UV_PYTHON_DOWNLOADS`](#UV_PYTHON_DOWNLOADS): Equivalent to the
  [`python-downloads`](../reference/settings.md#python-downloads) setting and, when disabled, the
  `--no-python-downloads` option. Whether uv should allow Python downloads.
- <a id="UV_COMPILE_BYTECODE"></a> [`UV_COMPILE_BYTECODE`](#UV_COMPILE_BYTECODE): Equivalent to the `--compile-bytecode` command-line argument. If set, uv
  will compile Python source files to bytecode after installation.
- <a id="UV_PUBLISH_URL"></a> [`UV_PUBLISH_URL`](#UV_PUBLISH_URL): Equivalent to the `--publish-url` command-line argument. The URL of the upload
  endpoint of the index to use with `uv publish`.
- <a id="UV_PUBLISH_TOKEN"></a> [`UV_PUBLISH_TOKEN`](#UV_PUBLISH_TOKEN): Equivalent to the `--token` command-line argument in `uv publish`. If set, uv
  will use this token (with the username `__token__`) for publishing.
- <a id="UV_PUBLISH_USERNAME"></a> [`UV_PUBLISH_USERNAME`](#UV_PUBLISH_USERNAME): Equivalent to the `--username` command-line argument in `uv publish`. If
  set, uv will use this username for publishing.
- <a id="UV_PUBLISH_PASSWORD"></a> [`UV_PUBLISH_PASSWORD`](#UV_PUBLISH_PASSWORD): Equivalent to the `--password` command-line argument in `uv publish`. If
  set, uv will use this password for publishing.
- <a id="UV_PUBLISH_CHECK_URL"></a> [`UV_PUBLISH_CHECK_URL`](#UV_PUBLISH_CHECK_URL): Don't upload a file if it already exists on the index. The value is the URL of the index.
- <a id="UV_NO_SYNC"></a> [`UV_NO_SYNC`](#UV_NO_SYNC): Equivalent to the `--no-sync` command-line argument. If set, uv will skip updating
  the environment.
- <a id="UV_LOCKED"></a> [`UV_LOCKED`](#UV_LOCKED): Equivalent to the `--locked` command-line argument. If set, uv will assert that the
  `uv.lock` remains unchanged.
- <a id="UV_FROZEN"></a> [`UV_FROZEN`](#UV_FROZEN): Equivalent to the `--frozen` command-line argument. If set, uv will run without
  updating the `uv.lock` file.
- <a id="UV_PREVIEW"></a> [`UV_PREVIEW`](#UV_PREVIEW): Equivalent to the `--preview` argument. Enables preview mode.
- <a id="UV_GITHUB_TOKEN"></a> [`UV_GITHUB_TOKEN`](#UV_GITHUB_TOKEN): Equivalent to the `--token` argument for self update. A GitHub token for authentication.
- <a id="UV_VERIFY_HASHES"></a> [`UV_VERIFY_HASHES`](#UV_VERIFY_HASHES): Equivalent to the `--verify-hashes` argument. Verifies included hashes.
- <a id="UV_INSECURE_HOST"></a> [`UV_INSECURE_HOST`](#UV_INSECURE_HOST): Equivalent to the `--allow-insecure-host` argument.
- <a id="UV_CONCURRENT_DOWNLOADS"></a> [`UV_CONCURRENT_DOWNLOADS`](#UV_CONCURRENT_DOWNLOADS): Sets the maximum number of in-flight concurrent downloads that uv will
  perform at any given time.
- <a id="UV_CONCURRENT_BUILDS"></a> [`UV_CONCURRENT_BUILDS`](#UV_CONCURRENT_BUILDS): Sets the maximum number of source distributions that uv will build
  concurrently at any given time.
- <a id="UV_CONCURRENT_INSTALLS"></a> [`UV_CONCURRENT_INSTALLS`](#UV_CONCURRENT_INSTALLS): Controls the number of threads used when installing and unzipping
  packages.
- <a id="UV_NO_PROGRESS"></a> [`UV_NO_PROGRESS`](#UV_NO_PROGRESS): Disables all progress output. For example, spinners and progress bars.
- <a id="UV_TOOL_DIR"></a> [`UV_TOOL_DIR`](#UV_TOOL_DIR): Specifies the directory where uv stores managed tools.
- <a id="UV_TOOL_BIN_DIR"></a> [`UV_TOOL_BIN_DIR`](#UV_TOOL_BIN_DIR): Specifies the "bin" directory for installing tool executables.
- <a id="UV_PROJECT_ENVIRONMENT"></a> [`UV_PROJECT_ENVIRONMENT`](#UV_PROJECT_ENVIRONMENT): Specifies the path to the directory to use for a project virtual environment.
  See the [project documentation](../concepts/projects.md#configuring-the-project-environment-path)
  for more details.
- <a id="UV_PYTHON_BIN_DIR"></a> [`UV_PYTHON_BIN_DIR`](#UV_PYTHON_BIN_DIR): Specifies the directory to place links to installed, managed Python executables.
- <a id="UV_PYTHON_INSTALL_DIR"></a> [`UV_PYTHON_INSTALL_DIR`](#UV_PYTHON_INSTALL_DIR): Specifies the directory for storing managed Python installations.
- <a id="UV_PYTHON_INSTALL_MIRROR"></a> [`UV_PYTHON_INSTALL_MIRROR`](#UV_PYTHON_INSTALL_MIRROR): Managed Python installations are downloaded from
  [`python-build-standalone`](https://github.com/indygreg/python-build-standalone).
  This variable can be set to a mirror URL to use a different source for Python installations.
  The provided URL will replace `https://github.com/indygreg/python-build-standalone/releases/download` in, e.g.,
  `https://github.com/indygreg/python-build-standalone/releases/download/20240713/cpython-3.12.4%2B20240713-aarch64-apple-darwin-install_only.tar.gz`.
  Distributions can be read from a local directory by using the `file://` URL scheme.
- <a id="UV_PYPY_INSTALL_MIRROR"></a> [`UV_PYPY_INSTALL_MIRROR`](#UV_PYPY_INSTALL_MIRROR): Managed PyPy installations are downloaded from
  [python.org](https://downloads.python.org/). This variable can be set to a mirror URL to use a
  different source for PyPy installations. The provided URL will replace
  `https://downloads.python.org/pypy` in, e.g.,
  `https://downloads.python.org/pypy/pypy3.8-v7.3.7-osx64.tar.bz2`.
  Distributions can be read from a local directory by using the `file://` URL scheme.
- <a id="UV_NO_WRAP"></a> [`UV_NO_WRAP`](#UV_NO_WRAP): Use to disable line wrapping for diagnostics.
- <a id="UV_STACK_SIZE"></a> [`UV_STACK_SIZE`](#UV_STACK_SIZE): Use to control the stack size used by uv. Typically more relevant for Windows in debug mode.
- <a id="UV_INDEX_{name}_USERNAME"></a> [`UV_INDEX_{name}_USERNAME`](#UV_INDEX_{name}_USERNAME): Generates the environment variable key for the HTTP Basic authentication username.
- <a id="UV_INDEX_{name}_PASSWORD"></a> [`UV_INDEX_{name}_PASSWORD`](#UV_INDEX_{name}_PASSWORD): Generates the environment variable key for the HTTP Basic authentication password.
- <a id="XDG_CONFIG_DIRS"></a> [`XDG_CONFIG_DIRS`](#XDG_CONFIG_DIRS): Path to system-level configuration directory on Unix systems.
- <a id="SYSTEMDRIVE"></a> [`SYSTEMDRIVE`](#SYSTEMDRIVE): Path to system-level configuration directory on Windows systems.
- <a id="XDG_CONFIG_HOME"></a> [`XDG_CONFIG_HOME`](#XDG_CONFIG_HOME): Path to user-level configuration directory on Unix systems.
- <a id="XDG_CACHE_HOME"></a> [`XDG_CACHE_HOME`](#XDG_CACHE_HOME): Path to cache directory on Unix systems.
- <a id="XDG_DATA_HOME"></a> [`XDG_DATA_HOME`](#XDG_DATA_HOME): Path to directory for storing managed Python installations and tools.
- <a id="XDG_BIN_HOME"></a> [`XDG_BIN_HOME`](#XDG_BIN_HOME): Path to directory where executables are installed.
- <a id="SSL_CERT_FILE"></a> [`SSL_CERT_FILE`](#SSL_CERT_FILE): Custom certificate bundle file path for SSL connections.
- <a id="SSL_CLIENT_CERT"></a> [`SSL_CLIENT_CERT`](#SSL_CLIENT_CERT): If set, uv will use this file for mTLS authentication.
  This should be a single file containing both the certificate and the private key in PEM format.
- <a id="HTTP_PROXY"></a> [`HTTP_PROXY`](#HTTP_PROXY): Proxy for HTTP requests.
- <a id="HTTPS_PROXY"></a> [`HTTPS_PROXY`](#HTTPS_PROXY): Proxy for HTTPS requests.
- <a id="ALL_PROXY"></a> [`ALL_PROXY`](#ALL_PROXY): General proxy for all network requests.
- <a id="UV_HTTP_TIMEOUT"></a> [`UV_HTTP_TIMEOUT`](#UV_HTTP_TIMEOUT): Timeout (in seconds) for HTTP requests. (default: 30 s)
- <a id="UV_REQUEST_TIMEOUT"></a> [`UV_REQUEST_TIMEOUT`](#UV_REQUEST_TIMEOUT): Timeout (in seconds) for HTTP requests. Equivalent to `UV_HTTP_TIMEOUT`.
- <a id="HTTP_TIMEOUT"></a> [`HTTP_TIMEOUT`](#HTTP_TIMEOUT): Timeout (in seconds) for HTTP requests. Equivalent to `UV_HTTP_TIMEOUT`.
- <a id="PYC_INVALIDATION_MODE"></a> [`PYC_INVALIDATION_MODE`](#PYC_INVALIDATION_MODE): The validation modes to use when run with `--compile`.
  See [`PycInvalidationMode`](https://docs.python.org/3/library/py_compile.html#py_compile.PycInvalidationMode).
- <a id="VIRTUAL_ENV"></a> [`VIRTUAL_ENV`](#VIRTUAL_ENV): Used to detect an activated virtual environment.
- <a id="CONDA_PREFIX"></a> [`CONDA_PREFIX`](#CONDA_PREFIX): Used to detect an activated Conda environment.
- <a id="CONDA_DEFAULT_ENV"></a> [`CONDA_DEFAULT_ENV`](#CONDA_DEFAULT_ENV): Used to determine if an active Conda environment is the base environment or not.
- <a id="VIRTUAL_ENV_DISABLE_PROMPT"></a> [`VIRTUAL_ENV_DISABLE_PROMPT`](#VIRTUAL_ENV_DISABLE_PROMPT): If set to `1` before a virtual environment is activated, then the
  virtual environment name will not be prepended to the terminal prompt.
- <a id="PROMPT"></a> [`PROMPT`](#PROMPT): Used to detect the use of the Windows Command Prompt (as opposed to PowerShell).
- <a id="NU_VERSION"></a> [`NU_VERSION`](#NU_VERSION): Used to detect `NuShell` usage.
- <a id="FISH_VERSION"></a> [`FISH_VERSION`](#FISH_VERSION): Used to detect Fish shell usage.
- <a id="BASH_VERSION"></a> [`BASH_VERSION`](#BASH_VERSION): Used to detect Bash shell usage.
- <a id="ZSH_VERSION"></a> [`ZSH_VERSION`](#ZSH_VERSION): Used to detect Zsh shell usage.
- <a id="ZDOTDIR"></a> [`ZDOTDIR`](#ZDOTDIR): Used to determine which `.zshenv` to use when Zsh is being used.
- <a id="KSH_VERSION"></a> [`KSH_VERSION`](#KSH_VERSION): Used to detect Ksh shell usage.
- <a id="MACOSX_DEPLOYMENT_TARGET"></a> [`MACOSX_DEPLOYMENT_TARGET`](#MACOSX_DEPLOYMENT_TARGET): Used with `--python-platform macos` and related variants to set the
  deployment target (i.e., the minimum supported macOS version).
  Defaults to `12.0`, the least-recent non-EOL macOS version at time of writing.
- <a id="NO_COLOR"></a> [`NO_COLOR`](#NO_COLOR): Disables colored output (takes precedence over `FORCE_COLOR`).
  See [no-color.org](https://no-color.org).
- <a id="FORCE_COLOR"></a> [`FORCE_COLOR`](#FORCE_COLOR): Forces colored output regardless of terminal support.
  See [force-color.org](https://force-color.org).
- <a id="CLICOLOR_FORCE"></a> [`CLICOLOR_FORCE`](#CLICOLOR_FORCE): Use to control color via `anstyle`.
- <a id="PATH"></a> [`PATH`](#PATH): The standard `PATH` env var.
- <a id="HOME"></a> [`HOME`](#HOME): The standard `HOME` env var.
- <a id="SHELL"></a> [`SHELL`](#SHELL): The standard `SHELL` posix env var.
- <a id="PWD"></a> [`PWD`](#PWD): The standard `PWD` posix env var.
- <a id="LOCALAPPDATA"></a> [`LOCALAPPDATA`](#LOCALAPPDATA): Used to look for Microsoft Store Pythons installations.
- <a id="GITHUB_ACTIONS"></a> [`GITHUB_ACTIONS`](#GITHUB_ACTIONS): Used for trusted publishing via `uv publish`.
- <a id="ACTIONS_ID_TOKEN_REQUEST_URL"></a> [`ACTIONS_ID_TOKEN_REQUEST_URL`](#ACTIONS_ID_TOKEN_REQUEST_URL): Used for trusted publishing via `uv publish`. Contains the oidc token url.
- <a id="ACTIONS_ID_TOKEN_REQUEST_TOKEN"></a> [`ACTIONS_ID_TOKEN_REQUEST_TOKEN`](#ACTIONS_ID_TOKEN_REQUEST_TOKEN): Used for trusted publishing via `uv publish`. Contains the oidc request token.
- <a id="PYTHONPATH"></a> [`PYTHONPATH`](#PYTHONPATH): Adds directories to Python module search path (e.g., PYTHONPATH=/path/to/modules).
- <a id="NETRC"></a> [`NETRC`](#NETRC): Use to set the .netrc file location.
- <a id="PAGER"></a> [`PAGER`](#PAGER): The standard `PAGER` posix env var. Used by `uv` to configure the appropriate pager.
- <a id="JPY_SESSION_NAME"></a> [`JPY_SESSION_NAME`](#JPY_SESSION_NAME): Used to detect when running inside a Jupyter notebook.
- <a id="TRACING_DURATIONS_FILE"></a> [`TRACING_DURATIONS_FILE`](#TRACING_DURATIONS_FILE): Use to create the tracing durations file via the `tracing-durations-export` feature.
- <a id="RUST_LOG"></a> [`RUST_LOG`](#RUST_LOG): If set, uv will use this value as the log level for its `--verbose` output. Accepts
  any filter compatible with the `tracing_subscriber` crate.
  For example, `RUST_LOG=trace` will enable trace-level logging.
  See the [tracing documentation](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#example-syntax)
  for more.
- <a id="UV_ENV_FILE"></a> [`UV_ENV_FILE`](#UV_ENV_FILE): `.env` files from which to load environment variables when executing `uv run` commands.
- <a id="UV_NO_ENV_FILE"></a> [`UV_NO_ENV_FILE`](#UV_NO_ENV_FILE): Ignore `.env` files when executing `uv run` commands.
