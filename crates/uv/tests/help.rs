use common::{uv_snapshot, TestContext};

mod common;

#[test]
fn help() {
    let context = TestContext::new_with_versions(&[]);

    // The `uv help` command should show the long help message
    uv_snapshot!(context.filters(), context.help(), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    An extremely fast Python package manager.

    Usage: uv [OPTIONS] <COMMAND>

    Commands:
      pip      Resolve and install Python packages
      tool     Run and manage executable Python packages
      python   Manage Python installations
      venv     Create a virtual environment
      cache    Manage the cache
      version  Display uv's version
      help     Display documentation for a command

    Options:
      -q, --quiet
              Do not print any output
      -v, --verbose...
              Use verbose output
          --color <COLOR_CHOICE>
              Control colors in output [default: auto] [possible values: auto, always, never]
          --native-tls
              Whether to load TLS certificates from the platform's native certificate store [env:
              UV_NATIVE_TLS=]
          --offline
              Disable network access, relying only on locally cached data and locally available files
          --python-preference <PYTHON_PREFERENCE>
              Whether to prefer using Python installations that are already present on the system, or
              those that are downloaded and installed by uv [possible values: only-managed, installed,
              managed, system, only-system]
          --python-fetch <PYTHON_FETCH>
              Whether to automatically download Python when required [possible values: automatic,
              manual]
          --no-progress
              Hides all progress outputs when set
      -n, --no-cache
              Avoid reading from or writing to the cache, instead using a temporary directory for the
              duration of the operation [env: UV_NO_CACHE=]
          --cache-dir [CACHE_DIR]
              Path to the cache directory [env: UV_CACHE_DIR=]
          --config-file <CONFIG_FILE>
              The path to a `uv.toml` file to use for configuration [env: UV_CONFIG_FILE=]
          --no-config
              Avoid discovering configuration files (`pyproject.toml`, `uv.toml`) in the current
              directory, parent directories, or user configuration directories [env: UV_NO_CONFIG=]
      -h, --help
              Print help
      -V, --version
              Print version

    Use `uv help <command>` for more information on a specific command.


    ----- stderr -----
    "###);
}

#[test]
fn help_flag() {
    let context = TestContext::new_with_versions(&[]);
    uv_snapshot!(context.filters(), context.command().arg("--help"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    An extremely fast Python package manager.

    Usage: uv [OPTIONS] <COMMAND>

    Commands:
      pip      Resolve and install Python packages
      tool     Run and manage executable Python packages
      python   Manage Python installations
      venv     Create a virtual environment
      cache    Manage the cache
      version  Display uv's version
      help     Display documentation for a command

    Options:
      -q, --quiet
              Do not print any output
      -v, --verbose...
              Use verbose output
          --color <COLOR_CHOICE>
              Control colors in output [default: auto] [possible values: auto, always, never]
          --native-tls
              Whether to load TLS certificates from the platform's native certificate store [env:
              UV_NATIVE_TLS=]
          --offline
              Disable network access, relying only on locally cached data and locally available files
          --python-preference <PYTHON_PREFERENCE>
              Whether to prefer using Python installations that are already present on the system, or
              those that are downloaded and installed by uv [possible values: only-managed, installed,
              managed, system, only-system]
          --python-fetch <PYTHON_FETCH>
              Whether to automatically download Python when required [possible values: automatic,
              manual]
          --no-progress
              Hides all progress outputs when set
      -n, --no-cache
              Avoid reading from or writing to the cache, instead using a temporary directory for the
              duration of the operation [env: UV_NO_CACHE=]
          --cache-dir [CACHE_DIR]
              Path to the cache directory [env: UV_CACHE_DIR=]
          --config-file <CONFIG_FILE>
              The path to a `uv.toml` file to use for configuration [env: UV_CONFIG_FILE=]
          --no-config
              Avoid discovering configuration files (`pyproject.toml`, `uv.toml`) in the current
              directory, parent directories, or user configuration directories [env: UV_NO_CONFIG=]
      -h, --help
              Print help
      -V, --version
              Print version

    Use `uv help` for more details.

    ----- stderr -----
    "###);
}

#[test]
fn help_short_flag() {
    let context = TestContext::new_with_versions(&[]);
    uv_snapshot!(context.filters(), context.command().arg("-h"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    An extremely fast Python package manager.

    Usage: uv [OPTIONS] <COMMAND>

    Commands:
      pip      Resolve and install Python packages
      tool     Run and manage executable Python packages
      python   Manage Python installations
      venv     Create a virtual environment
      cache    Manage the cache
      version  Display uv's version
      help     Display documentation for a command

    Options:
      -q, --quiet
              Do not print any output
      -v, --verbose...
              Use verbose output
          --color <COLOR_CHOICE>
              Control colors in output [default: auto] [possible values: auto, always, never]
          --native-tls
              Whether to load TLS certificates from the platform's native certificate store [env:
              UV_NATIVE_TLS=]
          --offline
              Disable network access, relying only on locally cached data and locally available files
          --python-preference <PYTHON_PREFERENCE>
              Whether to prefer using Python installations that are already present on the system, or
              those that are downloaded and installed by uv [possible values: only-managed, installed,
              managed, system, only-system]
          --python-fetch <PYTHON_FETCH>
              Whether to automatically download Python when required [possible values: automatic,
              manual]
          --no-progress
              Hides all progress outputs when set
      -n, --no-cache
              Avoid reading from or writing to the cache, instead using a temporary directory for the
              duration of the operation [env: UV_NO_CACHE=]
          --cache-dir [CACHE_DIR]
              Path to the cache directory [env: UV_CACHE_DIR=]
          --config-file <CONFIG_FILE>
              The path to a `uv.toml` file to use for configuration [env: UV_CONFIG_FILE=]
          --no-config
              Avoid discovering configuration files (`pyproject.toml`, `uv.toml`) in the current
              directory, parent directories, or user configuration directories [env: UV_NO_CONFIG=]
      -h, --help
              Print help
      -V, --version
              Print version

    Use `uv help` for more details.

    ----- stderr -----
    "###);
}

#[test]
fn help_subcommand() {
    let context = TestContext::new_with_versions(&[]);

    uv_snapshot!(context.filters(), context.help().arg("python"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Manage Python installations

    Usage: uv python [OPTIONS] <COMMAND>

    Commands:
      list       List the available Python installations
      install    Download and install Python versions
      find       Search for a Python installation
      pin        Pin to a specific Python version
      dir        Show the uv Python installation directory
      uninstall  Uninstall Python versions

    Options:
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
              Disable network access, relying only on locally cached data and locally available files

          --python-preference <PYTHON_PREFERENCE>
              Whether to prefer using Python installations that are already present on the system, or
              those that are downloaded and installed by uv

              Possible values:
              - only-managed: Only use managed Python installations; never use system Python
                installations
              - installed:    Prefer installed Python installations, only download managed Python
                installations if no system Python installation is found
              - managed:      Prefer managed Python installations over system Python installations, even
                if fetching is required
              - system:       Prefer system Python installations over managed Python installations
              - only-system:  Only use system Python installations; never use managed Python
                installations

          --python-fetch <PYTHON_FETCH>
              Whether to automatically download Python when required

              Possible values:
              - automatic: Automatically fetch managed Python installations when needed
              - manual:    Do not automatically fetch managed Python installations; require explicit
                installation

          --no-progress
              Hides all progress outputs when set

      -n, --no-cache
              Avoid reading from or writing to the cache, instead using a temporary directory for the
              duration of the operation
              
              [env: UV_NO_CACHE=]

          --cache-dir [CACHE_DIR]
              Path to the cache directory.
              
              Defaults to `$HOME/Library/Caches/uv` on macOS, `$XDG_CACHE_HOME/uv` or `$HOME/.cache/uv`
              on Linux, and `{FOLDERID_LocalAppData}/uv/cache` on Windows.
              
              [env: UV_CACHE_DIR=]

          --config-file <CONFIG_FILE>
              The path to a `uv.toml` file to use for configuration
              
              [env: UV_CONFIG_FILE=]

          --no-config
              Avoid discovering configuration files (`pyproject.toml`, `uv.toml`) in the current
              directory, parent directories, or user configuration directories
              
              [env: UV_NO_CONFIG=]

      -h, --help
              Print help

      -V, --version
              Print version

    Use `uv help python <command>` for more information on a specific command.


    ----- stderr -----
    "###);
}

#[test]
fn help_subsubcommand() {
    let context = TestContext::new_with_versions(&[]);

    uv_snapshot!(context.filters(), context.help().arg("python").arg("install"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Download and install Python versions

    Usage: uv python install [OPTIONS] [TARGETS]...

    Arguments:
      [TARGETS]...
              The Python version(s) to install.
              
              If not provided, the requested Python version(s) will be read from the `.python-versions`
              or `.python-version` files. If neither file is present, uv will check if it has installed
              any Python versions. If not, it will install the latest stable version of Python.

    Options:
      -r, --reinstall
              Reinstall the requested Python version, if it's already installed

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
              Disable network access, relying only on locally cached data and locally available files

          --python-preference <PYTHON_PREFERENCE>
              Whether to prefer using Python installations that are already present on the system, or
              those that are downloaded and installed by uv

              Possible values:
              - only-managed: Only use managed Python installations; never use system Python
                installations
              - installed:    Prefer installed Python installations, only download managed Python
                installations if no system Python installation is found
              - managed:      Prefer managed Python installations over system Python installations, even
                if fetching is required
              - system:       Prefer system Python installations over managed Python installations
              - only-system:  Only use system Python installations; never use managed Python
                installations

          --python-fetch <PYTHON_FETCH>
              Whether to automatically download Python when required

              Possible values:
              - automatic: Automatically fetch managed Python installations when needed
              - manual:    Do not automatically fetch managed Python installations; require explicit
                installation

          --no-progress
              Hides all progress outputs when set

      -n, --no-cache
              Avoid reading from or writing to the cache, instead using a temporary directory for the
              duration of the operation
              
              [env: UV_NO_CACHE=]

          --cache-dir [CACHE_DIR]
              Path to the cache directory.
              
              Defaults to `$HOME/Library/Caches/uv` on macOS, `$XDG_CACHE_HOME/uv` or `$HOME/.cache/uv`
              on Linux, and `{FOLDERID_LocalAppData}/uv/cache` on Windows.
              
              [env: UV_CACHE_DIR=]

          --config-file <CONFIG_FILE>
              The path to a `uv.toml` file to use for configuration
              
              [env: UV_CONFIG_FILE=]

          --no-config
              Avoid discovering configuration files (`pyproject.toml`, `uv.toml`) in the current
              directory, parent directories, or user configuration directories
              
              [env: UV_NO_CONFIG=]

      -h, --help
              Print help

      -V, --version
              Print version


    ----- stderr -----
    "###);
}

#[test]
fn help_flag_subcommand() {
    let context = TestContext::new_with_versions(&[]);

    uv_snapshot!(context.filters(), context.command().arg("python").arg("--help"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    Manage Python installations

    Usage: uv python [OPTIONS] <COMMAND>

    Commands:
      list       List the available Python installations
      install    Download and install Python versions
      find       Search for a Python installation
      pin        Pin to a specific Python version
      dir        Show the uv Python installation directory
      uninstall  Uninstall Python versions

    Options:
      -q, --quiet
              Do not print any output
      -v, --verbose...
              Use verbose output
          --color <COLOR_CHOICE>
              Control colors in output [default: auto] [possible values: auto, always, never]
          --native-tls
              Whether to load TLS certificates from the platform's native certificate store [env:
              UV_NATIVE_TLS=]
          --offline
              Disable network access, relying only on locally cached data and locally available files
          --python-preference <PYTHON_PREFERENCE>
              Whether to prefer using Python installations that are already present on the system, or
              those that are downloaded and installed by uv [possible values: only-managed, installed,
              managed, system, only-system]
          --python-fetch <PYTHON_FETCH>
              Whether to automatically download Python when required [possible values: automatic,
              manual]
          --no-progress
              Hides all progress outputs when set
      -n, --no-cache
              Avoid reading from or writing to the cache, instead using a temporary directory for the
              duration of the operation [env: UV_NO_CACHE=]
          --cache-dir [CACHE_DIR]
              Path to the cache directory [env: UV_CACHE_DIR=]
          --config-file <CONFIG_FILE>
              The path to a `uv.toml` file to use for configuration [env: UV_CONFIG_FILE=]
          --no-config
              Avoid discovering configuration files (`pyproject.toml`, `uv.toml`) in the current
              directory, parent directories, or user configuration directories [env: UV_NO_CONFIG=]
      -h, --help
              Print help
      -V, --version
              Print version

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
      -r, --reinstall
              Reinstall the requested Python version, if it's already installed
      -q, --quiet
              Do not print any output
      -v, --verbose...
              Use verbose output
          --color <COLOR_CHOICE>
              Control colors in output [default: auto] [possible values: auto, always, never]
          --native-tls
              Whether to load TLS certificates from the platform's native certificate store [env:
              UV_NATIVE_TLS=]
          --offline
              Disable network access, relying only on locally cached data and locally available files
          --python-preference <PYTHON_PREFERENCE>
              Whether to prefer using Python installations that are already present on the system, or
              those that are downloaded and installed by uv [possible values: only-managed, installed,
              managed, system, only-system]
          --python-fetch <PYTHON_FETCH>
              Whether to automatically download Python when required [possible values: automatic,
              manual]
          --no-progress
              Hides all progress outputs when set
      -n, --no-cache
              Avoid reading from or writing to the cache, instead using a temporary directory for the
              duration of the operation [env: UV_NO_CACHE=]
          --cache-dir [CACHE_DIR]
              Path to the cache directory [env: UV_CACHE_DIR=]
          --config-file <CONFIG_FILE>
              The path to a `uv.toml` file to use for configuration [env: UV_CONFIG_FILE=]
          --no-config
              Avoid discovering configuration files (`pyproject.toml`, `uv.toml`) in the current
              directory, parent directories, or user configuration directories [env: UV_NO_CONFIG=]
      -h, --help
              Print help
      -V, --version
              Print version

    ----- stderr -----
    "###);
}

#[test]
fn help_unknown_subcommand() {
    let context = TestContext::new_with_versions(&[]);

    uv_snapshot!(context.filters(), context.help().arg("foobar"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: There is no command `foobar` for `uv`. Did you mean one of:
        pip
        tool
        python
        venv
        cache
        version
    "###);

    uv_snapshot!(context.filters(), context.help().arg("foo").arg("bar"), @r###"
    success: false
    exit_code: 2
    ----- stdout -----

    ----- stderr -----
    error: There is no command `foo bar` for `uv`. Did you mean one of:
        pip
        tool
        python
        venv
        cache
        version
    "###);
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

    uv_snapshot!(context.filters(), context.help().arg("--cache-dir").arg("/dev/null"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    An extremely fast Python package manager.

    Usage: uv [OPTIONS] <COMMAND>

    Commands:
      pip      Resolve and install Python packages
      tool     Run and manage executable Python packages
      python   Manage Python installations
      venv     Create a virtual environment
      cache    Manage the cache
      version  Display uv's version
      help     Display documentation for a command

    Options:
      -q, --quiet
              Do not print any output
      -v, --verbose...
              Use verbose output
          --color <COLOR_CHOICE>
              Control colors in output [default: auto] [possible values: auto, always, never]
          --native-tls
              Whether to load TLS certificates from the platform's native certificate store [env:
              UV_NATIVE_TLS=]
          --offline
              Disable network access, relying only on locally cached data and locally available files
          --python-preference <PYTHON_PREFERENCE>
              Whether to prefer using Python installations that are already present on the system, or
              those that are downloaded and installed by uv [possible values: only-managed, installed,
              managed, system, only-system]
          --python-fetch <PYTHON_FETCH>
              Whether to automatically download Python when required [possible values: automatic,
              manual]
          --no-progress
              Hides all progress outputs when set
      -n, --no-cache
              Avoid reading from or writing to the cache, instead using a temporary directory for the
              duration of the operation [env: UV_NO_CACHE=]
          --cache-dir [CACHE_DIR]
              Path to the cache directory [env: UV_CACHE_DIR=]
          --config-file <CONFIG_FILE>
              The path to a `uv.toml` file to use for configuration [env: UV_CONFIG_FILE=]
          --no-config
              Avoid discovering configuration files (`pyproject.toml`, `uv.toml`) in the current
              directory, parent directories, or user configuration directories [env: UV_NO_CONFIG=]
      -h, --help
              Print help
      -V, --version
              Print version

    Use `uv help <command>` for more information on a specific command.


    ----- stderr -----
    "###);
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
fn test_with_no_pager() {
    let context = TestContext::new_with_versions(&[]);

    // We can't really test whether the --no-pager option works with a snapshot test.
    // It's still nice to have a test for the option to confirm the option exists.
    uv_snapshot!(context.filters(), context.help().arg("--no-pager"), @r###"
    success: true
    exit_code: 0
    ----- stdout -----
    An extremely fast Python package manager.

    Usage: uv [OPTIONS] <COMMAND>

    Commands:
      pip      Resolve and install Python packages
      tool     Run and manage executable Python packages
      python   Manage Python installations
      venv     Create a virtual environment
      cache    Manage the cache
      version  Display uv's version
      help     Display documentation for a command

    Options:
      -q, --quiet
              Do not print any output
      -v, --verbose...
              Use verbose output
          --color <COLOR_CHOICE>
              Control colors in output [default: auto] [possible values: auto, always, never]
          --native-tls
              Whether to load TLS certificates from the platform's native certificate store [env:
              UV_NATIVE_TLS=]
          --offline
              Disable network access, relying only on locally cached data and locally available files
          --python-preference <PYTHON_PREFERENCE>
              Whether to prefer using Python installations that are already present on the system, or
              those that are downloaded and installed by uv [possible values: only-managed, installed,
              managed, system, only-system]
          --python-fetch <PYTHON_FETCH>
              Whether to automatically download Python when required [possible values: automatic,
              manual]
          --no-progress
              Hides all progress outputs when set
      -n, --no-cache
              Avoid reading from or writing to the cache, instead using a temporary directory for the
              duration of the operation [env: UV_NO_CACHE=]
          --cache-dir [CACHE_DIR]
              Path to the cache directory [env: UV_CACHE_DIR=]
          --config-file <CONFIG_FILE>
              The path to a `uv.toml` file to use for configuration [env: UV_CONFIG_FILE=]
          --no-config
              Avoid discovering configuration files (`pyproject.toml`, `uv.toml`) in the current
              directory, parent directories, or user configuration directories [env: UV_NO_CONFIG=]
      -h, --help
              Print help
      -V, --version
              Print version

    Use `uv help <command>` for more information on a specific command.


    ----- stderr -----
    "###);
}
