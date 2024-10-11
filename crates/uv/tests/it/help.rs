use crate::common::{uv_snapshot, TestContext};

#[test]
fn help() {
    let context = TestContext::new_with_versions(&[]);

    // The `uv help` command should show the long help message
    uv_snapshot!(context.filters(), context.help(), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    An extremely fast Python package manager.

    Usage: uv [OPTIONS] <COMMAND>

    Commands:
      run                        Run a command or script
      init                       Create a new project
      add                        Add dependencies to the project
      remove                     Remove dependencies from the project
      sync                       Update the project's environment
      lock                       Update the project's lockfile
      export                     Export the project's lockfile to an alternate format
      tree                       Display the project's dependency tree
      tool                       Run and install commands provided by Python packages
      python                     Manage Python versions and installations
      pip                        Manage Python packages with a pip-compatible interface
      venv                       Create a virtual environment
      build                      Build Python packages into source distributions and wheels
      publish                    Upload distributions to an index
      cache                      Manage uv's cache
      version                    Display uv's version
      generate-shell-completion  Generate shell completion
      help                       Display documentation for a command

    Cache options:
      -n, --no-cache               Avoid reading from or writing to the cache, instead using a temporary
                                   directory for the duration of the operation [env: UV_NO_CACHE=]
          --cache-dir [CACHE_DIR]  Path to the cache directory [env: UV_CACHE_DIR=]

    Python options:
          --python-preference <PYTHON_PREFERENCE>
              Whether to prefer uv-managed or system Python installations [env: UV_PYTHON_PREFERENCE=]
              [possible values: only-managed, managed, system, only-system]
          --no-python-downloads
              Disable automatic downloads of Python. [env: "UV_PYTHON_DOWNLOADS=never"]

    Global options:
      -q, --quiet                      Do not print any output
      -v, --verbose...                 Use verbose output
          --color <COLOR_CHOICE>       Control colors in output [default: auto] [possible values: auto,
                                       always, never]
          --native-tls                 Whether to load TLS certificates from the platform's native
                                       certificate store [env: UV_NATIVE_TLS=]
          --offline                    Disable network access
          --no-progress                Hide all progress outputs
          --directory <DIRECTORY>      Change to the given directory prior to running the command
          --project <PROJECT>          Run the command within the given project directory
          --config-file <CONFIG_FILE>  The path to a `uv.toml` file to use for configuration [env:
                                       UV_CONFIG_FILE=]
          --no-config                  Avoid discovering configuration files (`pyproject.toml`,
                                       `uv.toml`) [env: UV_NO_CONFIG=]
      -h, --help                       Display the concise help for this command
      -V, --version                    Display the uv version

    Use `uv help <command>` for more information on a specific command.


    ----- stderr -----
    "#);
}

#[test]
fn help_flag() {
    let context = TestContext::new_with_versions(&[]);

    uv_snapshot!(context.filters(), context.command().arg("--help"), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    An extremely fast Python package manager.

    Usage: uv [OPTIONS] <COMMAND>

    Commands:
      run      Run a command or script
      init     Create a new project
      add      Add dependencies to the project
      remove   Remove dependencies from the project
      sync     Update the project's environment
      lock     Update the project's lockfile
      export   Export the project's lockfile to an alternate format
      tree     Display the project's dependency tree
      tool     Run and install commands provided by Python packages
      python   Manage Python versions and installations
      pip      Manage Python packages with a pip-compatible interface
      venv     Create a virtual environment
      build    Build Python packages into source distributions and wheels
      publish  Upload distributions to an index
      cache    Manage uv's cache
      version  Display uv's version
      help     Display documentation for a command

    Cache options:
      -n, --no-cache               Avoid reading from or writing to the cache, instead using a temporary
                                   directory for the duration of the operation [env: UV_NO_CACHE=]
          --cache-dir [CACHE_DIR]  Path to the cache directory [env: UV_CACHE_DIR=]

    Python options:
          --python-preference <PYTHON_PREFERENCE>
              Whether to prefer uv-managed or system Python installations [env: UV_PYTHON_PREFERENCE=]
              [possible values: only-managed, managed, system, only-system]
          --no-python-downloads
              Disable automatic downloads of Python. [env: "UV_PYTHON_DOWNLOADS=never"]

    Global options:
      -q, --quiet                      Do not print any output
      -v, --verbose...                 Use verbose output
          --color <COLOR_CHOICE>       Control colors in output [default: auto] [possible values: auto,
                                       always, never]
          --native-tls                 Whether to load TLS certificates from the platform's native
                                       certificate store [env: UV_NATIVE_TLS=]
          --offline                    Disable network access
          --no-progress                Hide all progress outputs
          --directory <DIRECTORY>      Change to the given directory prior to running the command
          --project <PROJECT>          Run the command within the given project directory
          --config-file <CONFIG_FILE>  The path to a `uv.toml` file to use for configuration [env:
                                       UV_CONFIG_FILE=]
          --no-config                  Avoid discovering configuration files (`pyproject.toml`,
                                       `uv.toml`) [env: UV_NO_CONFIG=]
      -h, --help                       Display the concise help for this command
      -V, --version                    Display the uv version

    Use `uv help` for more details.

    ----- stderr -----
    "#);
}

#[test]
fn help_short_flag() {
    let context = TestContext::new_with_versions(&[]);

    uv_snapshot!(context.filters(), context.command().arg("-h"), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    An extremely fast Python package manager.

    Usage: uv [OPTIONS] <COMMAND>

    Commands:
      run      Run a command or script
      init     Create a new project
      add      Add dependencies to the project
      remove   Remove dependencies from the project
      sync     Update the project's environment
      lock     Update the project's lockfile
      export   Export the project's lockfile to an alternate format
      tree     Display the project's dependency tree
      tool     Run and install commands provided by Python packages
      python   Manage Python versions and installations
      pip      Manage Python packages with a pip-compatible interface
      venv     Create a virtual environment
      build    Build Python packages into source distributions and wheels
      publish  Upload distributions to an index
      cache    Manage uv's cache
      version  Display uv's version
      help     Display documentation for a command

    Cache options:
      -n, --no-cache               Avoid reading from or writing to the cache, instead using a temporary
                                   directory for the duration of the operation [env: UV_NO_CACHE=]
          --cache-dir [CACHE_DIR]  Path to the cache directory [env: UV_CACHE_DIR=]

    Python options:
          --python-preference <PYTHON_PREFERENCE>
              Whether to prefer uv-managed or system Python installations [env: UV_PYTHON_PREFERENCE=]
              [possible values: only-managed, managed, system, only-system]
          --no-python-downloads
              Disable automatic downloads of Python. [env: "UV_PYTHON_DOWNLOADS=never"]

    Global options:
      -q, --quiet                      Do not print any output
      -v, --verbose...                 Use verbose output
          --color <COLOR_CHOICE>       Control colors in output [default: auto] [possible values: auto,
                                       always, never]
          --native-tls                 Whether to load TLS certificates from the platform's native
                                       certificate store [env: UV_NATIVE_TLS=]
          --offline                    Disable network access
          --no-progress                Hide all progress outputs
          --directory <DIRECTORY>      Change to the given directory prior to running the command
          --project <PROJECT>          Run the command within the given project directory
          --config-file <CONFIG_FILE>  The path to a `uv.toml` file to use for configuration [env:
                                       UV_CONFIG_FILE=]
          --no-config                  Avoid discovering configuration files (`pyproject.toml`,
                                       `uv.toml`) [env: UV_NO_CONFIG=]
      -h, --help                       Display the concise help for this command
      -V, --version                    Display the uv version

    Use `uv help` for more details.

    ----- stderr -----
    "#);
}

#[test]
fn help_subcommand() {
    let context = TestContext::new_with_versions(&[]);

    uv_snapshot!(context.filters(), context.help().arg("python"), @r##"
    success: true
    exit_code: 0
    ----- stdout -----
    Manage Python versions and installations

    Generally, uv first searches for Python in a virtual environment, either active or in a
    `.venv` directory in the current working directory or any parent directory. If a virtual
    environment is not required, uv will then search for a Python interpreter. Python
    interpreters are found by searching for Python executables in the `PATH` environment
    variable.

    On Windows, the `py` launcher is also invoked to find Python executables.

    By default, uv will download Python if a version cannot be found. This behavior can be
    disabled with the `--no-python-downloads` flag or the `python-downloads` setting.

    The `--python` option allows requesting a different interpreter.

    The following Python version request formats are supported:

    - `<version>` e.g. `3`, `3.12`, `3.12.3`
    - `<version-specifier>` e.g. `>=3.12,<3.13`
    - `<implementation>` e.g. `cpython` or `cp`
    - `<implementation>@<version>` e.g. `cpython@3.12`
    - `<implementation><version>` e.g. `cpython3.12` or `cp312`
    - `<implementation><version-specifier>` e.g. `cpython>=3.12,<3.13`
    - `<implementation>-<version>-<os>-<arch>-<libc>` e.g. `cpython-3.12.3-macos-aarch64-none`

    Additionally, a specific system Python interpreter can often be requested with:

    - `<executable-path>` e.g. `/opt/homebrew/bin/python3`
    - `<executable-name>` e.g. `mypython3`
    - `<install-dir>` e.g. `/some/environment/`

    When the `--python` option is used, normal discovery rules apply but discovered interpreters
    are checked for compatibility with the request, e.g., if `pypy` is requested, uv will first
    check if the virtual environment contains a PyPy interpreter then check if each executable
    in the path is a PyPy interpreter.

    uv supports discovering CPython, PyPy, and GraalPy interpreters. Unsupported interpreters
    will be skipped during discovery. If an unsupported interpreter implementation is requested,
    uv will exit with an error.

    Usage: uv python [OPTIONS] <COMMAND>

    Commands:
      list       List the available Python installations
      install    Download and install Python versions
      find       Search for a Python installation
      pin        Pin to a specific Python version
      dir        Show the uv Python installation directory
      uninstall  Uninstall Python versions

    Cache options:
      -n, --no-cache
              Avoid reading from or writing to the cache, instead using a temporary directory for the
              duration of the operation
              
              [env: UV_NO_CACHE=]

          --cache-dir [CACHE_DIR]
              Path to the cache directory.
              
              Defaults to `$HOME/Library/Caches/uv` on macOS, `$XDG_CACHE_HOME/uv` or `$HOME/.cache/uv`
              on Linux, and `%LOCALAPPDATA%/uv/cache` on Windows.
              
              [env: UV_CACHE_DIR=]

    Python options:
          --python-preference <PYTHON_PREFERENCE>
              Whether to prefer uv-managed or system Python installations.
              
              By default, uv prefers using Python versions it manages. However, it will use system
              Python installations if a uv-managed Python is not installed. This option allows
              prioritizing or ignoring system Python installations.
              
              [env: UV_PYTHON_PREFERENCE=]

              Possible values:
              - only-managed: Only use managed Python installations; never use system Python
                installations
              - managed:      Prefer managed Python installations over system Python installations
              - system:       Prefer system Python installations over managed Python installations
              - only-system:  Only use system Python installations; never use managed Python
                installations

          --no-python-downloads
              Disable automatic downloads of Python. [env: "UV_PYTHON_DOWNLOADS=never"]

    Global options:
      -q, --quiet
              Do not print any output

      -v, --verbose...
              Use verbose output.
              
              You can configure fine-grained logging using the `RUST_LOG` environment variable.
              (<https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives>)

          --color <COLOR_CHOICE>
              Control colors in output
              
              [default: auto]

              Possible values:
              - auto:   Enables colored output only when the output is going to a terminal or TTY with
                support
              - always: Enables colored output regardless of the detected environment
              - never:  Disables colored output

          --native-tls
              Whether to load TLS certificates from the platform's native certificate store.
              
              By default, uv loads certificates from the bundled `webpki-roots` crate. The
              `webpki-roots` are a reliable set of trust roots from Mozilla, and including them in uv
              improves portability and performance (especially on macOS).
              
              However, in some cases, you may want to use the platform's native certificate store,
              especially if you're relying on a corporate trust root (e.g., for a mandatory proxy)
              that's included in your system's certificate store.
              
              [env: UV_NATIVE_TLS=]

          --offline
              Disable network access.
              
              When disabled, uv will only use locally cached data and locally available files.

          --no-progress
              Hide all progress outputs.
              
              For example, spinners or progress bars.

          --directory <DIRECTORY>
              Change to the given directory prior to running the command.
              
              Relative paths are resolved with the given directory as the base.
              
              See `--project` to only change the project root directory.

          --project <PROJECT>
              Run the command within the given project directory.
              
              All `pyproject.toml`, `uv.toml`, and `.python-version` files will be discovered by walking
              up the directory tree from the project root, as will the project's virtual environment
              (`.venv`).
              
              Other command-line arguments (such as relative paths) will be resolved relative to the
              current working directory.
              
              See `--directory` to change the working directory entirely.
              
              This setting has no effect when used in the `uv pip` interface.

          --config-file <CONFIG_FILE>
              The path to a `uv.toml` file to use for configuration.
              
              While uv configuration can be included in a `pyproject.toml` file, it is not allowed in
              this context.
              
              [env: UV_CONFIG_FILE=]

          --no-config
              Avoid discovering configuration files (`pyproject.toml`, `uv.toml`).
              
              Normally, configuration files are discovered in the current directory, parent directories,
              or user configuration directories.
              
              [env: UV_NO_CONFIG=]

      -h, --help
              Display the concise help for this command

      -V, --version
              Display the uv version

    Use `uv help python <command>` for more information on a specific command.


    ----- stderr -----
    "##);
}

#[test]
fn help_subsubcommand() {
    let context = TestContext::new_with_versions(&[]);

    uv_snapshot!(context.filters(), context.help().arg("python").arg("install"), @r##"
    success: true
    exit_code: 0
    ----- stdout -----
    Download and install Python versions.

    Multiple Python versions may be requested.

    Supports CPython and PyPy.

    CPython distributions are downloaded from the `python-build-standalone` project.

    Python versions are installed into the uv Python directory, which can be retrieved with `uv python
    dir`. A `python` executable is not made globally available, managed Python versions are only used in
    uv commands or in active virtual environments.

    See `uv help python` to view supported request formats.

    Usage: uv python install [OPTIONS] [TARGETS]...

    Arguments:
      [TARGETS]...
              The Python version(s) to install.
              
              If not provided, the requested Python version(s) will be read from the `.python-versions`
              or `.python-version` files. If neither file is present, uv will check if it has installed
              any Python versions. If not, it will install the latest stable version of Python.
              
              See `uv help python` to view supported request formats.

    Options:
      -r, --reinstall
              Reinstall the requested Python version, if it's already installed.
              
              By default, uv will exit successfully if the version is already installed.

    Cache options:
      -n, --no-cache
              Avoid reading from or writing to the cache, instead using a temporary directory for the
              duration of the operation
              
              [env: UV_NO_CACHE=]

          --cache-dir [CACHE_DIR]
              Path to the cache directory.
              
              Defaults to `$HOME/Library/Caches/uv` on macOS, `$XDG_CACHE_HOME/uv` or `$HOME/.cache/uv`
              on Linux, and `%LOCALAPPDATA%/uv/cache` on Windows.
              
              [env: UV_CACHE_DIR=]

    Python options:
          --python-preference <PYTHON_PREFERENCE>
              Whether to prefer uv-managed or system Python installations.
              
              By default, uv prefers using Python versions it manages. However, it will use system
              Python installations if a uv-managed Python is not installed. This option allows
              prioritizing or ignoring system Python installations.
              
              [env: UV_PYTHON_PREFERENCE=]

              Possible values:
              - only-managed: Only use managed Python installations; never use system Python
                installations
              - managed:      Prefer managed Python installations over system Python installations
              - system:       Prefer system Python installations over managed Python installations
              - only-system:  Only use system Python installations; never use managed Python
                installations

          --no-python-downloads
              Disable automatic downloads of Python. [env: "UV_PYTHON_DOWNLOADS=never"]

    Global options:
      -q, --quiet
              Do not print any output

      -v, --verbose...
              Use verbose output.
              
              You can configure fine-grained logging using the `RUST_LOG` environment variable.
              (<https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives>)

          --color <COLOR_CHOICE>
              Control colors in output
              
              [default: auto]

              Possible values:
              - auto:   Enables colored output only when the output is going to a terminal or TTY with
                support
              - always: Enables colored output regardless of the detected environment
              - never:  Disables colored output

          --native-tls
              Whether to load TLS certificates from the platform's native certificate store.
              
              By default, uv loads certificates from the bundled `webpki-roots` crate. The
              `webpki-roots` are a reliable set of trust roots from Mozilla, and including them in uv
              improves portability and performance (especially on macOS).
              
              However, in some cases, you may want to use the platform's native certificate store,
              especially if you're relying on a corporate trust root (e.g., for a mandatory proxy)
              that's included in your system's certificate store.
              
              [env: UV_NATIVE_TLS=]

          --offline
              Disable network access.
              
              When disabled, uv will only use locally cached data and locally available files.

          --no-progress
              Hide all progress outputs.
              
              For example, spinners or progress bars.

          --directory <DIRECTORY>
              Change to the given directory prior to running the command.
              
              Relative paths are resolved with the given directory as the base.
              
              See `--project` to only change the project root directory.

          --project <PROJECT>
              Run the command within the given project directory.
              
              All `pyproject.toml`, `uv.toml`, and `.python-version` files will be discovered by walking
              up the directory tree from the project root, as will the project's virtual environment
              (`.venv`).
              
              Other command-line arguments (such as relative paths) will be resolved relative to the
              current working directory.
              
              See `--directory` to change the working directory entirely.
              
              This setting has no effect when used in the `uv pip` interface.

          --config-file <CONFIG_FILE>
              The path to a `uv.toml` file to use for configuration.
              
              While uv configuration can be included in a `pyproject.toml` file, it is not allowed in
              this context.
              
              [env: UV_CONFIG_FILE=]

          --no-config
              Avoid discovering configuration files (`pyproject.toml`, `uv.toml`).
              
              Normally, configuration files are discovered in the current directory, parent directories,
              or user configuration directories.
              
              [env: UV_NO_CONFIG=]

      -h, --help
              Display the concise help for this command

      -V, --version
              Display the uv version


    ----- stderr -----
    "##);
}

#[test]
fn help_flag_subcommand() {
    let context = TestContext::new_with_versions(&[]);

    uv_snapshot!(context.filters(), context.command().arg("python").arg("--help"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Manage Python versions and installations

    Usage: uv python [OPTIONS] <COMMAND>

    Commands:
      list       List the available Python installations
      install    Download and install Python versions
      find       Search for a Python installation
      pin        Pin to a specific Python version
      dir        Show the uv Python installation directory
      uninstall  Uninstall Python versions

    Cache options:
      -n, --no-cache               Avoid reading from or writing to the cache, instead using a temporary
                                   directory for the duration of the operation [env: UV_NO_CACHE=]
          --cache-dir [CACHE_DIR]  Path to the cache directory [env: UV_CACHE_DIR=]

    Python options:
          --python-preference <PYTHON_PREFERENCE>
              Whether to prefer uv-managed or system Python installations [env: UV_PYTHON_PREFERENCE=]
              [possible values: only-managed, managed, system, only-system]
          --no-python-downloads
              Disable automatic downloads of Python. [env: "UV_PYTHON_DOWNLOADS=never"]

    Global options:
      -q, --quiet                      Do not print any output
      -v, --verbose...                 Use verbose output
          --color <COLOR_CHOICE>       Control colors in output [default: auto] [possible values: auto,
                                       always, never]
          --native-tls                 Whether to load TLS certificates from the platform's native
                                       certificate store [env: UV_NATIVE_TLS=]
          --offline                    Disable network access
          --no-progress                Hide all progress outputs
          --directory <DIRECTORY>      Change to the given directory prior to running the command
          --project <PROJECT>          Run the command within the given project directory
          --config-file <CONFIG_FILE>  The path to a `uv.toml` file to use for configuration [env:
                                       UV_CONFIG_FILE=]
          --no-config                  Avoid discovering configuration files (`pyproject.toml`,
                                       `uv.toml`) [env: UV_NO_CONFIG=]
      -h, --help                       Display the concise help for this command
      -V, --version                    Display the uv version

    Use `uv help python` for more details.

    ----- stderr -----
    "###);
}

#[test]
fn help_flag_subsubcommand() {
    let context = TestContext::new_with_versions(&[]);

    uv_snapshot!(context.filters(), context.command().arg("python").arg("install").arg("--help"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Download and install Python versions

    Usage: uv python install [OPTIONS] [TARGETS]...

    Arguments:
      [TARGETS]...  The Python version(s) to install

    Options:
      -r, --reinstall  Reinstall the requested Python version, if it's already installed

    Cache options:
      -n, --no-cache               Avoid reading from or writing to the cache, instead using a temporary
                                   directory for the duration of the operation [env: UV_NO_CACHE=]
          --cache-dir [CACHE_DIR]  Path to the cache directory [env: UV_CACHE_DIR=]

    Python options:
          --python-preference <PYTHON_PREFERENCE>
              Whether to prefer uv-managed or system Python installations [env: UV_PYTHON_PREFERENCE=]
              [possible values: only-managed, managed, system, only-system]
          --no-python-downloads
              Disable automatic downloads of Python. [env: "UV_PYTHON_DOWNLOADS=never"]

    Global options:
      -q, --quiet                      Do not print any output
      -v, --verbose...                 Use verbose output
          --color <COLOR_CHOICE>       Control colors in output [default: auto] [possible values: auto,
                                       always, never]
          --native-tls                 Whether to load TLS certificates from the platform's native
                                       certificate store [env: UV_NATIVE_TLS=]
          --offline                    Disable network access
          --no-progress                Hide all progress outputs
          --directory <DIRECTORY>      Change to the given directory prior to running the command
          --project <PROJECT>          Run the command within the given project directory
          --config-file <CONFIG_FILE>  The path to a `uv.toml` file to use for configuration [env:
                                       UV_CONFIG_FILE=]
          --no-config                  Avoid discovering configuration files (`pyproject.toml`,
                                       `uv.toml`) [env: UV_NO_CONFIG=]
      -h, --help                       Display the concise help for this command
      -V, --version                    Display the uv version

    ----- stderr -----
    "###);
}

#[test]
fn help_unknown_subcommand() {
    let context = TestContext::new_with_versions(&[]);

    uv_snapshot!(context.filters(), context.help().arg("foobar"), @r#"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: There is no command `foobar` for `uv`. Did you mean one of:
        run
        init
        add
        remove
        sync
        lock
        export
        tree
        tool
        python
        pip
        venv
        build
        publish
        cache
        version
        generate-shell-completion
    "#);

    uv_snapshot!(context.filters(), context.help().arg("foo").arg("bar"), @r#"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: There is no command `foo bar` for `uv`. Did you mean one of:
        run
        init
        add
        remove
        sync
        lock
        export
        tree
        tool
        python
        pip
        venv
        build
        publish
        cache
        version
        generate-shell-completion
    "#);
}

#[test]
fn help_unknown_subsubcommand() {
    let context = TestContext::new_with_versions(&[]);

    uv_snapshot!(context.filters(), context.help().arg("python").arg("foobar"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: There is no command `foobar` for `uv python`. Did you mean one of:
        list
        install
        find
        pin
        dir
        uninstall
    "###);
}

#[test]
fn help_with_global_option() {
    let context = TestContext::new_with_versions(&[]);

    uv_snapshot!(context.filters(), context.help().arg("--no-cache"), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    An extremely fast Python package manager.

    Usage: uv [OPTIONS] <COMMAND>

    Commands:
      run                        Run a command or script
      init                       Create a new project
      add                        Add dependencies to the project
      remove                     Remove dependencies from the project
      sync                       Update the project's environment
      lock                       Update the project's lockfile
      export                     Export the project's lockfile to an alternate format
      tree                       Display the project's dependency tree
      tool                       Run and install commands provided by Python packages
      python                     Manage Python versions and installations
      pip                        Manage Python packages with a pip-compatible interface
      venv                       Create a virtual environment
      build                      Build Python packages into source distributions and wheels
      publish                    Upload distributions to an index
      cache                      Manage uv's cache
      version                    Display uv's version
      generate-shell-completion  Generate shell completion
      help                       Display documentation for a command

    Cache options:
      -n, --no-cache               Avoid reading from or writing to the cache, instead using a temporary
                                   directory for the duration of the operation [env: UV_NO_CACHE=]
          --cache-dir [CACHE_DIR]  Path to the cache directory [env: UV_CACHE_DIR=]

    Python options:
          --python-preference <PYTHON_PREFERENCE>
              Whether to prefer uv-managed or system Python installations [env: UV_PYTHON_PREFERENCE=]
              [possible values: only-managed, managed, system, only-system]
          --no-python-downloads
              Disable automatic downloads of Python. [env: "UV_PYTHON_DOWNLOADS=never"]

    Global options:
      -q, --quiet                      Do not print any output
      -v, --verbose...                 Use verbose output
          --color <COLOR_CHOICE>       Control colors in output [default: auto] [possible values: auto,
                                       always, never]
          --native-tls                 Whether to load TLS certificates from the platform's native
                                       certificate store [env: UV_NATIVE_TLS=]
          --offline                    Disable network access
          --no-progress                Hide all progress outputs
          --directory <DIRECTORY>      Change to the given directory prior to running the command
          --project <PROJECT>          Run the command within the given project directory
          --config-file <CONFIG_FILE>  The path to a `uv.toml` file to use for configuration [env:
                                       UV_CONFIG_FILE=]
          --no-config                  Avoid discovering configuration files (`pyproject.toml`,
                                       `uv.toml`) [env: UV_NO_CONFIG=]
      -h, --help                       Display the concise help for this command
      -V, --version                    Display the uv version

    Use `uv help <command>` for more information on a specific command.


    ----- stderr -----
    "#);
}

#[test]
fn help_with_help() {
    let context = TestContext::new_with_versions(&[]);

    uv_snapshot!(context.filters(), context.help().arg("--help"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Display documentation for a command

    Usage: uv help [OPTIONS] [COMMAND]...

    Options:
      --no-pager Disable pager when printing help

    ----- stderr -----
    "###);
}

#[test]
fn help_with_version() {
    let context = TestContext::new_with_versions(&[]);

    uv_snapshot!(context.filters(), context.help().arg("--version"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    uv [VERSION] ([COMMIT] DATE)

    ----- stderr -----
    "###);
}

#[test]
fn help_with_no_pager() {
    let context = TestContext::new_with_versions(&[]);

    // We can't really test whether the --no-pager option works with a snapshot test.
    // It's still nice to have a test for the option to confirm the option exists.
    uv_snapshot!(context.filters(), context.help().arg("--no-pager"), @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    An extremely fast Python package manager.

    Usage: uv [OPTIONS] <COMMAND>

    Commands:
      run                        Run a command or script
      init                       Create a new project
      add                        Add dependencies to the project
      remove                     Remove dependencies from the project
      sync                       Update the project's environment
      lock                       Update the project's lockfile
      export                     Export the project's lockfile to an alternate format
      tree                       Display the project's dependency tree
      tool                       Run and install commands provided by Python packages
      python                     Manage Python versions and installations
      pip                        Manage Python packages with a pip-compatible interface
      venv                       Create a virtual environment
      build                      Build Python packages into source distributions and wheels
      publish                    Upload distributions to an index
      cache                      Manage uv's cache
      version                    Display uv's version
      generate-shell-completion  Generate shell completion
      help                       Display documentation for a command

    Cache options:
      -n, --no-cache               Avoid reading from or writing to the cache, instead using a temporary
                                   directory for the duration of the operation [env: UV_NO_CACHE=]
          --cache-dir [CACHE_DIR]  Path to the cache directory [env: UV_CACHE_DIR=]

    Python options:
          --python-preference <PYTHON_PREFERENCE>
              Whether to prefer uv-managed or system Python installations [env: UV_PYTHON_PREFERENCE=]
              [possible values: only-managed, managed, system, only-system]
          --no-python-downloads
              Disable automatic downloads of Python. [env: "UV_PYTHON_DOWNLOADS=never"]

    Global options:
      -q, --quiet                      Do not print any output
      -v, --verbose...                 Use verbose output
          --color <COLOR_CHOICE>       Control colors in output [default: auto] [possible values: auto,
                                       always, never]
          --native-tls                 Whether to load TLS certificates from the platform's native
                                       certificate store [env: UV_NATIVE_TLS=]
          --offline                    Disable network access
          --no-progress                Hide all progress outputs
          --directory <DIRECTORY>      Change to the given directory prior to running the command
          --project <PROJECT>          Run the command within the given project directory
          --config-file <CONFIG_FILE>  The path to a `uv.toml` file to use for configuration [env:
                                       UV_CONFIG_FILE=]
          --no-config                  Avoid discovering configuration files (`pyproject.toml`,
                                       `uv.toml`) [env: UV_NO_CONFIG=]
      -h, --help                       Display the concise help for this command
      -V, --version                    Display the uv version

    Use `uv help <command>` for more information on a specific command.


    ----- stderr -----
    "#);
}
