# CLI Reference

## uv

An extremely fast Python package manager.

<h3 class="cli-reference">Usage</h3>

```
uv [OPTIONS] <COMMAND>
```

<h3 class="cli-reference">Commands</h3>

<dl class="cli-reference"><dt><a href="#uv-run"><code>uv run</code></a></dt><dd><p>Run a command or script</p></dd>
<dt><a href="#uv-init"><code>uv init</code></a></dt><dd><p>Create a new project</p></dd>
<dt><a href="#uv-add"><code>uv add</code></a></dt><dd><p>Add dependencies to the project</p></dd>
<dt><a href="#uv-remove"><code>uv remove</code></a></dt><dd><p>Remove dependencies from the project</p></dd>
<dt><a href="#uv-version"><code>uv version</code></a></dt><dd><p>Read or update the project's version</p></dd>
<dt><a href="#uv-sync"><code>uv sync</code></a></dt><dd><p>Update the project's environment</p></dd>
<dt><a href="#uv-lock"><code>uv lock</code></a></dt><dd><p>Update the project's lockfile</p></dd>
<dt><a href="#uv-export"><code>uv export</code></a></dt><dd><p>Export the project's lockfile to an alternate format</p></dd>
<dt><a href="#uv-tree"><code>uv tree</code></a></dt><dd><p>Display the project's dependency tree</p></dd>
<dt><a href="#uv-tool"><code>uv tool</code></a></dt><dd><p>Run and install commands provided by Python packages</p></dd>
<dt><a href="#uv-python"><code>uv python</code></a></dt><dd><p>Manage Python versions and installations</p></dd>
<dt><a href="#uv-pip"><code>uv pip</code></a></dt><dd><p>Manage Python packages with a pip-compatible interface</p></dd>
<dt><a href="#uv-venv"><code>uv venv</code></a></dt><dd><p>Create a virtual environment</p></dd>
<dt><a href="#uv-build"><code>uv build</code></a></dt><dd><p>Build Python packages into source distributions and wheels</p></dd>
<dt><a href="#uv-publish"><code>uv publish</code></a></dt><dd><p>Upload distributions to an index</p></dd>
<dt><a href="#uv-cache"><code>uv cache</code></a></dt><dd><p>Manage uv's cache</p></dd>
<dt><a href="#uv-self"><code>uv self</code></a></dt><dd><p>Manage the uv executable</p></dd>
<dt><a href="#uv-help"><code>uv help</code></a></dt><dd><p>Display documentation for a command</p></dd>
</dl>

## uv run

Run a command or script.

Ensures that the command runs in a Python environment.

When used with a file ending in `.py` or an HTTP(S) URL, the file will be treated as a script and run with a Python interpreter, i.e., `uv run file.py` is equivalent to `uv run python file.py`. For URLs, the script is temporarily downloaded before execution. If the script contains inline dependency metadata, it will be installed into an isolated, ephemeral environment. When used with `-`, the input will be read from stdin, and treated as a Python script.

When used in a project, the project environment will be created and updated before invoking the command.

When used outside a project, if a virtual environment can be found in the current directory or a parent directory, the command will be run in that environment. Otherwise, the command will be run in the environment of the discovered interpreter.

Arguments following the command (or script) are not interpreted as arguments to uv. All options to uv must be provided before the command, e.g., `uv run --verbose foo`. A `--` can be used to separate the command from uv options for clarity, e.g., `uv run --python 3.12 -- python`.

<h3 class="cli-reference">Usage</h3>

```
uv run [OPTIONS] [COMMAND]
```

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt id="uv-run--active"><a href="#uv-run--active"><code>--active</code></a></dt><dd><p>Prefer the active virtual environment over the project's virtual environment.</p>
<p>If the project virtual environment is active or no virtual environment is active, this has no effect.</p>
</dd><dt id="uv-run--all-extras"><a href="#uv-run--all-extras"><code>--all-extras</code></a></dt><dd><p>Include all optional dependencies.</p>
<p>Optional dependencies are defined via <code>project.optional-dependencies</code> in a <code>pyproject.toml</code>.</p>
<p>This option is only available when running in a project.</p>
</dd><dt id="uv-run--all-groups"><a href="#uv-run--all-groups"><code>--all-groups</code></a></dt><dd><p>Include dependencies from all dependency groups.</p>
<p><code>--no-group</code> can be used to exclude specific groups.</p>
</dd><dt id="uv-run--all-packages"><a href="#uv-run--all-packages"><code>--all-packages</code></a></dt><dd><p>Run the command with all workspace members installed.</p>
<p>The workspace's environment (<code>.venv</code>) is updated to include all workspace members.</p>
<p>Any extras or groups specified via <code>--extra</code>, <code>--group</code>, or related options will be applied to all workspace members.</p>
</dd><dt id="uv-run--allow-insecure-host"><a href="#uv-run--allow-insecure-host"><code>--allow-insecure-host</code></a>, <code>--trusted-host</code> <i>allow-insecure-host</i></dt><dd><p>Allow insecure connections to a host.</p>
<p>Can be provided multiple times.</p>
<p>Expects to receive either a hostname (e.g., <code>localhost</code>), a host-port pair (e.g., <code>localhost:8080</code>), or a URL (e.g., <code>https://localhost</code>).</p>
<p>WARNING: Hosts included in this list will not be verified against the system's certificate store. Only use <code>--allow-insecure-host</code> in a secure network with verified sources, as it bypasses SSL verification and could expose you to MITM attacks.</p>
<p>May also be set with the <code>UV_INSECURE_HOST</code> environment variable.</p></dd><dt id="uv-run--cache-dir"><a href="#uv-run--cache-dir"><code>--cache-dir</code></a> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>
<p>Defaults to <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on macOS and Linux, and <code>%LOCALAPPDATA%\uv\cache</code> on Windows.</p>
<p>To view the location of the cache directory, run <code>uv cache dir</code>.</p>
<p>May also be set with the <code>UV_CACHE_DIR</code> environment variable.</p></dd><dt id="uv-run--color"><a href="#uv-run--color"><code>--color</code></a> <i>color-choice</i></dt><dd><p>Control the use of color in output.</p>
<p>By default, uv will automatically detect support for colors when writing to a terminal.</p>
<p>Possible values:</p>
<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>
<li><code>always</code>:  Enables colored output regardless of the detected environment</li>
<li><code>never</code>:  Disables colored output</li>
</ul></dd><dt id="uv-run--compile-bytecode"><a href="#uv-run--compile-bytecode"><code>--compile-bytecode</code></a>, <code>--compile</code></dt><dd><p>Compile Python files to bytecode after installation.</p>
<p>By default, uv does not compile Python (<code>.py</code>) files to bytecode (<code>__pycache__/*.pyc</code>); instead, compilation is performed lazily the first time a module is imported. For use-cases in which start time is critical, such as CLI applications and Docker containers, this option can be enabled to trade longer installation times for faster start times.</p>
<p>When enabled, uv will process the entire site-packages directory (including packages that are not being modified by the current operation) for consistency. Like pip, it will also ignore errors.</p>
<p>May also be set with the <code>UV_COMPILE_BYTECODE</code> environment variable.</p></dd><dt id="uv-run--config-file"><a href="#uv-run--config-file"><code>--config-file</code></a> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>
<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>
<p>May also be set with the <code>UV_CONFIG_FILE</code> environment variable.</p></dd><dt id="uv-run--config-setting"><a href="#uv-run--config-setting"><code>--config-setting</code></a>, <code>--config-settings</code>, <code>-C</code> <i>config-setting</i></dt><dd><p>Settings to pass to the PEP 517 build backend, specified as <code>KEY=VALUE</code> pairs</p>
</dd><dt id="uv-run--default-index"><a href="#uv-run--default-index"><code>--default-index</code></a> <i>default-index</i></dt><dd><p>The URL of the default package index (by default: <a href="https://pypi.org/simple">https://pypi.org/simple</a>).</p>
<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>
<p>The index given by this flag is given lower priority than all other indexes specified via the <code>--index</code> flag.</p>
<p>May also be set with the <code>UV_DEFAULT_INDEX</code> environment variable.</p></dd><dt id="uv-run--directory"><a href="#uv-run--directory"><code>--directory</code></a> <i>directory</i></dt><dd><p>Change to the given directory prior to running the command.</p>
<p>Relative paths are resolved with the given directory as the base.</p>
<p>See <code>--project</code> to only change the project root directory.</p>
</dd><dt id="uv-run--env-file"><a href="#uv-run--env-file"><code>--env-file</code></a> <i>env-file</i></dt><dd><p>Load environment variables from a <code>.env</code> file.</p>
<p>Can be provided multiple times, with subsequent files overriding values defined in previous files.</p>
<p>May also be set with the <code>UV_ENV_FILE</code> environment variable.</p></dd><dt id="uv-run--exact"><a href="#uv-run--exact"><code>--exact</code></a></dt><dd><p>Perform an exact sync, removing extraneous packages.</p>
<p>When enabled, uv will remove any extraneous packages from the environment. By default, <code>uv run</code> will make the minimum necessary changes to satisfy the requirements.</p>
</dd><dt id="uv-run--exclude-newer"><a href="#uv-run--exclude-newer"><code>--exclude-newer</code></a> <i>exclude-newer</i></dt><dd><p>Limit candidate packages to those that were uploaded prior to the given date.</p>
<p>Accepts both RFC 3339 timestamps (e.g., <code>2006-12-02T02:07:43Z</code>) and local dates in the same format (e.g., <code>2006-12-02</code>) in your system's configured time zone.</p>
<p>May also be set with the <code>UV_EXCLUDE_NEWER</code> environment variable.</p></dd><dt id="uv-run--extra"><a href="#uv-run--extra"><code>--extra</code></a> <i>extra</i></dt><dd><p>Include optional dependencies from the specified extra name.</p>
<p>May be provided more than once.</p>
<p>Optional dependencies are defined via <code>project.optional-dependencies</code> in a <code>pyproject.toml</code>.</p>
<p>This option is only available when running in a project.</p>
</dd><dt id="uv-run--extra-index-url"><a href="#uv-run--extra-index-url"><code>--extra-index-url</code></a> <i>extra-index-url</i></dt><dd><p>(Deprecated: use <code>--index</code> instead) Extra URLs of package indexes to use, in addition to <code>--index-url</code>.</p>
<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>
<p>All indexes provided via this flag take priority over the index specified by <code>--index-url</code> (which defaults to PyPI). When multiple <code>--extra-index-url</code> flags are provided, earlier values take priority.</p>
<p>May also be set with the <code>UV_EXTRA_INDEX_URL</code> environment variable.</p></dd><dt id="uv-run--find-links"><a href="#uv-run--find-links"><code>--find-links</code></a>, <code>-f</code> <i>find-links</i></dt><dd><p>Locations to search for candidate distributions, in addition to those found in the registry indexes.</p>
<p>If a path, the target must be a directory that contains packages as wheel files (<code>.whl</code>) or source distributions (e.g., <code>.tar.gz</code> or <code>.zip</code>) at the top level.</p>
<p>If a URL, the page must contain a flat list of links to package files adhering to the formats described above.</p>
<p>May also be set with the <code>UV_FIND_LINKS</code> environment variable.</p></dd><dt id="uv-run--fork-strategy"><a href="#uv-run--fork-strategy"><code>--fork-strategy</code></a> <i>fork-strategy</i></dt><dd><p>The strategy to use when selecting multiple versions of a given package across Python versions and platforms.</p>
<p>By default, uv will optimize for selecting the latest version of each package for each supported Python version (<code>requires-python</code>), while minimizing the number of selected versions across platforms.</p>
<p>Under <code>fewest</code>, uv will minimize the number of selected versions for each package, preferring older versions that are compatible with a wider range of supported Python versions or platforms.</p>
<p>May also be set with the <code>UV_FORK_STRATEGY</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>fewest</code>:  Optimize for selecting the fewest number of versions for each package. Older versions may be preferred if they are compatible with a wider range of supported Python versions or platforms</li>
<li><code>requires-python</code>:  Optimize for selecting latest supported version of each package, for each supported Python version</li>
</ul></dd><dt id="uv-run--frozen"><a href="#uv-run--frozen"><code>--frozen</code></a></dt><dd><p>Run without updating the <code>uv.lock</code> file.</p>
<p>Instead of checking if the lockfile is up-to-date, uses the versions in the lockfile as the source of truth. If the lockfile is missing, uv will exit with an error. If the <code>pyproject.toml</code> includes changes to dependencies that have not been included in the lockfile yet, they will not be present in the environment.</p>
<p>May also be set with the <code>UV_FROZEN</code> environment variable.</p></dd><dt id="uv-run--group"><a href="#uv-run--group"><code>--group</code></a> <i>group</i></dt><dd><p>Include dependencies from the specified dependency group.</p>
<p>May be provided multiple times.</p>
</dd><dt id="uv-run--gui-script"><a href="#uv-run--gui-script"><code>--gui-script</code></a></dt><dd><p>Run the given path as a Python GUI script.</p>
<p>Using <code>--gui-script</code> will attempt to parse the path as a PEP 723 script and run it with <code>pythonw.exe</code>, irrespective of its extension. Only available on Windows.</p>
</dd><dt id="uv-run--help"><a href="#uv-run--help"><code>--help</code></a>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>
</dd><dt id="uv-run--index"><a href="#uv-run--index"><code>--index</code></a> <i>index</i></dt><dd><p>The URLs to use when resolving dependencies, in addition to the default index.</p>
<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>
<p>All indexes provided via this flag take priority over the index specified by <code>--default-index</code> (which defaults to PyPI). When multiple <code>--index</code> flags are provided, earlier values take priority.</p>
<p>May also be set with the <code>UV_INDEX</code> environment variable.</p></dd><dt id="uv-run--index-strategy"><a href="#uv-run--index-strategy"><code>--index-strategy</code></a> <i>index-strategy</i></dt><dd><p>The strategy to use when resolving against multiple index URLs.</p>
<p>By default, uv will stop at the first index on which a given package is available, and limit resolutions to those present on that first index (<code>first-index</code>). This prevents &quot;dependency confusion&quot; attacks, whereby an attacker can upload a malicious package under the same name to an alternate index.</p>
<p>May also be set with the <code>UV_INDEX_STRATEGY</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>first-index</code>:  Only use results from the first index that returns a match for a given package name</li>
<li><code>unsafe-first-match</code>:  Search for every package name across all indexes, exhausting the versions from the first index before moving on to the next</li>
<li><code>unsafe-best-match</code>:  Search for every package name across all indexes, preferring the &quot;best&quot; version found. If a package version is in multiple indexes, only look at the entry for the first index</li>
</ul></dd><dt id="uv-run--index-url"><a href="#uv-run--index-url"><code>--index-url</code></a>, <code>-i</code> <i>index-url</i></dt><dd><p>(Deprecated: use <code>--default-index</code> instead) The URL of the Python package index (by default: <a href="https://pypi.org/simple">https://pypi.org/simple</a>).</p>
<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>
<p>The index given by this flag is given lower priority than all other indexes specified via the <code>--extra-index-url</code> flag.</p>
<p>May also be set with the <code>UV_INDEX_URL</code> environment variable.</p></dd><dt id="uv-run--isolated"><a href="#uv-run--isolated"><code>--isolated</code></a></dt><dd><p>Run the command in an isolated virtual environment.</p>
<p>Usually, the project environment is reused for performance. This option forces a fresh environment to be used for the project, enforcing strict isolation between dependencies and declaration of requirements.</p>
<p>An editable installation is still used for the project.</p>
<p>When used with <code>--with</code> or <code>--with-requirements</code>, the additional dependencies will still be layered in a second environment.</p>
</dd><dt id="uv-run--keyring-provider"><a href="#uv-run--keyring-provider"><code>--keyring-provider</code></a> <i>keyring-provider</i></dt><dd><p>Attempt to use <code>keyring</code> for authentication for index URLs.</p>
<p>At present, only <code>--keyring-provider subprocess</code> is supported, which configures uv to use the <code>keyring</code> CLI to handle authentication.</p>
<p>Defaults to <code>disabled</code>.</p>
<p>May also be set with the <code>UV_KEYRING_PROVIDER</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>disabled</code>:  Do not use keyring for credential lookup</li>
<li><code>subprocess</code>:  Use the <code>keyring</code> command for credential lookup</li>
</ul></dd><dt id="uv-run--link-mode"><a href="#uv-run--link-mode"><code>--link-mode</code></a> <i>link-mode</i></dt><dd><p>The method to use when installing packages from the global cache.</p>
<p>Defaults to <code>clone</code> (also known as Copy-on-Write) on macOS, and <code>hardlink</code> on Linux and Windows.</p>
<p>May also be set with the <code>UV_LINK_MODE</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>clone</code>:  Clone (i.e., copy-on-write) packages from the wheel into the <code>site-packages</code> directory</li>
<li><code>copy</code>:  Copy packages from the wheel into the <code>site-packages</code> directory</li>
<li><code>hardlink</code>:  Hard link packages from the wheel into the <code>site-packages</code> directory</li>
<li><code>symlink</code>:  Symbolically link packages from the wheel into the <code>site-packages</code> directory</li>
</ul></dd><dt id="uv-run--locked"><a href="#uv-run--locked"><code>--locked</code></a></dt><dd><p>Assert that the <code>uv.lock</code> will remain unchanged.</p>
<p>Requires that the lockfile is up-to-date. If the lockfile is missing or needs to be updated, uv will exit with an error.</p>
<p>May also be set with the <code>UV_LOCKED</code> environment variable.</p></dd><dt id="uv-run--managed-python"><a href="#uv-run--managed-python"><code>--managed-python</code></a></dt><dd><p>Require use of uv-managed Python versions.</p>
<p>By default, uv prefers using Python versions it manages. However, it will use system Python versions if a uv-managed Python is not installed. This option disables use of system Python versions.</p>
<p>May also be set with the <code>UV_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-run--module"><a href="#uv-run--module"><code>--module</code></a>, <code>-m</code></dt><dd><p>Run a Python module.</p>
<p>Equivalent to <code>python -m &lt;module&gt;</code>.</p>
</dd><dt id="uv-run--native-tls"><a href="#uv-run--native-tls"><code>--native-tls</code></a></dt><dd><p>Whether to load TLS certificates from the platform's native certificate store.</p>
<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>
<p>However, in some cases, you may want to use the platform's native certificate store, especially if you're relying on a corporate trust root (e.g., for a mandatory proxy) that's included in your system's certificate store.</p>
<p>May also be set with the <code>UV_NATIVE_TLS</code> environment variable.</p></dd><dt id="uv-run--no-binary"><a href="#uv-run--no-binary"><code>--no-binary</code></a></dt><dd><p>Don't install pre-built wheels.</p>
<p>The given packages will be built and installed from source. The resolver will still use pre-built wheels to extract package metadata, if available.</p>
<p>May also be set with the <code>UV_NO_BINARY</code> environment variable.</p></dd><dt id="uv-run--no-binary-package"><a href="#uv-run--no-binary-package"><code>--no-binary-package</code></a> <i>no-binary-package</i></dt><dd><p>Don't install pre-built wheels for a specific package</p>
<p>May also be set with the <code>UV_NO_BINARY_PACKAGE</code> environment variable.</p></dd><dt id="uv-run--no-build"><a href="#uv-run--no-build"><code>--no-build</code></a></dt><dd><p>Don't build source distributions.</p>
<p>When enabled, resolving will not run arbitrary Python code. The cached wheels of already-built source distributions will be reused, but operations that require building distributions will exit with an error.</p>
<p>May also be set with the <code>UV_NO_BUILD</code> environment variable.</p></dd><dt id="uv-run--no-build-isolation"><a href="#uv-run--no-build-isolation"><code>--no-build-isolation</code></a></dt><dd><p>Disable isolation when building source distributions.</p>
<p>Assumes that build dependencies specified by PEP 518 are already installed.</p>
<p>May also be set with the <code>UV_NO_BUILD_ISOLATION</code> environment variable.</p></dd><dt id="uv-run--no-build-isolation-package"><a href="#uv-run--no-build-isolation-package"><code>--no-build-isolation-package</code></a> <i>no-build-isolation-package</i></dt><dd><p>Disable isolation when building source distributions for a specific package.</p>
<p>Assumes that the packages' build dependencies specified by PEP 518 are already installed.</p>
</dd><dt id="uv-run--no-build-package"><a href="#uv-run--no-build-package"><code>--no-build-package</code></a> <i>no-build-package</i></dt><dd><p>Don't build source distributions for a specific package</p>
<p>May also be set with the <code>UV_NO_BUILD_PACKAGE</code> environment variable.</p></dd><dt id="uv-run--no-cache"><a href="#uv-run--no-cache"><code>--no-cache</code></a>, <code>--no-cache-dir</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>
<p>May also be set with the <code>UV_NO_CACHE</code> environment variable.</p></dd><dt id="uv-run--no-config"><a href="#uv-run--no-config"><code>--no-config</code></a></dt><dd><p>Avoid discovering configuration files (<code>pyproject.toml</code>, <code>uv.toml</code>).</p>
<p>Normally, configuration files are discovered in the current directory, parent directories, or user configuration directories.</p>
<p>May also be set with the <code>UV_NO_CONFIG</code> environment variable.</p></dd><dt id="uv-run--no-default-groups"><a href="#uv-run--no-default-groups"><code>--no-default-groups</code></a></dt><dd><p>Ignore the default dependency groups.</p>
<p>uv includes the groups defined in <code>tool.uv.default-groups</code> by default. This disables that option, however, specific groups can still be included with <code>--group</code>.</p>
</dd><dt id="uv-run--no-dev"><a href="#uv-run--no-dev"><code>--no-dev</code></a></dt><dd><p>Disable the development dependency group.</p>
<p>This option is an alias of <code>--no-group dev</code>. See <code>--no-default-groups</code> to disable all default groups instead.</p>
<p>This option is only available when running in a project.</p>
</dd><dt id="uv-run--no-editable"><a href="#uv-run--no-editable"><code>--no-editable</code></a></dt><dd><p>Install any editable dependencies, including the project and any workspace members, as non-editable</p>
<p>May also be set with the <code>UV_NO_EDITABLE</code> environment variable.</p></dd><dt id="uv-run--no-env-file"><a href="#uv-run--no-env-file"><code>--no-env-file</code></a></dt><dd><p>Avoid reading environment variables from a <code>.env</code> file</p>
<p>May also be set with the <code>UV_NO_ENV_FILE</code> environment variable.</p></dd><dt id="uv-run--no-extra"><a href="#uv-run--no-extra"><code>--no-extra</code></a> <i>no-extra</i></dt><dd><p>Exclude the specified optional dependencies, if <code>--all-extras</code> is supplied.</p>
<p>May be provided multiple times.</p>
</dd><dt id="uv-run--no-group"><a href="#uv-run--no-group"><code>--no-group</code></a> <i>no-group</i></dt><dd><p>Disable the specified dependency group.</p>
<p>This option always takes precedence over default groups, <code>--all-groups</code>, and <code>--group</code>.</p>
<p>May be provided multiple times.</p>
</dd><dt id="uv-run--no-index"><a href="#uv-run--no-index"><code>--no-index</code></a></dt><dd><p>Ignore the registry index (e.g., PyPI), instead relying on direct URL dependencies and those provided via <code>--find-links</code></p>
</dd><dt id="uv-run--no-managed-python"><a href="#uv-run--no-managed-python"><code>--no-managed-python</code></a></dt><dd><p>Disable use of uv-managed Python versions.</p>
<p>Instead, uv will search for a suitable Python version on the system.</p>
<p>May also be set with the <code>UV_NO_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-run--no-progress"><a href="#uv-run--no-progress"><code>--no-progress</code></a></dt><dd><p>Hide all progress outputs.</p>
<p>For example, spinners or progress bars.</p>
<p>May also be set with the <code>UV_NO_PROGRESS</code> environment variable.</p></dd><dt id="uv-run--no-project"><a href="#uv-run--no-project"><code>--no-project</code></a>, <code>--no_workspace</code></dt><dd><p>Avoid discovering the project or workspace.</p>
<p>Instead of searching for projects in the current directory and parent directories, run in an isolated, ephemeral environment populated by the <code>--with</code> requirements.</p>
<p>If a virtual environment is active or found in a current or parent directory, it will be used as if there was no project or workspace.</p>
</dd><dt id="uv-run--no-python-downloads"><a href="#uv-run--no-python-downloads"><code>--no-python-downloads</code></a></dt><dd><p>Disable automatic downloads of Python.</p>
</dd><dt id="uv-run--no-sources"><a href="#uv-run--no-sources"><code>--no-sources</code></a></dt><dd><p>Ignore the <code>tool.uv.sources</code> table when resolving dependencies. Used to lock against the standards-compliant, publishable package metadata, as opposed to using any workspace, Git, URL, or local path sources</p>
</dd><dt id="uv-run--no-sync"><a href="#uv-run--no-sync"><code>--no-sync</code></a></dt><dd><p>Avoid syncing the virtual environment.</p>
<p>Implies <code>--frozen</code>, as the project dependencies will be ignored (i.e., the lockfile will not be updated, since the environment will not be synced regardless).</p>
<p>May also be set with the <code>UV_NO_SYNC</code> environment variable.</p></dd><dt id="uv-run--offline"><a href="#uv-run--offline"><code>--offline</code></a></dt><dd><p>Disable network access.</p>
<p>When disabled, uv will only use locally cached data and locally available files.</p>
<p>May also be set with the <code>UV_OFFLINE</code> environment variable.</p></dd><dt id="uv-run--only-dev"><a href="#uv-run--only-dev"><code>--only-dev</code></a></dt><dd><p>Only include the development dependency group.</p>
<p>The project and its dependencies will be omitted.</p>
<p>This option is an alias for <code>--only-group dev</code>. Implies <code>--no-default-groups</code>.</p>
</dd><dt id="uv-run--only-group"><a href="#uv-run--only-group"><code>--only-group</code></a> <i>only-group</i></dt><dd><p>Only include dependencies from the specified dependency group.</p>
<p>The project and its dependencies will be omitted.</p>
<p>May be provided multiple times. Implies <code>--no-default-groups</code>.</p>
</dd><dt id="uv-run--package"><a href="#uv-run--package"><code>--package</code></a> <i>package</i></dt><dd><p>Run the command in a specific package in the workspace.</p>
<p>If the workspace member does not exist, uv will exit with an error.</p>
</dd><dt id="uv-run--prerelease"><a href="#uv-run--prerelease"><code>--prerelease</code></a> <i>prerelease</i></dt><dd><p>The strategy to use when considering pre-release versions.</p>
<p>By default, uv will accept pre-releases for packages that <em>only</em> publish pre-releases, along with first-party requirements that contain an explicit pre-release marker in the declared specifiers (<code>if-necessary-or-explicit</code>).</p>
<p>May also be set with the <code>UV_PRERELEASE</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>disallow</code>:  Disallow all pre-release versions</li>
<li><code>allow</code>:  Allow all pre-release versions</li>
<li><code>if-necessary</code>:  Allow pre-release versions if all versions of a package are pre-release</li>
<li><code>explicit</code>:  Allow pre-release versions for first-party packages with explicit pre-release markers in their version requirements</li>
<li><code>if-necessary-or-explicit</code>:  Allow pre-release versions if all versions of a package are pre-release, or if the package has an explicit pre-release marker in its version requirements</li>
</ul></dd><dt id="uv-run--project"><a href="#uv-run--project"><code>--project</code></a> <i>project</i></dt><dd><p>Run the command within the given project directory.</p>
<p>All <code>pyproject.toml</code>, <code>uv.toml</code>, and <code>.python-version</code> files will be discovered by walking up the directory tree from the project root, as will the project's virtual environment (<code>.venv</code>).</p>
<p>Other command-line arguments (such as relative paths) will be resolved relative to the current working directory.</p>
<p>See <code>--directory</code> to change the working directory entirely.</p>
<p>This setting has no effect when used in the <code>uv pip</code> interface.</p>
<p>May also be set with the <code>UV_PROJECT</code> environment variable.</p></dd><dt id="uv-run--python"><a href="#uv-run--python"><code>--python</code></a>, <code>-p</code> <i>python</i></dt><dd><p>The Python interpreter to use for the run environment.</p>
<p>If the interpreter request is satisfied by a discovered environment, the environment will be
used.</p>
<p>See <a href="#uv-python">uv python</a> to view supported request formats.</p>
<p>May also be set with the <code>UV_PYTHON</code> environment variable.</p></dd><dt id="uv-run--quiet"><a href="#uv-run--quiet"><code>--quiet</code></a>, <code>-q</code></dt><dd><p>Use quiet output.</p>
<p>Repeating this option, e.g., <code>-qq</code>, will enable a silent mode in which uv will write no output to stdout.</p>
</dd><dt id="uv-run--refresh"><a href="#uv-run--refresh"><code>--refresh</code></a></dt><dd><p>Refresh all cached data</p>
</dd><dt id="uv-run--refresh-package"><a href="#uv-run--refresh-package"><code>--refresh-package</code></a> <i>refresh-package</i></dt><dd><p>Refresh cached data for a specific package</p>
</dd><dt id="uv-run--reinstall"><a href="#uv-run--reinstall"><code>--reinstall</code></a>, <code>--force-reinstall</code></dt><dd><p>Reinstall all packages, regardless of whether they're already installed. Implies <code>--refresh</code></p>
</dd><dt id="uv-run--reinstall-package"><a href="#uv-run--reinstall-package"><code>--reinstall-package</code></a> <i>reinstall-package</i></dt><dd><p>Reinstall a specific package, regardless of whether it's already installed. Implies <code>--refresh-package</code></p>
</dd><dt id="uv-run--resolution"><a href="#uv-run--resolution"><code>--resolution</code></a> <i>resolution</i></dt><dd><p>The strategy to use when selecting between the different compatible versions for a given package requirement.</p>
<p>By default, uv will use the latest compatible version of each package (<code>highest</code>).</p>
<p>May also be set with the <code>UV_RESOLUTION</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>highest</code>:  Resolve the highest compatible version of each package</li>
<li><code>lowest</code>:  Resolve the lowest compatible version of each package</li>
<li><code>lowest-direct</code>:  Resolve the lowest compatible version of any direct dependencies, and the highest compatible version of any transitive dependencies</li>
</ul></dd><dt id="uv-run--script"><a href="#uv-run--script"><code>--script</code></a>, <code>-s</code></dt><dd><p>Run the given path as a Python script.</p>
<p>Using <code>--script</code> will attempt to parse the path as a PEP 723 script, irrespective of its extension.</p>
</dd><dt id="uv-run--upgrade"><a href="#uv-run--upgrade"><code>--upgrade</code></a>, <code>-U</code></dt><dd><p>Allow package upgrades, ignoring pinned versions in any existing output file. Implies <code>--refresh</code></p>
</dd><dt id="uv-run--upgrade-package"><a href="#uv-run--upgrade-package"><code>--upgrade-package</code></a>, <code>-P</code> <i>upgrade-package</i></dt><dd><p>Allow upgrades for a specific package, ignoring pinned versions in any existing output file. Implies <code>--refresh-package</code></p>
</dd><dt id="uv-run--verbose"><a href="#uv-run--verbose"><code>--verbose</code></a>, <code>-v</code></dt><dd><p>Use verbose output.</p>
<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (<a href="https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives">https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives</a>)</p>
</dd><dt id="uv-run--with"><a href="#uv-run--with"><code>--with</code></a> <i>with</i></dt><dd><p>Run with the given packages installed.</p>
<p>When used in a project, these dependencies will be layered on top of the project environment in a separate, ephemeral environment. These dependencies are allowed to conflict with those specified by the project.</p>
</dd><dt id="uv-run--with-editable"><a href="#uv-run--with-editable"><code>--with-editable</code></a> <i>with-editable</i></dt><dd><p>Run with the given packages installed in editable mode.</p>
<p>When used in a project, these dependencies will be layered on top of the project environment in a separate, ephemeral environment. These dependencies are allowed to conflict with those specified by the project.</p>
</dd><dt id="uv-run--with-requirements"><a href="#uv-run--with-requirements"><code>--with-requirements</code></a> <i>with-requirements</i></dt><dd><p>Run with all packages listed in the given <code>requirements.txt</code> files.</p>
<p>The same environment semantics as <code>--with</code> apply.</p>
<p>Using <code>pyproject.toml</code>, <code>setup.py</code>, or <code>setup.cfg</code> files is not allowed.</p>
</dd></dl>

## uv init

Create a new project.

Follows the `pyproject.toml` specification.

If a `pyproject.toml` already exists at the target, uv will exit with an error.

If a `pyproject.toml` is found in any of the parent directories of the target path, the project will be added as a workspace member of the parent.

Some project state is not created until needed, e.g., the project virtual environment (`.venv`) and lockfile (`uv.lock`) are lazily created during the first sync.

<h3 class="cli-reference">Usage</h3>

```
uv init [OPTIONS] [PATH]
```

<h3 class="cli-reference">Arguments</h3>

<dl class="cli-reference"><dt id="uv-init--path"><a href="#uv-init--path"<code>PATH</code></a></dt><dd><p>The path to use for the project/script.</p>
<p>Defaults to the current working directory when initializing an app or library; required when initializing a script. Accepts relative and absolute paths.</p>
<p>If a <code>pyproject.toml</code> is found in any of the parent directories of the target path, the project will be added as a workspace member of the parent, unless <code>--no-workspace</code> is provided.</p>
</dd></dl>

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt id="uv-init--allow-insecure-host"><a href="#uv-init--allow-insecure-host"><code>--allow-insecure-host</code></a>, <code>--trusted-host</code> <i>allow-insecure-host</i></dt><dd><p>Allow insecure connections to a host.</p>
<p>Can be provided multiple times.</p>
<p>Expects to receive either a hostname (e.g., <code>localhost</code>), a host-port pair (e.g., <code>localhost:8080</code>), or a URL (e.g., <code>https://localhost</code>).</p>
<p>WARNING: Hosts included in this list will not be verified against the system's certificate store. Only use <code>--allow-insecure-host</code> in a secure network with verified sources, as it bypasses SSL verification and could expose you to MITM attacks.</p>
<p>May also be set with the <code>UV_INSECURE_HOST</code> environment variable.</p></dd><dt id="uv-init--app"><a href="#uv-init--app"><code>--app</code></a>, <code>--application</code></dt><dd><p>Create a project for an application.</p>
<p>This is the default behavior if <code>--lib</code> is not requested.</p>
<p>This project kind is for web servers, scripts, and command-line interfaces.</p>
<p>By default, an application is not intended to be built and distributed as a Python package. The <code>--package</code> option can be used to create an application that is distributable, e.g., if you want to distribute a command-line interface via PyPI.</p>
</dd><dt id="uv-init--author-from"><a href="#uv-init--author-from"><code>--author-from</code></a> <i>author-from</i></dt><dd><p>Fill in the <code>authors</code> field in the <code>pyproject.toml</code>.</p>
<p>By default, uv will attempt to infer the author information from some sources (e.g., Git) (<code>auto</code>). Use <code>--author-from git</code> to only infer from Git configuration. Use <code>--author-from none</code> to avoid inferring the author information.</p>
<p>Possible values:</p>
<ul>
<li><code>auto</code>:  Fetch the author information from some sources (e.g., Git) automatically</li>
<li><code>git</code>:  Fetch the author information from Git configuration only</li>
<li><code>none</code>:  Do not infer the author information</li>
</ul></dd><dt id="uv-init--bare"><a href="#uv-init--bare"><code>--bare</code></a></dt><dd><p>Only create a <code>pyproject.toml</code>.</p>
<p>Disables creating extra files like <code>README.md</code>, the <code>src/</code> tree, <code>.python-version</code> files, etc.</p>
</dd><dt id="uv-init--build-backend"><a href="#uv-init--build-backend"><code>--build-backend</code></a> <i>build-backend</i></dt><dd><p>Initialize a build-backend of choice for the project.</p>
<p>Implicitly sets <code>--package</code>.</p>
<p>Possible values:</p>
<ul>
<li><code>hatch</code>:  Use <a href="https://pypi.org/project/hatchling">hatchling</a> as the project build backend</li>
<li><code>flit</code>:  Use <a href="https://pypi.org/project/flit-core">flit-core</a> as the project build backend</li>
<li><code>pdm</code>:  Use <a href="https://pypi.org/project/pdm-backend">pdm-backend</a> as the project build backend</li>
<li><code>poetry</code>:  Use <a href="https://pypi.org/project/poetry-core">poetry-core</a> as the project build backend</li>
<li><code>setuptools</code>:  Use <a href="https://pypi.org/project/setuptools">setuptools</a> as the project build backend</li>
<li><code>maturin</code>:  Use <a href="https://pypi.org/project/maturin">maturin</a> as the project build backend</li>
<li><code>scikit</code>:  Use <a href="https://pypi.org/project/scikit-build-core">scikit-build-core</a> as the project build backend</li>
</ul></dd><dt id="uv-init--cache-dir"><a href="#uv-init--cache-dir"><code>--cache-dir</code></a> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>
<p>Defaults to <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on macOS and Linux, and <code>%LOCALAPPDATA%\uv\cache</code> on Windows.</p>
<p>To view the location of the cache directory, run <code>uv cache dir</code>.</p>
<p>May also be set with the <code>UV_CACHE_DIR</code> environment variable.</p></dd><dt id="uv-init--color"><a href="#uv-init--color"><code>--color</code></a> <i>color-choice</i></dt><dd><p>Control the use of color in output.</p>
<p>By default, uv will automatically detect support for colors when writing to a terminal.</p>
<p>Possible values:</p>
<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>
<li><code>always</code>:  Enables colored output regardless of the detected environment</li>
<li><code>never</code>:  Disables colored output</li>
</ul></dd><dt id="uv-init--config-file"><a href="#uv-init--config-file"><code>--config-file</code></a> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>
<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>
<p>May also be set with the <code>UV_CONFIG_FILE</code> environment variable.</p></dd><dt id="uv-init--description"><a href="#uv-init--description"><code>--description</code></a> <i>description</i></dt><dd><p>Set the project description</p>
</dd><dt id="uv-init--directory"><a href="#uv-init--directory"><code>--directory</code></a> <i>directory</i></dt><dd><p>Change to the given directory prior to running the command.</p>
<p>Relative paths are resolved with the given directory as the base.</p>
<p>See <code>--project</code> to only change the project root directory.</p>
</dd><dt id="uv-init--help"><a href="#uv-init--help"><code>--help</code></a>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>
</dd><dt id="uv-init--lib"><a href="#uv-init--lib"><code>--lib</code></a>, <code>--library</code></dt><dd><p>Create a project for a library.</p>
<p>A library is a project that is intended to be built and distributed as a Python package.</p>
</dd><dt id="uv-init--managed-python"><a href="#uv-init--managed-python"><code>--managed-python</code></a></dt><dd><p>Require use of uv-managed Python versions.</p>
<p>By default, uv prefers using Python versions it manages. However, it will use system Python versions if a uv-managed Python is not installed. This option disables use of system Python versions.</p>
<p>May also be set with the <code>UV_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-init--name"><a href="#uv-init--name"><code>--name</code></a> <i>name</i></dt><dd><p>The name of the project.</p>
<p>Defaults to the name of the directory.</p>
</dd><dt id="uv-init--native-tls"><a href="#uv-init--native-tls"><code>--native-tls</code></a></dt><dd><p>Whether to load TLS certificates from the platform's native certificate store.</p>
<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>
<p>However, in some cases, you may want to use the platform's native certificate store, especially if you're relying on a corporate trust root (e.g., for a mandatory proxy) that's included in your system's certificate store.</p>
<p>May also be set with the <code>UV_NATIVE_TLS</code> environment variable.</p></dd><dt id="uv-init--no-cache"><a href="#uv-init--no-cache"><code>--no-cache</code></a>, <code>--no-cache-dir</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>
<p>May also be set with the <code>UV_NO_CACHE</code> environment variable.</p></dd><dt id="uv-init--no-config"><a href="#uv-init--no-config"><code>--no-config</code></a></dt><dd><p>Avoid discovering configuration files (<code>pyproject.toml</code>, <code>uv.toml</code>).</p>
<p>Normally, configuration files are discovered in the current directory, parent directories, or user configuration directories.</p>
<p>May also be set with the <code>UV_NO_CONFIG</code> environment variable.</p></dd><dt id="uv-init--no-description"><a href="#uv-init--no-description"><code>--no-description</code></a></dt><dd><p>Disable the description for the project</p>
</dd><dt id="uv-init--no-managed-python"><a href="#uv-init--no-managed-python"><code>--no-managed-python</code></a></dt><dd><p>Disable use of uv-managed Python versions.</p>
<p>Instead, uv will search for a suitable Python version on the system.</p>
<p>May also be set with the <code>UV_NO_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-init--no-package"><a href="#uv-init--no-package"><code>--no-package</code></a></dt><dd><p>Do not set up the project to be built as a Python package.</p>
<p>Does not include a <code>[build-system]</code> for the project.</p>
<p>This is the default behavior when using <code>--app</code>.</p>
</dd><dt id="uv-init--no-pin-python"><a href="#uv-init--no-pin-python"><code>--no-pin-python</code></a></dt><dd><p>Do not create a <code>.python-version</code> file for the project.</p>
<p>By default, uv will create a <code>.python-version</code> file containing the minor version of the discovered Python interpreter, which will cause subsequent uv commands to use that version.</p>
</dd><dt id="uv-init--no-progress"><a href="#uv-init--no-progress"><code>--no-progress</code></a></dt><dd><p>Hide all progress outputs.</p>
<p>For example, spinners or progress bars.</p>
<p>May also be set with the <code>UV_NO_PROGRESS</code> environment variable.</p></dd><dt id="uv-init--no-python-downloads"><a href="#uv-init--no-python-downloads"><code>--no-python-downloads</code></a></dt><dd><p>Disable automatic downloads of Python.</p>
</dd><dt id="uv-init--no-readme"><a href="#uv-init--no-readme"><code>--no-readme</code></a></dt><dd><p>Do not create a <code>README.md</code> file</p>
</dd><dt id="uv-init--no-workspace"><a href="#uv-init--no-workspace"><code>--no-workspace</code></a>, <code>--no-project</code></dt><dd><p>Avoid discovering a workspace and create a standalone project.</p>
<p>By default, uv searches for workspaces in the current directory or any parent directory.</p>
</dd><dt id="uv-init--offline"><a href="#uv-init--offline"><code>--offline</code></a></dt><dd><p>Disable network access.</p>
<p>When disabled, uv will only use locally cached data and locally available files.</p>
<p>May also be set with the <code>UV_OFFLINE</code> environment variable.</p></dd><dt id="uv-init--package"><a href="#uv-init--package"><code>--package</code></a></dt><dd><p>Set up the project to be built as a Python package.</p>
<p>Defines a <code>[build-system]</code> for the project.</p>
<p>This is the default behavior when using <code>--lib</code> or <code>--build-backend</code>.</p>
<p>When using <code>--app</code>, this will include a <code>[project.scripts]</code> entrypoint and use a <code>src/</code> project structure.</p>
</dd><dt id="uv-init--project"><a href="#uv-init--project"><code>--project</code></a> <i>project</i></dt><dd><p>Run the command within the given project directory.</p>
<p>All <code>pyproject.toml</code>, <code>uv.toml</code>, and <code>.python-version</code> files will be discovered by walking up the directory tree from the project root, as will the project's virtual environment (<code>.venv</code>).</p>
<p>Other command-line arguments (such as relative paths) will be resolved relative to the current working directory.</p>
<p>See <code>--directory</code> to change the working directory entirely.</p>
<p>This setting has no effect when used in the <code>uv pip</code> interface.</p>
<p>May also be set with the <code>UV_PROJECT</code> environment variable.</p></dd><dt id="uv-init--python"><a href="#uv-init--python"><code>--python</code></a>, <code>-p</code> <i>python</i></dt><dd><p>The Python interpreter to use to determine the minimum supported Python version.</p>
<p>See <a href="#uv-python">uv python</a> to view supported request formats.</p>
<p>May also be set with the <code>UV_PYTHON</code> environment variable.</p></dd><dt id="uv-init--quiet"><a href="#uv-init--quiet"><code>--quiet</code></a>, <code>-q</code></dt><dd><p>Use quiet output.</p>
<p>Repeating this option, e.g., <code>-qq</code>, will enable a silent mode in which uv will write no output to stdout.</p>
</dd><dt id="uv-init--script"><a href="#uv-init--script"><code>--script</code></a></dt><dd><p>Create a script.</p>
<p>A script is a standalone file with embedded metadata enumerating its dependencies, along with any Python version requirements, as defined in the PEP 723 specification.</p>
<p>PEP 723 scripts can be executed directly with <code>uv run</code>.</p>
<p>By default, adds a requirement on the system Python version; use <code>--python</code> to specify an alternative Python version requirement.</p>
</dd><dt id="uv-init--vcs"><a href="#uv-init--vcs"><code>--vcs</code></a> <i>vcs</i></dt><dd><p>Initialize a version control system for the project.</p>
<p>By default, uv will initialize a Git repository (<code>git</code>). Use <code>--vcs none</code> to explicitly avoid initializing a version control system.</p>
<p>Possible values:</p>
<ul>
<li><code>git</code>:  Use Git for version control</li>
<li><code>none</code>:  Do not use any version control system</li>
</ul></dd><dt id="uv-init--verbose"><a href="#uv-init--verbose"><code>--verbose</code></a>, <code>-v</code></dt><dd><p>Use verbose output.</p>
<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (<a href="https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives">https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives</a>)</p>
</dd></dl>

## uv add

Add dependencies to the project.

Dependencies are added to the project's `pyproject.toml` file.

If a given dependency exists already, it will be updated to the new version specifier unless it includes markers that differ from the existing specifier in which case another entry for the dependency will be added.

The lockfile and project environment will be updated to reflect the added dependencies. To skip updating the lockfile, use `--frozen`. To skip updating the environment, use `--no-sync`.

If any of the requested dependencies cannot be found, uv will exit with an error, unless the `--frozen` flag is provided, in which case uv will add the dependencies verbatim without checking that they exist or are compatible with the project.

uv will search for a project in the current directory or any parent directory. If a project cannot be found, uv will exit with an error.

<h3 class="cli-reference">Usage</h3>

```
uv add [OPTIONS] <PACKAGES|--requirements <REQUIREMENTS>>
```

<h3 class="cli-reference">Arguments</h3>

<dl class="cli-reference"><dt id="uv-add--packages"><a href="#uv-add--packages"<code>PACKAGES</code></a></dt><dd><p>The packages to add, as PEP 508 requirements (e.g., <code>ruff==0.5.0</code>)</p>
</dd></dl>

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt id="uv-add--active"><a href="#uv-add--active"><code>--active</code></a></dt><dd><p>Prefer the active virtual environment over the project's virtual environment.</p>
<p>If the project virtual environment is active or no virtual environment is active, this has no effect.</p>
</dd><dt id="uv-add--allow-insecure-host"><a href="#uv-add--allow-insecure-host"><code>--allow-insecure-host</code></a>, <code>--trusted-host</code> <i>allow-insecure-host</i></dt><dd><p>Allow insecure connections to a host.</p>
<p>Can be provided multiple times.</p>
<p>Expects to receive either a hostname (e.g., <code>localhost</code>), a host-port pair (e.g., <code>localhost:8080</code>), or a URL (e.g., <code>https://localhost</code>).</p>
<p>WARNING: Hosts included in this list will not be verified against the system's certificate store. Only use <code>--allow-insecure-host</code> in a secure network with verified sources, as it bypasses SSL verification and could expose you to MITM attacks.</p>
<p>May also be set with the <code>UV_INSECURE_HOST</code> environment variable.</p></dd><dt id="uv-add--bounds"><a href="#uv-add--bounds"><code>--bounds</code></a> <i>bounds</i></dt><dd><p>The kind of version specifier to use when adding dependencies.</p>
<p>When adding a dependency to the project, if no constraint or URL is provided, a constraint is added based on the latest compatible version of the package. By default, a lower bound constraint is used, e.g., <code>&gt;=1.2.3</code>.</p>
<p>When <code>--frozen</code> is provided, no resolution is performed, and dependencies are always added without constraints.</p>
<p>This option is in preview and may change in any future release.</p>
<p>Possible values:</p>
<ul>
<li><code>lower</code>:  Only a lower bound, e.g., <code>&gt;=1.2.3</code></li>
<li><code>major</code>:  Allow the same major version, similar to the semver caret, e.g., <code>&gt;=1.2.3, &lt;2.0.0</code></li>
<li><code>minor</code>:  Allow the same minor version, similar to the semver tilde, e.g., <code>&gt;=1.2.3, &lt;1.3.0</code></li>
<li><code>exact</code>:  Pin the exact version, e.g., <code>==1.2.3</code></li>
</ul></dd><dt id="uv-add--branch"><a href="#uv-add--branch"><code>--branch</code></a> <i>branch</i></dt><dd><p>Branch to use when adding a dependency from Git</p>
</dd><dt id="uv-add--cache-dir"><a href="#uv-add--cache-dir"><code>--cache-dir</code></a> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>
<p>Defaults to <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on macOS and Linux, and <code>%LOCALAPPDATA%\uv\cache</code> on Windows.</p>
<p>To view the location of the cache directory, run <code>uv cache dir</code>.</p>
<p>May also be set with the <code>UV_CACHE_DIR</code> environment variable.</p></dd><dt id="uv-add--color"><a href="#uv-add--color"><code>--color</code></a> <i>color-choice</i></dt><dd><p>Control the use of color in output.</p>
<p>By default, uv will automatically detect support for colors when writing to a terminal.</p>
<p>Possible values:</p>
<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>
<li><code>always</code>:  Enables colored output regardless of the detected environment</li>
<li><code>never</code>:  Disables colored output</li>
</ul></dd><dt id="uv-add--compile-bytecode"><a href="#uv-add--compile-bytecode"><code>--compile-bytecode</code></a>, <code>--compile</code></dt><dd><p>Compile Python files to bytecode after installation.</p>
<p>By default, uv does not compile Python (<code>.py</code>) files to bytecode (<code>__pycache__/*.pyc</code>); instead, compilation is performed lazily the first time a module is imported. For use-cases in which start time is critical, such as CLI applications and Docker containers, this option can be enabled to trade longer installation times for faster start times.</p>
<p>When enabled, uv will process the entire site-packages directory (including packages that are not being modified by the current operation) for consistency. Like pip, it will also ignore errors.</p>
<p>May also be set with the <code>UV_COMPILE_BYTECODE</code> environment variable.</p></dd><dt id="uv-add--config-file"><a href="#uv-add--config-file"><code>--config-file</code></a> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>
<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>
<p>May also be set with the <code>UV_CONFIG_FILE</code> environment variable.</p></dd><dt id="uv-add--config-setting"><a href="#uv-add--config-setting"><code>--config-setting</code></a>, <code>--config-settings</code>, <code>-C</code> <i>config-setting</i></dt><dd><p>Settings to pass to the PEP 517 build backend, specified as <code>KEY=VALUE</code> pairs</p>
</dd><dt id="uv-add--constraints"><a href="#uv-add--constraints"><code>--constraints</code></a>, <code>--constraint</code>, <code>-c</code> <i>constraints</i></dt><dd><p>Constrain versions using the given requirements files.</p>
<p>Constraints files are <code>requirements.txt</code>-like files that only control the <em>version</em> of a requirement that's installed. The constraints will <em>not</em> be added to the project's <code>pyproject.toml</code> file, but <em>will</em> be respected during dependency resolution.</p>
<p>This is equivalent to pip's <code>--constraint</code> option.</p>
<p>May also be set with the <code>UV_CONSTRAINT</code> environment variable.</p></dd><dt id="uv-add--default-index"><a href="#uv-add--default-index"><code>--default-index</code></a> <i>default-index</i></dt><dd><p>The URL of the default package index (by default: <a href="https://pypi.org/simple">https://pypi.org/simple</a>).</p>
<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>
<p>The index given by this flag is given lower priority than all other indexes specified via the <code>--index</code> flag.</p>
<p>May also be set with the <code>UV_DEFAULT_INDEX</code> environment variable.</p></dd><dt id="uv-add--dev"><a href="#uv-add--dev"><code>--dev</code></a></dt><dd><p>Add the requirements to the development dependency group.</p>
<p>This option is an alias for <code>--group dev</code>.</p>
</dd><dt id="uv-add--directory"><a href="#uv-add--directory"><code>--directory</code></a> <i>directory</i></dt><dd><p>Change to the given directory prior to running the command.</p>
<p>Relative paths are resolved with the given directory as the base.</p>
<p>See <code>--project</code> to only change the project root directory.</p>
</dd><dt id="uv-add--editable"><a href="#uv-add--editable"><code>--editable</code></a></dt><dd><p>Add the requirements as editable</p>
</dd><dt id="uv-add--exclude-newer"><a href="#uv-add--exclude-newer"><code>--exclude-newer</code></a> <i>exclude-newer</i></dt><dd><p>Limit candidate packages to those that were uploaded prior to the given date.</p>
<p>Accepts both RFC 3339 timestamps (e.g., <code>2006-12-02T02:07:43Z</code>) and local dates in the same format (e.g., <code>2006-12-02</code>) in your system's configured time zone.</p>
<p>May also be set with the <code>UV_EXCLUDE_NEWER</code> environment variable.</p></dd><dt id="uv-add--extra"><a href="#uv-add--extra"><code>--extra</code></a> <i>extra</i></dt><dd><p>Extras to enable for the dependency.</p>
<p>May be provided more than once.</p>
<p>To add this dependency to an optional extra instead, see <code>--optional</code>.</p>
</dd><dt id="uv-add--extra-index-url"><a href="#uv-add--extra-index-url"><code>--extra-index-url</code></a> <i>extra-index-url</i></dt><dd><p>(Deprecated: use <code>--index</code> instead) Extra URLs of package indexes to use, in addition to <code>--index-url</code>.</p>
<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>
<p>All indexes provided via this flag take priority over the index specified by <code>--index-url</code> (which defaults to PyPI). When multiple <code>--extra-index-url</code> flags are provided, earlier values take priority.</p>
<p>May also be set with the <code>UV_EXTRA_INDEX_URL</code> environment variable.</p></dd><dt id="uv-add--find-links"><a href="#uv-add--find-links"><code>--find-links</code></a>, <code>-f</code> <i>find-links</i></dt><dd><p>Locations to search for candidate distributions, in addition to those found in the registry indexes.</p>
<p>If a path, the target must be a directory that contains packages as wheel files (<code>.whl</code>) or source distributions (e.g., <code>.tar.gz</code> or <code>.zip</code>) at the top level.</p>
<p>If a URL, the page must contain a flat list of links to package files adhering to the formats described above.</p>
<p>May also be set with the <code>UV_FIND_LINKS</code> environment variable.</p></dd><dt id="uv-add--fork-strategy"><a href="#uv-add--fork-strategy"><code>--fork-strategy</code></a> <i>fork-strategy</i></dt><dd><p>The strategy to use when selecting multiple versions of a given package across Python versions and platforms.</p>
<p>By default, uv will optimize for selecting the latest version of each package for each supported Python version (<code>requires-python</code>), while minimizing the number of selected versions across platforms.</p>
<p>Under <code>fewest</code>, uv will minimize the number of selected versions for each package, preferring older versions that are compatible with a wider range of supported Python versions or platforms.</p>
<p>May also be set with the <code>UV_FORK_STRATEGY</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>fewest</code>:  Optimize for selecting the fewest number of versions for each package. Older versions may be preferred if they are compatible with a wider range of supported Python versions or platforms</li>
<li><code>requires-python</code>:  Optimize for selecting latest supported version of each package, for each supported Python version</li>
</ul></dd><dt id="uv-add--frozen"><a href="#uv-add--frozen"><code>--frozen</code></a></dt><dd><p>Add dependencies without re-locking the project.</p>
<p>The project environment will not be synced.</p>
<p>May also be set with the <code>UV_FROZEN</code> environment variable.</p></dd><dt id="uv-add--group"><a href="#uv-add--group"><code>--group</code></a> <i>group</i></dt><dd><p>Add the requirements to the specified dependency group.</p>
<p>These requirements will not be included in the published metadata for the project.</p>
</dd><dt id="uv-add--help"><a href="#uv-add--help"><code>--help</code></a>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>
</dd><dt id="uv-add--index"><a href="#uv-add--index"><code>--index</code></a> <i>index</i></dt><dd><p>The URLs to use when resolving dependencies, in addition to the default index.</p>
<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>
<p>All indexes provided via this flag take priority over the index specified by <code>--default-index</code> (which defaults to PyPI). When multiple <code>--index</code> flags are provided, earlier values take priority.</p>
<p>May also be set with the <code>UV_INDEX</code> environment variable.</p></dd><dt id="uv-add--index-strategy"><a href="#uv-add--index-strategy"><code>--index-strategy</code></a> <i>index-strategy</i></dt><dd><p>The strategy to use when resolving against multiple index URLs.</p>
<p>By default, uv will stop at the first index on which a given package is available, and limit resolutions to those present on that first index (<code>first-index</code>). This prevents &quot;dependency confusion&quot; attacks, whereby an attacker can upload a malicious package under the same name to an alternate index.</p>
<p>May also be set with the <code>UV_INDEX_STRATEGY</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>first-index</code>:  Only use results from the first index that returns a match for a given package name</li>
<li><code>unsafe-first-match</code>:  Search for every package name across all indexes, exhausting the versions from the first index before moving on to the next</li>
<li><code>unsafe-best-match</code>:  Search for every package name across all indexes, preferring the &quot;best&quot; version found. If a package version is in multiple indexes, only look at the entry for the first index</li>
</ul></dd><dt id="uv-add--index-url"><a href="#uv-add--index-url"><code>--index-url</code></a>, <code>-i</code> <i>index-url</i></dt><dd><p>(Deprecated: use <code>--default-index</code> instead) The URL of the Python package index (by default: <a href="https://pypi.org/simple">https://pypi.org/simple</a>).</p>
<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>
<p>The index given by this flag is given lower priority than all other indexes specified via the <code>--extra-index-url</code> flag.</p>
<p>May also be set with the <code>UV_INDEX_URL</code> environment variable.</p></dd><dt id="uv-add--keyring-provider"><a href="#uv-add--keyring-provider"><code>--keyring-provider</code></a> <i>keyring-provider</i></dt><dd><p>Attempt to use <code>keyring</code> for authentication for index URLs.</p>
<p>At present, only <code>--keyring-provider subprocess</code> is supported, which configures uv to use the <code>keyring</code> CLI to handle authentication.</p>
<p>Defaults to <code>disabled</code>.</p>
<p>May also be set with the <code>UV_KEYRING_PROVIDER</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>disabled</code>:  Do not use keyring for credential lookup</li>
<li><code>subprocess</code>:  Use the <code>keyring</code> command for credential lookup</li>
</ul></dd><dt id="uv-add--link-mode"><a href="#uv-add--link-mode"><code>--link-mode</code></a> <i>link-mode</i></dt><dd><p>The method to use when installing packages from the global cache.</p>
<p>Defaults to <code>clone</code> (also known as Copy-on-Write) on macOS, and <code>hardlink</code> on Linux and Windows.</p>
<p>May also be set with the <code>UV_LINK_MODE</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>clone</code>:  Clone (i.e., copy-on-write) packages from the wheel into the <code>site-packages</code> directory</li>
<li><code>copy</code>:  Copy packages from the wheel into the <code>site-packages</code> directory</li>
<li><code>hardlink</code>:  Hard link packages from the wheel into the <code>site-packages</code> directory</li>
<li><code>symlink</code>:  Symbolically link packages from the wheel into the <code>site-packages</code> directory</li>
</ul></dd><dt id="uv-add--locked"><a href="#uv-add--locked"><code>--locked</code></a></dt><dd><p>Assert that the <code>uv.lock</code> will remain unchanged.</p>
<p>Requires that the lockfile is up-to-date. If the lockfile is missing or needs to be updated, uv will exit with an error.</p>
<p>May also be set with the <code>UV_LOCKED</code> environment variable.</p></dd><dt id="uv-add--managed-python"><a href="#uv-add--managed-python"><code>--managed-python</code></a></dt><dd><p>Require use of uv-managed Python versions.</p>
<p>By default, uv prefers using Python versions it manages. However, it will use system Python versions if a uv-managed Python is not installed. This option disables use of system Python versions.</p>
<p>May also be set with the <code>UV_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-add--marker"><a href="#uv-add--marker"><code>--marker</code></a>, <code>-m</code> <i>marker</i></dt><dd><p>Apply this marker to all added packages</p>
</dd><dt id="uv-add--native-tls"><a href="#uv-add--native-tls"><code>--native-tls</code></a></dt><dd><p>Whether to load TLS certificates from the platform's native certificate store.</p>
<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>
<p>However, in some cases, you may want to use the platform's native certificate store, especially if you're relying on a corporate trust root (e.g., for a mandatory proxy) that's included in your system's certificate store.</p>
<p>May also be set with the <code>UV_NATIVE_TLS</code> environment variable.</p></dd><dt id="uv-add--no-binary"><a href="#uv-add--no-binary"><code>--no-binary</code></a></dt><dd><p>Don't install pre-built wheels.</p>
<p>The given packages will be built and installed from source. The resolver will still use pre-built wheels to extract package metadata, if available.</p>
<p>May also be set with the <code>UV_NO_BINARY</code> environment variable.</p></dd><dt id="uv-add--no-binary-package"><a href="#uv-add--no-binary-package"><code>--no-binary-package</code></a> <i>no-binary-package</i></dt><dd><p>Don't install pre-built wheels for a specific package</p>
<p>May also be set with the <code>UV_NO_BINARY_PACKAGE</code> environment variable.</p></dd><dt id="uv-add--no-build"><a href="#uv-add--no-build"><code>--no-build</code></a></dt><dd><p>Don't build source distributions.</p>
<p>When enabled, resolving will not run arbitrary Python code. The cached wheels of already-built source distributions will be reused, but operations that require building distributions will exit with an error.</p>
<p>May also be set with the <code>UV_NO_BUILD</code> environment variable.</p></dd><dt id="uv-add--no-build-isolation"><a href="#uv-add--no-build-isolation"><code>--no-build-isolation</code></a></dt><dd><p>Disable isolation when building source distributions.</p>
<p>Assumes that build dependencies specified by PEP 518 are already installed.</p>
<p>May also be set with the <code>UV_NO_BUILD_ISOLATION</code> environment variable.</p></dd><dt id="uv-add--no-build-isolation-package"><a href="#uv-add--no-build-isolation-package"><code>--no-build-isolation-package</code></a> <i>no-build-isolation-package</i></dt><dd><p>Disable isolation when building source distributions for a specific package.</p>
<p>Assumes that the packages' build dependencies specified by PEP 518 are already installed.</p>
</dd><dt id="uv-add--no-build-package"><a href="#uv-add--no-build-package"><code>--no-build-package</code></a> <i>no-build-package</i></dt><dd><p>Don't build source distributions for a specific package</p>
<p>May also be set with the <code>UV_NO_BUILD_PACKAGE</code> environment variable.</p></dd><dt id="uv-add--no-cache"><a href="#uv-add--no-cache"><code>--no-cache</code></a>, <code>--no-cache-dir</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>
<p>May also be set with the <code>UV_NO_CACHE</code> environment variable.</p></dd><dt id="uv-add--no-config"><a href="#uv-add--no-config"><code>--no-config</code></a></dt><dd><p>Avoid discovering configuration files (<code>pyproject.toml</code>, <code>uv.toml</code>).</p>
<p>Normally, configuration files are discovered in the current directory, parent directories, or user configuration directories.</p>
<p>May also be set with the <code>UV_NO_CONFIG</code> environment variable.</p></dd><dt id="uv-add--no-index"><a href="#uv-add--no-index"><code>--no-index</code></a></dt><dd><p>Ignore the registry index (e.g., PyPI), instead relying on direct URL dependencies and those provided via <code>--find-links</code></p>
</dd><dt id="uv-add--no-managed-python"><a href="#uv-add--no-managed-python"><code>--no-managed-python</code></a></dt><dd><p>Disable use of uv-managed Python versions.</p>
<p>Instead, uv will search for a suitable Python version on the system.</p>
<p>May also be set with the <code>UV_NO_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-add--no-progress"><a href="#uv-add--no-progress"><code>--no-progress</code></a></dt><dd><p>Hide all progress outputs.</p>
<p>For example, spinners or progress bars.</p>
<p>May also be set with the <code>UV_NO_PROGRESS</code> environment variable.</p></dd><dt id="uv-add--no-python-downloads"><a href="#uv-add--no-python-downloads"><code>--no-python-downloads</code></a></dt><dd><p>Disable automatic downloads of Python.</p>
</dd><dt id="uv-add--no-sources"><a href="#uv-add--no-sources"><code>--no-sources</code></a></dt><dd><p>Ignore the <code>tool.uv.sources</code> table when resolving dependencies. Used to lock against the standards-compliant, publishable package metadata, as opposed to using any workspace, Git, URL, or local path sources</p>
</dd><dt id="uv-add--no-sync"><a href="#uv-add--no-sync"><code>--no-sync</code></a></dt><dd><p>Avoid syncing the virtual environment</p>
<p>May also be set with the <code>UV_NO_SYNC</code> environment variable.</p></dd><dt id="uv-add--offline"><a href="#uv-add--offline"><code>--offline</code></a></dt><dd><p>Disable network access.</p>
<p>When disabled, uv will only use locally cached data and locally available files.</p>
<p>May also be set with the <code>UV_OFFLINE</code> environment variable.</p></dd><dt id="uv-add--optional"><a href="#uv-add--optional"><code>--optional</code></a> <i>optional</i></dt><dd><p>Add the requirements to the package's optional dependencies for the specified extra.</p>
<p>The group may then be activated when installing the project with the <code>--extra</code> flag.</p>
<p>To enable an optional extra for this requirement instead, see <code>--extra</code>.</p>
</dd><dt id="uv-add--package"><a href="#uv-add--package"><code>--package</code></a> <i>package</i></dt><dd><p>Add the dependency to a specific package in the workspace</p>
</dd><dt id="uv-add--prerelease"><a href="#uv-add--prerelease"><code>--prerelease</code></a> <i>prerelease</i></dt><dd><p>The strategy to use when considering pre-release versions.</p>
<p>By default, uv will accept pre-releases for packages that <em>only</em> publish pre-releases, along with first-party requirements that contain an explicit pre-release marker in the declared specifiers (<code>if-necessary-or-explicit</code>).</p>
<p>May also be set with the <code>UV_PRERELEASE</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>disallow</code>:  Disallow all pre-release versions</li>
<li><code>allow</code>:  Allow all pre-release versions</li>
<li><code>if-necessary</code>:  Allow pre-release versions if all versions of a package are pre-release</li>
<li><code>explicit</code>:  Allow pre-release versions for first-party packages with explicit pre-release markers in their version requirements</li>
<li><code>if-necessary-or-explicit</code>:  Allow pre-release versions if all versions of a package are pre-release, or if the package has an explicit pre-release marker in its version requirements</li>
</ul></dd><dt id="uv-add--project"><a href="#uv-add--project"><code>--project</code></a> <i>project</i></dt><dd><p>Run the command within the given project directory.</p>
<p>All <code>pyproject.toml</code>, <code>uv.toml</code>, and <code>.python-version</code> files will be discovered by walking up the directory tree from the project root, as will the project's virtual environment (<code>.venv</code>).</p>
<p>Other command-line arguments (such as relative paths) will be resolved relative to the current working directory.</p>
<p>See <code>--directory</code> to change the working directory entirely.</p>
<p>This setting has no effect when used in the <code>uv pip</code> interface.</p>
<p>May also be set with the <code>UV_PROJECT</code> environment variable.</p></dd><dt id="uv-add--python"><a href="#uv-add--python"><code>--python</code></a>, <code>-p</code> <i>python</i></dt><dd><p>The Python interpreter to use for resolving and syncing.</p>
<p>See <a href="#uv-python">uv python</a> for details on Python discovery and supported request formats.</p>
<p>May also be set with the <code>UV_PYTHON</code> environment variable.</p></dd><dt id="uv-add--quiet"><a href="#uv-add--quiet"><code>--quiet</code></a>, <code>-q</code></dt><dd><p>Use quiet output.</p>
<p>Repeating this option, e.g., <code>-qq</code>, will enable a silent mode in which uv will write no output to stdout.</p>
</dd><dt id="uv-add--raw"><a href="#uv-add--raw"><code>--raw</code></a>, <code>--raw-sources</code></dt><dd><p>Add a dependency as provided.</p>
<p>By default, uv will use the <code>tool.uv.sources</code> section to record source information for Git, local, editable, and direct URL requirements. When <code>--raw</code> is provided, uv will add source requirements to <code>project.dependencies</code>, rather than <code>tool.uv.sources</code>.</p>
<p>Additionally, by default, uv will add bounds to your dependency, e.g., <code>foo&gt;=1.0.0</code>. When <code>--raw</code> is provided, uv will add the dependency without bounds.</p>
</dd><dt id="uv-add--refresh"><a href="#uv-add--refresh"><code>--refresh</code></a></dt><dd><p>Refresh all cached data</p>
</dd><dt id="uv-add--refresh-package"><a href="#uv-add--refresh-package"><code>--refresh-package</code></a> <i>refresh-package</i></dt><dd><p>Refresh cached data for a specific package</p>
</dd><dt id="uv-add--reinstall"><a href="#uv-add--reinstall"><code>--reinstall</code></a>, <code>--force-reinstall</code></dt><dd><p>Reinstall all packages, regardless of whether they're already installed. Implies <code>--refresh</code></p>
</dd><dt id="uv-add--reinstall-package"><a href="#uv-add--reinstall-package"><code>--reinstall-package</code></a> <i>reinstall-package</i></dt><dd><p>Reinstall a specific package, regardless of whether it's already installed. Implies <code>--refresh-package</code></p>
</dd><dt id="uv-add--requirements"><a href="#uv-add--requirements"><code>--requirements</code></a>, <code>--requirement</code>, <code>-r</code> <i>requirements</i></dt><dd><p>Add all packages listed in the given <code>requirements.txt</code> files</p>
</dd><dt id="uv-add--resolution"><a href="#uv-add--resolution"><code>--resolution</code></a> <i>resolution</i></dt><dd><p>The strategy to use when selecting between the different compatible versions for a given package requirement.</p>
<p>By default, uv will use the latest compatible version of each package (<code>highest</code>).</p>
<p>May also be set with the <code>UV_RESOLUTION</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>highest</code>:  Resolve the highest compatible version of each package</li>
<li><code>lowest</code>:  Resolve the lowest compatible version of each package</li>
<li><code>lowest-direct</code>:  Resolve the lowest compatible version of any direct dependencies, and the highest compatible version of any transitive dependencies</li>
</ul></dd><dt id="uv-add--rev"><a href="#uv-add--rev"><code>--rev</code></a> <i>rev</i></dt><dd><p>Commit to use when adding a dependency from Git</p>
</dd><dt id="uv-add--script"><a href="#uv-add--script"><code>--script</code></a> <i>script</i></dt><dd><p>Add the dependency to the specified Python script, rather than to a project.</p>
<p>If provided, uv will add the dependency to the script's inline metadata table, in adherence with PEP 723. If no such inline metadata table is present, a new one will be created and added to the script. When executed via <code>uv run</code>, uv will create a temporary environment for the script with all inline dependencies installed.</p>
</dd><dt id="uv-add--tag"><a href="#uv-add--tag"><code>--tag</code></a> <i>tag</i></dt><dd><p>Tag to use when adding a dependency from Git</p>
</dd><dt id="uv-add--upgrade"><a href="#uv-add--upgrade"><code>--upgrade</code></a>, <code>-U</code></dt><dd><p>Allow package upgrades, ignoring pinned versions in any existing output file. Implies <code>--refresh</code></p>
</dd><dt id="uv-add--upgrade-package"><a href="#uv-add--upgrade-package"><code>--upgrade-package</code></a>, <code>-P</code> <i>upgrade-package</i></dt><dd><p>Allow upgrades for a specific package, ignoring pinned versions in any existing output file. Implies <code>--refresh-package</code></p>
</dd><dt id="uv-add--verbose"><a href="#uv-add--verbose"><code>--verbose</code></a>, <code>-v</code></dt><dd><p>Use verbose output.</p>
<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (<a href="https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives">https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives</a>)</p>
</dd></dl>

## uv remove

Remove dependencies from the project.

Dependencies are removed from the project's `pyproject.toml` file.

If multiple entries exist for a given dependency, i.e., each with different markers, all of the entries will be removed.

The lockfile and project environment will be updated to reflect the removed dependencies. To skip updating the lockfile, use `--frozen`. To skip updating the environment, use `--no-sync`.

If any of the requested dependencies are not present in the project, uv will exit with an error.

If a package has been manually installed in the environment, i.e., with `uv pip install`, it will not be removed by `uv remove`.

uv will search for a project in the current directory or any parent directory. If a project cannot be found, uv will exit with an error.

<h3 class="cli-reference">Usage</h3>

```
uv remove [OPTIONS] <PACKAGES>...
```

<h3 class="cli-reference">Arguments</h3>

<dl class="cli-reference"><dt id="uv-remove--packages"><a href="#uv-remove--packages"<code>PACKAGES</code></a></dt><dd><p>The names of the dependencies to remove (e.g., <code>ruff</code>)</p>
</dd></dl>

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt id="uv-remove--active"><a href="#uv-remove--active"><code>--active</code></a></dt><dd><p>Prefer the active virtual environment over the project's virtual environment.</p>
<p>If the project virtual environment is active or no virtual environment is active, this has no effect.</p>
</dd><dt id="uv-remove--allow-insecure-host"><a href="#uv-remove--allow-insecure-host"><code>--allow-insecure-host</code></a>, <code>--trusted-host</code> <i>allow-insecure-host</i></dt><dd><p>Allow insecure connections to a host.</p>
<p>Can be provided multiple times.</p>
<p>Expects to receive either a hostname (e.g., <code>localhost</code>), a host-port pair (e.g., <code>localhost:8080</code>), or a URL (e.g., <code>https://localhost</code>).</p>
<p>WARNING: Hosts included in this list will not be verified against the system's certificate store. Only use <code>--allow-insecure-host</code> in a secure network with verified sources, as it bypasses SSL verification and could expose you to MITM attacks.</p>
<p>May also be set with the <code>UV_INSECURE_HOST</code> environment variable.</p></dd><dt id="uv-remove--cache-dir"><a href="#uv-remove--cache-dir"><code>--cache-dir</code></a> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>
<p>Defaults to <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on macOS and Linux, and <code>%LOCALAPPDATA%\uv\cache</code> on Windows.</p>
<p>To view the location of the cache directory, run <code>uv cache dir</code>.</p>
<p>May also be set with the <code>UV_CACHE_DIR</code> environment variable.</p></dd><dt id="uv-remove--color"><a href="#uv-remove--color"><code>--color</code></a> <i>color-choice</i></dt><dd><p>Control the use of color in output.</p>
<p>By default, uv will automatically detect support for colors when writing to a terminal.</p>
<p>Possible values:</p>
<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>
<li><code>always</code>:  Enables colored output regardless of the detected environment</li>
<li><code>never</code>:  Disables colored output</li>
</ul></dd><dt id="uv-remove--compile-bytecode"><a href="#uv-remove--compile-bytecode"><code>--compile-bytecode</code></a>, <code>--compile</code></dt><dd><p>Compile Python files to bytecode after installation.</p>
<p>By default, uv does not compile Python (<code>.py</code>) files to bytecode (<code>__pycache__/*.pyc</code>); instead, compilation is performed lazily the first time a module is imported. For use-cases in which start time is critical, such as CLI applications and Docker containers, this option can be enabled to trade longer installation times for faster start times.</p>
<p>When enabled, uv will process the entire site-packages directory (including packages that are not being modified by the current operation) for consistency. Like pip, it will also ignore errors.</p>
<p>May also be set with the <code>UV_COMPILE_BYTECODE</code> environment variable.</p></dd><dt id="uv-remove--config-file"><a href="#uv-remove--config-file"><code>--config-file</code></a> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>
<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>
<p>May also be set with the <code>UV_CONFIG_FILE</code> environment variable.</p></dd><dt id="uv-remove--config-setting"><a href="#uv-remove--config-setting"><code>--config-setting</code></a>, <code>--config-settings</code>, <code>-C</code> <i>config-setting</i></dt><dd><p>Settings to pass to the PEP 517 build backend, specified as <code>KEY=VALUE</code> pairs</p>
</dd><dt id="uv-remove--default-index"><a href="#uv-remove--default-index"><code>--default-index</code></a> <i>default-index</i></dt><dd><p>The URL of the default package index (by default: <a href="https://pypi.org/simple">https://pypi.org/simple</a>).</p>
<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>
<p>The index given by this flag is given lower priority than all other indexes specified via the <code>--index</code> flag.</p>
<p>May also be set with the <code>UV_DEFAULT_INDEX</code> environment variable.</p></dd><dt id="uv-remove--dev"><a href="#uv-remove--dev"><code>--dev</code></a></dt><dd><p>Remove the packages from the development dependency group.</p>
<p>This option is an alias for <code>--group dev</code>.</p>
</dd><dt id="uv-remove--directory"><a href="#uv-remove--directory"><code>--directory</code></a> <i>directory</i></dt><dd><p>Change to the given directory prior to running the command.</p>
<p>Relative paths are resolved with the given directory as the base.</p>
<p>See <code>--project</code> to only change the project root directory.</p>
</dd><dt id="uv-remove--exclude-newer"><a href="#uv-remove--exclude-newer"><code>--exclude-newer</code></a> <i>exclude-newer</i></dt><dd><p>Limit candidate packages to those that were uploaded prior to the given date.</p>
<p>Accepts both RFC 3339 timestamps (e.g., <code>2006-12-02T02:07:43Z</code>) and local dates in the same format (e.g., <code>2006-12-02</code>) in your system's configured time zone.</p>
<p>May also be set with the <code>UV_EXCLUDE_NEWER</code> environment variable.</p></dd><dt id="uv-remove--extra-index-url"><a href="#uv-remove--extra-index-url"><code>--extra-index-url</code></a> <i>extra-index-url</i></dt><dd><p>(Deprecated: use <code>--index</code> instead) Extra URLs of package indexes to use, in addition to <code>--index-url</code>.</p>
<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>
<p>All indexes provided via this flag take priority over the index specified by <code>--index-url</code> (which defaults to PyPI). When multiple <code>--extra-index-url</code> flags are provided, earlier values take priority.</p>
<p>May also be set with the <code>UV_EXTRA_INDEX_URL</code> environment variable.</p></dd><dt id="uv-remove--find-links"><a href="#uv-remove--find-links"><code>--find-links</code></a>, <code>-f</code> <i>find-links</i></dt><dd><p>Locations to search for candidate distributions, in addition to those found in the registry indexes.</p>
<p>If a path, the target must be a directory that contains packages as wheel files (<code>.whl</code>) or source distributions (e.g., <code>.tar.gz</code> or <code>.zip</code>) at the top level.</p>
<p>If a URL, the page must contain a flat list of links to package files adhering to the formats described above.</p>
<p>May also be set with the <code>UV_FIND_LINKS</code> environment variable.</p></dd><dt id="uv-remove--fork-strategy"><a href="#uv-remove--fork-strategy"><code>--fork-strategy</code></a> <i>fork-strategy</i></dt><dd><p>The strategy to use when selecting multiple versions of a given package across Python versions and platforms.</p>
<p>By default, uv will optimize for selecting the latest version of each package for each supported Python version (<code>requires-python</code>), while minimizing the number of selected versions across platforms.</p>
<p>Under <code>fewest</code>, uv will minimize the number of selected versions for each package, preferring older versions that are compatible with a wider range of supported Python versions or platforms.</p>
<p>May also be set with the <code>UV_FORK_STRATEGY</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>fewest</code>:  Optimize for selecting the fewest number of versions for each package. Older versions may be preferred if they are compatible with a wider range of supported Python versions or platforms</li>
<li><code>requires-python</code>:  Optimize for selecting latest supported version of each package, for each supported Python version</li>
</ul></dd><dt id="uv-remove--frozen"><a href="#uv-remove--frozen"><code>--frozen</code></a></dt><dd><p>Remove dependencies without re-locking the project.</p>
<p>The project environment will not be synced.</p>
<p>May also be set with the <code>UV_FROZEN</code> environment variable.</p></dd><dt id="uv-remove--group"><a href="#uv-remove--group"><code>--group</code></a> <i>group</i></dt><dd><p>Remove the packages from the specified dependency group</p>
</dd><dt id="uv-remove--help"><a href="#uv-remove--help"><code>--help</code></a>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>
</dd><dt id="uv-remove--index"><a href="#uv-remove--index"><code>--index</code></a> <i>index</i></dt><dd><p>The URLs to use when resolving dependencies, in addition to the default index.</p>
<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>
<p>All indexes provided via this flag take priority over the index specified by <code>--default-index</code> (which defaults to PyPI). When multiple <code>--index</code> flags are provided, earlier values take priority.</p>
<p>May also be set with the <code>UV_INDEX</code> environment variable.</p></dd><dt id="uv-remove--index-strategy"><a href="#uv-remove--index-strategy"><code>--index-strategy</code></a> <i>index-strategy</i></dt><dd><p>The strategy to use when resolving against multiple index URLs.</p>
<p>By default, uv will stop at the first index on which a given package is available, and limit resolutions to those present on that first index (<code>first-index</code>). This prevents &quot;dependency confusion&quot; attacks, whereby an attacker can upload a malicious package under the same name to an alternate index.</p>
<p>May also be set with the <code>UV_INDEX_STRATEGY</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>first-index</code>:  Only use results from the first index that returns a match for a given package name</li>
<li><code>unsafe-first-match</code>:  Search for every package name across all indexes, exhausting the versions from the first index before moving on to the next</li>
<li><code>unsafe-best-match</code>:  Search for every package name across all indexes, preferring the &quot;best&quot; version found. If a package version is in multiple indexes, only look at the entry for the first index</li>
</ul></dd><dt id="uv-remove--index-url"><a href="#uv-remove--index-url"><code>--index-url</code></a>, <code>-i</code> <i>index-url</i></dt><dd><p>(Deprecated: use <code>--default-index</code> instead) The URL of the Python package index (by default: <a href="https://pypi.org/simple">https://pypi.org/simple</a>).</p>
<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>
<p>The index given by this flag is given lower priority than all other indexes specified via the <code>--extra-index-url</code> flag.</p>
<p>May also be set with the <code>UV_INDEX_URL</code> environment variable.</p></dd><dt id="uv-remove--keyring-provider"><a href="#uv-remove--keyring-provider"><code>--keyring-provider</code></a> <i>keyring-provider</i></dt><dd><p>Attempt to use <code>keyring</code> for authentication for index URLs.</p>
<p>At present, only <code>--keyring-provider subprocess</code> is supported, which configures uv to use the <code>keyring</code> CLI to handle authentication.</p>
<p>Defaults to <code>disabled</code>.</p>
<p>May also be set with the <code>UV_KEYRING_PROVIDER</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>disabled</code>:  Do not use keyring for credential lookup</li>
<li><code>subprocess</code>:  Use the <code>keyring</code> command for credential lookup</li>
</ul></dd><dt id="uv-remove--link-mode"><a href="#uv-remove--link-mode"><code>--link-mode</code></a> <i>link-mode</i></dt><dd><p>The method to use when installing packages from the global cache.</p>
<p>Defaults to <code>clone</code> (also known as Copy-on-Write) on macOS, and <code>hardlink</code> on Linux and Windows.</p>
<p>May also be set with the <code>UV_LINK_MODE</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>clone</code>:  Clone (i.e., copy-on-write) packages from the wheel into the <code>site-packages</code> directory</li>
<li><code>copy</code>:  Copy packages from the wheel into the <code>site-packages</code> directory</li>
<li><code>hardlink</code>:  Hard link packages from the wheel into the <code>site-packages</code> directory</li>
<li><code>symlink</code>:  Symbolically link packages from the wheel into the <code>site-packages</code> directory</li>
</ul></dd><dt id="uv-remove--locked"><a href="#uv-remove--locked"><code>--locked</code></a></dt><dd><p>Assert that the <code>uv.lock</code> will remain unchanged.</p>
<p>Requires that the lockfile is up-to-date. If the lockfile is missing or needs to be updated, uv will exit with an error.</p>
<p>May also be set with the <code>UV_LOCKED</code> environment variable.</p></dd><dt id="uv-remove--managed-python"><a href="#uv-remove--managed-python"><code>--managed-python</code></a></dt><dd><p>Require use of uv-managed Python versions.</p>
<p>By default, uv prefers using Python versions it manages. However, it will use system Python versions if a uv-managed Python is not installed. This option disables use of system Python versions.</p>
<p>May also be set with the <code>UV_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-remove--native-tls"><a href="#uv-remove--native-tls"><code>--native-tls</code></a></dt><dd><p>Whether to load TLS certificates from the platform's native certificate store.</p>
<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>
<p>However, in some cases, you may want to use the platform's native certificate store, especially if you're relying on a corporate trust root (e.g., for a mandatory proxy) that's included in your system's certificate store.</p>
<p>May also be set with the <code>UV_NATIVE_TLS</code> environment variable.</p></dd><dt id="uv-remove--no-binary"><a href="#uv-remove--no-binary"><code>--no-binary</code></a></dt><dd><p>Don't install pre-built wheels.</p>
<p>The given packages will be built and installed from source. The resolver will still use pre-built wheels to extract package metadata, if available.</p>
<p>May also be set with the <code>UV_NO_BINARY</code> environment variable.</p></dd><dt id="uv-remove--no-binary-package"><a href="#uv-remove--no-binary-package"><code>--no-binary-package</code></a> <i>no-binary-package</i></dt><dd><p>Don't install pre-built wheels for a specific package</p>
<p>May also be set with the <code>UV_NO_BINARY_PACKAGE</code> environment variable.</p></dd><dt id="uv-remove--no-build"><a href="#uv-remove--no-build"><code>--no-build</code></a></dt><dd><p>Don't build source distributions.</p>
<p>When enabled, resolving will not run arbitrary Python code. The cached wheels of already-built source distributions will be reused, but operations that require building distributions will exit with an error.</p>
<p>May also be set with the <code>UV_NO_BUILD</code> environment variable.</p></dd><dt id="uv-remove--no-build-isolation"><a href="#uv-remove--no-build-isolation"><code>--no-build-isolation</code></a></dt><dd><p>Disable isolation when building source distributions.</p>
<p>Assumes that build dependencies specified by PEP 518 are already installed.</p>
<p>May also be set with the <code>UV_NO_BUILD_ISOLATION</code> environment variable.</p></dd><dt id="uv-remove--no-build-isolation-package"><a href="#uv-remove--no-build-isolation-package"><code>--no-build-isolation-package</code></a> <i>no-build-isolation-package</i></dt><dd><p>Disable isolation when building source distributions for a specific package.</p>
<p>Assumes that the packages' build dependencies specified by PEP 518 are already installed.</p>
</dd><dt id="uv-remove--no-build-package"><a href="#uv-remove--no-build-package"><code>--no-build-package</code></a> <i>no-build-package</i></dt><dd><p>Don't build source distributions for a specific package</p>
<p>May also be set with the <code>UV_NO_BUILD_PACKAGE</code> environment variable.</p></dd><dt id="uv-remove--no-cache"><a href="#uv-remove--no-cache"><code>--no-cache</code></a>, <code>--no-cache-dir</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>
<p>May also be set with the <code>UV_NO_CACHE</code> environment variable.</p></dd><dt id="uv-remove--no-config"><a href="#uv-remove--no-config"><code>--no-config</code></a></dt><dd><p>Avoid discovering configuration files (<code>pyproject.toml</code>, <code>uv.toml</code>).</p>
<p>Normally, configuration files are discovered in the current directory, parent directories, or user configuration directories.</p>
<p>May also be set with the <code>UV_NO_CONFIG</code> environment variable.</p></dd><dt id="uv-remove--no-index"><a href="#uv-remove--no-index"><code>--no-index</code></a></dt><dd><p>Ignore the registry index (e.g., PyPI), instead relying on direct URL dependencies and those provided via <code>--find-links</code></p>
</dd><dt id="uv-remove--no-managed-python"><a href="#uv-remove--no-managed-python"><code>--no-managed-python</code></a></dt><dd><p>Disable use of uv-managed Python versions.</p>
<p>Instead, uv will search for a suitable Python version on the system.</p>
<p>May also be set with the <code>UV_NO_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-remove--no-progress"><a href="#uv-remove--no-progress"><code>--no-progress</code></a></dt><dd><p>Hide all progress outputs.</p>
<p>For example, spinners or progress bars.</p>
<p>May also be set with the <code>UV_NO_PROGRESS</code> environment variable.</p></dd><dt id="uv-remove--no-python-downloads"><a href="#uv-remove--no-python-downloads"><code>--no-python-downloads</code></a></dt><dd><p>Disable automatic downloads of Python.</p>
</dd><dt id="uv-remove--no-sources"><a href="#uv-remove--no-sources"><code>--no-sources</code></a></dt><dd><p>Ignore the <code>tool.uv.sources</code> table when resolving dependencies. Used to lock against the standards-compliant, publishable package metadata, as opposed to using any workspace, Git, URL, or local path sources</p>
</dd><dt id="uv-remove--no-sync"><a href="#uv-remove--no-sync"><code>--no-sync</code></a></dt><dd><p>Avoid syncing the virtual environment after re-locking the project</p>
<p>May also be set with the <code>UV_NO_SYNC</code> environment variable.</p></dd><dt id="uv-remove--offline"><a href="#uv-remove--offline"><code>--offline</code></a></dt><dd><p>Disable network access.</p>
<p>When disabled, uv will only use locally cached data and locally available files.</p>
<p>May also be set with the <code>UV_OFFLINE</code> environment variable.</p></dd><dt id="uv-remove--optional"><a href="#uv-remove--optional"><code>--optional</code></a> <i>optional</i></dt><dd><p>Remove the packages from the project's optional dependencies for the specified extra</p>
</dd><dt id="uv-remove--package"><a href="#uv-remove--package"><code>--package</code></a> <i>package</i></dt><dd><p>Remove the dependencies from a specific package in the workspace</p>
</dd><dt id="uv-remove--prerelease"><a href="#uv-remove--prerelease"><code>--prerelease</code></a> <i>prerelease</i></dt><dd><p>The strategy to use when considering pre-release versions.</p>
<p>By default, uv will accept pre-releases for packages that <em>only</em> publish pre-releases, along with first-party requirements that contain an explicit pre-release marker in the declared specifiers (<code>if-necessary-or-explicit</code>).</p>
<p>May also be set with the <code>UV_PRERELEASE</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>disallow</code>:  Disallow all pre-release versions</li>
<li><code>allow</code>:  Allow all pre-release versions</li>
<li><code>if-necessary</code>:  Allow pre-release versions if all versions of a package are pre-release</li>
<li><code>explicit</code>:  Allow pre-release versions for first-party packages with explicit pre-release markers in their version requirements</li>
<li><code>if-necessary-or-explicit</code>:  Allow pre-release versions if all versions of a package are pre-release, or if the package has an explicit pre-release marker in its version requirements</li>
</ul></dd><dt id="uv-remove--project"><a href="#uv-remove--project"><code>--project</code></a> <i>project</i></dt><dd><p>Run the command within the given project directory.</p>
<p>All <code>pyproject.toml</code>, <code>uv.toml</code>, and <code>.python-version</code> files will be discovered by walking up the directory tree from the project root, as will the project's virtual environment (<code>.venv</code>).</p>
<p>Other command-line arguments (such as relative paths) will be resolved relative to the current working directory.</p>
<p>See <code>--directory</code> to change the working directory entirely.</p>
<p>This setting has no effect when used in the <code>uv pip</code> interface.</p>
<p>May also be set with the <code>UV_PROJECT</code> environment variable.</p></dd><dt id="uv-remove--python"><a href="#uv-remove--python"><code>--python</code></a>, <code>-p</code> <i>python</i></dt><dd><p>The Python interpreter to use for resolving and syncing.</p>
<p>See <a href="#uv-python">uv python</a> for details on Python discovery and supported request formats.</p>
<p>May also be set with the <code>UV_PYTHON</code> environment variable.</p></dd><dt id="uv-remove--quiet"><a href="#uv-remove--quiet"><code>--quiet</code></a>, <code>-q</code></dt><dd><p>Use quiet output.</p>
<p>Repeating this option, e.g., <code>-qq</code>, will enable a silent mode in which uv will write no output to stdout.</p>
</dd><dt id="uv-remove--refresh"><a href="#uv-remove--refresh"><code>--refresh</code></a></dt><dd><p>Refresh all cached data</p>
</dd><dt id="uv-remove--refresh-package"><a href="#uv-remove--refresh-package"><code>--refresh-package</code></a> <i>refresh-package</i></dt><dd><p>Refresh cached data for a specific package</p>
</dd><dt id="uv-remove--reinstall"><a href="#uv-remove--reinstall"><code>--reinstall</code></a>, <code>--force-reinstall</code></dt><dd><p>Reinstall all packages, regardless of whether they're already installed. Implies <code>--refresh</code></p>
</dd><dt id="uv-remove--reinstall-package"><a href="#uv-remove--reinstall-package"><code>--reinstall-package</code></a> <i>reinstall-package</i></dt><dd><p>Reinstall a specific package, regardless of whether it's already installed. Implies <code>--refresh-package</code></p>
</dd><dt id="uv-remove--resolution"><a href="#uv-remove--resolution"><code>--resolution</code></a> <i>resolution</i></dt><dd><p>The strategy to use when selecting between the different compatible versions for a given package requirement.</p>
<p>By default, uv will use the latest compatible version of each package (<code>highest</code>).</p>
<p>May also be set with the <code>UV_RESOLUTION</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>highest</code>:  Resolve the highest compatible version of each package</li>
<li><code>lowest</code>:  Resolve the lowest compatible version of each package</li>
<li><code>lowest-direct</code>:  Resolve the lowest compatible version of any direct dependencies, and the highest compatible version of any transitive dependencies</li>
</ul></dd><dt id="uv-remove--script"><a href="#uv-remove--script"><code>--script</code></a> <i>script</i></dt><dd><p>Remove the dependency from the specified Python script, rather than from a project.</p>
<p>If provided, uv will remove the dependency from the script's inline metadata table, in adherence with PEP 723.</p>
</dd><dt id="uv-remove--upgrade"><a href="#uv-remove--upgrade"><code>--upgrade</code></a>, <code>-U</code></dt><dd><p>Allow package upgrades, ignoring pinned versions in any existing output file. Implies <code>--refresh</code></p>
</dd><dt id="uv-remove--upgrade-package"><a href="#uv-remove--upgrade-package"><code>--upgrade-package</code></a>, <code>-P</code> <i>upgrade-package</i></dt><dd><p>Allow upgrades for a specific package, ignoring pinned versions in any existing output file. Implies <code>--refresh-package</code></p>
</dd><dt id="uv-remove--verbose"><a href="#uv-remove--verbose"><code>--verbose</code></a>, <code>-v</code></dt><dd><p>Use verbose output.</p>
<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (<a href="https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives">https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives</a>)</p>
</dd></dl>

## uv version

Read or update the project's version

<h3 class="cli-reference">Usage</h3>

```
uv version [OPTIONS] [VALUE]
```

<h3 class="cli-reference">Arguments</h3>

<dl class="cli-reference"><dt id="uv-version--value"><a href="#uv-version--value"<code>VALUE</code></a></dt><dd><p>Set the project version to this value</p>
<p>To update the project using semantic versioning components instead, use <code>--bump</code>.</p>
</dd></dl>

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt id="uv-version--active"><a href="#uv-version--active"><code>--active</code></a></dt><dd><p>Prefer the active virtual environment over the project's virtual environment.</p>
<p>If the project virtual environment is active or no virtual environment is active, this has no effect.</p>
</dd><dt id="uv-version--allow-insecure-host"><a href="#uv-version--allow-insecure-host"><code>--allow-insecure-host</code></a>, <code>--trusted-host</code> <i>allow-insecure-host</i></dt><dd><p>Allow insecure connections to a host.</p>
<p>Can be provided multiple times.</p>
<p>Expects to receive either a hostname (e.g., <code>localhost</code>), a host-port pair (e.g., <code>localhost:8080</code>), or a URL (e.g., <code>https://localhost</code>).</p>
<p>WARNING: Hosts included in this list will not be verified against the system's certificate store. Only use <code>--allow-insecure-host</code> in a secure network with verified sources, as it bypasses SSL verification and could expose you to MITM attacks.</p>
<p>May also be set with the <code>UV_INSECURE_HOST</code> environment variable.</p></dd><dt id="uv-version--bump"><a href="#uv-version--bump"><code>--bump</code></a> <i>bump</i></dt><dd><p>Update the project version using the given semantics</p>
<p>Possible values:</p>
<ul>
<li><code>major</code>:  Increase the major version (1.2.3 =&gt; 2.0.0)</li>
<li><code>minor</code>:  Increase the minor version (1.2.3 =&gt; 1.3.0)</li>
<li><code>patch</code>:  Increase the patch version (1.2.3 =&gt; 1.2.4)</li>
</ul></dd><dt id="uv-version--cache-dir"><a href="#uv-version--cache-dir"><code>--cache-dir</code></a> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>
<p>Defaults to <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on macOS and Linux, and <code>%LOCALAPPDATA%\uv\cache</code> on Windows.</p>
<p>To view the location of the cache directory, run <code>uv cache dir</code>.</p>
<p>May also be set with the <code>UV_CACHE_DIR</code> environment variable.</p></dd><dt id="uv-version--color"><a href="#uv-version--color"><code>--color</code></a> <i>color-choice</i></dt><dd><p>Control the use of color in output.</p>
<p>By default, uv will automatically detect support for colors when writing to a terminal.</p>
<p>Possible values:</p>
<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>
<li><code>always</code>:  Enables colored output regardless of the detected environment</li>
<li><code>never</code>:  Disables colored output</li>
</ul></dd><dt id="uv-version--compile-bytecode"><a href="#uv-version--compile-bytecode"><code>--compile-bytecode</code></a>, <code>--compile</code></dt><dd><p>Compile Python files to bytecode after installation.</p>
<p>By default, uv does not compile Python (<code>.py</code>) files to bytecode (<code>__pycache__/*.pyc</code>); instead, compilation is performed lazily the first time a module is imported. For use-cases in which start time is critical, such as CLI applications and Docker containers, this option can be enabled to trade longer installation times for faster start times.</p>
<p>When enabled, uv will process the entire site-packages directory (including packages that are not being modified by the current operation) for consistency. Like pip, it will also ignore errors.</p>
<p>May also be set with the <code>UV_COMPILE_BYTECODE</code> environment variable.</p></dd><dt id="uv-version--config-file"><a href="#uv-version--config-file"><code>--config-file</code></a> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>
<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>
<p>May also be set with the <code>UV_CONFIG_FILE</code> environment variable.</p></dd><dt id="uv-version--config-setting"><a href="#uv-version--config-setting"><code>--config-setting</code></a>, <code>--config-settings</code>, <code>-C</code> <i>config-setting</i></dt><dd><p>Settings to pass to the PEP 517 build backend, specified as <code>KEY=VALUE</code> pairs</p>
</dd><dt id="uv-version--default-index"><a href="#uv-version--default-index"><code>--default-index</code></a> <i>default-index</i></dt><dd><p>The URL of the default package index (by default: <a href="https://pypi.org/simple">https://pypi.org/simple</a>).</p>
<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>
<p>The index given by this flag is given lower priority than all other indexes specified via the <code>--index</code> flag.</p>
<p>May also be set with the <code>UV_DEFAULT_INDEX</code> environment variable.</p></dd><dt id="uv-version--directory"><a href="#uv-version--directory"><code>--directory</code></a> <i>directory</i></dt><dd><p>Change to the given directory prior to running the command.</p>
<p>Relative paths are resolved with the given directory as the base.</p>
<p>See <code>--project</code> to only change the project root directory.</p>
</dd><dt id="uv-version--dry-run"><a href="#uv-version--dry-run"><code>--dry-run</code></a></dt><dd><p>Don't write a new version to the <code>pyproject.toml</code></p>
<p>Instead, the version will be displayed.</p>
</dd><dt id="uv-version--exclude-newer"><a href="#uv-version--exclude-newer"><code>--exclude-newer</code></a> <i>exclude-newer</i></dt><dd><p>Limit candidate packages to those that were uploaded prior to the given date.</p>
<p>Accepts both RFC 3339 timestamps (e.g., <code>2006-12-02T02:07:43Z</code>) and local dates in the same format (e.g., <code>2006-12-02</code>) in your system's configured time zone.</p>
<p>May also be set with the <code>UV_EXCLUDE_NEWER</code> environment variable.</p></dd><dt id="uv-version--extra-index-url"><a href="#uv-version--extra-index-url"><code>--extra-index-url</code></a> <i>extra-index-url</i></dt><dd><p>(Deprecated: use <code>--index</code> instead) Extra URLs of package indexes to use, in addition to <code>--index-url</code>.</p>
<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>
<p>All indexes provided via this flag take priority over the index specified by <code>--index-url</code> (which defaults to PyPI). When multiple <code>--extra-index-url</code> flags are provided, earlier values take priority.</p>
<p>May also be set with the <code>UV_EXTRA_INDEX_URL</code> environment variable.</p></dd><dt id="uv-version--find-links"><a href="#uv-version--find-links"><code>--find-links</code></a>, <code>-f</code> <i>find-links</i></dt><dd><p>Locations to search for candidate distributions, in addition to those found in the registry indexes.</p>
<p>If a path, the target must be a directory that contains packages as wheel files (<code>.whl</code>) or source distributions (e.g., <code>.tar.gz</code> or <code>.zip</code>) at the top level.</p>
<p>If a URL, the page must contain a flat list of links to package files adhering to the formats described above.</p>
<p>May also be set with the <code>UV_FIND_LINKS</code> environment variable.</p></dd><dt id="uv-version--fork-strategy"><a href="#uv-version--fork-strategy"><code>--fork-strategy</code></a> <i>fork-strategy</i></dt><dd><p>The strategy to use when selecting multiple versions of a given package across Python versions and platforms.</p>
<p>By default, uv will optimize for selecting the latest version of each package for each supported Python version (<code>requires-python</code>), while minimizing the number of selected versions across platforms.</p>
<p>Under <code>fewest</code>, uv will minimize the number of selected versions for each package, preferring older versions that are compatible with a wider range of supported Python versions or platforms.</p>
<p>May also be set with the <code>UV_FORK_STRATEGY</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>fewest</code>:  Optimize for selecting the fewest number of versions for each package. Older versions may be preferred if they are compatible with a wider range of supported Python versions or platforms</li>
<li><code>requires-python</code>:  Optimize for selecting latest supported version of each package, for each supported Python version</li>
</ul></dd><dt id="uv-version--frozen"><a href="#uv-version--frozen"><code>--frozen</code></a></dt><dd><p>Update the version without re-locking the project.</p>
<p>The project environment will not be synced.</p>
<p>May also be set with the <code>UV_FROZEN</code> environment variable.</p></dd><dt id="uv-version--help"><a href="#uv-version--help"><code>--help</code></a>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>
</dd><dt id="uv-version--index"><a href="#uv-version--index"><code>--index</code></a> <i>index</i></dt><dd><p>The URLs to use when resolving dependencies, in addition to the default index.</p>
<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>
<p>All indexes provided via this flag take priority over the index specified by <code>--default-index</code> (which defaults to PyPI). When multiple <code>--index</code> flags are provided, earlier values take priority.</p>
<p>May also be set with the <code>UV_INDEX</code> environment variable.</p></dd><dt id="uv-version--index-strategy"><a href="#uv-version--index-strategy"><code>--index-strategy</code></a> <i>index-strategy</i></dt><dd><p>The strategy to use when resolving against multiple index URLs.</p>
<p>By default, uv will stop at the first index on which a given package is available, and limit resolutions to those present on that first index (<code>first-index</code>). This prevents &quot;dependency confusion&quot; attacks, whereby an attacker can upload a malicious package under the same name to an alternate index.</p>
<p>May also be set with the <code>UV_INDEX_STRATEGY</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>first-index</code>:  Only use results from the first index that returns a match for a given package name</li>
<li><code>unsafe-first-match</code>:  Search for every package name across all indexes, exhausting the versions from the first index before moving on to the next</li>
<li><code>unsafe-best-match</code>:  Search for every package name across all indexes, preferring the &quot;best&quot; version found. If a package version is in multiple indexes, only look at the entry for the first index</li>
</ul></dd><dt id="uv-version--index-url"><a href="#uv-version--index-url"><code>--index-url</code></a>, <code>-i</code> <i>index-url</i></dt><dd><p>(Deprecated: use <code>--default-index</code> instead) The URL of the Python package index (by default: <a href="https://pypi.org/simple">https://pypi.org/simple</a>).</p>
<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>
<p>The index given by this flag is given lower priority than all other indexes specified via the <code>--extra-index-url</code> flag.</p>
<p>May also be set with the <code>UV_INDEX_URL</code> environment variable.</p></dd><dt id="uv-version--keyring-provider"><a href="#uv-version--keyring-provider"><code>--keyring-provider</code></a> <i>keyring-provider</i></dt><dd><p>Attempt to use <code>keyring</code> for authentication for index URLs.</p>
<p>At present, only <code>--keyring-provider subprocess</code> is supported, which configures uv to use the <code>keyring</code> CLI to handle authentication.</p>
<p>Defaults to <code>disabled</code>.</p>
<p>May also be set with the <code>UV_KEYRING_PROVIDER</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>disabled</code>:  Do not use keyring for credential lookup</li>
<li><code>subprocess</code>:  Use the <code>keyring</code> command for credential lookup</li>
</ul></dd><dt id="uv-version--link-mode"><a href="#uv-version--link-mode"><code>--link-mode</code></a> <i>link-mode</i></dt><dd><p>The method to use when installing packages from the global cache.</p>
<p>Defaults to <code>clone</code> (also known as Copy-on-Write) on macOS, and <code>hardlink</code> on Linux and Windows.</p>
<p>May also be set with the <code>UV_LINK_MODE</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>clone</code>:  Clone (i.e., copy-on-write) packages from the wheel into the <code>site-packages</code> directory</li>
<li><code>copy</code>:  Copy packages from the wheel into the <code>site-packages</code> directory</li>
<li><code>hardlink</code>:  Hard link packages from the wheel into the <code>site-packages</code> directory</li>
<li><code>symlink</code>:  Symbolically link packages from the wheel into the <code>site-packages</code> directory</li>
</ul></dd><dt id="uv-version--locked"><a href="#uv-version--locked"><code>--locked</code></a></dt><dd><p>Assert that the <code>uv.lock</code> will remain unchanged.</p>
<p>Requires that the lockfile is up-to-date. If the lockfile is missing or needs to be updated, uv will exit with an error.</p>
<p>May also be set with the <code>UV_LOCKED</code> environment variable.</p></dd><dt id="uv-version--managed-python"><a href="#uv-version--managed-python"><code>--managed-python</code></a></dt><dd><p>Require use of uv-managed Python versions.</p>
<p>By default, uv prefers using Python versions it manages. However, it will use system Python versions if a uv-managed Python is not installed. This option disables use of system Python versions.</p>
<p>May also be set with the <code>UV_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-version--native-tls"><a href="#uv-version--native-tls"><code>--native-tls</code></a></dt><dd><p>Whether to load TLS certificates from the platform's native certificate store.</p>
<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>
<p>However, in some cases, you may want to use the platform's native certificate store, especially if you're relying on a corporate trust root (e.g., for a mandatory proxy) that's included in your system's certificate store.</p>
<p>May also be set with the <code>UV_NATIVE_TLS</code> environment variable.</p></dd><dt id="uv-version--no-binary"><a href="#uv-version--no-binary"><code>--no-binary</code></a></dt><dd><p>Don't install pre-built wheels.</p>
<p>The given packages will be built and installed from source. The resolver will still use pre-built wheels to extract package metadata, if available.</p>
<p>May also be set with the <code>UV_NO_BINARY</code> environment variable.</p></dd><dt id="uv-version--no-binary-package"><a href="#uv-version--no-binary-package"><code>--no-binary-package</code></a> <i>no-binary-package</i></dt><dd><p>Don't install pre-built wheels for a specific package</p>
<p>May also be set with the <code>UV_NO_BINARY_PACKAGE</code> environment variable.</p></dd><dt id="uv-version--no-build"><a href="#uv-version--no-build"><code>--no-build</code></a></dt><dd><p>Don't build source distributions.</p>
<p>When enabled, resolving will not run arbitrary Python code. The cached wheels of already-built source distributions will be reused, but operations that require building distributions will exit with an error.</p>
<p>May also be set with the <code>UV_NO_BUILD</code> environment variable.</p></dd><dt id="uv-version--no-build-isolation"><a href="#uv-version--no-build-isolation"><code>--no-build-isolation</code></a></dt><dd><p>Disable isolation when building source distributions.</p>
<p>Assumes that build dependencies specified by PEP 518 are already installed.</p>
<p>May also be set with the <code>UV_NO_BUILD_ISOLATION</code> environment variable.</p></dd><dt id="uv-version--no-build-isolation-package"><a href="#uv-version--no-build-isolation-package"><code>--no-build-isolation-package</code></a> <i>no-build-isolation-package</i></dt><dd><p>Disable isolation when building source distributions for a specific package.</p>
<p>Assumes that the packages' build dependencies specified by PEP 518 are already installed.</p>
</dd><dt id="uv-version--no-build-package"><a href="#uv-version--no-build-package"><code>--no-build-package</code></a> <i>no-build-package</i></dt><dd><p>Don't build source distributions for a specific package</p>
<p>May also be set with the <code>UV_NO_BUILD_PACKAGE</code> environment variable.</p></dd><dt id="uv-version--no-cache"><a href="#uv-version--no-cache"><code>--no-cache</code></a>, <code>--no-cache-dir</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>
<p>May also be set with the <code>UV_NO_CACHE</code> environment variable.</p></dd><dt id="uv-version--no-config"><a href="#uv-version--no-config"><code>--no-config</code></a></dt><dd><p>Avoid discovering configuration files (<code>pyproject.toml</code>, <code>uv.toml</code>).</p>
<p>Normally, configuration files are discovered in the current directory, parent directories, or user configuration directories.</p>
<p>May also be set with the <code>UV_NO_CONFIG</code> environment variable.</p></dd><dt id="uv-version--no-index"><a href="#uv-version--no-index"><code>--no-index</code></a></dt><dd><p>Ignore the registry index (e.g., PyPI), instead relying on direct URL dependencies and those provided via <code>--find-links</code></p>
</dd><dt id="uv-version--no-managed-python"><a href="#uv-version--no-managed-python"><code>--no-managed-python</code></a></dt><dd><p>Disable use of uv-managed Python versions.</p>
<p>Instead, uv will search for a suitable Python version on the system.</p>
<p>May also be set with the <code>UV_NO_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-version--no-progress"><a href="#uv-version--no-progress"><code>--no-progress</code></a></dt><dd><p>Hide all progress outputs.</p>
<p>For example, spinners or progress bars.</p>
<p>May also be set with the <code>UV_NO_PROGRESS</code> environment variable.</p></dd><dt id="uv-version--no-python-downloads"><a href="#uv-version--no-python-downloads"><code>--no-python-downloads</code></a></dt><dd><p>Disable automatic downloads of Python.</p>
</dd><dt id="uv-version--no-sources"><a href="#uv-version--no-sources"><code>--no-sources</code></a></dt><dd><p>Ignore the <code>tool.uv.sources</code> table when resolving dependencies. Used to lock against the standards-compliant, publishable package metadata, as opposed to using any workspace, Git, URL, or local path sources</p>
</dd><dt id="uv-version--no-sync"><a href="#uv-version--no-sync"><code>--no-sync</code></a></dt><dd><p>Avoid syncing the virtual environment after re-locking the project</p>
<p>May also be set with the <code>UV_NO_SYNC</code> environment variable.</p></dd><dt id="uv-version--offline"><a href="#uv-version--offline"><code>--offline</code></a></dt><dd><p>Disable network access.</p>
<p>When disabled, uv will only use locally cached data and locally available files.</p>
<p>May also be set with the <code>UV_OFFLINE</code> environment variable.</p></dd><dt id="uv-version--output-format"><a href="#uv-version--output-format"><code>--output-format</code></a> <i>output-format</i></dt><dd><p>The format of the output</p>
<p>[default: text]</p><p>Possible values:</p>
<ul>
<li><code>text</code>:  Display the version as plain text</li>
<li><code>json</code>:  Display the version as JSON</li>
</ul></dd><dt id="uv-version--package"><a href="#uv-version--package"><code>--package</code></a> <i>package</i></dt><dd><p>Update the version of a specific package in the workspace</p>
</dd><dt id="uv-version--prerelease"><a href="#uv-version--prerelease"><code>--prerelease</code></a> <i>prerelease</i></dt><dd><p>The strategy to use when considering pre-release versions.</p>
<p>By default, uv will accept pre-releases for packages that <em>only</em> publish pre-releases, along with first-party requirements that contain an explicit pre-release marker in the declared specifiers (<code>if-necessary-or-explicit</code>).</p>
<p>May also be set with the <code>UV_PRERELEASE</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>disallow</code>:  Disallow all pre-release versions</li>
<li><code>allow</code>:  Allow all pre-release versions</li>
<li><code>if-necessary</code>:  Allow pre-release versions if all versions of a package are pre-release</li>
<li><code>explicit</code>:  Allow pre-release versions for first-party packages with explicit pre-release markers in their version requirements</li>
<li><code>if-necessary-or-explicit</code>:  Allow pre-release versions if all versions of a package are pre-release, or if the package has an explicit pre-release marker in its version requirements</li>
</ul></dd><dt id="uv-version--project"><a href="#uv-version--project"><code>--project</code></a> <i>project</i></dt><dd><p>Run the command within the given project directory.</p>
<p>All <code>pyproject.toml</code>, <code>uv.toml</code>, and <code>.python-version</code> files will be discovered by walking up the directory tree from the project root, as will the project's virtual environment (<code>.venv</code>).</p>
<p>Other command-line arguments (such as relative paths) will be resolved relative to the current working directory.</p>
<p>See <code>--directory</code> to change the working directory entirely.</p>
<p>This setting has no effect when used in the <code>uv pip</code> interface.</p>
<p>May also be set with the <code>UV_PROJECT</code> environment variable.</p></dd><dt id="uv-version--python"><a href="#uv-version--python"><code>--python</code></a>, <code>-p</code> <i>python</i></dt><dd><p>The Python interpreter to use for resolving and syncing.</p>
<p>See <a href="#uv-python">uv python</a> for details on Python discovery and supported request formats.</p>
<p>May also be set with the <code>UV_PYTHON</code> environment variable.</p></dd><dt id="uv-version--quiet"><a href="#uv-version--quiet"><code>--quiet</code></a>, <code>-q</code></dt><dd><p>Use quiet output.</p>
<p>Repeating this option, e.g., <code>-qq</code>, will enable a silent mode in which uv will write no output to stdout.</p>
</dd><dt id="uv-version--refresh"><a href="#uv-version--refresh"><code>--refresh</code></a></dt><dd><p>Refresh all cached data</p>
</dd><dt id="uv-version--refresh-package"><a href="#uv-version--refresh-package"><code>--refresh-package</code></a> <i>refresh-package</i></dt><dd><p>Refresh cached data for a specific package</p>
</dd><dt id="uv-version--reinstall"><a href="#uv-version--reinstall"><code>--reinstall</code></a>, <code>--force-reinstall</code></dt><dd><p>Reinstall all packages, regardless of whether they're already installed. Implies <code>--refresh</code></p>
</dd><dt id="uv-version--reinstall-package"><a href="#uv-version--reinstall-package"><code>--reinstall-package</code></a> <i>reinstall-package</i></dt><dd><p>Reinstall a specific package, regardless of whether it's already installed. Implies <code>--refresh-package</code></p>
</dd><dt id="uv-version--resolution"><a href="#uv-version--resolution"><code>--resolution</code></a> <i>resolution</i></dt><dd><p>The strategy to use when selecting between the different compatible versions for a given package requirement.</p>
<p>By default, uv will use the latest compatible version of each package (<code>highest</code>).</p>
<p>May also be set with the <code>UV_RESOLUTION</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>highest</code>:  Resolve the highest compatible version of each package</li>
<li><code>lowest</code>:  Resolve the lowest compatible version of each package</li>
<li><code>lowest-direct</code>:  Resolve the lowest compatible version of any direct dependencies, and the highest compatible version of any transitive dependencies</li>
</ul></dd><dt id="uv-version--short"><a href="#uv-version--short"><code>--short</code></a></dt><dd><p>Only show the version</p>
<p>By default, uv will show the project name before the version.</p>
</dd><dt id="uv-version--upgrade"><a href="#uv-version--upgrade"><code>--upgrade</code></a>, <code>-U</code></dt><dd><p>Allow package upgrades, ignoring pinned versions in any existing output file. Implies <code>--refresh</code></p>
</dd><dt id="uv-version--upgrade-package"><a href="#uv-version--upgrade-package"><code>--upgrade-package</code></a>, <code>-P</code> <i>upgrade-package</i></dt><dd><p>Allow upgrades for a specific package, ignoring pinned versions in any existing output file. Implies <code>--refresh-package</code></p>
</dd><dt id="uv-version--verbose"><a href="#uv-version--verbose"><code>--verbose</code></a>, <code>-v</code></dt><dd><p>Use verbose output.</p>
<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (<a href="https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives">https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives</a>)</p>
</dd></dl>

## uv sync

Update the project's environment.

Syncing ensures that all project dependencies are installed and up-to-date with the lockfile.

By default, an exact sync is performed: uv removes packages that are not declared as dependencies of the project. Use the `--inexact` flag to keep extraneous packages. Note that if an extraneous package conflicts with a project dependency, it will still be removed. Additionally, if `--no-build-isolation` is used, uv will not remove extraneous packages to avoid removing possible build dependencies.

If the project virtual environment (`.venv`) does not exist, it will be created.

The project is re-locked before syncing unless the `--locked` or `--frozen` flag is provided.

uv will search for a project in the current directory or any parent directory. If a project cannot be found, uv will exit with an error.

Note that, when installing from a lockfile, uv will not provide warnings for yanked package versions.

<h3 class="cli-reference">Usage</h3>

```
uv sync [OPTIONS]
```

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt id="uv-sync--active"><a href="#uv-sync--active"><code>--active</code></a></dt><dd><p>Sync dependencies to the active virtual environment.</p>
<p>Instead of creating or updating the virtual environment for the project or script, the active virtual environment will be preferred, if the <code>VIRTUAL_ENV</code> environment variable is set.</p>
</dd><dt id="uv-sync--all-extras"><a href="#uv-sync--all-extras"><code>--all-extras</code></a></dt><dd><p>Include all optional dependencies.</p>
<p>When two or more extras are declared as conflicting in <code>tool.uv.conflicts</code>, using this flag will always result in an error.</p>
<p>Note that all optional dependencies are always included in the resolution; this option only affects the selection of packages to install.</p>
</dd><dt id="uv-sync--all-groups"><a href="#uv-sync--all-groups"><code>--all-groups</code></a></dt><dd><p>Include dependencies from all dependency groups.</p>
<p><code>--no-group</code> can be used to exclude specific groups.</p>
</dd><dt id="uv-sync--all-packages"><a href="#uv-sync--all-packages"><code>--all-packages</code></a></dt><dd><p>Sync all packages in the workspace.</p>
<p>The workspace's environment (<code>.venv</code>) is updated to include all workspace members.</p>
<p>Any extras or groups specified via <code>--extra</code>, <code>--group</code>, or related options will be applied to all workspace members.</p>
</dd><dt id="uv-sync--allow-insecure-host"><a href="#uv-sync--allow-insecure-host"><code>--allow-insecure-host</code></a>, <code>--trusted-host</code> <i>allow-insecure-host</i></dt><dd><p>Allow insecure connections to a host.</p>
<p>Can be provided multiple times.</p>
<p>Expects to receive either a hostname (e.g., <code>localhost</code>), a host-port pair (e.g., <code>localhost:8080</code>), or a URL (e.g., <code>https://localhost</code>).</p>
<p>WARNING: Hosts included in this list will not be verified against the system's certificate store. Only use <code>--allow-insecure-host</code> in a secure network with verified sources, as it bypasses SSL verification and could expose you to MITM attacks.</p>
<p>May also be set with the <code>UV_INSECURE_HOST</code> environment variable.</p></dd><dt id="uv-sync--cache-dir"><a href="#uv-sync--cache-dir"><code>--cache-dir</code></a> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>
<p>Defaults to <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on macOS and Linux, and <code>%LOCALAPPDATA%\uv\cache</code> on Windows.</p>
<p>To view the location of the cache directory, run <code>uv cache dir</code>.</p>
<p>May also be set with the <code>UV_CACHE_DIR</code> environment variable.</p></dd><dt id="uv-sync--check"><a href="#uv-sync--check"><code>--check</code></a></dt><dd><p>Check if the Python environment is synchronized with the project.</p>
<p>If the environment is not up to date, uv will exit with an error.</p>
</dd><dt id="uv-sync--color"><a href="#uv-sync--color"><code>--color</code></a> <i>color-choice</i></dt><dd><p>Control the use of color in output.</p>
<p>By default, uv will automatically detect support for colors when writing to a terminal.</p>
<p>Possible values:</p>
<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>
<li><code>always</code>:  Enables colored output regardless of the detected environment</li>
<li><code>never</code>:  Disables colored output</li>
</ul></dd><dt id="uv-sync--compile-bytecode"><a href="#uv-sync--compile-bytecode"><code>--compile-bytecode</code></a>, <code>--compile</code></dt><dd><p>Compile Python files to bytecode after installation.</p>
<p>By default, uv does not compile Python (<code>.py</code>) files to bytecode (<code>__pycache__/*.pyc</code>); instead, compilation is performed lazily the first time a module is imported. For use-cases in which start time is critical, such as CLI applications and Docker containers, this option can be enabled to trade longer installation times for faster start times.</p>
<p>When enabled, uv will process the entire site-packages directory (including packages that are not being modified by the current operation) for consistency. Like pip, it will also ignore errors.</p>
<p>May also be set with the <code>UV_COMPILE_BYTECODE</code> environment variable.</p></dd><dt id="uv-sync--config-file"><a href="#uv-sync--config-file"><code>--config-file</code></a> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>
<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>
<p>May also be set with the <code>UV_CONFIG_FILE</code> environment variable.</p></dd><dt id="uv-sync--config-setting"><a href="#uv-sync--config-setting"><code>--config-setting</code></a>, <code>--config-settings</code>, <code>-C</code> <i>config-setting</i></dt><dd><p>Settings to pass to the PEP 517 build backend, specified as <code>KEY=VALUE</code> pairs</p>
</dd><dt id="uv-sync--default-index"><a href="#uv-sync--default-index"><code>--default-index</code></a> <i>default-index</i></dt><dd><p>The URL of the default package index (by default: <a href="https://pypi.org/simple">https://pypi.org/simple</a>).</p>
<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>
<p>The index given by this flag is given lower priority than all other indexes specified via the <code>--index</code> flag.</p>
<p>May also be set with the <code>UV_DEFAULT_INDEX</code> environment variable.</p></dd><dt id="uv-sync--directory"><a href="#uv-sync--directory"><code>--directory</code></a> <i>directory</i></dt><dd><p>Change to the given directory prior to running the command.</p>
<p>Relative paths are resolved with the given directory as the base.</p>
<p>See <code>--project</code> to only change the project root directory.</p>
</dd><dt id="uv-sync--dry-run"><a href="#uv-sync--dry-run"><code>--dry-run</code></a></dt><dd><p>Perform a dry run, without writing the lockfile or modifying the project environment.</p>
<p>In dry-run mode, uv will resolve the project's dependencies and report on the resulting changes to both the lockfile and the project environment, but will not modify either.</p>
</dd><dt id="uv-sync--exclude-newer"><a href="#uv-sync--exclude-newer"><code>--exclude-newer</code></a> <i>exclude-newer</i></dt><dd><p>Limit candidate packages to those that were uploaded prior to the given date.</p>
<p>Accepts both RFC 3339 timestamps (e.g., <code>2006-12-02T02:07:43Z</code>) and local dates in the same format (e.g., <code>2006-12-02</code>) in your system's configured time zone.</p>
<p>May also be set with the <code>UV_EXCLUDE_NEWER</code> environment variable.</p></dd><dt id="uv-sync--extra"><a href="#uv-sync--extra"><code>--extra</code></a> <i>extra</i></dt><dd><p>Include optional dependencies from the specified extra name.</p>
<p>May be provided more than once.</p>
<p>When multiple extras or groups are specified that appear in <code>tool.uv.conflicts</code>, uv will report an error.</p>
<p>Note that all optional dependencies are always included in the resolution; this option only affects the selection of packages to install.</p>
</dd><dt id="uv-sync--extra-index-url"><a href="#uv-sync--extra-index-url"><code>--extra-index-url</code></a> <i>extra-index-url</i></dt><dd><p>(Deprecated: use <code>--index</code> instead) Extra URLs of package indexes to use, in addition to <code>--index-url</code>.</p>
<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>
<p>All indexes provided via this flag take priority over the index specified by <code>--index-url</code> (which defaults to PyPI). When multiple <code>--extra-index-url</code> flags are provided, earlier values take priority.</p>
<p>May also be set with the <code>UV_EXTRA_INDEX_URL</code> environment variable.</p></dd><dt id="uv-sync--find-links"><a href="#uv-sync--find-links"><code>--find-links</code></a>, <code>-f</code> <i>find-links</i></dt><dd><p>Locations to search for candidate distributions, in addition to those found in the registry indexes.</p>
<p>If a path, the target must be a directory that contains packages as wheel files (<code>.whl</code>) or source distributions (e.g., <code>.tar.gz</code> or <code>.zip</code>) at the top level.</p>
<p>If a URL, the page must contain a flat list of links to package files adhering to the formats described above.</p>
<p>May also be set with the <code>UV_FIND_LINKS</code> environment variable.</p></dd><dt id="uv-sync--fork-strategy"><a href="#uv-sync--fork-strategy"><code>--fork-strategy</code></a> <i>fork-strategy</i></dt><dd><p>The strategy to use when selecting multiple versions of a given package across Python versions and platforms.</p>
<p>By default, uv will optimize for selecting the latest version of each package for each supported Python version (<code>requires-python</code>), while minimizing the number of selected versions across platforms.</p>
<p>Under <code>fewest</code>, uv will minimize the number of selected versions for each package, preferring older versions that are compatible with a wider range of supported Python versions or platforms.</p>
<p>May also be set with the <code>UV_FORK_STRATEGY</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>fewest</code>:  Optimize for selecting the fewest number of versions for each package. Older versions may be preferred if they are compatible with a wider range of supported Python versions or platforms</li>
<li><code>requires-python</code>:  Optimize for selecting latest supported version of each package, for each supported Python version</li>
</ul></dd><dt id="uv-sync--frozen"><a href="#uv-sync--frozen"><code>--frozen</code></a></dt><dd><p>Sync without updating the <code>uv.lock</code> file.</p>
<p>Instead of checking if the lockfile is up-to-date, uses the versions in the lockfile as the source of truth. If the lockfile is missing, uv will exit with an error. If the <code>pyproject.toml</code> includes changes to dependencies that have not been included in the lockfile yet, they will not be present in the environment.</p>
<p>May also be set with the <code>UV_FROZEN</code> environment variable.</p></dd><dt id="uv-sync--group"><a href="#uv-sync--group"><code>--group</code></a> <i>group</i></dt><dd><p>Include dependencies from the specified dependency group.</p>
<p>When multiple extras or groups are specified that appear in <code>tool.uv.conflicts</code>, uv will report an error.</p>
<p>May be provided multiple times.</p>
</dd><dt id="uv-sync--help"><a href="#uv-sync--help"><code>--help</code></a>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>
</dd><dt id="uv-sync--index"><a href="#uv-sync--index"><code>--index</code></a> <i>index</i></dt><dd><p>The URLs to use when resolving dependencies, in addition to the default index.</p>
<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>
<p>All indexes provided via this flag take priority over the index specified by <code>--default-index</code> (which defaults to PyPI). When multiple <code>--index</code> flags are provided, earlier values take priority.</p>
<p>May also be set with the <code>UV_INDEX</code> environment variable.</p></dd><dt id="uv-sync--index-strategy"><a href="#uv-sync--index-strategy"><code>--index-strategy</code></a> <i>index-strategy</i></dt><dd><p>The strategy to use when resolving against multiple index URLs.</p>
<p>By default, uv will stop at the first index on which a given package is available, and limit resolutions to those present on that first index (<code>first-index</code>). This prevents &quot;dependency confusion&quot; attacks, whereby an attacker can upload a malicious package under the same name to an alternate index.</p>
<p>May also be set with the <code>UV_INDEX_STRATEGY</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>first-index</code>:  Only use results from the first index that returns a match for a given package name</li>
<li><code>unsafe-first-match</code>:  Search for every package name across all indexes, exhausting the versions from the first index before moving on to the next</li>
<li><code>unsafe-best-match</code>:  Search for every package name across all indexes, preferring the &quot;best&quot; version found. If a package version is in multiple indexes, only look at the entry for the first index</li>
</ul></dd><dt id="uv-sync--index-url"><a href="#uv-sync--index-url"><code>--index-url</code></a>, <code>-i</code> <i>index-url</i></dt><dd><p>(Deprecated: use <code>--default-index</code> instead) The URL of the Python package index (by default: <a href="https://pypi.org/simple">https://pypi.org/simple</a>).</p>
<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>
<p>The index given by this flag is given lower priority than all other indexes specified via the <code>--extra-index-url</code> flag.</p>
<p>May also be set with the <code>UV_INDEX_URL</code> environment variable.</p></dd><dt id="uv-sync--inexact"><a href="#uv-sync--inexact"><code>--inexact</code></a>, <code>--no-exact</code></dt><dd><p>Do not remove extraneous packages present in the environment.</p>
<p>When enabled, uv will make the minimum necessary changes to satisfy the requirements. By default, syncing will remove any extraneous packages from the environment</p>
</dd><dt id="uv-sync--keyring-provider"><a href="#uv-sync--keyring-provider"><code>--keyring-provider</code></a> <i>keyring-provider</i></dt><dd><p>Attempt to use <code>keyring</code> for authentication for index URLs.</p>
<p>At present, only <code>--keyring-provider subprocess</code> is supported, which configures uv to use the <code>keyring</code> CLI to handle authentication.</p>
<p>Defaults to <code>disabled</code>.</p>
<p>May also be set with the <code>UV_KEYRING_PROVIDER</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>disabled</code>:  Do not use keyring for credential lookup</li>
<li><code>subprocess</code>:  Use the <code>keyring</code> command for credential lookup</li>
</ul></dd><dt id="uv-sync--link-mode"><a href="#uv-sync--link-mode"><code>--link-mode</code></a> <i>link-mode</i></dt><dd><p>The method to use when installing packages from the global cache.</p>
<p>Defaults to <code>clone</code> (also known as Copy-on-Write) on macOS, and <code>hardlink</code> on Linux and Windows.</p>
<p>May also be set with the <code>UV_LINK_MODE</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>clone</code>:  Clone (i.e., copy-on-write) packages from the wheel into the <code>site-packages</code> directory</li>
<li><code>copy</code>:  Copy packages from the wheel into the <code>site-packages</code> directory</li>
<li><code>hardlink</code>:  Hard link packages from the wheel into the <code>site-packages</code> directory</li>
<li><code>symlink</code>:  Symbolically link packages from the wheel into the <code>site-packages</code> directory</li>
</ul></dd><dt id="uv-sync--locked"><a href="#uv-sync--locked"><code>--locked</code></a></dt><dd><p>Assert that the <code>uv.lock</code> will remain unchanged.</p>
<p>Requires that the lockfile is up-to-date. If the lockfile is missing or needs to be updated, uv will exit with an error.</p>
<p>May also be set with the <code>UV_LOCKED</code> environment variable.</p></dd><dt id="uv-sync--managed-python"><a href="#uv-sync--managed-python"><code>--managed-python</code></a></dt><dd><p>Require use of uv-managed Python versions.</p>
<p>By default, uv prefers using Python versions it manages. However, it will use system Python versions if a uv-managed Python is not installed. This option disables use of system Python versions.</p>
<p>May also be set with the <code>UV_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-sync--native-tls"><a href="#uv-sync--native-tls"><code>--native-tls</code></a></dt><dd><p>Whether to load TLS certificates from the platform's native certificate store.</p>
<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>
<p>However, in some cases, you may want to use the platform's native certificate store, especially if you're relying on a corporate trust root (e.g., for a mandatory proxy) that's included in your system's certificate store.</p>
<p>May also be set with the <code>UV_NATIVE_TLS</code> environment variable.</p></dd><dt id="uv-sync--no-binary"><a href="#uv-sync--no-binary"><code>--no-binary</code></a></dt><dd><p>Don't install pre-built wheels.</p>
<p>The given packages will be built and installed from source. The resolver will still use pre-built wheels to extract package metadata, if available.</p>
<p>May also be set with the <code>UV_NO_BINARY</code> environment variable.</p></dd><dt id="uv-sync--no-binary-package"><a href="#uv-sync--no-binary-package"><code>--no-binary-package</code></a> <i>no-binary-package</i></dt><dd><p>Don't install pre-built wheels for a specific package</p>
<p>May also be set with the <code>UV_NO_BINARY_PACKAGE</code> environment variable.</p></dd><dt id="uv-sync--no-build"><a href="#uv-sync--no-build"><code>--no-build</code></a></dt><dd><p>Don't build source distributions.</p>
<p>When enabled, resolving will not run arbitrary Python code. The cached wheels of already-built source distributions will be reused, but operations that require building distributions will exit with an error.</p>
<p>May also be set with the <code>UV_NO_BUILD</code> environment variable.</p></dd><dt id="uv-sync--no-build-isolation"><a href="#uv-sync--no-build-isolation"><code>--no-build-isolation</code></a></dt><dd><p>Disable isolation when building source distributions.</p>
<p>Assumes that build dependencies specified by PEP 518 are already installed.</p>
<p>May also be set with the <code>UV_NO_BUILD_ISOLATION</code> environment variable.</p></dd><dt id="uv-sync--no-build-isolation-package"><a href="#uv-sync--no-build-isolation-package"><code>--no-build-isolation-package</code></a> <i>no-build-isolation-package</i></dt><dd><p>Disable isolation when building source distributions for a specific package.</p>
<p>Assumes that the packages' build dependencies specified by PEP 518 are already installed.</p>
</dd><dt id="uv-sync--no-build-package"><a href="#uv-sync--no-build-package"><code>--no-build-package</code></a> <i>no-build-package</i></dt><dd><p>Don't build source distributions for a specific package</p>
<p>May also be set with the <code>UV_NO_BUILD_PACKAGE</code> environment variable.</p></dd><dt id="uv-sync--no-cache"><a href="#uv-sync--no-cache"><code>--no-cache</code></a>, <code>--no-cache-dir</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>
<p>May also be set with the <code>UV_NO_CACHE</code> environment variable.</p></dd><dt id="uv-sync--no-config"><a href="#uv-sync--no-config"><code>--no-config</code></a></dt><dd><p>Avoid discovering configuration files (<code>pyproject.toml</code>, <code>uv.toml</code>).</p>
<p>Normally, configuration files are discovered in the current directory, parent directories, or user configuration directories.</p>
<p>May also be set with the <code>UV_NO_CONFIG</code> environment variable.</p></dd><dt id="uv-sync--no-default-groups"><a href="#uv-sync--no-default-groups"><code>--no-default-groups</code></a></dt><dd><p>Ignore the default dependency groups.</p>
<p>uv includes the groups defined in <code>tool.uv.default-groups</code> by default. This disables that option, however, specific groups can still be included with <code>--group</code>.</p>
</dd><dt id="uv-sync--no-dev"><a href="#uv-sync--no-dev"><code>--no-dev</code></a></dt><dd><p>Disable the development dependency group.</p>
<p>This option is an alias of <code>--no-group dev</code>. See <code>--no-default-groups</code> to disable all default groups instead.</p>
</dd><dt id="uv-sync--no-editable"><a href="#uv-sync--no-editable"><code>--no-editable</code></a></dt><dd><p>Install any editable dependencies, including the project and any workspace members, as non-editable</p>
<p>May also be set with the <code>UV_NO_EDITABLE</code> environment variable.</p></dd><dt id="uv-sync--no-extra"><a href="#uv-sync--no-extra"><code>--no-extra</code></a> <i>no-extra</i></dt><dd><p>Exclude the specified optional dependencies, if <code>--all-extras</code> is supplied.</p>
<p>May be provided multiple times.</p>
</dd><dt id="uv-sync--no-group"><a href="#uv-sync--no-group"><code>--no-group</code></a> <i>no-group</i></dt><dd><p>Disable the specified dependency group.</p>
<p>This option always takes precedence over default groups, <code>--all-groups</code>, and <code>--group</code>.</p>
<p>May be provided multiple times.</p>
</dd><dt id="uv-sync--no-index"><a href="#uv-sync--no-index"><code>--no-index</code></a></dt><dd><p>Ignore the registry index (e.g., PyPI), instead relying on direct URL dependencies and those provided via <code>--find-links</code></p>
</dd><dt id="uv-sync--no-install-package"><a href="#uv-sync--no-install-package"><code>--no-install-package</code></a> <i>no-install-package</i></dt><dd><p>Do not install the given package(s).</p>
<p>By default, all of the project's dependencies are installed into the environment. The <code>--no-install-package</code> option allows exclusion of specific packages. Note this can result in a broken environment, and should be used with caution.</p>
</dd><dt id="uv-sync--no-install-project"><a href="#uv-sync--no-install-project"><code>--no-install-project</code></a></dt><dd><p>Do not install the current project.</p>
<p>By default, the current project is installed into the environment with all of its dependencies. The <code>--no-install-project</code> option allows the project to be excluded, but all of its dependencies are still installed. This is particularly useful in situations like building Docker images where installing the project separately from its dependencies allows optimal layer caching.</p>
</dd><dt id="uv-sync--no-install-workspace"><a href="#uv-sync--no-install-workspace"><code>--no-install-workspace</code></a></dt><dd><p>Do not install any workspace members, including the root project.</p>
<p>By default, all of the workspace members and their dependencies are installed into the environment. The <code>--no-install-workspace</code> option allows exclusion of all the workspace members while retaining their dependencies. This is particularly useful in situations like building Docker images where installing the workspace separately from its dependencies allows optimal layer caching.</p>
</dd><dt id="uv-sync--no-managed-python"><a href="#uv-sync--no-managed-python"><code>--no-managed-python</code></a></dt><dd><p>Disable use of uv-managed Python versions.</p>
<p>Instead, uv will search for a suitable Python version on the system.</p>
<p>May also be set with the <code>UV_NO_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-sync--no-progress"><a href="#uv-sync--no-progress"><code>--no-progress</code></a></dt><dd><p>Hide all progress outputs.</p>
<p>For example, spinners or progress bars.</p>
<p>May also be set with the <code>UV_NO_PROGRESS</code> environment variable.</p></dd><dt id="uv-sync--no-python-downloads"><a href="#uv-sync--no-python-downloads"><code>--no-python-downloads</code></a></dt><dd><p>Disable automatic downloads of Python.</p>
</dd><dt id="uv-sync--no-sources"><a href="#uv-sync--no-sources"><code>--no-sources</code></a></dt><dd><p>Ignore the <code>tool.uv.sources</code> table when resolving dependencies. Used to lock against the standards-compliant, publishable package metadata, as opposed to using any workspace, Git, URL, or local path sources</p>
</dd><dt id="uv-sync--offline"><a href="#uv-sync--offline"><code>--offline</code></a></dt><dd><p>Disable network access.</p>
<p>When disabled, uv will only use locally cached data and locally available files.</p>
<p>May also be set with the <code>UV_OFFLINE</code> environment variable.</p></dd><dt id="uv-sync--only-dev"><a href="#uv-sync--only-dev"><code>--only-dev</code></a></dt><dd><p>Only include the development dependency group.</p>
<p>The project and its dependencies will be omitted.</p>
<p>This option is an alias for <code>--only-group dev</code>. Implies <code>--no-default-groups</code>.</p>
</dd><dt id="uv-sync--only-group"><a href="#uv-sync--only-group"><code>--only-group</code></a> <i>only-group</i></dt><dd><p>Only include dependencies from the specified dependency group.</p>
<p>The project and its dependencies will be omitted.</p>
<p>May be provided multiple times. Implies <code>--no-default-groups</code>.</p>
</dd><dt id="uv-sync--package"><a href="#uv-sync--package"><code>--package</code></a> <i>package</i></dt><dd><p>Sync for a specific package in the workspace.</p>
<p>The workspace's environment (<code>.venv</code>) is updated to reflect the subset of dependencies declared by the specified workspace member package.</p>
<p>If the workspace member does not exist, uv will exit with an error.</p>
</dd><dt id="uv-sync--prerelease"><a href="#uv-sync--prerelease"><code>--prerelease</code></a> <i>prerelease</i></dt><dd><p>The strategy to use when considering pre-release versions.</p>
<p>By default, uv will accept pre-releases for packages that <em>only</em> publish pre-releases, along with first-party requirements that contain an explicit pre-release marker in the declared specifiers (<code>if-necessary-or-explicit</code>).</p>
<p>May also be set with the <code>UV_PRERELEASE</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>disallow</code>:  Disallow all pre-release versions</li>
<li><code>allow</code>:  Allow all pre-release versions</li>
<li><code>if-necessary</code>:  Allow pre-release versions if all versions of a package are pre-release</li>
<li><code>explicit</code>:  Allow pre-release versions for first-party packages with explicit pre-release markers in their version requirements</li>
<li><code>if-necessary-or-explicit</code>:  Allow pre-release versions if all versions of a package are pre-release, or if the package has an explicit pre-release marker in its version requirements</li>
</ul></dd><dt id="uv-sync--project"><a href="#uv-sync--project"><code>--project</code></a> <i>project</i></dt><dd><p>Run the command within the given project directory.</p>
<p>All <code>pyproject.toml</code>, <code>uv.toml</code>, and <code>.python-version</code> files will be discovered by walking up the directory tree from the project root, as will the project's virtual environment (<code>.venv</code>).</p>
<p>Other command-line arguments (such as relative paths) will be resolved relative to the current working directory.</p>
<p>See <code>--directory</code> to change the working directory entirely.</p>
<p>This setting has no effect when used in the <code>uv pip</code> interface.</p>
<p>May also be set with the <code>UV_PROJECT</code> environment variable.</p></dd><dt id="uv-sync--python"><a href="#uv-sync--python"><code>--python</code></a>, <code>-p</code> <i>python</i></dt><dd><p>The Python interpreter to use for the project environment.</p>
<p>By default, the first interpreter that meets the project's <code>requires-python</code> constraint is
used.</p>
<p>If a Python interpreter in a virtual environment is provided, the packages will not be
synced to the given environment. The interpreter will be used to create a virtual
environment in the project.</p>
<p>See <a href="#uv-python">uv python</a> for details on Python discovery and supported request formats.</p>
<p>May also be set with the <code>UV_PYTHON</code> environment variable.</p></dd><dt id="uv-sync--quiet"><a href="#uv-sync--quiet"><code>--quiet</code></a>, <code>-q</code></dt><dd><p>Use quiet output.</p>
<p>Repeating this option, e.g., <code>-qq</code>, will enable a silent mode in which uv will write no output to stdout.</p>
</dd><dt id="uv-sync--refresh"><a href="#uv-sync--refresh"><code>--refresh</code></a></dt><dd><p>Refresh all cached data</p>
</dd><dt id="uv-sync--refresh-package"><a href="#uv-sync--refresh-package"><code>--refresh-package</code></a> <i>refresh-package</i></dt><dd><p>Refresh cached data for a specific package</p>
</dd><dt id="uv-sync--reinstall"><a href="#uv-sync--reinstall"><code>--reinstall</code></a>, <code>--force-reinstall</code></dt><dd><p>Reinstall all packages, regardless of whether they're already installed. Implies <code>--refresh</code></p>
</dd><dt id="uv-sync--reinstall-package"><a href="#uv-sync--reinstall-package"><code>--reinstall-package</code></a> <i>reinstall-package</i></dt><dd><p>Reinstall a specific package, regardless of whether it's already installed. Implies <code>--refresh-package</code></p>
</dd><dt id="uv-sync--resolution"><a href="#uv-sync--resolution"><code>--resolution</code></a> <i>resolution</i></dt><dd><p>The strategy to use when selecting between the different compatible versions for a given package requirement.</p>
<p>By default, uv will use the latest compatible version of each package (<code>highest</code>).</p>
<p>May also be set with the <code>UV_RESOLUTION</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>highest</code>:  Resolve the highest compatible version of each package</li>
<li><code>lowest</code>:  Resolve the lowest compatible version of each package</li>
<li><code>lowest-direct</code>:  Resolve the lowest compatible version of any direct dependencies, and the highest compatible version of any transitive dependencies</li>
</ul></dd><dt id="uv-sync--script"><a href="#uv-sync--script"><code>--script</code></a> <i>script</i></dt><dd><p>Sync the environment for a Python script, rather than the current project.</p>
<p>If provided, uv will sync the dependencies based on the script's inline metadata table, in adherence with PEP 723.</p>
</dd><dt id="uv-sync--upgrade"><a href="#uv-sync--upgrade"><code>--upgrade</code></a>, <code>-U</code></dt><dd><p>Allow package upgrades, ignoring pinned versions in any existing output file. Implies <code>--refresh</code></p>
</dd><dt id="uv-sync--upgrade-package"><a href="#uv-sync--upgrade-package"><code>--upgrade-package</code></a>, <code>-P</code> <i>upgrade-package</i></dt><dd><p>Allow upgrades for a specific package, ignoring pinned versions in any existing output file. Implies <code>--refresh-package</code></p>
</dd><dt id="uv-sync--verbose"><a href="#uv-sync--verbose"><code>--verbose</code></a>, <code>-v</code></dt><dd><p>Use verbose output.</p>
<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (<a href="https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives">https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives</a>)</p>
</dd></dl>

## uv lock

Update the project's lockfile.

If the project lockfile (`uv.lock`) does not exist, it will be created. If a lockfile is present, its contents will be used as preferences for the resolution.

If there are no changes to the project's dependencies, locking will have no effect unless the `--upgrade` flag is provided.

<h3 class="cli-reference">Usage</h3>

```
uv lock [OPTIONS]
```

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt id="uv-lock--allow-insecure-host"><a href="#uv-lock--allow-insecure-host"><code>--allow-insecure-host</code></a>, <code>--trusted-host</code> <i>allow-insecure-host</i></dt><dd><p>Allow insecure connections to a host.</p>
<p>Can be provided multiple times.</p>
<p>Expects to receive either a hostname (e.g., <code>localhost</code>), a host-port pair (e.g., <code>localhost:8080</code>), or a URL (e.g., <code>https://localhost</code>).</p>
<p>WARNING: Hosts included in this list will not be verified against the system's certificate store. Only use <code>--allow-insecure-host</code> in a secure network with verified sources, as it bypasses SSL verification and could expose you to MITM attacks.</p>
<p>May also be set with the <code>UV_INSECURE_HOST</code> environment variable.</p></dd><dt id="uv-lock--cache-dir"><a href="#uv-lock--cache-dir"><code>--cache-dir</code></a> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>
<p>Defaults to <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on macOS and Linux, and <code>%LOCALAPPDATA%\uv\cache</code> on Windows.</p>
<p>To view the location of the cache directory, run <code>uv cache dir</code>.</p>
<p>May also be set with the <code>UV_CACHE_DIR</code> environment variable.</p></dd><dt id="uv-lock--check"><a href="#uv-lock--check"><code>--check</code></a>, <code>--locked</code></dt><dd><p>Check if the lockfile is up-to-date.</p>
<p>Asserts that the <code>uv.lock</code> would remain unchanged after a resolution. If the lockfile is missing or needs to be updated, uv will exit with an error.</p>
<p>Equivalent to <code>--locked</code>.</p>
<p>May also be set with the <code>UV_LOCKED</code> environment variable.</p></dd><dt id="uv-lock--check-exists"><a href="#uv-lock--check-exists"><code>--check-exists</code></a>, <code>--frozen</code></dt><dd><p>Assert that a <code>uv.lock</code> exists without checking if it is up-to-date.</p>
<p>Equivalent to <code>--frozen</code>.</p>
<p>May also be set with the <code>UV_FROZEN</code> environment variable.</p></dd><dt id="uv-lock--color"><a href="#uv-lock--color"><code>--color</code></a> <i>color-choice</i></dt><dd><p>Control the use of color in output.</p>
<p>By default, uv will automatically detect support for colors when writing to a terminal.</p>
<p>Possible values:</p>
<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>
<li><code>always</code>:  Enables colored output regardless of the detected environment</li>
<li><code>never</code>:  Disables colored output</li>
</ul></dd><dt id="uv-lock--config-file"><a href="#uv-lock--config-file"><code>--config-file</code></a> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>
<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>
<p>May also be set with the <code>UV_CONFIG_FILE</code> environment variable.</p></dd><dt id="uv-lock--config-setting"><a href="#uv-lock--config-setting"><code>--config-setting</code></a>, <code>--config-settings</code>, <code>-C</code> <i>config-setting</i></dt><dd><p>Settings to pass to the PEP 517 build backend, specified as <code>KEY=VALUE</code> pairs</p>
</dd><dt id="uv-lock--default-index"><a href="#uv-lock--default-index"><code>--default-index</code></a> <i>default-index</i></dt><dd><p>The URL of the default package index (by default: <a href="https://pypi.org/simple">https://pypi.org/simple</a>).</p>
<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>
<p>The index given by this flag is given lower priority than all other indexes specified via the <code>--index</code> flag.</p>
<p>May also be set with the <code>UV_DEFAULT_INDEX</code> environment variable.</p></dd><dt id="uv-lock--directory"><a href="#uv-lock--directory"><code>--directory</code></a> <i>directory</i></dt><dd><p>Change to the given directory prior to running the command.</p>
<p>Relative paths are resolved with the given directory as the base.</p>
<p>See <code>--project</code> to only change the project root directory.</p>
</dd><dt id="uv-lock--dry-run"><a href="#uv-lock--dry-run"><code>--dry-run</code></a></dt><dd><p>Perform a dry run, without writing the lockfile.</p>
<p>In dry-run mode, uv will resolve the project's dependencies and report on the resulting changes, but will not write the lockfile to disk.</p>
</dd><dt id="uv-lock--exclude-newer"><a href="#uv-lock--exclude-newer"><code>--exclude-newer</code></a> <i>exclude-newer</i></dt><dd><p>Limit candidate packages to those that were uploaded prior to the given date.</p>
<p>Accepts both RFC 3339 timestamps (e.g., <code>2006-12-02T02:07:43Z</code>) and local dates in the same format (e.g., <code>2006-12-02</code>) in your system's configured time zone.</p>
<p>May also be set with the <code>UV_EXCLUDE_NEWER</code> environment variable.</p></dd><dt id="uv-lock--extra-index-url"><a href="#uv-lock--extra-index-url"><code>--extra-index-url</code></a> <i>extra-index-url</i></dt><dd><p>(Deprecated: use <code>--index</code> instead) Extra URLs of package indexes to use, in addition to <code>--index-url</code>.</p>
<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>
<p>All indexes provided via this flag take priority over the index specified by <code>--index-url</code> (which defaults to PyPI). When multiple <code>--extra-index-url</code> flags are provided, earlier values take priority.</p>
<p>May also be set with the <code>UV_EXTRA_INDEX_URL</code> environment variable.</p></dd><dt id="uv-lock--find-links"><a href="#uv-lock--find-links"><code>--find-links</code></a>, <code>-f</code> <i>find-links</i></dt><dd><p>Locations to search for candidate distributions, in addition to those found in the registry indexes.</p>
<p>If a path, the target must be a directory that contains packages as wheel files (<code>.whl</code>) or source distributions (e.g., <code>.tar.gz</code> or <code>.zip</code>) at the top level.</p>
<p>If a URL, the page must contain a flat list of links to package files adhering to the formats described above.</p>
<p>May also be set with the <code>UV_FIND_LINKS</code> environment variable.</p></dd><dt id="uv-lock--fork-strategy"><a href="#uv-lock--fork-strategy"><code>--fork-strategy</code></a> <i>fork-strategy</i></dt><dd><p>The strategy to use when selecting multiple versions of a given package across Python versions and platforms.</p>
<p>By default, uv will optimize for selecting the latest version of each package for each supported Python version (<code>requires-python</code>), while minimizing the number of selected versions across platforms.</p>
<p>Under <code>fewest</code>, uv will minimize the number of selected versions for each package, preferring older versions that are compatible with a wider range of supported Python versions or platforms.</p>
<p>May also be set with the <code>UV_FORK_STRATEGY</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>fewest</code>:  Optimize for selecting the fewest number of versions for each package. Older versions may be preferred if they are compatible with a wider range of supported Python versions or platforms</li>
<li><code>requires-python</code>:  Optimize for selecting latest supported version of each package, for each supported Python version</li>
</ul></dd><dt id="uv-lock--help"><a href="#uv-lock--help"><code>--help</code></a>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>
</dd><dt id="uv-lock--index"><a href="#uv-lock--index"><code>--index</code></a> <i>index</i></dt><dd><p>The URLs to use when resolving dependencies, in addition to the default index.</p>
<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>
<p>All indexes provided via this flag take priority over the index specified by <code>--default-index</code> (which defaults to PyPI). When multiple <code>--index</code> flags are provided, earlier values take priority.</p>
<p>May also be set with the <code>UV_INDEX</code> environment variable.</p></dd><dt id="uv-lock--index-strategy"><a href="#uv-lock--index-strategy"><code>--index-strategy</code></a> <i>index-strategy</i></dt><dd><p>The strategy to use when resolving against multiple index URLs.</p>
<p>By default, uv will stop at the first index on which a given package is available, and limit resolutions to those present on that first index (<code>first-index</code>). This prevents &quot;dependency confusion&quot; attacks, whereby an attacker can upload a malicious package under the same name to an alternate index.</p>
<p>May also be set with the <code>UV_INDEX_STRATEGY</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>first-index</code>:  Only use results from the first index that returns a match for a given package name</li>
<li><code>unsafe-first-match</code>:  Search for every package name across all indexes, exhausting the versions from the first index before moving on to the next</li>
<li><code>unsafe-best-match</code>:  Search for every package name across all indexes, preferring the &quot;best&quot; version found. If a package version is in multiple indexes, only look at the entry for the first index</li>
</ul></dd><dt id="uv-lock--index-url"><a href="#uv-lock--index-url"><code>--index-url</code></a>, <code>-i</code> <i>index-url</i></dt><dd><p>(Deprecated: use <code>--default-index</code> instead) The URL of the Python package index (by default: <a href="https://pypi.org/simple">https://pypi.org/simple</a>).</p>
<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>
<p>The index given by this flag is given lower priority than all other indexes specified via the <code>--extra-index-url</code> flag.</p>
<p>May also be set with the <code>UV_INDEX_URL</code> environment variable.</p></dd><dt id="uv-lock--keyring-provider"><a href="#uv-lock--keyring-provider"><code>--keyring-provider</code></a> <i>keyring-provider</i></dt><dd><p>Attempt to use <code>keyring</code> for authentication for index URLs.</p>
<p>At present, only <code>--keyring-provider subprocess</code> is supported, which configures uv to use the <code>keyring</code> CLI to handle authentication.</p>
<p>Defaults to <code>disabled</code>.</p>
<p>May also be set with the <code>UV_KEYRING_PROVIDER</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>disabled</code>:  Do not use keyring for credential lookup</li>
<li><code>subprocess</code>:  Use the <code>keyring</code> command for credential lookup</li>
</ul></dd><dt id="uv-lock--link-mode"><a href="#uv-lock--link-mode"><code>--link-mode</code></a> <i>link-mode</i></dt><dd><p>The method to use when installing packages from the global cache.</p>
<p>This option is only used when building source distributions.</p>
<p>Defaults to <code>clone</code> (also known as Copy-on-Write) on macOS, and <code>hardlink</code> on Linux and Windows.</p>
<p>May also be set with the <code>UV_LINK_MODE</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>clone</code>:  Clone (i.e., copy-on-write) packages from the wheel into the <code>site-packages</code> directory</li>
<li><code>copy</code>:  Copy packages from the wheel into the <code>site-packages</code> directory</li>
<li><code>hardlink</code>:  Hard link packages from the wheel into the <code>site-packages</code> directory</li>
<li><code>symlink</code>:  Symbolically link packages from the wheel into the <code>site-packages</code> directory</li>
</ul></dd><dt id="uv-lock--managed-python"><a href="#uv-lock--managed-python"><code>--managed-python</code></a></dt><dd><p>Require use of uv-managed Python versions.</p>
<p>By default, uv prefers using Python versions it manages. However, it will use system Python versions if a uv-managed Python is not installed. This option disables use of system Python versions.</p>
<p>May also be set with the <code>UV_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-lock--native-tls"><a href="#uv-lock--native-tls"><code>--native-tls</code></a></dt><dd><p>Whether to load TLS certificates from the platform's native certificate store.</p>
<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>
<p>However, in some cases, you may want to use the platform's native certificate store, especially if you're relying on a corporate trust root (e.g., for a mandatory proxy) that's included in your system's certificate store.</p>
<p>May also be set with the <code>UV_NATIVE_TLS</code> environment variable.</p></dd><dt id="uv-lock--no-binary"><a href="#uv-lock--no-binary"><code>--no-binary</code></a></dt><dd><p>Don't install pre-built wheels.</p>
<p>The given packages will be built and installed from source. The resolver will still use pre-built wheels to extract package metadata, if available.</p>
<p>May also be set with the <code>UV_NO_BINARY</code> environment variable.</p></dd><dt id="uv-lock--no-binary-package"><a href="#uv-lock--no-binary-package"><code>--no-binary-package</code></a> <i>no-binary-package</i></dt><dd><p>Don't install pre-built wheels for a specific package</p>
<p>May also be set with the <code>UV_NO_BINARY_PACKAGE</code> environment variable.</p></dd><dt id="uv-lock--no-build"><a href="#uv-lock--no-build"><code>--no-build</code></a></dt><dd><p>Don't build source distributions.</p>
<p>When enabled, resolving will not run arbitrary Python code. The cached wheels of already-built source distributions will be reused, but operations that require building distributions will exit with an error.</p>
<p>May also be set with the <code>UV_NO_BUILD</code> environment variable.</p></dd><dt id="uv-lock--no-build-isolation"><a href="#uv-lock--no-build-isolation"><code>--no-build-isolation</code></a></dt><dd><p>Disable isolation when building source distributions.</p>
<p>Assumes that build dependencies specified by PEP 518 are already installed.</p>
<p>May also be set with the <code>UV_NO_BUILD_ISOLATION</code> environment variable.</p></dd><dt id="uv-lock--no-build-isolation-package"><a href="#uv-lock--no-build-isolation-package"><code>--no-build-isolation-package</code></a> <i>no-build-isolation-package</i></dt><dd><p>Disable isolation when building source distributions for a specific package.</p>
<p>Assumes that the packages' build dependencies specified by PEP 518 are already installed.</p>
</dd><dt id="uv-lock--no-build-package"><a href="#uv-lock--no-build-package"><code>--no-build-package</code></a> <i>no-build-package</i></dt><dd><p>Don't build source distributions for a specific package</p>
<p>May also be set with the <code>UV_NO_BUILD_PACKAGE</code> environment variable.</p></dd><dt id="uv-lock--no-cache"><a href="#uv-lock--no-cache"><code>--no-cache</code></a>, <code>--no-cache-dir</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>
<p>May also be set with the <code>UV_NO_CACHE</code> environment variable.</p></dd><dt id="uv-lock--no-config"><a href="#uv-lock--no-config"><code>--no-config</code></a></dt><dd><p>Avoid discovering configuration files (<code>pyproject.toml</code>, <code>uv.toml</code>).</p>
<p>Normally, configuration files are discovered in the current directory, parent directories, or user configuration directories.</p>
<p>May also be set with the <code>UV_NO_CONFIG</code> environment variable.</p></dd><dt id="uv-lock--no-index"><a href="#uv-lock--no-index"><code>--no-index</code></a></dt><dd><p>Ignore the registry index (e.g., PyPI), instead relying on direct URL dependencies and those provided via <code>--find-links</code></p>
</dd><dt id="uv-lock--no-managed-python"><a href="#uv-lock--no-managed-python"><code>--no-managed-python</code></a></dt><dd><p>Disable use of uv-managed Python versions.</p>
<p>Instead, uv will search for a suitable Python version on the system.</p>
<p>May also be set with the <code>UV_NO_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-lock--no-progress"><a href="#uv-lock--no-progress"><code>--no-progress</code></a></dt><dd><p>Hide all progress outputs.</p>
<p>For example, spinners or progress bars.</p>
<p>May also be set with the <code>UV_NO_PROGRESS</code> environment variable.</p></dd><dt id="uv-lock--no-python-downloads"><a href="#uv-lock--no-python-downloads"><code>--no-python-downloads</code></a></dt><dd><p>Disable automatic downloads of Python.</p>
</dd><dt id="uv-lock--no-sources"><a href="#uv-lock--no-sources"><code>--no-sources</code></a></dt><dd><p>Ignore the <code>tool.uv.sources</code> table when resolving dependencies. Used to lock against the standards-compliant, publishable package metadata, as opposed to using any workspace, Git, URL, or local path sources</p>
</dd><dt id="uv-lock--offline"><a href="#uv-lock--offline"><code>--offline</code></a></dt><dd><p>Disable network access.</p>
<p>When disabled, uv will only use locally cached data and locally available files.</p>
<p>May also be set with the <code>UV_OFFLINE</code> environment variable.</p></dd><dt id="uv-lock--prerelease"><a href="#uv-lock--prerelease"><code>--prerelease</code></a> <i>prerelease</i></dt><dd><p>The strategy to use when considering pre-release versions.</p>
<p>By default, uv will accept pre-releases for packages that <em>only</em> publish pre-releases, along with first-party requirements that contain an explicit pre-release marker in the declared specifiers (<code>if-necessary-or-explicit</code>).</p>
<p>May also be set with the <code>UV_PRERELEASE</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>disallow</code>:  Disallow all pre-release versions</li>
<li><code>allow</code>:  Allow all pre-release versions</li>
<li><code>if-necessary</code>:  Allow pre-release versions if all versions of a package are pre-release</li>
<li><code>explicit</code>:  Allow pre-release versions for first-party packages with explicit pre-release markers in their version requirements</li>
<li><code>if-necessary-or-explicit</code>:  Allow pre-release versions if all versions of a package are pre-release, or if the package has an explicit pre-release marker in its version requirements</li>
</ul></dd><dt id="uv-lock--project"><a href="#uv-lock--project"><code>--project</code></a> <i>project</i></dt><dd><p>Run the command within the given project directory.</p>
<p>All <code>pyproject.toml</code>, <code>uv.toml</code>, and <code>.python-version</code> files will be discovered by walking up the directory tree from the project root, as will the project's virtual environment (<code>.venv</code>).</p>
<p>Other command-line arguments (such as relative paths) will be resolved relative to the current working directory.</p>
<p>See <code>--directory</code> to change the working directory entirely.</p>
<p>This setting has no effect when used in the <code>uv pip</code> interface.</p>
<p>May also be set with the <code>UV_PROJECT</code> environment variable.</p></dd><dt id="uv-lock--python"><a href="#uv-lock--python"><code>--python</code></a>, <code>-p</code> <i>python</i></dt><dd><p>The Python interpreter to use during resolution.</p>
<p>A Python interpreter is required for building source distributions to determine package
metadata when there are not wheels.</p>
<p>The interpreter is also used as the fallback value for the minimum Python version if
<code>requires-python</code> is not set.</p>
<p>See <a href="#uv-python">uv python</a> for details on Python discovery and supported request formats.</p>
<p>May also be set with the <code>UV_PYTHON</code> environment variable.</p></dd><dt id="uv-lock--quiet"><a href="#uv-lock--quiet"><code>--quiet</code></a>, <code>-q</code></dt><dd><p>Use quiet output.</p>
<p>Repeating this option, e.g., <code>-qq</code>, will enable a silent mode in which uv will write no output to stdout.</p>
</dd><dt id="uv-lock--refresh"><a href="#uv-lock--refresh"><code>--refresh</code></a></dt><dd><p>Refresh all cached data</p>
</dd><dt id="uv-lock--refresh-package"><a href="#uv-lock--refresh-package"><code>--refresh-package</code></a> <i>refresh-package</i></dt><dd><p>Refresh cached data for a specific package</p>
</dd><dt id="uv-lock--resolution"><a href="#uv-lock--resolution"><code>--resolution</code></a> <i>resolution</i></dt><dd><p>The strategy to use when selecting between the different compatible versions for a given package requirement.</p>
<p>By default, uv will use the latest compatible version of each package (<code>highest</code>).</p>
<p>May also be set with the <code>UV_RESOLUTION</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>highest</code>:  Resolve the highest compatible version of each package</li>
<li><code>lowest</code>:  Resolve the lowest compatible version of each package</li>
<li><code>lowest-direct</code>:  Resolve the lowest compatible version of any direct dependencies, and the highest compatible version of any transitive dependencies</li>
</ul></dd><dt id="uv-lock--script"><a href="#uv-lock--script"><code>--script</code></a> <i>script</i></dt><dd><p>Lock the specified Python script, rather than the current project.</p>
<p>If provided, uv will lock the script (based on its inline metadata table, in adherence with PEP 723) to a <code>.lock</code> file adjacent to the script itself.</p>
</dd><dt id="uv-lock--upgrade"><a href="#uv-lock--upgrade"><code>--upgrade</code></a>, <code>-U</code></dt><dd><p>Allow package upgrades, ignoring pinned versions in any existing output file. Implies <code>--refresh</code></p>
</dd><dt id="uv-lock--upgrade-package"><a href="#uv-lock--upgrade-package"><code>--upgrade-package</code></a>, <code>-P</code> <i>upgrade-package</i></dt><dd><p>Allow upgrades for a specific package, ignoring pinned versions in any existing output file. Implies <code>--refresh-package</code></p>
</dd><dt id="uv-lock--verbose"><a href="#uv-lock--verbose"><code>--verbose</code></a>, <code>-v</code></dt><dd><p>Use verbose output.</p>
<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (<a href="https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives">https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives</a>)</p>
</dd></dl>

## uv export

Export the project's lockfile to an alternate format.

At present, both `requirements.txt` and `pylock.toml` (PEP 751) formats are supported.

The project is re-locked before exporting unless the `--locked` or `--frozen` flag is provided.

uv will search for a project in the current directory or any parent directory. If a project cannot be found, uv will exit with an error.

If operating in a workspace, the root will be exported by default; however, a specific member can be selected using the `--package` option.

<h3 class="cli-reference">Usage</h3>

```
uv export [OPTIONS]
```

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt id="uv-export--all-extras"><a href="#uv-export--all-extras"><code>--all-extras</code></a></dt><dd><p>Include all optional dependencies</p>
</dd><dt id="uv-export--all-groups"><a href="#uv-export--all-groups"><code>--all-groups</code></a></dt><dd><p>Include dependencies from all dependency groups.</p>
<p><code>--no-group</code> can be used to exclude specific groups.</p>
</dd><dt id="uv-export--all-packages"><a href="#uv-export--all-packages"><code>--all-packages</code></a></dt><dd><p>Export the entire workspace.</p>
<p>The dependencies for all workspace members will be included in the exported requirements file.</p>
<p>Any extras or groups specified via <code>--extra</code>, <code>--group</code>, or related options will be applied to all workspace members.</p>
</dd><dt id="uv-export--allow-insecure-host"><a href="#uv-export--allow-insecure-host"><code>--allow-insecure-host</code></a>, <code>--trusted-host</code> <i>allow-insecure-host</i></dt><dd><p>Allow insecure connections to a host.</p>
<p>Can be provided multiple times.</p>
<p>Expects to receive either a hostname (e.g., <code>localhost</code>), a host-port pair (e.g., <code>localhost:8080</code>), or a URL (e.g., <code>https://localhost</code>).</p>
<p>WARNING: Hosts included in this list will not be verified against the system's certificate store. Only use <code>--allow-insecure-host</code> in a secure network with verified sources, as it bypasses SSL verification and could expose you to MITM attacks.</p>
<p>May also be set with the <code>UV_INSECURE_HOST</code> environment variable.</p></dd><dt id="uv-export--cache-dir"><a href="#uv-export--cache-dir"><code>--cache-dir</code></a> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>
<p>Defaults to <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on macOS and Linux, and <code>%LOCALAPPDATA%\uv\cache</code> on Windows.</p>
<p>To view the location of the cache directory, run <code>uv cache dir</code>.</p>
<p>May also be set with the <code>UV_CACHE_DIR</code> environment variable.</p></dd><dt id="uv-export--color"><a href="#uv-export--color"><code>--color</code></a> <i>color-choice</i></dt><dd><p>Control the use of color in output.</p>
<p>By default, uv will automatically detect support for colors when writing to a terminal.</p>
<p>Possible values:</p>
<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>
<li><code>always</code>:  Enables colored output regardless of the detected environment</li>
<li><code>never</code>:  Disables colored output</li>
</ul></dd><dt id="uv-export--config-file"><a href="#uv-export--config-file"><code>--config-file</code></a> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>
<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>
<p>May also be set with the <code>UV_CONFIG_FILE</code> environment variable.</p></dd><dt id="uv-export--config-setting"><a href="#uv-export--config-setting"><code>--config-setting</code></a>, <code>--config-settings</code>, <code>-C</code> <i>config-setting</i></dt><dd><p>Settings to pass to the PEP 517 build backend, specified as <code>KEY=VALUE</code> pairs</p>
</dd><dt id="uv-export--default-index"><a href="#uv-export--default-index"><code>--default-index</code></a> <i>default-index</i></dt><dd><p>The URL of the default package index (by default: <a href="https://pypi.org/simple">https://pypi.org/simple</a>).</p>
<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>
<p>The index given by this flag is given lower priority than all other indexes specified via the <code>--index</code> flag.</p>
<p>May also be set with the <code>UV_DEFAULT_INDEX</code> environment variable.</p></dd><dt id="uv-export--directory"><a href="#uv-export--directory"><code>--directory</code></a> <i>directory</i></dt><dd><p>Change to the given directory prior to running the command.</p>
<p>Relative paths are resolved with the given directory as the base.</p>
<p>See <code>--project</code> to only change the project root directory.</p>
</dd><dt id="uv-export--exclude-newer"><a href="#uv-export--exclude-newer"><code>--exclude-newer</code></a> <i>exclude-newer</i></dt><dd><p>Limit candidate packages to those that were uploaded prior to the given date.</p>
<p>Accepts both RFC 3339 timestamps (e.g., <code>2006-12-02T02:07:43Z</code>) and local dates in the same format (e.g., <code>2006-12-02</code>) in your system's configured time zone.</p>
<p>May also be set with the <code>UV_EXCLUDE_NEWER</code> environment variable.</p></dd><dt id="uv-export--extra"><a href="#uv-export--extra"><code>--extra</code></a> <i>extra</i></dt><dd><p>Include optional dependencies from the specified extra name.</p>
<p>May be provided more than once.</p>
</dd><dt id="uv-export--extra-index-url"><a href="#uv-export--extra-index-url"><code>--extra-index-url</code></a> <i>extra-index-url</i></dt><dd><p>(Deprecated: use <code>--index</code> instead) Extra URLs of package indexes to use, in addition to <code>--index-url</code>.</p>
<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>
<p>All indexes provided via this flag take priority over the index specified by <code>--index-url</code> (which defaults to PyPI). When multiple <code>--extra-index-url</code> flags are provided, earlier values take priority.</p>
<p>May also be set with the <code>UV_EXTRA_INDEX_URL</code> environment variable.</p></dd><dt id="uv-export--find-links"><a href="#uv-export--find-links"><code>--find-links</code></a>, <code>-f</code> <i>find-links</i></dt><dd><p>Locations to search for candidate distributions, in addition to those found in the registry indexes.</p>
<p>If a path, the target must be a directory that contains packages as wheel files (<code>.whl</code>) or source distributions (e.g., <code>.tar.gz</code> or <code>.zip</code>) at the top level.</p>
<p>If a URL, the page must contain a flat list of links to package files adhering to the formats described above.</p>
<p>May also be set with the <code>UV_FIND_LINKS</code> environment variable.</p></dd><dt id="uv-export--fork-strategy"><a href="#uv-export--fork-strategy"><code>--fork-strategy</code></a> <i>fork-strategy</i></dt><dd><p>The strategy to use when selecting multiple versions of a given package across Python versions and platforms.</p>
<p>By default, uv will optimize for selecting the latest version of each package for each supported Python version (<code>requires-python</code>), while minimizing the number of selected versions across platforms.</p>
<p>Under <code>fewest</code>, uv will minimize the number of selected versions for each package, preferring older versions that are compatible with a wider range of supported Python versions or platforms.</p>
<p>May also be set with the <code>UV_FORK_STRATEGY</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>fewest</code>:  Optimize for selecting the fewest number of versions for each package. Older versions may be preferred if they are compatible with a wider range of supported Python versions or platforms</li>
<li><code>requires-python</code>:  Optimize for selecting latest supported version of each package, for each supported Python version</li>
</ul></dd><dt id="uv-export--format"><a href="#uv-export--format"><code>--format</code></a> <i>format</i></dt><dd><p>The format to which <code>uv.lock</code> should be exported.</p>
<p>Supports both <code>requirements.txt</code> and <code>pylock.toml</code> (PEP 751) output formats.</p>
<p>uv will infer the output format from the file extension of the output file, if provided. Otherwise, defaults to <code>requirements.txt</code>.</p>
<p>Possible values:</p>
<ul>
<li><code>requirements.txt</code>:  Export in <code>requirements.txt</code> format</li>
<li><code>pylock.toml</code>:  Export in <code>pylock.toml</code> format</li>
</ul></dd><dt id="uv-export--frozen"><a href="#uv-export--frozen"><code>--frozen</code></a></dt><dd><p>Do not update the <code>uv.lock</code> before exporting.</p>
<p>If a <code>uv.lock</code> does not exist, uv will exit with an error.</p>
<p>May also be set with the <code>UV_FROZEN</code> environment variable.</p></dd><dt id="uv-export--group"><a href="#uv-export--group"><code>--group</code></a> <i>group</i></dt><dd><p>Include dependencies from the specified dependency group.</p>
<p>May be provided multiple times.</p>
</dd><dt id="uv-export--help"><a href="#uv-export--help"><code>--help</code></a>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>
</dd><dt id="uv-export--index"><a href="#uv-export--index"><code>--index</code></a> <i>index</i></dt><dd><p>The URLs to use when resolving dependencies, in addition to the default index.</p>
<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>
<p>All indexes provided via this flag take priority over the index specified by <code>--default-index</code> (which defaults to PyPI). When multiple <code>--index</code> flags are provided, earlier values take priority.</p>
<p>May also be set with the <code>UV_INDEX</code> environment variable.</p></dd><dt id="uv-export--index-strategy"><a href="#uv-export--index-strategy"><code>--index-strategy</code></a> <i>index-strategy</i></dt><dd><p>The strategy to use when resolving against multiple index URLs.</p>
<p>By default, uv will stop at the first index on which a given package is available, and limit resolutions to those present on that first index (<code>first-index</code>). This prevents &quot;dependency confusion&quot; attacks, whereby an attacker can upload a malicious package under the same name to an alternate index.</p>
<p>May also be set with the <code>UV_INDEX_STRATEGY</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>first-index</code>:  Only use results from the first index that returns a match for a given package name</li>
<li><code>unsafe-first-match</code>:  Search for every package name across all indexes, exhausting the versions from the first index before moving on to the next</li>
<li><code>unsafe-best-match</code>:  Search for every package name across all indexes, preferring the &quot;best&quot; version found. If a package version is in multiple indexes, only look at the entry for the first index</li>
</ul></dd><dt id="uv-export--index-url"><a href="#uv-export--index-url"><code>--index-url</code></a>, <code>-i</code> <i>index-url</i></dt><dd><p>(Deprecated: use <code>--default-index</code> instead) The URL of the Python package index (by default: <a href="https://pypi.org/simple">https://pypi.org/simple</a>).</p>
<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>
<p>The index given by this flag is given lower priority than all other indexes specified via the <code>--extra-index-url</code> flag.</p>
<p>May also be set with the <code>UV_INDEX_URL</code> environment variable.</p></dd><dt id="uv-export--keyring-provider"><a href="#uv-export--keyring-provider"><code>--keyring-provider</code></a> <i>keyring-provider</i></dt><dd><p>Attempt to use <code>keyring</code> for authentication for index URLs.</p>
<p>At present, only <code>--keyring-provider subprocess</code> is supported, which configures uv to use the <code>keyring</code> CLI to handle authentication.</p>
<p>Defaults to <code>disabled</code>.</p>
<p>May also be set with the <code>UV_KEYRING_PROVIDER</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>disabled</code>:  Do not use keyring for credential lookup</li>
<li><code>subprocess</code>:  Use the <code>keyring</code> command for credential lookup</li>
</ul></dd><dt id="uv-export--link-mode"><a href="#uv-export--link-mode"><code>--link-mode</code></a> <i>link-mode</i></dt><dd><p>The method to use when installing packages from the global cache.</p>
<p>This option is only used when building source distributions.</p>
<p>Defaults to <code>clone</code> (also known as Copy-on-Write) on macOS, and <code>hardlink</code> on Linux and Windows.</p>
<p>May also be set with the <code>UV_LINK_MODE</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>clone</code>:  Clone (i.e., copy-on-write) packages from the wheel into the <code>site-packages</code> directory</li>
<li><code>copy</code>:  Copy packages from the wheel into the <code>site-packages</code> directory</li>
<li><code>hardlink</code>:  Hard link packages from the wheel into the <code>site-packages</code> directory</li>
<li><code>symlink</code>:  Symbolically link packages from the wheel into the <code>site-packages</code> directory</li>
</ul></dd><dt id="uv-export--locked"><a href="#uv-export--locked"><code>--locked</code></a></dt><dd><p>Assert that the <code>uv.lock</code> will remain unchanged.</p>
<p>Requires that the lockfile is up-to-date. If the lockfile is missing or needs to be updated, uv will exit with an error.</p>
<p>May also be set with the <code>UV_LOCKED</code> environment variable.</p></dd><dt id="uv-export--managed-python"><a href="#uv-export--managed-python"><code>--managed-python</code></a></dt><dd><p>Require use of uv-managed Python versions.</p>
<p>By default, uv prefers using Python versions it manages. However, it will use system Python versions if a uv-managed Python is not installed. This option disables use of system Python versions.</p>
<p>May also be set with the <code>UV_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-export--native-tls"><a href="#uv-export--native-tls"><code>--native-tls</code></a></dt><dd><p>Whether to load TLS certificates from the platform's native certificate store.</p>
<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>
<p>However, in some cases, you may want to use the platform's native certificate store, especially if you're relying on a corporate trust root (e.g., for a mandatory proxy) that's included in your system's certificate store.</p>
<p>May also be set with the <code>UV_NATIVE_TLS</code> environment variable.</p></dd><dt id="uv-export--no-annotate"><a href="#uv-export--no-annotate"><code>--no-annotate</code></a></dt><dd><p>Exclude comment annotations indicating the source of each package</p>
</dd><dt id="uv-export--no-binary"><a href="#uv-export--no-binary"><code>--no-binary</code></a></dt><dd><p>Don't install pre-built wheels.</p>
<p>The given packages will be built and installed from source. The resolver will still use pre-built wheels to extract package metadata, if available.</p>
<p>May also be set with the <code>UV_NO_BINARY</code> environment variable.</p></dd><dt id="uv-export--no-binary-package"><a href="#uv-export--no-binary-package"><code>--no-binary-package</code></a> <i>no-binary-package</i></dt><dd><p>Don't install pre-built wheels for a specific package</p>
<p>May also be set with the <code>UV_NO_BINARY_PACKAGE</code> environment variable.</p></dd><dt id="uv-export--no-build"><a href="#uv-export--no-build"><code>--no-build</code></a></dt><dd><p>Don't build source distributions.</p>
<p>When enabled, resolving will not run arbitrary Python code. The cached wheels of already-built source distributions will be reused, but operations that require building distributions will exit with an error.</p>
<p>May also be set with the <code>UV_NO_BUILD</code> environment variable.</p></dd><dt id="uv-export--no-build-isolation"><a href="#uv-export--no-build-isolation"><code>--no-build-isolation</code></a></dt><dd><p>Disable isolation when building source distributions.</p>
<p>Assumes that build dependencies specified by PEP 518 are already installed.</p>
<p>May also be set with the <code>UV_NO_BUILD_ISOLATION</code> environment variable.</p></dd><dt id="uv-export--no-build-isolation-package"><a href="#uv-export--no-build-isolation-package"><code>--no-build-isolation-package</code></a> <i>no-build-isolation-package</i></dt><dd><p>Disable isolation when building source distributions for a specific package.</p>
<p>Assumes that the packages' build dependencies specified by PEP 518 are already installed.</p>
</dd><dt id="uv-export--no-build-package"><a href="#uv-export--no-build-package"><code>--no-build-package</code></a> <i>no-build-package</i></dt><dd><p>Don't build source distributions for a specific package</p>
<p>May also be set with the <code>UV_NO_BUILD_PACKAGE</code> environment variable.</p></dd><dt id="uv-export--no-cache"><a href="#uv-export--no-cache"><code>--no-cache</code></a>, <code>--no-cache-dir</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>
<p>May also be set with the <code>UV_NO_CACHE</code> environment variable.</p></dd><dt id="uv-export--no-config"><a href="#uv-export--no-config"><code>--no-config</code></a></dt><dd><p>Avoid discovering configuration files (<code>pyproject.toml</code>, <code>uv.toml</code>).</p>
<p>Normally, configuration files are discovered in the current directory, parent directories, or user configuration directories.</p>
<p>May also be set with the <code>UV_NO_CONFIG</code> environment variable.</p></dd><dt id="uv-export--no-default-groups"><a href="#uv-export--no-default-groups"><code>--no-default-groups</code></a></dt><dd><p>Ignore the default dependency groups.</p>
<p>uv includes the groups defined in <code>tool.uv.default-groups</code> by default. This disables that option, however, specific groups can still be included with <code>--group</code>.</p>
</dd><dt id="uv-export--no-dev"><a href="#uv-export--no-dev"><code>--no-dev</code></a></dt><dd><p>Disable the development dependency group.</p>
<p>This option is an alias of <code>--no-group dev</code>. See <code>--no-default-groups</code> to disable all default groups instead.</p>
</dd><dt id="uv-export--no-editable"><a href="#uv-export--no-editable"><code>--no-editable</code></a></dt><dd><p>Export any editable dependencies, including the project and any workspace members, as non-editable</p>
</dd><dt id="uv-export--no-emit-package"><a href="#uv-export--no-emit-package"><code>--no-emit-package</code></a>, <code>--no-install-package</code> <i>no-emit-package</i></dt><dd><p>Do not emit the given package(s).</p>
<p>By default, all of the project's dependencies are included in the exported requirements file. The <code>--no-emit-package</code> option allows exclusion of specific packages.</p>
</dd><dt id="uv-export--no-emit-project"><a href="#uv-export--no-emit-project"><code>--no-emit-project</code></a>, <code>--no-install-project</code></dt><dd><p>Do not emit the current project.</p>
<p>By default, the current project is included in the exported requirements file with all of its dependencies. The <code>--no-emit-project</code> option allows the project to be excluded, but all of its dependencies to remain included.</p>
</dd><dt id="uv-export--no-emit-workspace"><a href="#uv-export--no-emit-workspace"><code>--no-emit-workspace</code></a>, <code>--no-install-workspace</code></dt><dd><p>Do not emit any workspace members, including the root project.</p>
<p>By default, all workspace members and their dependencies are included in the exported requirements file, with all of their dependencies. The <code>--no-emit-workspace</code> option allows exclusion of all the workspace members while retaining their dependencies.</p>
</dd><dt id="uv-export--no-extra"><a href="#uv-export--no-extra"><code>--no-extra</code></a> <i>no-extra</i></dt><dd><p>Exclude the specified optional dependencies, if <code>--all-extras</code> is supplied.</p>
<p>May be provided multiple times.</p>
</dd><dt id="uv-export--no-group"><a href="#uv-export--no-group"><code>--no-group</code></a> <i>no-group</i></dt><dd><p>Disable the specified dependency group.</p>
<p>This option always takes precedence over default groups, <code>--all-groups</code>, and <code>--group</code>.</p>
<p>May be provided multiple times.</p>
</dd><dt id="uv-export--no-hashes"><a href="#uv-export--no-hashes"><code>--no-hashes</code></a></dt><dd><p>Omit hashes in the generated output</p>
</dd><dt id="uv-export--no-header"><a href="#uv-export--no-header"><code>--no-header</code></a></dt><dd><p>Exclude the comment header at the top of the generated output file</p>
</dd><dt id="uv-export--no-index"><a href="#uv-export--no-index"><code>--no-index</code></a></dt><dd><p>Ignore the registry index (e.g., PyPI), instead relying on direct URL dependencies and those provided via <code>--find-links</code></p>
</dd><dt id="uv-export--no-managed-python"><a href="#uv-export--no-managed-python"><code>--no-managed-python</code></a></dt><dd><p>Disable use of uv-managed Python versions.</p>
<p>Instead, uv will search for a suitable Python version on the system.</p>
<p>May also be set with the <code>UV_NO_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-export--no-progress"><a href="#uv-export--no-progress"><code>--no-progress</code></a></dt><dd><p>Hide all progress outputs.</p>
<p>For example, spinners or progress bars.</p>
<p>May also be set with the <code>UV_NO_PROGRESS</code> environment variable.</p></dd><dt id="uv-export--no-python-downloads"><a href="#uv-export--no-python-downloads"><code>--no-python-downloads</code></a></dt><dd><p>Disable automatic downloads of Python.</p>
</dd><dt id="uv-export--no-sources"><a href="#uv-export--no-sources"><code>--no-sources</code></a></dt><dd><p>Ignore the <code>tool.uv.sources</code> table when resolving dependencies. Used to lock against the standards-compliant, publishable package metadata, as opposed to using any workspace, Git, URL, or local path sources</p>
</dd><dt id="uv-export--offline"><a href="#uv-export--offline"><code>--offline</code></a></dt><dd><p>Disable network access.</p>
<p>When disabled, uv will only use locally cached data and locally available files.</p>
<p>May also be set with the <code>UV_OFFLINE</code> environment variable.</p></dd><dt id="uv-export--only-dev"><a href="#uv-export--only-dev"><code>--only-dev</code></a></dt><dd><p>Only include the development dependency group.</p>
<p>The project and its dependencies will be omitted.</p>
<p>This option is an alias for <code>--only-group dev</code>. Implies <code>--no-default-groups</code>.</p>
</dd><dt id="uv-export--only-group"><a href="#uv-export--only-group"><code>--only-group</code></a> <i>only-group</i></dt><dd><p>Only include dependencies from the specified dependency group.</p>
<p>The project and its dependencies will be omitted.</p>
<p>May be provided multiple times. Implies <code>--no-default-groups</code>.</p>
</dd><dt id="uv-export--output-file"><a href="#uv-export--output-file"><code>--output-file</code></a>, <code>-o</code> <i>output-file</i></dt><dd><p>Write the exported requirements to the given file</p>
</dd><dt id="uv-export--package"><a href="#uv-export--package"><code>--package</code></a> <i>package</i></dt><dd><p>Export the dependencies for a specific package in the workspace.</p>
<p>If the workspace member does not exist, uv will exit with an error.</p>
</dd><dt id="uv-export--prerelease"><a href="#uv-export--prerelease"><code>--prerelease</code></a> <i>prerelease</i></dt><dd><p>The strategy to use when considering pre-release versions.</p>
<p>By default, uv will accept pre-releases for packages that <em>only</em> publish pre-releases, along with first-party requirements that contain an explicit pre-release marker in the declared specifiers (<code>if-necessary-or-explicit</code>).</p>
<p>May also be set with the <code>UV_PRERELEASE</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>disallow</code>:  Disallow all pre-release versions</li>
<li><code>allow</code>:  Allow all pre-release versions</li>
<li><code>if-necessary</code>:  Allow pre-release versions if all versions of a package are pre-release</li>
<li><code>explicit</code>:  Allow pre-release versions for first-party packages with explicit pre-release markers in their version requirements</li>
<li><code>if-necessary-or-explicit</code>:  Allow pre-release versions if all versions of a package are pre-release, or if the package has an explicit pre-release marker in its version requirements</li>
</ul></dd><dt id="uv-export--project"><a href="#uv-export--project"><code>--project</code></a> <i>project</i></dt><dd><p>Run the command within the given project directory.</p>
<p>All <code>pyproject.toml</code>, <code>uv.toml</code>, and <code>.python-version</code> files will be discovered by walking up the directory tree from the project root, as will the project's virtual environment (<code>.venv</code>).</p>
<p>Other command-line arguments (such as relative paths) will be resolved relative to the current working directory.</p>
<p>See <code>--directory</code> to change the working directory entirely.</p>
<p>This setting has no effect when used in the <code>uv pip</code> interface.</p>
<p>May also be set with the <code>UV_PROJECT</code> environment variable.</p></dd><dt id="uv-export--prune"><a href="#uv-export--prune"><code>--prune</code></a> <i>package</i></dt><dd><p>Prune the given package from the dependency tree.</p>
<p>Pruned packages will be excluded from the exported requirements file, as will any dependencies that are no longer required after the pruned package is removed.</p>
</dd><dt id="uv-export--python"><a href="#uv-export--python"><code>--python</code></a>, <code>-p</code> <i>python</i></dt><dd><p>The Python interpreter to use during resolution.</p>
<p>A Python interpreter is required for building source distributions to determine package
metadata when there are not wheels.</p>
<p>The interpreter is also used as the fallback value for the minimum Python version if
<code>requires-python</code> is not set.</p>
<p>See <a href="#uv-python">uv python</a> for details on Python discovery and supported request formats.</p>
<p>May also be set with the <code>UV_PYTHON</code> environment variable.</p></dd><dt id="uv-export--quiet"><a href="#uv-export--quiet"><code>--quiet</code></a>, <code>-q</code></dt><dd><p>Use quiet output.</p>
<p>Repeating this option, e.g., <code>-qq</code>, will enable a silent mode in which uv will write no output to stdout.</p>
</dd><dt id="uv-export--refresh"><a href="#uv-export--refresh"><code>--refresh</code></a></dt><dd><p>Refresh all cached data</p>
</dd><dt id="uv-export--refresh-package"><a href="#uv-export--refresh-package"><code>--refresh-package</code></a> <i>refresh-package</i></dt><dd><p>Refresh cached data for a specific package</p>
</dd><dt id="uv-export--resolution"><a href="#uv-export--resolution"><code>--resolution</code></a> <i>resolution</i></dt><dd><p>The strategy to use when selecting between the different compatible versions for a given package requirement.</p>
<p>By default, uv will use the latest compatible version of each package (<code>highest</code>).</p>
<p>May also be set with the <code>UV_RESOLUTION</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>highest</code>:  Resolve the highest compatible version of each package</li>
<li><code>lowest</code>:  Resolve the lowest compatible version of each package</li>
<li><code>lowest-direct</code>:  Resolve the lowest compatible version of any direct dependencies, and the highest compatible version of any transitive dependencies</li>
</ul></dd><dt id="uv-export--script"><a href="#uv-export--script"><code>--script</code></a> <i>script</i></dt><dd><p>Export the dependencies for the specified PEP 723 Python script, rather than the current project.</p>
<p>If provided, uv will resolve the dependencies based on its inline metadata table, in adherence with PEP 723.</p>
</dd><dt id="uv-export--upgrade"><a href="#uv-export--upgrade"><code>--upgrade</code></a>, <code>-U</code></dt><dd><p>Allow package upgrades, ignoring pinned versions in any existing output file. Implies <code>--refresh</code></p>
</dd><dt id="uv-export--upgrade-package"><a href="#uv-export--upgrade-package"><code>--upgrade-package</code></a>, <code>-P</code> <i>upgrade-package</i></dt><dd><p>Allow upgrades for a specific package, ignoring pinned versions in any existing output file. Implies <code>--refresh-package</code></p>
</dd><dt id="uv-export--verbose"><a href="#uv-export--verbose"><code>--verbose</code></a>, <code>-v</code></dt><dd><p>Use verbose output.</p>
<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (<a href="https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives">https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives</a>)</p>
</dd></dl>

## uv tree

Display the project's dependency tree

<h3 class="cli-reference">Usage</h3>

```
uv tree [OPTIONS]
```

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt id="uv-tree--all-groups"><a href="#uv-tree--all-groups"><code>--all-groups</code></a></dt><dd><p>Include dependencies from all dependency groups.</p>
<p><code>--no-group</code> can be used to exclude specific groups.</p>
</dd><dt id="uv-tree--allow-insecure-host"><a href="#uv-tree--allow-insecure-host"><code>--allow-insecure-host</code></a>, <code>--trusted-host</code> <i>allow-insecure-host</i></dt><dd><p>Allow insecure connections to a host.</p>
<p>Can be provided multiple times.</p>
<p>Expects to receive either a hostname (e.g., <code>localhost</code>), a host-port pair (e.g., <code>localhost:8080</code>), or a URL (e.g., <code>https://localhost</code>).</p>
<p>WARNING: Hosts included in this list will not be verified against the system's certificate store. Only use <code>--allow-insecure-host</code> in a secure network with verified sources, as it bypasses SSL verification and could expose you to MITM attacks.</p>
<p>May also be set with the <code>UV_INSECURE_HOST</code> environment variable.</p></dd><dt id="uv-tree--cache-dir"><a href="#uv-tree--cache-dir"><code>--cache-dir</code></a> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>
<p>Defaults to <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on macOS and Linux, and <code>%LOCALAPPDATA%\uv\cache</code> on Windows.</p>
<p>To view the location of the cache directory, run <code>uv cache dir</code>.</p>
<p>May also be set with the <code>UV_CACHE_DIR</code> environment variable.</p></dd><dt id="uv-tree--color"><a href="#uv-tree--color"><code>--color</code></a> <i>color-choice</i></dt><dd><p>Control the use of color in output.</p>
<p>By default, uv will automatically detect support for colors when writing to a terminal.</p>
<p>Possible values:</p>
<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>
<li><code>always</code>:  Enables colored output regardless of the detected environment</li>
<li><code>never</code>:  Disables colored output</li>
</ul></dd><dt id="uv-tree--config-file"><a href="#uv-tree--config-file"><code>--config-file</code></a> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>
<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>
<p>May also be set with the <code>UV_CONFIG_FILE</code> environment variable.</p></dd><dt id="uv-tree--config-setting"><a href="#uv-tree--config-setting"><code>--config-setting</code></a>, <code>--config-settings</code>, <code>-C</code> <i>config-setting</i></dt><dd><p>Settings to pass to the PEP 517 build backend, specified as <code>KEY=VALUE</code> pairs</p>
</dd><dt id="uv-tree--default-index"><a href="#uv-tree--default-index"><code>--default-index</code></a> <i>default-index</i></dt><dd><p>The URL of the default package index (by default: <a href="https://pypi.org/simple">https://pypi.org/simple</a>).</p>
<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>
<p>The index given by this flag is given lower priority than all other indexes specified via the <code>--index</code> flag.</p>
<p>May also be set with the <code>UV_DEFAULT_INDEX</code> environment variable.</p></dd><dt id="uv-tree--depth"><a href="#uv-tree--depth"><code>--depth</code></a>, <code>-d</code> <i>depth</i></dt><dd><p>Maximum display depth of the dependency tree</p>
<p>[default: 255]</p></dd><dt id="uv-tree--directory"><a href="#uv-tree--directory"><code>--directory</code></a> <i>directory</i></dt><dd><p>Change to the given directory prior to running the command.</p>
<p>Relative paths are resolved with the given directory as the base.</p>
<p>See <code>--project</code> to only change the project root directory.</p>
</dd><dt id="uv-tree--exclude-newer"><a href="#uv-tree--exclude-newer"><code>--exclude-newer</code></a> <i>exclude-newer</i></dt><dd><p>Limit candidate packages to those that were uploaded prior to the given date.</p>
<p>Accepts both RFC 3339 timestamps (e.g., <code>2006-12-02T02:07:43Z</code>) and local dates in the same format (e.g., <code>2006-12-02</code>) in your system's configured time zone.</p>
<p>May also be set with the <code>UV_EXCLUDE_NEWER</code> environment variable.</p></dd><dt id="uv-tree--extra-index-url"><a href="#uv-tree--extra-index-url"><code>--extra-index-url</code></a> <i>extra-index-url</i></dt><dd><p>(Deprecated: use <code>--index</code> instead) Extra URLs of package indexes to use, in addition to <code>--index-url</code>.</p>
<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>
<p>All indexes provided via this flag take priority over the index specified by <code>--index-url</code> (which defaults to PyPI). When multiple <code>--extra-index-url</code> flags are provided, earlier values take priority.</p>
<p>May also be set with the <code>UV_EXTRA_INDEX_URL</code> environment variable.</p></dd><dt id="uv-tree--find-links"><a href="#uv-tree--find-links"><code>--find-links</code></a>, <code>-f</code> <i>find-links</i></dt><dd><p>Locations to search for candidate distributions, in addition to those found in the registry indexes.</p>
<p>If a path, the target must be a directory that contains packages as wheel files (<code>.whl</code>) or source distributions (e.g., <code>.tar.gz</code> or <code>.zip</code>) at the top level.</p>
<p>If a URL, the page must contain a flat list of links to package files adhering to the formats described above.</p>
<p>May also be set with the <code>UV_FIND_LINKS</code> environment variable.</p></dd><dt id="uv-tree--fork-strategy"><a href="#uv-tree--fork-strategy"><code>--fork-strategy</code></a> <i>fork-strategy</i></dt><dd><p>The strategy to use when selecting multiple versions of a given package across Python versions and platforms.</p>
<p>By default, uv will optimize for selecting the latest version of each package for each supported Python version (<code>requires-python</code>), while minimizing the number of selected versions across platforms.</p>
<p>Under <code>fewest</code>, uv will minimize the number of selected versions for each package, preferring older versions that are compatible with a wider range of supported Python versions or platforms.</p>
<p>May also be set with the <code>UV_FORK_STRATEGY</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>fewest</code>:  Optimize for selecting the fewest number of versions for each package. Older versions may be preferred if they are compatible with a wider range of supported Python versions or platforms</li>
<li><code>requires-python</code>:  Optimize for selecting latest supported version of each package, for each supported Python version</li>
</ul></dd><dt id="uv-tree--frozen"><a href="#uv-tree--frozen"><code>--frozen</code></a></dt><dd><p>Display the requirements without locking the project.</p>
<p>If the lockfile is missing, uv will exit with an error.</p>
<p>May also be set with the <code>UV_FROZEN</code> environment variable.</p></dd><dt id="uv-tree--group"><a href="#uv-tree--group"><code>--group</code></a> <i>group</i></dt><dd><p>Include dependencies from the specified dependency group.</p>
<p>May be provided multiple times.</p>
</dd><dt id="uv-tree--help"><a href="#uv-tree--help"><code>--help</code></a>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>
</dd><dt id="uv-tree--index"><a href="#uv-tree--index"><code>--index</code></a> <i>index</i></dt><dd><p>The URLs to use when resolving dependencies, in addition to the default index.</p>
<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>
<p>All indexes provided via this flag take priority over the index specified by <code>--default-index</code> (which defaults to PyPI). When multiple <code>--index</code> flags are provided, earlier values take priority.</p>
<p>May also be set with the <code>UV_INDEX</code> environment variable.</p></dd><dt id="uv-tree--index-strategy"><a href="#uv-tree--index-strategy"><code>--index-strategy</code></a> <i>index-strategy</i></dt><dd><p>The strategy to use when resolving against multiple index URLs.</p>
<p>By default, uv will stop at the first index on which a given package is available, and limit resolutions to those present on that first index (<code>first-index</code>). This prevents &quot;dependency confusion&quot; attacks, whereby an attacker can upload a malicious package under the same name to an alternate index.</p>
<p>May also be set with the <code>UV_INDEX_STRATEGY</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>first-index</code>:  Only use results from the first index that returns a match for a given package name</li>
<li><code>unsafe-first-match</code>:  Search for every package name across all indexes, exhausting the versions from the first index before moving on to the next</li>
<li><code>unsafe-best-match</code>:  Search for every package name across all indexes, preferring the &quot;best&quot; version found. If a package version is in multiple indexes, only look at the entry for the first index</li>
</ul></dd><dt id="uv-tree--index-url"><a href="#uv-tree--index-url"><code>--index-url</code></a>, <code>-i</code> <i>index-url</i></dt><dd><p>(Deprecated: use <code>--default-index</code> instead) The URL of the Python package index (by default: <a href="https://pypi.org/simple">https://pypi.org/simple</a>).</p>
<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>
<p>The index given by this flag is given lower priority than all other indexes specified via the <code>--extra-index-url</code> flag.</p>
<p>May also be set with the <code>UV_INDEX_URL</code> environment variable.</p></dd><dt id="uv-tree--invert"><a href="#uv-tree--invert"><code>--invert</code></a>, <code>--reverse</code></dt><dd><p>Show the reverse dependencies for the given package. This flag will invert the tree and display the packages that depend on the given package</p>
</dd><dt id="uv-tree--keyring-provider"><a href="#uv-tree--keyring-provider"><code>--keyring-provider</code></a> <i>keyring-provider</i></dt><dd><p>Attempt to use <code>keyring</code> for authentication for index URLs.</p>
<p>At present, only <code>--keyring-provider subprocess</code> is supported, which configures uv to use the <code>keyring</code> CLI to handle authentication.</p>
<p>Defaults to <code>disabled</code>.</p>
<p>May also be set with the <code>UV_KEYRING_PROVIDER</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>disabled</code>:  Do not use keyring for credential lookup</li>
<li><code>subprocess</code>:  Use the <code>keyring</code> command for credential lookup</li>
</ul></dd><dt id="uv-tree--link-mode"><a href="#uv-tree--link-mode"><code>--link-mode</code></a> <i>link-mode</i></dt><dd><p>The method to use when installing packages from the global cache.</p>
<p>This option is only used when building source distributions.</p>
<p>Defaults to <code>clone</code> (also known as Copy-on-Write) on macOS, and <code>hardlink</code> on Linux and Windows.</p>
<p>May also be set with the <code>UV_LINK_MODE</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>clone</code>:  Clone (i.e., copy-on-write) packages from the wheel into the <code>site-packages</code> directory</li>
<li><code>copy</code>:  Copy packages from the wheel into the <code>site-packages</code> directory</li>
<li><code>hardlink</code>:  Hard link packages from the wheel into the <code>site-packages</code> directory</li>
<li><code>symlink</code>:  Symbolically link packages from the wheel into the <code>site-packages</code> directory</li>
</ul></dd><dt id="uv-tree--locked"><a href="#uv-tree--locked"><code>--locked</code></a></dt><dd><p>Assert that the <code>uv.lock</code> will remain unchanged.</p>
<p>Requires that the lockfile is up-to-date. If the lockfile is missing or needs to be updated, uv will exit with an error.</p>
<p>May also be set with the <code>UV_LOCKED</code> environment variable.</p></dd><dt id="uv-tree--managed-python"><a href="#uv-tree--managed-python"><code>--managed-python</code></a></dt><dd><p>Require use of uv-managed Python versions.</p>
<p>By default, uv prefers using Python versions it manages. However, it will use system Python versions if a uv-managed Python is not installed. This option disables use of system Python versions.</p>
<p>May also be set with the <code>UV_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-tree--native-tls"><a href="#uv-tree--native-tls"><code>--native-tls</code></a></dt><dd><p>Whether to load TLS certificates from the platform's native certificate store.</p>
<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>
<p>However, in some cases, you may want to use the platform's native certificate store, especially if you're relying on a corporate trust root (e.g., for a mandatory proxy) that's included in your system's certificate store.</p>
<p>May also be set with the <code>UV_NATIVE_TLS</code> environment variable.</p></dd><dt id="uv-tree--no-binary"><a href="#uv-tree--no-binary"><code>--no-binary</code></a></dt><dd><p>Don't install pre-built wheels.</p>
<p>The given packages will be built and installed from source. The resolver will still use pre-built wheels to extract package metadata, if available.</p>
<p>May also be set with the <code>UV_NO_BINARY</code> environment variable.</p></dd><dt id="uv-tree--no-binary-package"><a href="#uv-tree--no-binary-package"><code>--no-binary-package</code></a> <i>no-binary-package</i></dt><dd><p>Don't install pre-built wheels for a specific package</p>
<p>May also be set with the <code>UV_NO_BINARY_PACKAGE</code> environment variable.</p></dd><dt id="uv-tree--no-build"><a href="#uv-tree--no-build"><code>--no-build</code></a></dt><dd><p>Don't build source distributions.</p>
<p>When enabled, resolving will not run arbitrary Python code. The cached wheels of already-built source distributions will be reused, but operations that require building distributions will exit with an error.</p>
<p>May also be set with the <code>UV_NO_BUILD</code> environment variable.</p></dd><dt id="uv-tree--no-build-isolation"><a href="#uv-tree--no-build-isolation"><code>--no-build-isolation</code></a></dt><dd><p>Disable isolation when building source distributions.</p>
<p>Assumes that build dependencies specified by PEP 518 are already installed.</p>
<p>May also be set with the <code>UV_NO_BUILD_ISOLATION</code> environment variable.</p></dd><dt id="uv-tree--no-build-isolation-package"><a href="#uv-tree--no-build-isolation-package"><code>--no-build-isolation-package</code></a> <i>no-build-isolation-package</i></dt><dd><p>Disable isolation when building source distributions for a specific package.</p>
<p>Assumes that the packages' build dependencies specified by PEP 518 are already installed.</p>
</dd><dt id="uv-tree--no-build-package"><a href="#uv-tree--no-build-package"><code>--no-build-package</code></a> <i>no-build-package</i></dt><dd><p>Don't build source distributions for a specific package</p>
<p>May also be set with the <code>UV_NO_BUILD_PACKAGE</code> environment variable.</p></dd><dt id="uv-tree--no-cache"><a href="#uv-tree--no-cache"><code>--no-cache</code></a>, <code>--no-cache-dir</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>
<p>May also be set with the <code>UV_NO_CACHE</code> environment variable.</p></dd><dt id="uv-tree--no-config"><a href="#uv-tree--no-config"><code>--no-config</code></a></dt><dd><p>Avoid discovering configuration files (<code>pyproject.toml</code>, <code>uv.toml</code>).</p>
<p>Normally, configuration files are discovered in the current directory, parent directories, or user configuration directories.</p>
<p>May also be set with the <code>UV_NO_CONFIG</code> environment variable.</p></dd><dt id="uv-tree--no-dedupe"><a href="#uv-tree--no-dedupe"><code>--no-dedupe</code></a></dt><dd><p>Do not de-duplicate repeated dependencies. Usually, when a package has already displayed its dependencies, further occurrences will not re-display its dependencies, and will include a (*) to indicate it has already been shown. This flag will cause those duplicates to be repeated</p>
</dd><dt id="uv-tree--no-default-groups"><a href="#uv-tree--no-default-groups"><code>--no-default-groups</code></a></dt><dd><p>Ignore the default dependency groups.</p>
<p>uv includes the groups defined in <code>tool.uv.default-groups</code> by default. This disables that option, however, specific groups can still be included with <code>--group</code>.</p>
</dd><dt id="uv-tree--no-dev"><a href="#uv-tree--no-dev"><code>--no-dev</code></a></dt><dd><p>Disable the development dependency group.</p>
<p>This option is an alias of <code>--no-group dev</code>. See <code>--no-default-groups</code> to disable all default groups instead.</p>
</dd><dt id="uv-tree--no-group"><a href="#uv-tree--no-group"><code>--no-group</code></a> <i>no-group</i></dt><dd><p>Disable the specified dependency group.</p>
<p>This option always takes precedence over default groups, <code>--all-groups</code>, and <code>--group</code>.</p>
<p>May be provided multiple times.</p>
</dd><dt id="uv-tree--no-index"><a href="#uv-tree--no-index"><code>--no-index</code></a></dt><dd><p>Ignore the registry index (e.g., PyPI), instead relying on direct URL dependencies and those provided via <code>--find-links</code></p>
</dd><dt id="uv-tree--no-managed-python"><a href="#uv-tree--no-managed-python"><code>--no-managed-python</code></a></dt><dd><p>Disable use of uv-managed Python versions.</p>
<p>Instead, uv will search for a suitable Python version on the system.</p>
<p>May also be set with the <code>UV_NO_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-tree--no-progress"><a href="#uv-tree--no-progress"><code>--no-progress</code></a></dt><dd><p>Hide all progress outputs.</p>
<p>For example, spinners or progress bars.</p>
<p>May also be set with the <code>UV_NO_PROGRESS</code> environment variable.</p></dd><dt id="uv-tree--no-python-downloads"><a href="#uv-tree--no-python-downloads"><code>--no-python-downloads</code></a></dt><dd><p>Disable automatic downloads of Python.</p>
</dd><dt id="uv-tree--no-sources"><a href="#uv-tree--no-sources"><code>--no-sources</code></a></dt><dd><p>Ignore the <code>tool.uv.sources</code> table when resolving dependencies. Used to lock against the standards-compliant, publishable package metadata, as opposed to using any workspace, Git, URL, or local path sources</p>
</dd><dt id="uv-tree--offline"><a href="#uv-tree--offline"><code>--offline</code></a></dt><dd><p>Disable network access.</p>
<p>When disabled, uv will only use locally cached data and locally available files.</p>
<p>May also be set with the <code>UV_OFFLINE</code> environment variable.</p></dd><dt id="uv-tree--only-dev"><a href="#uv-tree--only-dev"><code>--only-dev</code></a></dt><dd><p>Only include the development dependency group.</p>
<p>The project and its dependencies will be omitted.</p>
<p>This option is an alias for <code>--only-group dev</code>. Implies <code>--no-default-groups</code>.</p>
</dd><dt id="uv-tree--only-group"><a href="#uv-tree--only-group"><code>--only-group</code></a> <i>only-group</i></dt><dd><p>Only include dependencies from the specified dependency group.</p>
<p>The project and its dependencies will be omitted.</p>
<p>May be provided multiple times. Implies <code>--no-default-groups</code>.</p>
</dd><dt id="uv-tree--outdated"><a href="#uv-tree--outdated"><code>--outdated</code></a></dt><dd><p>Show the latest available version of each package in the tree</p>
</dd><dt id="uv-tree--package"><a href="#uv-tree--package"><code>--package</code></a> <i>package</i></dt><dd><p>Display only the specified packages</p>
</dd><dt id="uv-tree--prerelease"><a href="#uv-tree--prerelease"><code>--prerelease</code></a> <i>prerelease</i></dt><dd><p>The strategy to use when considering pre-release versions.</p>
<p>By default, uv will accept pre-releases for packages that <em>only</em> publish pre-releases, along with first-party requirements that contain an explicit pre-release marker in the declared specifiers (<code>if-necessary-or-explicit</code>).</p>
<p>May also be set with the <code>UV_PRERELEASE</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>disallow</code>:  Disallow all pre-release versions</li>
<li><code>allow</code>:  Allow all pre-release versions</li>
<li><code>if-necessary</code>:  Allow pre-release versions if all versions of a package are pre-release</li>
<li><code>explicit</code>:  Allow pre-release versions for first-party packages with explicit pre-release markers in their version requirements</li>
<li><code>if-necessary-or-explicit</code>:  Allow pre-release versions if all versions of a package are pre-release, or if the package has an explicit pre-release marker in its version requirements</li>
</ul></dd><dt id="uv-tree--project"><a href="#uv-tree--project"><code>--project</code></a> <i>project</i></dt><dd><p>Run the command within the given project directory.</p>
<p>All <code>pyproject.toml</code>, <code>uv.toml</code>, and <code>.python-version</code> files will be discovered by walking up the directory tree from the project root, as will the project's virtual environment (<code>.venv</code>).</p>
<p>Other command-line arguments (such as relative paths) will be resolved relative to the current working directory.</p>
<p>See <code>--directory</code> to change the working directory entirely.</p>
<p>This setting has no effect when used in the <code>uv pip</code> interface.</p>
<p>May also be set with the <code>UV_PROJECT</code> environment variable.</p></dd><dt id="uv-tree--prune"><a href="#uv-tree--prune"><code>--prune</code></a> <i>prune</i></dt><dd><p>Prune the given package from the display of the dependency tree</p>
</dd><dt id="uv-tree--python"><a href="#uv-tree--python"><code>--python</code></a>, <code>-p</code> <i>python</i></dt><dd><p>The Python interpreter to use for locking and filtering.</p>
<p>By default, the tree is filtered to match the platform as reported by the Python
interpreter. Use <code>--universal</code> to display the tree for all platforms, or use
<code>--python-version</code> or <code>--python-platform</code> to override a subset of markers.</p>
<p>See <a href="#uv-python">uv python</a> for details on Python discovery and supported request formats.</p>
<p>May also be set with the <code>UV_PYTHON</code> environment variable.</p></dd><dt id="uv-tree--python-platform"><a href="#uv-tree--python-platform"><code>--python-platform</code></a> <i>python-platform</i></dt><dd><p>The platform to use when filtering the tree.</p>
<p>For example, pass <code>--platform windows</code> to display the dependencies that would be included when installing on Windows.</p>
<p>Represented as a &quot;target triple&quot;, a string that describes the target platform in terms of its CPU, vendor, and operating system name, like <code>x86_64-unknown-linux-gnu</code> or <code>aarch64-apple-darwin</code>.</p>
<p>Possible values:</p>
<ul>
<li><code>windows</code>:  An alias for <code>x86_64-pc-windows-msvc</code>, the default target for Windows</li>
<li><code>linux</code>:  An alias for <code>x86_64-unknown-linux-gnu</code>, the default target for Linux</li>
<li><code>macos</code>:  An alias for <code>aarch64-apple-darwin</code>, the default target for macOS</li>
<li><code>x86_64-pc-windows-msvc</code>:  A 64-bit x86 Windows target</li>
<li><code>i686-pc-windows-msvc</code>:  A 32-bit x86 Windows target</li>
<li><code>x86_64-unknown-linux-gnu</code>:  An x86 Linux target. Equivalent to <code>x86_64-manylinux_2_17</code></li>
<li><code>aarch64-apple-darwin</code>:  An ARM-based macOS target, as seen on Apple Silicon devices</li>
<li><code>x86_64-apple-darwin</code>:  An x86 macOS target</li>
<li><code>aarch64-unknown-linux-gnu</code>:  An ARM64 Linux target. Equivalent to <code>aarch64-manylinux_2_17</code></li>
<li><code>aarch64-unknown-linux-musl</code>:  An ARM64 Linux target</li>
<li><code>x86_64-unknown-linux-musl</code>:  An <code>x86_64</code> Linux target</li>
<li><code>x86_64-manylinux2014</code>:  An <code>x86_64</code> target for the <code>manylinux2014</code> platform. Equivalent to <code>x86_64-manylinux_2_17</code></li>
<li><code>x86_64-manylinux_2_17</code>:  An <code>x86_64</code> target for the <code>manylinux_2_17</code> platform</li>
<li><code>x86_64-manylinux_2_28</code>:  An <code>x86_64</code> target for the <code>manylinux_2_28</code> platform</li>
<li><code>x86_64-manylinux_2_31</code>:  An <code>x86_64</code> target for the <code>manylinux_2_31</code> platform</li>
<li><code>x86_64-manylinux_2_32</code>:  An <code>x86_64</code> target for the <code>manylinux_2_32</code> platform</li>
<li><code>x86_64-manylinux_2_33</code>:  An <code>x86_64</code> target for the <code>manylinux_2_33</code> platform</li>
<li><code>x86_64-manylinux_2_34</code>:  An <code>x86_64</code> target for the <code>manylinux_2_34</code> platform</li>
<li><code>x86_64-manylinux_2_35</code>:  An <code>x86_64</code> target for the <code>manylinux_2_35</code> platform</li>
<li><code>x86_64-manylinux_2_36</code>:  An <code>x86_64</code> target for the <code>manylinux_2_36</code> platform</li>
<li><code>x86_64-manylinux_2_37</code>:  An <code>x86_64</code> target for the <code>manylinux_2_37</code> platform</li>
<li><code>x86_64-manylinux_2_38</code>:  An <code>x86_64</code> target for the <code>manylinux_2_38</code> platform</li>
<li><code>x86_64-manylinux_2_39</code>:  An <code>x86_64</code> target for the <code>manylinux_2_39</code> platform</li>
<li><code>x86_64-manylinux_2_40</code>:  An <code>x86_64</code> target for the <code>manylinux_2_40</code> platform</li>
<li><code>aarch64-manylinux2014</code>:  An ARM64 target for the <code>manylinux2014</code> platform. Equivalent to <code>aarch64-manylinux_2_17</code></li>
<li><code>aarch64-manylinux_2_17</code>:  An ARM64 target for the <code>manylinux_2_17</code> platform</li>
<li><code>aarch64-manylinux_2_28</code>:  An ARM64 target for the <code>manylinux_2_28</code> platform</li>
<li><code>aarch64-manylinux_2_31</code>:  An ARM64 target for the <code>manylinux_2_31</code> platform</li>
<li><code>aarch64-manylinux_2_32</code>:  An ARM64 target for the <code>manylinux_2_32</code> platform</li>
<li><code>aarch64-manylinux_2_33</code>:  An ARM64 target for the <code>manylinux_2_33</code> platform</li>
<li><code>aarch64-manylinux_2_34</code>:  An ARM64 target for the <code>manylinux_2_34</code> platform</li>
<li><code>aarch64-manylinux_2_35</code>:  An ARM64 target for the <code>manylinux_2_35</code> platform</li>
<li><code>aarch64-manylinux_2_36</code>:  An ARM64 target for the <code>manylinux_2_36</code> platform</li>
<li><code>aarch64-manylinux_2_37</code>:  An ARM64 target for the <code>manylinux_2_37</code> platform</li>
<li><code>aarch64-manylinux_2_38</code>:  An ARM64 target for the <code>manylinux_2_38</code> platform</li>
<li><code>aarch64-manylinux_2_39</code>:  An ARM64 target for the <code>manylinux_2_39</code> platform</li>
<li><code>aarch64-manylinux_2_40</code>:  An ARM64 target for the <code>manylinux_2_40</code> platform</li>
</ul></dd><dt id="uv-tree--python-version"><a href="#uv-tree--python-version"><code>--python-version</code></a> <i>python-version</i></dt><dd><p>The Python version to use when filtering the tree.</p>
<p>For example, pass <code>--python-version 3.10</code> to display the dependencies that would be included when installing on Python 3.10.</p>
<p>Defaults to the version of the discovered Python interpreter.</p>
</dd><dt id="uv-tree--quiet"><a href="#uv-tree--quiet"><code>--quiet</code></a>, <code>-q</code></dt><dd><p>Use quiet output.</p>
<p>Repeating this option, e.g., <code>-qq</code>, will enable a silent mode in which uv will write no output to stdout.</p>
</dd><dt id="uv-tree--resolution"><a href="#uv-tree--resolution"><code>--resolution</code></a> <i>resolution</i></dt><dd><p>The strategy to use when selecting between the different compatible versions for a given package requirement.</p>
<p>By default, uv will use the latest compatible version of each package (<code>highest</code>).</p>
<p>May also be set with the <code>UV_RESOLUTION</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>highest</code>:  Resolve the highest compatible version of each package</li>
<li><code>lowest</code>:  Resolve the lowest compatible version of each package</li>
<li><code>lowest-direct</code>:  Resolve the lowest compatible version of any direct dependencies, and the highest compatible version of any transitive dependencies</li>
</ul></dd><dt id="uv-tree--script"><a href="#uv-tree--script"><code>--script</code></a> <i>script</i></dt><dd><p>Show the dependency tree the specified PEP 723 Python script, rather than the current project.</p>
<p>If provided, uv will resolve the dependencies based on its inline metadata table, in adherence with PEP 723.</p>
</dd><dt id="uv-tree--universal"><a href="#uv-tree--universal"><code>--universal</code></a></dt><dd><p>Show a platform-independent dependency tree.</p>
<p>Shows resolved package versions for all Python versions and platforms, rather than filtering to those that are relevant for the current environment.</p>
<p>Multiple versions may be shown for a each package.</p>
</dd><dt id="uv-tree--upgrade"><a href="#uv-tree--upgrade"><code>--upgrade</code></a>, <code>-U</code></dt><dd><p>Allow package upgrades, ignoring pinned versions in any existing output file. Implies <code>--refresh</code></p>
</dd><dt id="uv-tree--upgrade-package"><a href="#uv-tree--upgrade-package"><code>--upgrade-package</code></a>, <code>-P</code> <i>upgrade-package</i></dt><dd><p>Allow upgrades for a specific package, ignoring pinned versions in any existing output file. Implies <code>--refresh-package</code></p>
</dd><dt id="uv-tree--verbose"><a href="#uv-tree--verbose"><code>--verbose</code></a>, <code>-v</code></dt><dd><p>Use verbose output.</p>
<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (<a href="https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives">https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives</a>)</p>
</dd></dl>

## uv tool

Run and install commands provided by Python packages

<h3 class="cli-reference">Usage</h3>

```
uv tool [OPTIONS] <COMMAND>
```

<h3 class="cli-reference">Commands</h3>

<dl class="cli-reference"><dt><a href="#uv-tool-run"><code>uv tool run</code></a></dt><dd><p>Run a command provided by a Python package</p></dd>
<dt><a href="#uv-tool-install"><code>uv tool install</code></a></dt><dd><p>Install commands provided by a Python package</p></dd>
<dt><a href="#uv-tool-upgrade"><code>uv tool upgrade</code></a></dt><dd><p>Upgrade installed tools</p></dd>
<dt><a href="#uv-tool-list"><code>uv tool list</code></a></dt><dd><p>List installed tools</p></dd>
<dt><a href="#uv-tool-uninstall"><code>uv tool uninstall</code></a></dt><dd><p>Uninstall a tool</p></dd>
<dt><a href="#uv-tool-update-shell"><code>uv tool update-shell</code></a></dt><dd><p>Ensure that the tool executable directory is on the <code>PATH</code></p></dd>
<dt><a href="#uv-tool-dir"><code>uv tool dir</code></a></dt><dd><p>Show the path to the uv tools directory</p></dd>
</dl>

### uv tool run

Run a command provided by a Python package.

By default, the package to install is assumed to match the command name.

The name of the command can include an exact version in the format `<package>@<version>`, e.g., `uv tool run ruff@0.3.0`. If more complex version specification is desired or if the command is provided by a different package, use `--from`.

`uvx` can be used to invoke Python, e.g., with `uvx python` or `uvx python@<version>`. A Python interpreter will be started in an isolated virtual environment.

If the tool was previously installed, i.e., via `uv tool install`, the installed version will be used unless a version is requested or the `--isolated` flag is used.

`uvx` is provided as a convenient alias for `uv tool run`, their behavior is identical.

If no command is provided, the installed tools are displayed.

Packages are installed into an ephemeral virtual environment in the uv cache directory.

<h3 class="cli-reference">Usage</h3>

```
uv tool run [OPTIONS] [COMMAND]
```

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt id="uv-tool-run--allow-insecure-host"><a href="#uv-tool-run--allow-insecure-host"><code>--allow-insecure-host</code></a>, <code>--trusted-host</code> <i>allow-insecure-host</i></dt><dd><p>Allow insecure connections to a host.</p>
<p>Can be provided multiple times.</p>
<p>Expects to receive either a hostname (e.g., <code>localhost</code>), a host-port pair (e.g., <code>localhost:8080</code>), or a URL (e.g., <code>https://localhost</code>).</p>
<p>WARNING: Hosts included in this list will not be verified against the system's certificate store. Only use <code>--allow-insecure-host</code> in a secure network with verified sources, as it bypasses SSL verification and could expose you to MITM attacks.</p>
<p>May also be set with the <code>UV_INSECURE_HOST</code> environment variable.</p></dd><dt id="uv-tool-run--build-constraints"><a href="#uv-tool-run--build-constraints"><code>--build-constraints</code></a>, <code>--build-constraint</code>, <code>-b</code> <i>build-constraints</i></dt><dd><p>Constrain build dependencies using the given requirements files when building source distributions.</p>
<p>Constraints files are <code>requirements.txt</code>-like files that only control the <em>version</em> of a requirement that's installed. However, including a package in a constraints file will <em>not</em> trigger the installation of that package.</p>
<p>May also be set with the <code>UV_BUILD_CONSTRAINT</code> environment variable.</p></dd><dt id="uv-tool-run--cache-dir"><a href="#uv-tool-run--cache-dir"><code>--cache-dir</code></a> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>
<p>Defaults to <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on macOS and Linux, and <code>%LOCALAPPDATA%\uv\cache</code> on Windows.</p>
<p>To view the location of the cache directory, run <code>uv cache dir</code>.</p>
<p>May also be set with the <code>UV_CACHE_DIR</code> environment variable.</p></dd><dt id="uv-tool-run--color"><a href="#uv-tool-run--color"><code>--color</code></a> <i>color-choice</i></dt><dd><p>Control the use of color in output.</p>
<p>By default, uv will automatically detect support for colors when writing to a terminal.</p>
<p>Possible values:</p>
<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>
<li><code>always</code>:  Enables colored output regardless of the detected environment</li>
<li><code>never</code>:  Disables colored output</li>
</ul></dd><dt id="uv-tool-run--compile-bytecode"><a href="#uv-tool-run--compile-bytecode"><code>--compile-bytecode</code></a>, <code>--compile</code></dt><dd><p>Compile Python files to bytecode after installation.</p>
<p>By default, uv does not compile Python (<code>.py</code>) files to bytecode (<code>__pycache__/*.pyc</code>); instead, compilation is performed lazily the first time a module is imported. For use-cases in which start time is critical, such as CLI applications and Docker containers, this option can be enabled to trade longer installation times for faster start times.</p>
<p>When enabled, uv will process the entire site-packages directory (including packages that are not being modified by the current operation) for consistency. Like pip, it will also ignore errors.</p>
<p>May also be set with the <code>UV_COMPILE_BYTECODE</code> environment variable.</p></dd><dt id="uv-tool-run--config-file"><a href="#uv-tool-run--config-file"><code>--config-file</code></a> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>
<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>
<p>May also be set with the <code>UV_CONFIG_FILE</code> environment variable.</p></dd><dt id="uv-tool-run--config-setting"><a href="#uv-tool-run--config-setting"><code>--config-setting</code></a>, <code>--config-settings</code>, <code>-C</code> <i>config-setting</i></dt><dd><p>Settings to pass to the PEP 517 build backend, specified as <code>KEY=VALUE</code> pairs</p>
</dd><dt id="uv-tool-run--constraints"><a href="#uv-tool-run--constraints"><code>--constraints</code></a>, <code>--constraint</code>, <code>-c</code> <i>constraints</i></dt><dd><p>Constrain versions using the given requirements files.</p>
<p>Constraints files are <code>requirements.txt</code>-like files that only control the <em>version</em> of a requirement that's installed. However, including a package in a constraints file will <em>not</em> trigger the installation of that package.</p>
<p>This is equivalent to pip's <code>--constraint</code> option.</p>
<p>May also be set with the <code>UV_CONSTRAINT</code> environment variable.</p></dd><dt id="uv-tool-run--default-index"><a href="#uv-tool-run--default-index"><code>--default-index</code></a> <i>default-index</i></dt><dd><p>The URL of the default package index (by default: <a href="https://pypi.org/simple">https://pypi.org/simple</a>).</p>
<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>
<p>The index given by this flag is given lower priority than all other indexes specified via the <code>--index</code> flag.</p>
<p>May also be set with the <code>UV_DEFAULT_INDEX</code> environment variable.</p></dd><dt id="uv-tool-run--directory"><a href="#uv-tool-run--directory"><code>--directory</code></a> <i>directory</i></dt><dd><p>Change to the given directory prior to running the command.</p>
<p>Relative paths are resolved with the given directory as the base.</p>
<p>See <code>--project</code> to only change the project root directory.</p>
</dd><dt id="uv-tool-run--env-file"><a href="#uv-tool-run--env-file"><code>--env-file</code></a> <i>env-file</i></dt><dd><p>Load environment variables from a <code>.env</code> file.</p>
<p>Can be provided multiple times, with subsequent files overriding values defined in previous files.</p>
<p>May also be set with the <code>UV_ENV_FILE</code> environment variable.</p></dd><dt id="uv-tool-run--exclude-newer"><a href="#uv-tool-run--exclude-newer"><code>--exclude-newer</code></a> <i>exclude-newer</i></dt><dd><p>Limit candidate packages to those that were uploaded prior to the given date.</p>
<p>Accepts both RFC 3339 timestamps (e.g., <code>2006-12-02T02:07:43Z</code>) and local dates in the same format (e.g., <code>2006-12-02</code>) in your system's configured time zone.</p>
<p>May also be set with the <code>UV_EXCLUDE_NEWER</code> environment variable.</p></dd><dt id="uv-tool-run--extra-index-url"><a href="#uv-tool-run--extra-index-url"><code>--extra-index-url</code></a> <i>extra-index-url</i></dt><dd><p>(Deprecated: use <code>--index</code> instead) Extra URLs of package indexes to use, in addition to <code>--index-url</code>.</p>
<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>
<p>All indexes provided via this flag take priority over the index specified by <code>--index-url</code> (which defaults to PyPI). When multiple <code>--extra-index-url</code> flags are provided, earlier values take priority.</p>
<p>May also be set with the <code>UV_EXTRA_INDEX_URL</code> environment variable.</p></dd><dt id="uv-tool-run--find-links"><a href="#uv-tool-run--find-links"><code>--find-links</code></a>, <code>-f</code> <i>find-links</i></dt><dd><p>Locations to search for candidate distributions, in addition to those found in the registry indexes.</p>
<p>If a path, the target must be a directory that contains packages as wheel files (<code>.whl</code>) or source distributions (e.g., <code>.tar.gz</code> or <code>.zip</code>) at the top level.</p>
<p>If a URL, the page must contain a flat list of links to package files adhering to the formats described above.</p>
<p>May also be set with the <code>UV_FIND_LINKS</code> environment variable.</p></dd><dt id="uv-tool-run--fork-strategy"><a href="#uv-tool-run--fork-strategy"><code>--fork-strategy</code></a> <i>fork-strategy</i></dt><dd><p>The strategy to use when selecting multiple versions of a given package across Python versions and platforms.</p>
<p>By default, uv will optimize for selecting the latest version of each package for each supported Python version (<code>requires-python</code>), while minimizing the number of selected versions across platforms.</p>
<p>Under <code>fewest</code>, uv will minimize the number of selected versions for each package, preferring older versions that are compatible with a wider range of supported Python versions or platforms.</p>
<p>May also be set with the <code>UV_FORK_STRATEGY</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>fewest</code>:  Optimize for selecting the fewest number of versions for each package. Older versions may be preferred if they are compatible with a wider range of supported Python versions or platforms</li>
<li><code>requires-python</code>:  Optimize for selecting latest supported version of each package, for each supported Python version</li>
</ul></dd><dt id="uv-tool-run--from"><a href="#uv-tool-run--from"><code>--from</code></a> <i>from</i></dt><dd><p>Use the given package to provide the command.</p>
<p>By default, the package name is assumed to match the command name.</p>
</dd><dt id="uv-tool-run--help"><a href="#uv-tool-run--help"><code>--help</code></a>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>
</dd><dt id="uv-tool-run--index"><a href="#uv-tool-run--index"><code>--index</code></a> <i>index</i></dt><dd><p>The URLs to use when resolving dependencies, in addition to the default index.</p>
<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>
<p>All indexes provided via this flag take priority over the index specified by <code>--default-index</code> (which defaults to PyPI). When multiple <code>--index</code> flags are provided, earlier values take priority.</p>
<p>May also be set with the <code>UV_INDEX</code> environment variable.</p></dd><dt id="uv-tool-run--index-strategy"><a href="#uv-tool-run--index-strategy"><code>--index-strategy</code></a> <i>index-strategy</i></dt><dd><p>The strategy to use when resolving against multiple index URLs.</p>
<p>By default, uv will stop at the first index on which a given package is available, and limit resolutions to those present on that first index (<code>first-index</code>). This prevents &quot;dependency confusion&quot; attacks, whereby an attacker can upload a malicious package under the same name to an alternate index.</p>
<p>May also be set with the <code>UV_INDEX_STRATEGY</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>first-index</code>:  Only use results from the first index that returns a match for a given package name</li>
<li><code>unsafe-first-match</code>:  Search for every package name across all indexes, exhausting the versions from the first index before moving on to the next</li>
<li><code>unsafe-best-match</code>:  Search for every package name across all indexes, preferring the &quot;best&quot; version found. If a package version is in multiple indexes, only look at the entry for the first index</li>
</ul></dd><dt id="uv-tool-run--index-url"><a href="#uv-tool-run--index-url"><code>--index-url</code></a>, <code>-i</code> <i>index-url</i></dt><dd><p>(Deprecated: use <code>--default-index</code> instead) The URL of the Python package index (by default: <a href="https://pypi.org/simple">https://pypi.org/simple</a>).</p>
<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>
<p>The index given by this flag is given lower priority than all other indexes specified via the <code>--extra-index-url</code> flag.</p>
<p>May also be set with the <code>UV_INDEX_URL</code> environment variable.</p></dd><dt id="uv-tool-run--isolated"><a href="#uv-tool-run--isolated"><code>--isolated</code></a></dt><dd><p>Run the tool in an isolated virtual environment, ignoring any already-installed tools</p>
</dd><dt id="uv-tool-run--keyring-provider"><a href="#uv-tool-run--keyring-provider"><code>--keyring-provider</code></a> <i>keyring-provider</i></dt><dd><p>Attempt to use <code>keyring</code> for authentication for index URLs.</p>
<p>At present, only <code>--keyring-provider subprocess</code> is supported, which configures uv to use the <code>keyring</code> CLI to handle authentication.</p>
<p>Defaults to <code>disabled</code>.</p>
<p>May also be set with the <code>UV_KEYRING_PROVIDER</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>disabled</code>:  Do not use keyring for credential lookup</li>
<li><code>subprocess</code>:  Use the <code>keyring</code> command for credential lookup</li>
</ul></dd><dt id="uv-tool-run--link-mode"><a href="#uv-tool-run--link-mode"><code>--link-mode</code></a> <i>link-mode</i></dt><dd><p>The method to use when installing packages from the global cache.</p>
<p>Defaults to <code>clone</code> (also known as Copy-on-Write) on macOS, and <code>hardlink</code> on Linux and Windows.</p>
<p>May also be set with the <code>UV_LINK_MODE</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>clone</code>:  Clone (i.e., copy-on-write) packages from the wheel into the <code>site-packages</code> directory</li>
<li><code>copy</code>:  Copy packages from the wheel into the <code>site-packages</code> directory</li>
<li><code>hardlink</code>:  Hard link packages from the wheel into the <code>site-packages</code> directory</li>
<li><code>symlink</code>:  Symbolically link packages from the wheel into the <code>site-packages</code> directory</li>
</ul></dd><dt id="uv-tool-run--managed-python"><a href="#uv-tool-run--managed-python"><code>--managed-python</code></a></dt><dd><p>Require use of uv-managed Python versions.</p>
<p>By default, uv prefers using Python versions it manages. However, it will use system Python versions if a uv-managed Python is not installed. This option disables use of system Python versions.</p>
<p>May also be set with the <code>UV_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-tool-run--native-tls"><a href="#uv-tool-run--native-tls"><code>--native-tls</code></a></dt><dd><p>Whether to load TLS certificates from the platform's native certificate store.</p>
<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>
<p>However, in some cases, you may want to use the platform's native certificate store, especially if you're relying on a corporate trust root (e.g., for a mandatory proxy) that's included in your system's certificate store.</p>
<p>May also be set with the <code>UV_NATIVE_TLS</code> environment variable.</p></dd><dt id="uv-tool-run--no-binary"><a href="#uv-tool-run--no-binary"><code>--no-binary</code></a></dt><dd><p>Don't install pre-built wheels.</p>
<p>The given packages will be built and installed from source. The resolver will still use pre-built wheels to extract package metadata, if available.</p>
<p>May also be set with the <code>UV_NO_BINARY</code> environment variable.</p></dd><dt id="uv-tool-run--no-binary-package"><a href="#uv-tool-run--no-binary-package"><code>--no-binary-package</code></a> <i>no-binary-package</i></dt><dd><p>Don't install pre-built wheels for a specific package</p>
<p>May also be set with the <code>UV_NO_BINARY_PACKAGE</code> environment variable.</p></dd><dt id="uv-tool-run--no-build"><a href="#uv-tool-run--no-build"><code>--no-build</code></a></dt><dd><p>Don't build source distributions.</p>
<p>When enabled, resolving will not run arbitrary Python code. The cached wheels of already-built source distributions will be reused, but operations that require building distributions will exit with an error.</p>
<p>May also be set with the <code>UV_NO_BUILD</code> environment variable.</p></dd><dt id="uv-tool-run--no-build-isolation"><a href="#uv-tool-run--no-build-isolation"><code>--no-build-isolation</code></a></dt><dd><p>Disable isolation when building source distributions.</p>
<p>Assumes that build dependencies specified by PEP 518 are already installed.</p>
<p>May also be set with the <code>UV_NO_BUILD_ISOLATION</code> environment variable.</p></dd><dt id="uv-tool-run--no-build-isolation-package"><a href="#uv-tool-run--no-build-isolation-package"><code>--no-build-isolation-package</code></a> <i>no-build-isolation-package</i></dt><dd><p>Disable isolation when building source distributions for a specific package.</p>
<p>Assumes that the packages' build dependencies specified by PEP 518 are already installed.</p>
</dd><dt id="uv-tool-run--no-build-package"><a href="#uv-tool-run--no-build-package"><code>--no-build-package</code></a> <i>no-build-package</i></dt><dd><p>Don't build source distributions for a specific package</p>
<p>May also be set with the <code>UV_NO_BUILD_PACKAGE</code> environment variable.</p></dd><dt id="uv-tool-run--no-cache"><a href="#uv-tool-run--no-cache"><code>--no-cache</code></a>, <code>--no-cache-dir</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>
<p>May also be set with the <code>UV_NO_CACHE</code> environment variable.</p></dd><dt id="uv-tool-run--no-config"><a href="#uv-tool-run--no-config"><code>--no-config</code></a></dt><dd><p>Avoid discovering configuration files (<code>pyproject.toml</code>, <code>uv.toml</code>).</p>
<p>Normally, configuration files are discovered in the current directory, parent directories, or user configuration directories.</p>
<p>May also be set with the <code>UV_NO_CONFIG</code> environment variable.</p></dd><dt id="uv-tool-run--no-env-file"><a href="#uv-tool-run--no-env-file"><code>--no-env-file</code></a></dt><dd><p>Avoid reading environment variables from a <code>.env</code> file</p>
<p>May also be set with the <code>UV_NO_ENV_FILE</code> environment variable.</p></dd><dt id="uv-tool-run--no-index"><a href="#uv-tool-run--no-index"><code>--no-index</code></a></dt><dd><p>Ignore the registry index (e.g., PyPI), instead relying on direct URL dependencies and those provided via <code>--find-links</code></p>
</dd><dt id="uv-tool-run--no-managed-python"><a href="#uv-tool-run--no-managed-python"><code>--no-managed-python</code></a></dt><dd><p>Disable use of uv-managed Python versions.</p>
<p>Instead, uv will search for a suitable Python version on the system.</p>
<p>May also be set with the <code>UV_NO_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-tool-run--no-progress"><a href="#uv-tool-run--no-progress"><code>--no-progress</code></a></dt><dd><p>Hide all progress outputs.</p>
<p>For example, spinners or progress bars.</p>
<p>May also be set with the <code>UV_NO_PROGRESS</code> environment variable.</p></dd><dt id="uv-tool-run--no-python-downloads"><a href="#uv-tool-run--no-python-downloads"><code>--no-python-downloads</code></a></dt><dd><p>Disable automatic downloads of Python.</p>
</dd><dt id="uv-tool-run--no-sources"><a href="#uv-tool-run--no-sources"><code>--no-sources</code></a></dt><dd><p>Ignore the <code>tool.uv.sources</code> table when resolving dependencies. Used to lock against the standards-compliant, publishable package metadata, as opposed to using any workspace, Git, URL, or local path sources</p>
</dd><dt id="uv-tool-run--offline"><a href="#uv-tool-run--offline"><code>--offline</code></a></dt><dd><p>Disable network access.</p>
<p>When disabled, uv will only use locally cached data and locally available files.</p>
<p>May also be set with the <code>UV_OFFLINE</code> environment variable.</p></dd><dt id="uv-tool-run--overrides"><a href="#uv-tool-run--overrides"><code>--overrides</code></a>, <code>--override</code> <i>overrides</i></dt><dd><p>Override versions using the given requirements files.</p>
<p>Overrides files are <code>requirements.txt</code>-like files that force a specific version of a requirement to be installed, regardless of the requirements declared by any constituent package, and regardless of whether this would be considered an invalid resolution.</p>
<p>While constraints are <em>additive</em>, in that they're combined with the requirements of the constituent packages, overrides are <em>absolute</em>, in that they completely replace the requirements of the constituent packages.</p>
<p>May also be set with the <code>UV_OVERRIDE</code> environment variable.</p></dd><dt id="uv-tool-run--prerelease"><a href="#uv-tool-run--prerelease"><code>--prerelease</code></a> <i>prerelease</i></dt><dd><p>The strategy to use when considering pre-release versions.</p>
<p>By default, uv will accept pre-releases for packages that <em>only</em> publish pre-releases, along with first-party requirements that contain an explicit pre-release marker in the declared specifiers (<code>if-necessary-or-explicit</code>).</p>
<p>May also be set with the <code>UV_PRERELEASE</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>disallow</code>:  Disallow all pre-release versions</li>
<li><code>allow</code>:  Allow all pre-release versions</li>
<li><code>if-necessary</code>:  Allow pre-release versions if all versions of a package are pre-release</li>
<li><code>explicit</code>:  Allow pre-release versions for first-party packages with explicit pre-release markers in their version requirements</li>
<li><code>if-necessary-or-explicit</code>:  Allow pre-release versions if all versions of a package are pre-release, or if the package has an explicit pre-release marker in its version requirements</li>
</ul></dd><dt id="uv-tool-run--project"><a href="#uv-tool-run--project"><code>--project</code></a> <i>project</i></dt><dd><p>Run the command within the given project directory.</p>
<p>All <code>pyproject.toml</code>, <code>uv.toml</code>, and <code>.python-version</code> files will be discovered by walking up the directory tree from the project root, as will the project's virtual environment (<code>.venv</code>).</p>
<p>Other command-line arguments (such as relative paths) will be resolved relative to the current working directory.</p>
<p>See <code>--directory</code> to change the working directory entirely.</p>
<p>This setting has no effect when used in the <code>uv pip</code> interface.</p>
<p>May also be set with the <code>UV_PROJECT</code> environment variable.</p></dd><dt id="uv-tool-run--python"><a href="#uv-tool-run--python"><code>--python</code></a>, <code>-p</code> <i>python</i></dt><dd><p>The Python interpreter to use to build the run environment.</p>
<p>See <a href="#uv-python">uv python</a> for details on Python discovery and supported request formats.</p>
<p>May also be set with the <code>UV_PYTHON</code> environment variable.</p></dd><dt id="uv-tool-run--quiet"><a href="#uv-tool-run--quiet"><code>--quiet</code></a>, <code>-q</code></dt><dd><p>Use quiet output.</p>
<p>Repeating this option, e.g., <code>-qq</code>, will enable a silent mode in which uv will write no output to stdout.</p>
</dd><dt id="uv-tool-run--refresh"><a href="#uv-tool-run--refresh"><code>--refresh</code></a></dt><dd><p>Refresh all cached data</p>
</dd><dt id="uv-tool-run--refresh-package"><a href="#uv-tool-run--refresh-package"><code>--refresh-package</code></a> <i>refresh-package</i></dt><dd><p>Refresh cached data for a specific package</p>
</dd><dt id="uv-tool-run--reinstall"><a href="#uv-tool-run--reinstall"><code>--reinstall</code></a>, <code>--force-reinstall</code></dt><dd><p>Reinstall all packages, regardless of whether they're already installed. Implies <code>--refresh</code></p>
</dd><dt id="uv-tool-run--reinstall-package"><a href="#uv-tool-run--reinstall-package"><code>--reinstall-package</code></a> <i>reinstall-package</i></dt><dd><p>Reinstall a specific package, regardless of whether it's already installed. Implies <code>--refresh-package</code></p>
</dd><dt id="uv-tool-run--resolution"><a href="#uv-tool-run--resolution"><code>--resolution</code></a> <i>resolution</i></dt><dd><p>The strategy to use when selecting between the different compatible versions for a given package requirement.</p>
<p>By default, uv will use the latest compatible version of each package (<code>highest</code>).</p>
<p>May also be set with the <code>UV_RESOLUTION</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>highest</code>:  Resolve the highest compatible version of each package</li>
<li><code>lowest</code>:  Resolve the lowest compatible version of each package</li>
<li><code>lowest-direct</code>:  Resolve the lowest compatible version of any direct dependencies, and the highest compatible version of any transitive dependencies</li>
</ul></dd><dt id="uv-tool-run--upgrade"><a href="#uv-tool-run--upgrade"><code>--upgrade</code></a>, <code>-U</code></dt><dd><p>Allow package upgrades, ignoring pinned versions in any existing output file. Implies <code>--refresh</code></p>
</dd><dt id="uv-tool-run--upgrade-package"><a href="#uv-tool-run--upgrade-package"><code>--upgrade-package</code></a>, <code>-P</code> <i>upgrade-package</i></dt><dd><p>Allow upgrades for a specific package, ignoring pinned versions in any existing output file. Implies <code>--refresh-package</code></p>
</dd><dt id="uv-tool-run--verbose"><a href="#uv-tool-run--verbose"><code>--verbose</code></a>, <code>-v</code></dt><dd><p>Use verbose output.</p>
<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (<a href="https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives">https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives</a>)</p>
</dd><dt id="uv-tool-run--with"><a href="#uv-tool-run--with"><code>--with</code></a> <i>with</i></dt><dd><p>Run with the given packages installed</p>
</dd><dt id="uv-tool-run--with-editable"><a href="#uv-tool-run--with-editable"><code>--with-editable</code></a> <i>with-editable</i></dt><dd><p>Run with the given packages installed in editable mode</p>
<p>When used in a project, these dependencies will be layered on top of the uv tool's environment in a separate, ephemeral environment. These dependencies are allowed to conflict with those specified.</p>
</dd><dt id="uv-tool-run--with-requirements"><a href="#uv-tool-run--with-requirements"><code>--with-requirements</code></a> <i>with-requirements</i></dt><dd><p>Run with all packages listed in the given <code>requirements.txt</code> files</p>
</dd></dl>

### uv tool install

Install commands provided by a Python package.

Packages are installed into an isolated virtual environment in the uv tools directory. The executables are linked the tool executable directory, which is determined according to the XDG standard and can be retrieved with `uv tool dir --bin`.

If the tool was previously installed, the existing tool will generally be replaced.

<h3 class="cli-reference">Usage</h3>

```
uv tool install [OPTIONS] <PACKAGE>
```

<h3 class="cli-reference">Arguments</h3>

<dl class="cli-reference"><dt id="uv-tool-install--package"><a href="#uv-tool-install--package"<code>PACKAGE</code></a></dt><dd><p>The package to install commands from</p>
</dd></dl>

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt id="uv-tool-install--allow-insecure-host"><a href="#uv-tool-install--allow-insecure-host"><code>--allow-insecure-host</code></a>, <code>--trusted-host</code> <i>allow-insecure-host</i></dt><dd><p>Allow insecure connections to a host.</p>
<p>Can be provided multiple times.</p>
<p>Expects to receive either a hostname (e.g., <code>localhost</code>), a host-port pair (e.g., <code>localhost:8080</code>), or a URL (e.g., <code>https://localhost</code>).</p>
<p>WARNING: Hosts included in this list will not be verified against the system's certificate store. Only use <code>--allow-insecure-host</code> in a secure network with verified sources, as it bypasses SSL verification and could expose you to MITM attacks.</p>
<p>May also be set with the <code>UV_INSECURE_HOST</code> environment variable.</p></dd><dt id="uv-tool-install--build-constraints"><a href="#uv-tool-install--build-constraints"><code>--build-constraints</code></a>, <code>--build-constraint</code>, <code>-b</code> <i>build-constraints</i></dt><dd><p>Constrain build dependencies using the given requirements files when building source distributions.</p>
<p>Constraints files are <code>requirements.txt</code>-like files that only control the <em>version</em> of a requirement that's installed. However, including a package in a constraints file will <em>not</em> trigger the installation of that package.</p>
<p>May also be set with the <code>UV_BUILD_CONSTRAINT</code> environment variable.</p></dd><dt id="uv-tool-install--cache-dir"><a href="#uv-tool-install--cache-dir"><code>--cache-dir</code></a> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>
<p>Defaults to <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on macOS and Linux, and <code>%LOCALAPPDATA%\uv\cache</code> on Windows.</p>
<p>To view the location of the cache directory, run <code>uv cache dir</code>.</p>
<p>May also be set with the <code>UV_CACHE_DIR</code> environment variable.</p></dd><dt id="uv-tool-install--color"><a href="#uv-tool-install--color"><code>--color</code></a> <i>color-choice</i></dt><dd><p>Control the use of color in output.</p>
<p>By default, uv will automatically detect support for colors when writing to a terminal.</p>
<p>Possible values:</p>
<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>
<li><code>always</code>:  Enables colored output regardless of the detected environment</li>
<li><code>never</code>:  Disables colored output</li>
</ul></dd><dt id="uv-tool-install--compile-bytecode"><a href="#uv-tool-install--compile-bytecode"><code>--compile-bytecode</code></a>, <code>--compile</code></dt><dd><p>Compile Python files to bytecode after installation.</p>
<p>By default, uv does not compile Python (<code>.py</code>) files to bytecode (<code>__pycache__/*.pyc</code>); instead, compilation is performed lazily the first time a module is imported. For use-cases in which start time is critical, such as CLI applications and Docker containers, this option can be enabled to trade longer installation times for faster start times.</p>
<p>When enabled, uv will process the entire site-packages directory (including packages that are not being modified by the current operation) for consistency. Like pip, it will also ignore errors.</p>
<p>May also be set with the <code>UV_COMPILE_BYTECODE</code> environment variable.</p></dd><dt id="uv-tool-install--config-file"><a href="#uv-tool-install--config-file"><code>--config-file</code></a> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>
<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>
<p>May also be set with the <code>UV_CONFIG_FILE</code> environment variable.</p></dd><dt id="uv-tool-install--config-setting"><a href="#uv-tool-install--config-setting"><code>--config-setting</code></a>, <code>--config-settings</code>, <code>-C</code> <i>config-setting</i></dt><dd><p>Settings to pass to the PEP 517 build backend, specified as <code>KEY=VALUE</code> pairs</p>
</dd><dt id="uv-tool-install--constraints"><a href="#uv-tool-install--constraints"><code>--constraints</code></a>, <code>--constraint</code>, <code>-c</code> <i>constraints</i></dt><dd><p>Constrain versions using the given requirements files.</p>
<p>Constraints files are <code>requirements.txt</code>-like files that only control the <em>version</em> of a requirement that's installed. However, including a package in a constraints file will <em>not</em> trigger the installation of that package.</p>
<p>This is equivalent to pip's <code>--constraint</code> option.</p>
<p>May also be set with the <code>UV_CONSTRAINT</code> environment variable.</p></dd><dt id="uv-tool-install--default-index"><a href="#uv-tool-install--default-index"><code>--default-index</code></a> <i>default-index</i></dt><dd><p>The URL of the default package index (by default: <a href="https://pypi.org/simple">https://pypi.org/simple</a>).</p>
<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>
<p>The index given by this flag is given lower priority than all other indexes specified via the <code>--index</code> flag.</p>
<p>May also be set with the <code>UV_DEFAULT_INDEX</code> environment variable.</p></dd><dt id="uv-tool-install--directory"><a href="#uv-tool-install--directory"><code>--directory</code></a> <i>directory</i></dt><dd><p>Change to the given directory prior to running the command.</p>
<p>Relative paths are resolved with the given directory as the base.</p>
<p>See <code>--project</code> to only change the project root directory.</p>
</dd><dt id="uv-tool-install--editable"><a href="#uv-tool-install--editable"><code>--editable</code></a>, <code>-e</code></dt><dd><p>Install the target package in editable mode, such that changes in the package's source directory are reflected without reinstallation</p>
</dd><dt id="uv-tool-install--exclude-newer"><a href="#uv-tool-install--exclude-newer"><code>--exclude-newer</code></a> <i>exclude-newer</i></dt><dd><p>Limit candidate packages to those that were uploaded prior to the given date.</p>
<p>Accepts both RFC 3339 timestamps (e.g., <code>2006-12-02T02:07:43Z</code>) and local dates in the same format (e.g., <code>2006-12-02</code>) in your system's configured time zone.</p>
<p>May also be set with the <code>UV_EXCLUDE_NEWER</code> environment variable.</p></dd><dt id="uv-tool-install--extra-index-url"><a href="#uv-tool-install--extra-index-url"><code>--extra-index-url</code></a> <i>extra-index-url</i></dt><dd><p>(Deprecated: use <code>--index</code> instead) Extra URLs of package indexes to use, in addition to <code>--index-url</code>.</p>
<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>
<p>All indexes provided via this flag take priority over the index specified by <code>--index-url</code> (which defaults to PyPI). When multiple <code>--extra-index-url</code> flags are provided, earlier values take priority.</p>
<p>May also be set with the <code>UV_EXTRA_INDEX_URL</code> environment variable.</p></dd><dt id="uv-tool-install--find-links"><a href="#uv-tool-install--find-links"><code>--find-links</code></a>, <code>-f</code> <i>find-links</i></dt><dd><p>Locations to search for candidate distributions, in addition to those found in the registry indexes.</p>
<p>If a path, the target must be a directory that contains packages as wheel files (<code>.whl</code>) or source distributions (e.g., <code>.tar.gz</code> or <code>.zip</code>) at the top level.</p>
<p>If a URL, the page must contain a flat list of links to package files adhering to the formats described above.</p>
<p>May also be set with the <code>UV_FIND_LINKS</code> environment variable.</p></dd><dt id="uv-tool-install--force"><a href="#uv-tool-install--force"><code>--force</code></a></dt><dd><p>Force installation of the tool.</p>
<p>Will replace any existing entry points with the same name in the executable directory.</p>
</dd><dt id="uv-tool-install--fork-strategy"><a href="#uv-tool-install--fork-strategy"><code>--fork-strategy</code></a> <i>fork-strategy</i></dt><dd><p>The strategy to use when selecting multiple versions of a given package across Python versions and platforms.</p>
<p>By default, uv will optimize for selecting the latest version of each package for each supported Python version (<code>requires-python</code>), while minimizing the number of selected versions across platforms.</p>
<p>Under <code>fewest</code>, uv will minimize the number of selected versions for each package, preferring older versions that are compatible with a wider range of supported Python versions or platforms.</p>
<p>May also be set with the <code>UV_FORK_STRATEGY</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>fewest</code>:  Optimize for selecting the fewest number of versions for each package. Older versions may be preferred if they are compatible with a wider range of supported Python versions or platforms</li>
<li><code>requires-python</code>:  Optimize for selecting latest supported version of each package, for each supported Python version</li>
</ul></dd><dt id="uv-tool-install--help"><a href="#uv-tool-install--help"><code>--help</code></a>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>
</dd><dt id="uv-tool-install--index"><a href="#uv-tool-install--index"><code>--index</code></a> <i>index</i></dt><dd><p>The URLs to use when resolving dependencies, in addition to the default index.</p>
<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>
<p>All indexes provided via this flag take priority over the index specified by <code>--default-index</code> (which defaults to PyPI). When multiple <code>--index</code> flags are provided, earlier values take priority.</p>
<p>May also be set with the <code>UV_INDEX</code> environment variable.</p></dd><dt id="uv-tool-install--index-strategy"><a href="#uv-tool-install--index-strategy"><code>--index-strategy</code></a> <i>index-strategy</i></dt><dd><p>The strategy to use when resolving against multiple index URLs.</p>
<p>By default, uv will stop at the first index on which a given package is available, and limit resolutions to those present on that first index (<code>first-index</code>). This prevents &quot;dependency confusion&quot; attacks, whereby an attacker can upload a malicious package under the same name to an alternate index.</p>
<p>May also be set with the <code>UV_INDEX_STRATEGY</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>first-index</code>:  Only use results from the first index that returns a match for a given package name</li>
<li><code>unsafe-first-match</code>:  Search for every package name across all indexes, exhausting the versions from the first index before moving on to the next</li>
<li><code>unsafe-best-match</code>:  Search for every package name across all indexes, preferring the &quot;best&quot; version found. If a package version is in multiple indexes, only look at the entry for the first index</li>
</ul></dd><dt id="uv-tool-install--index-url"><a href="#uv-tool-install--index-url"><code>--index-url</code></a>, <code>-i</code> <i>index-url</i></dt><dd><p>(Deprecated: use <code>--default-index</code> instead) The URL of the Python package index (by default: <a href="https://pypi.org/simple">https://pypi.org/simple</a>).</p>
<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>
<p>The index given by this flag is given lower priority than all other indexes specified via the <code>--extra-index-url</code> flag.</p>
<p>May also be set with the <code>UV_INDEX_URL</code> environment variable.</p></dd><dt id="uv-tool-install--keyring-provider"><a href="#uv-tool-install--keyring-provider"><code>--keyring-provider</code></a> <i>keyring-provider</i></dt><dd><p>Attempt to use <code>keyring</code> for authentication for index URLs.</p>
<p>At present, only <code>--keyring-provider subprocess</code> is supported, which configures uv to use the <code>keyring</code> CLI to handle authentication.</p>
<p>Defaults to <code>disabled</code>.</p>
<p>May also be set with the <code>UV_KEYRING_PROVIDER</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>disabled</code>:  Do not use keyring for credential lookup</li>
<li><code>subprocess</code>:  Use the <code>keyring</code> command for credential lookup</li>
</ul></dd><dt id="uv-tool-install--link-mode"><a href="#uv-tool-install--link-mode"><code>--link-mode</code></a> <i>link-mode</i></dt><dd><p>The method to use when installing packages from the global cache.</p>
<p>Defaults to <code>clone</code> (also known as Copy-on-Write) on macOS, and <code>hardlink</code> on Linux and Windows.</p>
<p>May also be set with the <code>UV_LINK_MODE</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>clone</code>:  Clone (i.e., copy-on-write) packages from the wheel into the <code>site-packages</code> directory</li>
<li><code>copy</code>:  Copy packages from the wheel into the <code>site-packages</code> directory</li>
<li><code>hardlink</code>:  Hard link packages from the wheel into the <code>site-packages</code> directory</li>
<li><code>symlink</code>:  Symbolically link packages from the wheel into the <code>site-packages</code> directory</li>
</ul></dd><dt id="uv-tool-install--managed-python"><a href="#uv-tool-install--managed-python"><code>--managed-python</code></a></dt><dd><p>Require use of uv-managed Python versions.</p>
<p>By default, uv prefers using Python versions it manages. However, it will use system Python versions if a uv-managed Python is not installed. This option disables use of system Python versions.</p>
<p>May also be set with the <code>UV_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-tool-install--native-tls"><a href="#uv-tool-install--native-tls"><code>--native-tls</code></a></dt><dd><p>Whether to load TLS certificates from the platform's native certificate store.</p>
<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>
<p>However, in some cases, you may want to use the platform's native certificate store, especially if you're relying on a corporate trust root (e.g., for a mandatory proxy) that's included in your system's certificate store.</p>
<p>May also be set with the <code>UV_NATIVE_TLS</code> environment variable.</p></dd><dt id="uv-tool-install--no-binary"><a href="#uv-tool-install--no-binary"><code>--no-binary</code></a></dt><dd><p>Don't install pre-built wheels.</p>
<p>The given packages will be built and installed from source. The resolver will still use pre-built wheels to extract package metadata, if available.</p>
<p>May also be set with the <code>UV_NO_BINARY</code> environment variable.</p></dd><dt id="uv-tool-install--no-binary-package"><a href="#uv-tool-install--no-binary-package"><code>--no-binary-package</code></a> <i>no-binary-package</i></dt><dd><p>Don't install pre-built wheels for a specific package</p>
<p>May also be set with the <code>UV_NO_BINARY_PACKAGE</code> environment variable.</p></dd><dt id="uv-tool-install--no-build"><a href="#uv-tool-install--no-build"><code>--no-build</code></a></dt><dd><p>Don't build source distributions.</p>
<p>When enabled, resolving will not run arbitrary Python code. The cached wheels of already-built source distributions will be reused, but operations that require building distributions will exit with an error.</p>
<p>May also be set with the <code>UV_NO_BUILD</code> environment variable.</p></dd><dt id="uv-tool-install--no-build-isolation"><a href="#uv-tool-install--no-build-isolation"><code>--no-build-isolation</code></a></dt><dd><p>Disable isolation when building source distributions.</p>
<p>Assumes that build dependencies specified by PEP 518 are already installed.</p>
<p>May also be set with the <code>UV_NO_BUILD_ISOLATION</code> environment variable.</p></dd><dt id="uv-tool-install--no-build-isolation-package"><a href="#uv-tool-install--no-build-isolation-package"><code>--no-build-isolation-package</code></a> <i>no-build-isolation-package</i></dt><dd><p>Disable isolation when building source distributions for a specific package.</p>
<p>Assumes that the packages' build dependencies specified by PEP 518 are already installed.</p>
</dd><dt id="uv-tool-install--no-build-package"><a href="#uv-tool-install--no-build-package"><code>--no-build-package</code></a> <i>no-build-package</i></dt><dd><p>Don't build source distributions for a specific package</p>
<p>May also be set with the <code>UV_NO_BUILD_PACKAGE</code> environment variable.</p></dd><dt id="uv-tool-install--no-cache"><a href="#uv-tool-install--no-cache"><code>--no-cache</code></a>, <code>--no-cache-dir</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>
<p>May also be set with the <code>UV_NO_CACHE</code> environment variable.</p></dd><dt id="uv-tool-install--no-config"><a href="#uv-tool-install--no-config"><code>--no-config</code></a></dt><dd><p>Avoid discovering configuration files (<code>pyproject.toml</code>, <code>uv.toml</code>).</p>
<p>Normally, configuration files are discovered in the current directory, parent directories, or user configuration directories.</p>
<p>May also be set with the <code>UV_NO_CONFIG</code> environment variable.</p></dd><dt id="uv-tool-install--no-index"><a href="#uv-tool-install--no-index"><code>--no-index</code></a></dt><dd><p>Ignore the registry index (e.g., PyPI), instead relying on direct URL dependencies and those provided via <code>--find-links</code></p>
</dd><dt id="uv-tool-install--no-managed-python"><a href="#uv-tool-install--no-managed-python"><code>--no-managed-python</code></a></dt><dd><p>Disable use of uv-managed Python versions.</p>
<p>Instead, uv will search for a suitable Python version on the system.</p>
<p>May also be set with the <code>UV_NO_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-tool-install--no-progress"><a href="#uv-tool-install--no-progress"><code>--no-progress</code></a></dt><dd><p>Hide all progress outputs.</p>
<p>For example, spinners or progress bars.</p>
<p>May also be set with the <code>UV_NO_PROGRESS</code> environment variable.</p></dd><dt id="uv-tool-install--no-python-downloads"><a href="#uv-tool-install--no-python-downloads"><code>--no-python-downloads</code></a></dt><dd><p>Disable automatic downloads of Python.</p>
</dd><dt id="uv-tool-install--no-sources"><a href="#uv-tool-install--no-sources"><code>--no-sources</code></a></dt><dd><p>Ignore the <code>tool.uv.sources</code> table when resolving dependencies. Used to lock against the standards-compliant, publishable package metadata, as opposed to using any workspace, Git, URL, or local path sources</p>
</dd><dt id="uv-tool-install--offline"><a href="#uv-tool-install--offline"><code>--offline</code></a></dt><dd><p>Disable network access.</p>
<p>When disabled, uv will only use locally cached data and locally available files.</p>
<p>May also be set with the <code>UV_OFFLINE</code> environment variable.</p></dd><dt id="uv-tool-install--overrides"><a href="#uv-tool-install--overrides"><code>--overrides</code></a>, <code>--override</code> <i>overrides</i></dt><dd><p>Override versions using the given requirements files.</p>
<p>Overrides files are <code>requirements.txt</code>-like files that force a specific version of a requirement to be installed, regardless of the requirements declared by any constituent package, and regardless of whether this would be considered an invalid resolution.</p>
<p>While constraints are <em>additive</em>, in that they're combined with the requirements of the constituent packages, overrides are <em>absolute</em>, in that they completely replace the requirements of the constituent packages.</p>
<p>May also be set with the <code>UV_OVERRIDE</code> environment variable.</p></dd><dt id="uv-tool-install--prerelease"><a href="#uv-tool-install--prerelease"><code>--prerelease</code></a> <i>prerelease</i></dt><dd><p>The strategy to use when considering pre-release versions.</p>
<p>By default, uv will accept pre-releases for packages that <em>only</em> publish pre-releases, along with first-party requirements that contain an explicit pre-release marker in the declared specifiers (<code>if-necessary-or-explicit</code>).</p>
<p>May also be set with the <code>UV_PRERELEASE</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>disallow</code>:  Disallow all pre-release versions</li>
<li><code>allow</code>:  Allow all pre-release versions</li>
<li><code>if-necessary</code>:  Allow pre-release versions if all versions of a package are pre-release</li>
<li><code>explicit</code>:  Allow pre-release versions for first-party packages with explicit pre-release markers in their version requirements</li>
<li><code>if-necessary-or-explicit</code>:  Allow pre-release versions if all versions of a package are pre-release, or if the package has an explicit pre-release marker in its version requirements</li>
</ul></dd><dt id="uv-tool-install--project"><a href="#uv-tool-install--project"><code>--project</code></a> <i>project</i></dt><dd><p>Run the command within the given project directory.</p>
<p>All <code>pyproject.toml</code>, <code>uv.toml</code>, and <code>.python-version</code> files will be discovered by walking up the directory tree from the project root, as will the project's virtual environment (<code>.venv</code>).</p>
<p>Other command-line arguments (such as relative paths) will be resolved relative to the current working directory.</p>
<p>See <code>--directory</code> to change the working directory entirely.</p>
<p>This setting has no effect when used in the <code>uv pip</code> interface.</p>
<p>May also be set with the <code>UV_PROJECT</code> environment variable.</p></dd><dt id="uv-tool-install--python"><a href="#uv-tool-install--python"><code>--python</code></a>, <code>-p</code> <i>python</i></dt><dd><p>The Python interpreter to use to build the tool environment.</p>
<p>See <a href="#uv-python">uv python</a> for details on Python discovery and supported request formats.</p>
<p>May also be set with the <code>UV_PYTHON</code> environment variable.</p></dd><dt id="uv-tool-install--quiet"><a href="#uv-tool-install--quiet"><code>--quiet</code></a>, <code>-q</code></dt><dd><p>Use quiet output.</p>
<p>Repeating this option, e.g., <code>-qq</code>, will enable a silent mode in which uv will write no output to stdout.</p>
</dd><dt id="uv-tool-install--refresh"><a href="#uv-tool-install--refresh"><code>--refresh</code></a></dt><dd><p>Refresh all cached data</p>
</dd><dt id="uv-tool-install--refresh-package"><a href="#uv-tool-install--refresh-package"><code>--refresh-package</code></a> <i>refresh-package</i></dt><dd><p>Refresh cached data for a specific package</p>
</dd><dt id="uv-tool-install--reinstall"><a href="#uv-tool-install--reinstall"><code>--reinstall</code></a>, <code>--force-reinstall</code></dt><dd><p>Reinstall all packages, regardless of whether they're already installed. Implies <code>--refresh</code></p>
</dd><dt id="uv-tool-install--reinstall-package"><a href="#uv-tool-install--reinstall-package"><code>--reinstall-package</code></a> <i>reinstall-package</i></dt><dd><p>Reinstall a specific package, regardless of whether it's already installed. Implies <code>--refresh-package</code></p>
</dd><dt id="uv-tool-install--resolution"><a href="#uv-tool-install--resolution"><code>--resolution</code></a> <i>resolution</i></dt><dd><p>The strategy to use when selecting between the different compatible versions for a given package requirement.</p>
<p>By default, uv will use the latest compatible version of each package (<code>highest</code>).</p>
<p>May also be set with the <code>UV_RESOLUTION</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>highest</code>:  Resolve the highest compatible version of each package</li>
<li><code>lowest</code>:  Resolve the lowest compatible version of each package</li>
<li><code>lowest-direct</code>:  Resolve the lowest compatible version of any direct dependencies, and the highest compatible version of any transitive dependencies</li>
</ul></dd><dt id="uv-tool-install--upgrade"><a href="#uv-tool-install--upgrade"><code>--upgrade</code></a>, <code>-U</code></dt><dd><p>Allow package upgrades, ignoring pinned versions in any existing output file. Implies <code>--refresh</code></p>
</dd><dt id="uv-tool-install--upgrade-package"><a href="#uv-tool-install--upgrade-package"><code>--upgrade-package</code></a>, <code>-P</code> <i>upgrade-package</i></dt><dd><p>Allow upgrades for a specific package, ignoring pinned versions in any existing output file. Implies <code>--refresh-package</code></p>
</dd><dt id="uv-tool-install--verbose"><a href="#uv-tool-install--verbose"><code>--verbose</code></a>, <code>-v</code></dt><dd><p>Use verbose output.</p>
<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (<a href="https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives">https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives</a>)</p>
</dd><dt id="uv-tool-install--with"><a href="#uv-tool-install--with"><code>--with</code></a> <i>with</i></dt><dd><p>Include the following additional requirements</p>
</dd><dt id="uv-tool-install--with-editable"><a href="#uv-tool-install--with-editable"><code>--with-editable</code></a> <i>with-editable</i></dt><dd><p>Include the given packages in editable mode</p>
</dd><dt id="uv-tool-install--with-requirements"><a href="#uv-tool-install--with-requirements"><code>--with-requirements</code></a> <i>with-requirements</i></dt><dd><p>Include all requirements listed in the given <code>requirements.txt</code> files</p>
</dd></dl>

### uv tool upgrade

Upgrade installed tools.

If a tool was installed with version constraints, they will be respected on upgrade — to upgrade a tool beyond the originally provided constraints, use `uv tool install` again.

If a tool was installed with specific settings, they will be respected on upgraded. For example, if `--prereleases allow` was provided during installation, it will continue to be respected in upgrades.

<h3 class="cli-reference">Usage</h3>

```
uv tool upgrade [OPTIONS] <NAME>...
```

<h3 class="cli-reference">Arguments</h3>

<dl class="cli-reference"><dt id="uv-tool-upgrade--name"><a href="#uv-tool-upgrade--name"<code>NAME</code></a></dt><dd><p>The name of the tool to upgrade, along with an optional version specifier</p>
</dd></dl>

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt id="uv-tool-upgrade--all"><a href="#uv-tool-upgrade--all"><code>--all</code></a></dt><dd><p>Upgrade all tools</p>
</dd><dt id="uv-tool-upgrade--allow-insecure-host"><a href="#uv-tool-upgrade--allow-insecure-host"><code>--allow-insecure-host</code></a>, <code>--trusted-host</code> <i>allow-insecure-host</i></dt><dd><p>Allow insecure connections to a host.</p>
<p>Can be provided multiple times.</p>
<p>Expects to receive either a hostname (e.g., <code>localhost</code>), a host-port pair (e.g., <code>localhost:8080</code>), or a URL (e.g., <code>https://localhost</code>).</p>
<p>WARNING: Hosts included in this list will not be verified against the system's certificate store. Only use <code>--allow-insecure-host</code> in a secure network with verified sources, as it bypasses SSL verification and could expose you to MITM attacks.</p>
<p>May also be set with the <code>UV_INSECURE_HOST</code> environment variable.</p></dd><dt id="uv-tool-upgrade--cache-dir"><a href="#uv-tool-upgrade--cache-dir"><code>--cache-dir</code></a> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>
<p>Defaults to <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on macOS and Linux, and <code>%LOCALAPPDATA%\uv\cache</code> on Windows.</p>
<p>To view the location of the cache directory, run <code>uv cache dir</code>.</p>
<p>May also be set with the <code>UV_CACHE_DIR</code> environment variable.</p></dd><dt id="uv-tool-upgrade--color"><a href="#uv-tool-upgrade--color"><code>--color</code></a> <i>color-choice</i></dt><dd><p>Control the use of color in output.</p>
<p>By default, uv will automatically detect support for colors when writing to a terminal.</p>
<p>Possible values:</p>
<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>
<li><code>always</code>:  Enables colored output regardless of the detected environment</li>
<li><code>never</code>:  Disables colored output</li>
</ul></dd><dt id="uv-tool-upgrade--compile-bytecode"><a href="#uv-tool-upgrade--compile-bytecode"><code>--compile-bytecode</code></a>, <code>--compile</code></dt><dd><p>Compile Python files to bytecode after installation.</p>
<p>By default, uv does not compile Python (<code>.py</code>) files to bytecode (<code>__pycache__/*.pyc</code>); instead, compilation is performed lazily the first time a module is imported. For use-cases in which start time is critical, such as CLI applications and Docker containers, this option can be enabled to trade longer installation times for faster start times.</p>
<p>When enabled, uv will process the entire site-packages directory (including packages that are not being modified by the current operation) for consistency. Like pip, it will also ignore errors.</p>
<p>May also be set with the <code>UV_COMPILE_BYTECODE</code> environment variable.</p></dd><dt id="uv-tool-upgrade--config-file"><a href="#uv-tool-upgrade--config-file"><code>--config-file</code></a> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>
<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>
<p>May also be set with the <code>UV_CONFIG_FILE</code> environment variable.</p></dd><dt id="uv-tool-upgrade--config-setting"><a href="#uv-tool-upgrade--config-setting"><code>--config-setting</code></a>, <code>--config-settings</code>, <code>-C</code> <i>config-setting</i></dt><dd><p>Settings to pass to the PEP 517 build backend, specified as <code>KEY=VALUE</code> pairs</p>
</dd><dt id="uv-tool-upgrade--default-index"><a href="#uv-tool-upgrade--default-index"><code>--default-index</code></a> <i>default-index</i></dt><dd><p>The URL of the default package index (by default: <a href="https://pypi.org/simple">https://pypi.org/simple</a>).</p>
<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>
<p>The index given by this flag is given lower priority than all other indexes specified via the <code>--index</code> flag.</p>
<p>May also be set with the <code>UV_DEFAULT_INDEX</code> environment variable.</p></dd><dt id="uv-tool-upgrade--directory"><a href="#uv-tool-upgrade--directory"><code>--directory</code></a> <i>directory</i></dt><dd><p>Change to the given directory prior to running the command.</p>
<p>Relative paths are resolved with the given directory as the base.</p>
<p>See <code>--project</code> to only change the project root directory.</p>
</dd><dt id="uv-tool-upgrade--exclude-newer"><a href="#uv-tool-upgrade--exclude-newer"><code>--exclude-newer</code></a> <i>exclude-newer</i></dt><dd><p>Limit candidate packages to those that were uploaded prior to the given date.</p>
<p>Accepts both RFC 3339 timestamps (e.g., <code>2006-12-02T02:07:43Z</code>) and local dates in the same format (e.g., <code>2006-12-02</code>) in your system's configured time zone.</p>
<p>May also be set with the <code>UV_EXCLUDE_NEWER</code> environment variable.</p></dd><dt id="uv-tool-upgrade--extra-index-url"><a href="#uv-tool-upgrade--extra-index-url"><code>--extra-index-url</code></a> <i>extra-index-url</i></dt><dd><p>(Deprecated: use <code>--index</code> instead) Extra URLs of package indexes to use, in addition to <code>--index-url</code>.</p>
<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>
<p>All indexes provided via this flag take priority over the index specified by <code>--index-url</code> (which defaults to PyPI). When multiple <code>--extra-index-url</code> flags are provided, earlier values take priority.</p>
<p>May also be set with the <code>UV_EXTRA_INDEX_URL</code> environment variable.</p></dd><dt id="uv-tool-upgrade--find-links"><a href="#uv-tool-upgrade--find-links"><code>--find-links</code></a>, <code>-f</code> <i>find-links</i></dt><dd><p>Locations to search for candidate distributions, in addition to those found in the registry indexes.</p>
<p>If a path, the target must be a directory that contains packages as wheel files (<code>.whl</code>) or source distributions (e.g., <code>.tar.gz</code> or <code>.zip</code>) at the top level.</p>
<p>If a URL, the page must contain a flat list of links to package files adhering to the formats described above.</p>
<p>May also be set with the <code>UV_FIND_LINKS</code> environment variable.</p></dd><dt id="uv-tool-upgrade--fork-strategy"><a href="#uv-tool-upgrade--fork-strategy"><code>--fork-strategy</code></a> <i>fork-strategy</i></dt><dd><p>The strategy to use when selecting multiple versions of a given package across Python versions and platforms.</p>
<p>By default, uv will optimize for selecting the latest version of each package for each supported Python version (<code>requires-python</code>), while minimizing the number of selected versions across platforms.</p>
<p>Under <code>fewest</code>, uv will minimize the number of selected versions for each package, preferring older versions that are compatible with a wider range of supported Python versions or platforms.</p>
<p>May also be set with the <code>UV_FORK_STRATEGY</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>fewest</code>:  Optimize for selecting the fewest number of versions for each package. Older versions may be preferred if they are compatible with a wider range of supported Python versions or platforms</li>
<li><code>requires-python</code>:  Optimize for selecting latest supported version of each package, for each supported Python version</li>
</ul></dd><dt id="uv-tool-upgrade--help"><a href="#uv-tool-upgrade--help"><code>--help</code></a>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>
</dd><dt id="uv-tool-upgrade--index"><a href="#uv-tool-upgrade--index"><code>--index</code></a> <i>index</i></dt><dd><p>The URLs to use when resolving dependencies, in addition to the default index.</p>
<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>
<p>All indexes provided via this flag take priority over the index specified by <code>--default-index</code> (which defaults to PyPI). When multiple <code>--index</code> flags are provided, earlier values take priority.</p>
<p>May also be set with the <code>UV_INDEX</code> environment variable.</p></dd><dt id="uv-tool-upgrade--index-strategy"><a href="#uv-tool-upgrade--index-strategy"><code>--index-strategy</code></a> <i>index-strategy</i></dt><dd><p>The strategy to use when resolving against multiple index URLs.</p>
<p>By default, uv will stop at the first index on which a given package is available, and limit resolutions to those present on that first index (<code>first-index</code>). This prevents &quot;dependency confusion&quot; attacks, whereby an attacker can upload a malicious package under the same name to an alternate index.</p>
<p>May also be set with the <code>UV_INDEX_STRATEGY</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>first-index</code>:  Only use results from the first index that returns a match for a given package name</li>
<li><code>unsafe-first-match</code>:  Search for every package name across all indexes, exhausting the versions from the first index before moving on to the next</li>
<li><code>unsafe-best-match</code>:  Search for every package name across all indexes, preferring the &quot;best&quot; version found. If a package version is in multiple indexes, only look at the entry for the first index</li>
</ul></dd><dt id="uv-tool-upgrade--index-url"><a href="#uv-tool-upgrade--index-url"><code>--index-url</code></a>, <code>-i</code> <i>index-url</i></dt><dd><p>(Deprecated: use <code>--default-index</code> instead) The URL of the Python package index (by default: <a href="https://pypi.org/simple">https://pypi.org/simple</a>).</p>
<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>
<p>The index given by this flag is given lower priority than all other indexes specified via the <code>--extra-index-url</code> flag.</p>
<p>May also be set with the <code>UV_INDEX_URL</code> environment variable.</p></dd><dt id="uv-tool-upgrade--keyring-provider"><a href="#uv-tool-upgrade--keyring-provider"><code>--keyring-provider</code></a> <i>keyring-provider</i></dt><dd><p>Attempt to use <code>keyring</code> for authentication for index URLs.</p>
<p>At present, only <code>--keyring-provider subprocess</code> is supported, which configures uv to use the <code>keyring</code> CLI to handle authentication.</p>
<p>Defaults to <code>disabled</code>.</p>
<p>May also be set with the <code>UV_KEYRING_PROVIDER</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>disabled</code>:  Do not use keyring for credential lookup</li>
<li><code>subprocess</code>:  Use the <code>keyring</code> command for credential lookup</li>
</ul></dd><dt id="uv-tool-upgrade--link-mode"><a href="#uv-tool-upgrade--link-mode"><code>--link-mode</code></a> <i>link-mode</i></dt><dd><p>The method to use when installing packages from the global cache.</p>
<p>Defaults to <code>clone</code> (also known as Copy-on-Write) on macOS, and <code>hardlink</code> on Linux and Windows.</p>
<p>May also be set with the <code>UV_LINK_MODE</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>clone</code>:  Clone (i.e., copy-on-write) packages from the wheel into the <code>site-packages</code> directory</li>
<li><code>copy</code>:  Copy packages from the wheel into the <code>site-packages</code> directory</li>
<li><code>hardlink</code>:  Hard link packages from the wheel into the <code>site-packages</code> directory</li>
<li><code>symlink</code>:  Symbolically link packages from the wheel into the <code>site-packages</code> directory</li>
</ul></dd><dt id="uv-tool-upgrade--managed-python"><a href="#uv-tool-upgrade--managed-python"><code>--managed-python</code></a></dt><dd><p>Require use of uv-managed Python versions.</p>
<p>By default, uv prefers using Python versions it manages. However, it will use system Python versions if a uv-managed Python is not installed. This option disables use of system Python versions.</p>
<p>May also be set with the <code>UV_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-tool-upgrade--native-tls"><a href="#uv-tool-upgrade--native-tls"><code>--native-tls</code></a></dt><dd><p>Whether to load TLS certificates from the platform's native certificate store.</p>
<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>
<p>However, in some cases, you may want to use the platform's native certificate store, especially if you're relying on a corporate trust root (e.g., for a mandatory proxy) that's included in your system's certificate store.</p>
<p>May also be set with the <code>UV_NATIVE_TLS</code> environment variable.</p></dd><dt id="uv-tool-upgrade--no-binary"><a href="#uv-tool-upgrade--no-binary"><code>--no-binary</code></a></dt><dd><p>Don't install pre-built wheels.</p>
<p>The given packages will be built and installed from source. The resolver will still use pre-built wheels to extract package metadata, if available.</p>
<p>May also be set with the <code>UV_NO_BINARY</code> environment variable.</p></dd><dt id="uv-tool-upgrade--no-binary-package"><a href="#uv-tool-upgrade--no-binary-package"><code>--no-binary-package</code></a> <i>no-binary-package</i></dt><dd><p>Don't install pre-built wheels for a specific package</p>
<p>May also be set with the <code>UV_NO_BINARY_PACKAGE</code> environment variable.</p></dd><dt id="uv-tool-upgrade--no-build"><a href="#uv-tool-upgrade--no-build"><code>--no-build</code></a></dt><dd><p>Don't build source distributions.</p>
<p>When enabled, resolving will not run arbitrary Python code. The cached wheels of already-built source distributions will be reused, but operations that require building distributions will exit with an error.</p>
<p>May also be set with the <code>UV_NO_BUILD</code> environment variable.</p></dd><dt id="uv-tool-upgrade--no-build-isolation"><a href="#uv-tool-upgrade--no-build-isolation"><code>--no-build-isolation</code></a></dt><dd><p>Disable isolation when building source distributions.</p>
<p>Assumes that build dependencies specified by PEP 518 are already installed.</p>
<p>May also be set with the <code>UV_NO_BUILD_ISOLATION</code> environment variable.</p></dd><dt id="uv-tool-upgrade--no-build-isolation-package"><a href="#uv-tool-upgrade--no-build-isolation-package"><code>--no-build-isolation-package</code></a> <i>no-build-isolation-package</i></dt><dd><p>Disable isolation when building source distributions for a specific package.</p>
<p>Assumes that the packages' build dependencies specified by PEP 518 are already installed.</p>
</dd><dt id="uv-tool-upgrade--no-build-package"><a href="#uv-tool-upgrade--no-build-package"><code>--no-build-package</code></a> <i>no-build-package</i></dt><dd><p>Don't build source distributions for a specific package</p>
<p>May also be set with the <code>UV_NO_BUILD_PACKAGE</code> environment variable.</p></dd><dt id="uv-tool-upgrade--no-cache"><a href="#uv-tool-upgrade--no-cache"><code>--no-cache</code></a>, <code>--no-cache-dir</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>
<p>May also be set with the <code>UV_NO_CACHE</code> environment variable.</p></dd><dt id="uv-tool-upgrade--no-config"><a href="#uv-tool-upgrade--no-config"><code>--no-config</code></a></dt><dd><p>Avoid discovering configuration files (<code>pyproject.toml</code>, <code>uv.toml</code>).</p>
<p>Normally, configuration files are discovered in the current directory, parent directories, or user configuration directories.</p>
<p>May also be set with the <code>UV_NO_CONFIG</code> environment variable.</p></dd><dt id="uv-tool-upgrade--no-index"><a href="#uv-tool-upgrade--no-index"><code>--no-index</code></a></dt><dd><p>Ignore the registry index (e.g., PyPI), instead relying on direct URL dependencies and those provided via <code>--find-links</code></p>
</dd><dt id="uv-tool-upgrade--no-managed-python"><a href="#uv-tool-upgrade--no-managed-python"><code>--no-managed-python</code></a></dt><dd><p>Disable use of uv-managed Python versions.</p>
<p>Instead, uv will search for a suitable Python version on the system.</p>
<p>May also be set with the <code>UV_NO_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-tool-upgrade--no-progress"><a href="#uv-tool-upgrade--no-progress"><code>--no-progress</code></a></dt><dd><p>Hide all progress outputs.</p>
<p>For example, spinners or progress bars.</p>
<p>May also be set with the <code>UV_NO_PROGRESS</code> environment variable.</p></dd><dt id="uv-tool-upgrade--no-python-downloads"><a href="#uv-tool-upgrade--no-python-downloads"><code>--no-python-downloads</code></a></dt><dd><p>Disable automatic downloads of Python.</p>
</dd><dt id="uv-tool-upgrade--no-sources"><a href="#uv-tool-upgrade--no-sources"><code>--no-sources</code></a></dt><dd><p>Ignore the <code>tool.uv.sources</code> table when resolving dependencies. Used to lock against the standards-compliant, publishable package metadata, as opposed to using any workspace, Git, URL, or local path sources</p>
</dd><dt id="uv-tool-upgrade--offline"><a href="#uv-tool-upgrade--offline"><code>--offline</code></a></dt><dd><p>Disable network access.</p>
<p>When disabled, uv will only use locally cached data and locally available files.</p>
<p>May also be set with the <code>UV_OFFLINE</code> environment variable.</p></dd><dt id="uv-tool-upgrade--prerelease"><a href="#uv-tool-upgrade--prerelease"><code>--prerelease</code></a> <i>prerelease</i></dt><dd><p>The strategy to use when considering pre-release versions.</p>
<p>By default, uv will accept pre-releases for packages that <em>only</em> publish pre-releases, along with first-party requirements that contain an explicit pre-release marker in the declared specifiers (<code>if-necessary-or-explicit</code>).</p>
<p>May also be set with the <code>UV_PRERELEASE</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>disallow</code>:  Disallow all pre-release versions</li>
<li><code>allow</code>:  Allow all pre-release versions</li>
<li><code>if-necessary</code>:  Allow pre-release versions if all versions of a package are pre-release</li>
<li><code>explicit</code>:  Allow pre-release versions for first-party packages with explicit pre-release markers in their version requirements</li>
<li><code>if-necessary-or-explicit</code>:  Allow pre-release versions if all versions of a package are pre-release, or if the package has an explicit pre-release marker in its version requirements</li>
</ul></dd><dt id="uv-tool-upgrade--project"><a href="#uv-tool-upgrade--project"><code>--project</code></a> <i>project</i></dt><dd><p>Run the command within the given project directory.</p>
<p>All <code>pyproject.toml</code>, <code>uv.toml</code>, and <code>.python-version</code> files will be discovered by walking up the directory tree from the project root, as will the project's virtual environment (<code>.venv</code>).</p>
<p>Other command-line arguments (such as relative paths) will be resolved relative to the current working directory.</p>
<p>See <code>--directory</code> to change the working directory entirely.</p>
<p>This setting has no effect when used in the <code>uv pip</code> interface.</p>
<p>May also be set with the <code>UV_PROJECT</code> environment variable.</p></dd><dt id="uv-tool-upgrade--python"><a href="#uv-tool-upgrade--python"><code>--python</code></a>, <code>-p</code> <i>python</i></dt><dd><p>Upgrade a tool, and specify it to use the given Python interpreter to build its environment.
Use with <code>--all</code> to apply to all tools.</p>
<p>See <a href="#uv-python">uv python</a> for details on Python discovery and supported request formats.</p>
<p>May also be set with the <code>UV_PYTHON</code> environment variable.</p></dd><dt id="uv-tool-upgrade--quiet"><a href="#uv-tool-upgrade--quiet"><code>--quiet</code></a>, <code>-q</code></dt><dd><p>Use quiet output.</p>
<p>Repeating this option, e.g., <code>-qq</code>, will enable a silent mode in which uv will write no output to stdout.</p>
</dd><dt id="uv-tool-upgrade--reinstall"><a href="#uv-tool-upgrade--reinstall"><code>--reinstall</code></a>, <code>--force-reinstall</code></dt><dd><p>Reinstall all packages, regardless of whether they're already installed. Implies <code>--refresh</code></p>
</dd><dt id="uv-tool-upgrade--reinstall-package"><a href="#uv-tool-upgrade--reinstall-package"><code>--reinstall-package</code></a> <i>reinstall-package</i></dt><dd><p>Reinstall a specific package, regardless of whether it's already installed. Implies <code>--refresh-package</code></p>
</dd><dt id="uv-tool-upgrade--resolution"><a href="#uv-tool-upgrade--resolution"><code>--resolution</code></a> <i>resolution</i></dt><dd><p>The strategy to use when selecting between the different compatible versions for a given package requirement.</p>
<p>By default, uv will use the latest compatible version of each package (<code>highest</code>).</p>
<p>May also be set with the <code>UV_RESOLUTION</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>highest</code>:  Resolve the highest compatible version of each package</li>
<li><code>lowest</code>:  Resolve the lowest compatible version of each package</li>
<li><code>lowest-direct</code>:  Resolve the lowest compatible version of any direct dependencies, and the highest compatible version of any transitive dependencies</li>
</ul></dd><dt id="uv-tool-upgrade--verbose"><a href="#uv-tool-upgrade--verbose"><code>--verbose</code></a>, <code>-v</code></dt><dd><p>Use verbose output.</p>
<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (<a href="https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives">https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives</a>)</p>
</dd></dl>

### uv tool list

List installed tools

<h3 class="cli-reference">Usage</h3>

```
uv tool list [OPTIONS]
```

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt id="uv-tool-list--allow-insecure-host"><a href="#uv-tool-list--allow-insecure-host"><code>--allow-insecure-host</code></a>, <code>--trusted-host</code> <i>allow-insecure-host</i></dt><dd><p>Allow insecure connections to a host.</p>
<p>Can be provided multiple times.</p>
<p>Expects to receive either a hostname (e.g., <code>localhost</code>), a host-port pair (e.g., <code>localhost:8080</code>), or a URL (e.g., <code>https://localhost</code>).</p>
<p>WARNING: Hosts included in this list will not be verified against the system's certificate store. Only use <code>--allow-insecure-host</code> in a secure network with verified sources, as it bypasses SSL verification and could expose you to MITM attacks.</p>
<p>May also be set with the <code>UV_INSECURE_HOST</code> environment variable.</p></dd><dt id="uv-tool-list--cache-dir"><a href="#uv-tool-list--cache-dir"><code>--cache-dir</code></a> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>
<p>Defaults to <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on macOS and Linux, and <code>%LOCALAPPDATA%\uv\cache</code> on Windows.</p>
<p>To view the location of the cache directory, run <code>uv cache dir</code>.</p>
<p>May also be set with the <code>UV_CACHE_DIR</code> environment variable.</p></dd><dt id="uv-tool-list--color"><a href="#uv-tool-list--color"><code>--color</code></a> <i>color-choice</i></dt><dd><p>Control the use of color in output.</p>
<p>By default, uv will automatically detect support for colors when writing to a terminal.</p>
<p>Possible values:</p>
<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>
<li><code>always</code>:  Enables colored output regardless of the detected environment</li>
<li><code>never</code>:  Disables colored output</li>
</ul></dd><dt id="uv-tool-list--config-file"><a href="#uv-tool-list--config-file"><code>--config-file</code></a> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>
<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>
<p>May also be set with the <code>UV_CONFIG_FILE</code> environment variable.</p></dd><dt id="uv-tool-list--directory"><a href="#uv-tool-list--directory"><code>--directory</code></a> <i>directory</i></dt><dd><p>Change to the given directory prior to running the command.</p>
<p>Relative paths are resolved with the given directory as the base.</p>
<p>See <code>--project</code> to only change the project root directory.</p>
</dd><dt id="uv-tool-list--help"><a href="#uv-tool-list--help"><code>--help</code></a>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>
</dd><dt id="uv-tool-list--managed-python"><a href="#uv-tool-list--managed-python"><code>--managed-python</code></a></dt><dd><p>Require use of uv-managed Python versions.</p>
<p>By default, uv prefers using Python versions it manages. However, it will use system Python versions if a uv-managed Python is not installed. This option disables use of system Python versions.</p>
<p>May also be set with the <code>UV_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-tool-list--native-tls"><a href="#uv-tool-list--native-tls"><code>--native-tls</code></a></dt><dd><p>Whether to load TLS certificates from the platform's native certificate store.</p>
<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>
<p>However, in some cases, you may want to use the platform's native certificate store, especially if you're relying on a corporate trust root (e.g., for a mandatory proxy) that's included in your system's certificate store.</p>
<p>May also be set with the <code>UV_NATIVE_TLS</code> environment variable.</p></dd><dt id="uv-tool-list--no-cache"><a href="#uv-tool-list--no-cache"><code>--no-cache</code></a>, <code>--no-cache-dir</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>
<p>May also be set with the <code>UV_NO_CACHE</code> environment variable.</p></dd><dt id="uv-tool-list--no-config"><a href="#uv-tool-list--no-config"><code>--no-config</code></a></dt><dd><p>Avoid discovering configuration files (<code>pyproject.toml</code>, <code>uv.toml</code>).</p>
<p>Normally, configuration files are discovered in the current directory, parent directories, or user configuration directories.</p>
<p>May also be set with the <code>UV_NO_CONFIG</code> environment variable.</p></dd><dt id="uv-tool-list--no-managed-python"><a href="#uv-tool-list--no-managed-python"><code>--no-managed-python</code></a></dt><dd><p>Disable use of uv-managed Python versions.</p>
<p>Instead, uv will search for a suitable Python version on the system.</p>
<p>May also be set with the <code>UV_NO_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-tool-list--no-progress"><a href="#uv-tool-list--no-progress"><code>--no-progress</code></a></dt><dd><p>Hide all progress outputs.</p>
<p>For example, spinners or progress bars.</p>
<p>May also be set with the <code>UV_NO_PROGRESS</code> environment variable.</p></dd><dt id="uv-tool-list--offline"><a href="#uv-tool-list--offline"><code>--offline</code></a></dt><dd><p>Disable network access.</p>
<p>When disabled, uv will only use locally cached data and locally available files.</p>
<p>May also be set with the <code>UV_OFFLINE</code> environment variable.</p></dd><dt id="uv-tool-list--project"><a href="#uv-tool-list--project"><code>--project</code></a> <i>project</i></dt><dd><p>Run the command within the given project directory.</p>
<p>All <code>pyproject.toml</code>, <code>uv.toml</code>, and <code>.python-version</code> files will be discovered by walking up the directory tree from the project root, as will the project's virtual environment (<code>.venv</code>).</p>
<p>Other command-line arguments (such as relative paths) will be resolved relative to the current working directory.</p>
<p>See <code>--directory</code> to change the working directory entirely.</p>
<p>This setting has no effect when used in the <code>uv pip</code> interface.</p>
<p>May also be set with the <code>UV_PROJECT</code> environment variable.</p></dd><dt id="uv-tool-list--quiet"><a href="#uv-tool-list--quiet"><code>--quiet</code></a>, <code>-q</code></dt><dd><p>Use quiet output.</p>
<p>Repeating this option, e.g., <code>-qq</code>, will enable a silent mode in which uv will write no output to stdout.</p>
</dd><dt id="uv-tool-list--show-paths"><a href="#uv-tool-list--show-paths"><code>--show-paths</code></a></dt><dd><p>Whether to display the path to each tool environment and installed executable</p>
</dd><dt id="uv-tool-list--show-version-specifiers"><a href="#uv-tool-list--show-version-specifiers"><code>--show-version-specifiers</code></a></dt><dd><p>Whether to display the version specifier(s) used to install each tool</p>
</dd><dt id="uv-tool-list--show-with"><a href="#uv-tool-list--show-with"><code>--show-with</code></a></dt><dd><p>Whether to display the additional requirements installed with each tool</p>
</dd><dt id="uv-tool-list--verbose"><a href="#uv-tool-list--verbose"><code>--verbose</code></a>, <code>-v</code></dt><dd><p>Use verbose output.</p>
<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (<a href="https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives">https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives</a>)</p>
</dd></dl>

### uv tool uninstall

Uninstall a tool

<h3 class="cli-reference">Usage</h3>

```
uv tool uninstall [OPTIONS] <NAME>...
```

<h3 class="cli-reference">Arguments</h3>

<dl class="cli-reference"><dt id="uv-tool-uninstall--name"><a href="#uv-tool-uninstall--name"<code>NAME</code></a></dt><dd><p>The name of the tool to uninstall</p>
</dd></dl>

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt id="uv-tool-uninstall--all"><a href="#uv-tool-uninstall--all"><code>--all</code></a></dt><dd><p>Uninstall all tools</p>
</dd><dt id="uv-tool-uninstall--allow-insecure-host"><a href="#uv-tool-uninstall--allow-insecure-host"><code>--allow-insecure-host</code></a>, <code>--trusted-host</code> <i>allow-insecure-host</i></dt><dd><p>Allow insecure connections to a host.</p>
<p>Can be provided multiple times.</p>
<p>Expects to receive either a hostname (e.g., <code>localhost</code>), a host-port pair (e.g., <code>localhost:8080</code>), or a URL (e.g., <code>https://localhost</code>).</p>
<p>WARNING: Hosts included in this list will not be verified against the system's certificate store. Only use <code>--allow-insecure-host</code> in a secure network with verified sources, as it bypasses SSL verification and could expose you to MITM attacks.</p>
<p>May also be set with the <code>UV_INSECURE_HOST</code> environment variable.</p></dd><dt id="uv-tool-uninstall--cache-dir"><a href="#uv-tool-uninstall--cache-dir"><code>--cache-dir</code></a> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>
<p>Defaults to <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on macOS and Linux, and <code>%LOCALAPPDATA%\uv\cache</code> on Windows.</p>
<p>To view the location of the cache directory, run <code>uv cache dir</code>.</p>
<p>May also be set with the <code>UV_CACHE_DIR</code> environment variable.</p></dd><dt id="uv-tool-uninstall--color"><a href="#uv-tool-uninstall--color"><code>--color</code></a> <i>color-choice</i></dt><dd><p>Control the use of color in output.</p>
<p>By default, uv will automatically detect support for colors when writing to a terminal.</p>
<p>Possible values:</p>
<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>
<li><code>always</code>:  Enables colored output regardless of the detected environment</li>
<li><code>never</code>:  Disables colored output</li>
</ul></dd><dt id="uv-tool-uninstall--config-file"><a href="#uv-tool-uninstall--config-file"><code>--config-file</code></a> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>
<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>
<p>May also be set with the <code>UV_CONFIG_FILE</code> environment variable.</p></dd><dt id="uv-tool-uninstall--directory"><a href="#uv-tool-uninstall--directory"><code>--directory</code></a> <i>directory</i></dt><dd><p>Change to the given directory prior to running the command.</p>
<p>Relative paths are resolved with the given directory as the base.</p>
<p>See <code>--project</code> to only change the project root directory.</p>
</dd><dt id="uv-tool-uninstall--help"><a href="#uv-tool-uninstall--help"><code>--help</code></a>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>
</dd><dt id="uv-tool-uninstall--managed-python"><a href="#uv-tool-uninstall--managed-python"><code>--managed-python</code></a></dt><dd><p>Require use of uv-managed Python versions.</p>
<p>By default, uv prefers using Python versions it manages. However, it will use system Python versions if a uv-managed Python is not installed. This option disables use of system Python versions.</p>
<p>May also be set with the <code>UV_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-tool-uninstall--native-tls"><a href="#uv-tool-uninstall--native-tls"><code>--native-tls</code></a></dt><dd><p>Whether to load TLS certificates from the platform's native certificate store.</p>
<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>
<p>However, in some cases, you may want to use the platform's native certificate store, especially if you're relying on a corporate trust root (e.g., for a mandatory proxy) that's included in your system's certificate store.</p>
<p>May also be set with the <code>UV_NATIVE_TLS</code> environment variable.</p></dd><dt id="uv-tool-uninstall--no-cache"><a href="#uv-tool-uninstall--no-cache"><code>--no-cache</code></a>, <code>--no-cache-dir</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>
<p>May also be set with the <code>UV_NO_CACHE</code> environment variable.</p></dd><dt id="uv-tool-uninstall--no-config"><a href="#uv-tool-uninstall--no-config"><code>--no-config</code></a></dt><dd><p>Avoid discovering configuration files (<code>pyproject.toml</code>, <code>uv.toml</code>).</p>
<p>Normally, configuration files are discovered in the current directory, parent directories, or user configuration directories.</p>
<p>May also be set with the <code>UV_NO_CONFIG</code> environment variable.</p></dd><dt id="uv-tool-uninstall--no-managed-python"><a href="#uv-tool-uninstall--no-managed-python"><code>--no-managed-python</code></a></dt><dd><p>Disable use of uv-managed Python versions.</p>
<p>Instead, uv will search for a suitable Python version on the system.</p>
<p>May also be set with the <code>UV_NO_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-tool-uninstall--no-progress"><a href="#uv-tool-uninstall--no-progress"><code>--no-progress</code></a></dt><dd><p>Hide all progress outputs.</p>
<p>For example, spinners or progress bars.</p>
<p>May also be set with the <code>UV_NO_PROGRESS</code> environment variable.</p></dd><dt id="uv-tool-uninstall--no-python-downloads"><a href="#uv-tool-uninstall--no-python-downloads"><code>--no-python-downloads</code></a></dt><dd><p>Disable automatic downloads of Python.</p>
</dd><dt id="uv-tool-uninstall--offline"><a href="#uv-tool-uninstall--offline"><code>--offline</code></a></dt><dd><p>Disable network access.</p>
<p>When disabled, uv will only use locally cached data and locally available files.</p>
<p>May also be set with the <code>UV_OFFLINE</code> environment variable.</p></dd><dt id="uv-tool-uninstall--project"><a href="#uv-tool-uninstall--project"><code>--project</code></a> <i>project</i></dt><dd><p>Run the command within the given project directory.</p>
<p>All <code>pyproject.toml</code>, <code>uv.toml</code>, and <code>.python-version</code> files will be discovered by walking up the directory tree from the project root, as will the project's virtual environment (<code>.venv</code>).</p>
<p>Other command-line arguments (such as relative paths) will be resolved relative to the current working directory.</p>
<p>See <code>--directory</code> to change the working directory entirely.</p>
<p>This setting has no effect when used in the <code>uv pip</code> interface.</p>
<p>May also be set with the <code>UV_PROJECT</code> environment variable.</p></dd><dt id="uv-tool-uninstall--quiet"><a href="#uv-tool-uninstall--quiet"><code>--quiet</code></a>, <code>-q</code></dt><dd><p>Use quiet output.</p>
<p>Repeating this option, e.g., <code>-qq</code>, will enable a silent mode in which uv will write no output to stdout.</p>
</dd><dt id="uv-tool-uninstall--verbose"><a href="#uv-tool-uninstall--verbose"><code>--verbose</code></a>, <code>-v</code></dt><dd><p>Use verbose output.</p>
<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (<a href="https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives">https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives</a>)</p>
</dd></dl>

### uv tool update-shell

Ensure that the tool executable directory is on the `PATH`.

If the tool executable directory is not present on the `PATH`, uv will attempt to add it to the relevant shell configuration files.

If the shell configuration files already include a blurb to add the executable directory to the path, but the directory is not present on the `PATH`, uv will exit with an error.

The tool executable directory is determined according to the XDG standard and can be retrieved with `uv tool dir --bin`.

<h3 class="cli-reference">Usage</h3>

```
uv tool update-shell [OPTIONS]
```

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt id="uv-tool-update-shell--allow-insecure-host"><a href="#uv-tool-update-shell--allow-insecure-host"><code>--allow-insecure-host</code></a>, <code>--trusted-host</code> <i>allow-insecure-host</i></dt><dd><p>Allow insecure connections to a host.</p>
<p>Can be provided multiple times.</p>
<p>Expects to receive either a hostname (e.g., <code>localhost</code>), a host-port pair (e.g., <code>localhost:8080</code>), or a URL (e.g., <code>https://localhost</code>).</p>
<p>WARNING: Hosts included in this list will not be verified against the system's certificate store. Only use <code>--allow-insecure-host</code> in a secure network with verified sources, as it bypasses SSL verification and could expose you to MITM attacks.</p>
<p>May also be set with the <code>UV_INSECURE_HOST</code> environment variable.</p></dd><dt id="uv-tool-update-shell--cache-dir"><a href="#uv-tool-update-shell--cache-dir"><code>--cache-dir</code></a> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>
<p>Defaults to <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on macOS and Linux, and <code>%LOCALAPPDATA%\uv\cache</code> on Windows.</p>
<p>To view the location of the cache directory, run <code>uv cache dir</code>.</p>
<p>May also be set with the <code>UV_CACHE_DIR</code> environment variable.</p></dd><dt id="uv-tool-update-shell--color"><a href="#uv-tool-update-shell--color"><code>--color</code></a> <i>color-choice</i></dt><dd><p>Control the use of color in output.</p>
<p>By default, uv will automatically detect support for colors when writing to a terminal.</p>
<p>Possible values:</p>
<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>
<li><code>always</code>:  Enables colored output regardless of the detected environment</li>
<li><code>never</code>:  Disables colored output</li>
</ul></dd><dt id="uv-tool-update-shell--config-file"><a href="#uv-tool-update-shell--config-file"><code>--config-file</code></a> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>
<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>
<p>May also be set with the <code>UV_CONFIG_FILE</code> environment variable.</p></dd><dt id="uv-tool-update-shell--directory"><a href="#uv-tool-update-shell--directory"><code>--directory</code></a> <i>directory</i></dt><dd><p>Change to the given directory prior to running the command.</p>
<p>Relative paths are resolved with the given directory as the base.</p>
<p>See <code>--project</code> to only change the project root directory.</p>
</dd><dt id="uv-tool-update-shell--help"><a href="#uv-tool-update-shell--help"><code>--help</code></a>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>
</dd><dt id="uv-tool-update-shell--managed-python"><a href="#uv-tool-update-shell--managed-python"><code>--managed-python</code></a></dt><dd><p>Require use of uv-managed Python versions.</p>
<p>By default, uv prefers using Python versions it manages. However, it will use system Python versions if a uv-managed Python is not installed. This option disables use of system Python versions.</p>
<p>May also be set with the <code>UV_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-tool-update-shell--native-tls"><a href="#uv-tool-update-shell--native-tls"><code>--native-tls</code></a></dt><dd><p>Whether to load TLS certificates from the platform's native certificate store.</p>
<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>
<p>However, in some cases, you may want to use the platform's native certificate store, especially if you're relying on a corporate trust root (e.g., for a mandatory proxy) that's included in your system's certificate store.</p>
<p>May also be set with the <code>UV_NATIVE_TLS</code> environment variable.</p></dd><dt id="uv-tool-update-shell--no-cache"><a href="#uv-tool-update-shell--no-cache"><code>--no-cache</code></a>, <code>--no-cache-dir</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>
<p>May also be set with the <code>UV_NO_CACHE</code> environment variable.</p></dd><dt id="uv-tool-update-shell--no-config"><a href="#uv-tool-update-shell--no-config"><code>--no-config</code></a></dt><dd><p>Avoid discovering configuration files (<code>pyproject.toml</code>, <code>uv.toml</code>).</p>
<p>Normally, configuration files are discovered in the current directory, parent directories, or user configuration directories.</p>
<p>May also be set with the <code>UV_NO_CONFIG</code> environment variable.</p></dd><dt id="uv-tool-update-shell--no-managed-python"><a href="#uv-tool-update-shell--no-managed-python"><code>--no-managed-python</code></a></dt><dd><p>Disable use of uv-managed Python versions.</p>
<p>Instead, uv will search for a suitable Python version on the system.</p>
<p>May also be set with the <code>UV_NO_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-tool-update-shell--no-progress"><a href="#uv-tool-update-shell--no-progress"><code>--no-progress</code></a></dt><dd><p>Hide all progress outputs.</p>
<p>For example, spinners or progress bars.</p>
<p>May also be set with the <code>UV_NO_PROGRESS</code> environment variable.</p></dd><dt id="uv-tool-update-shell--no-python-downloads"><a href="#uv-tool-update-shell--no-python-downloads"><code>--no-python-downloads</code></a></dt><dd><p>Disable automatic downloads of Python.</p>
</dd><dt id="uv-tool-update-shell--offline"><a href="#uv-tool-update-shell--offline"><code>--offline</code></a></dt><dd><p>Disable network access.</p>
<p>When disabled, uv will only use locally cached data and locally available files.</p>
<p>May also be set with the <code>UV_OFFLINE</code> environment variable.</p></dd><dt id="uv-tool-update-shell--project"><a href="#uv-tool-update-shell--project"><code>--project</code></a> <i>project</i></dt><dd><p>Run the command within the given project directory.</p>
<p>All <code>pyproject.toml</code>, <code>uv.toml</code>, and <code>.python-version</code> files will be discovered by walking up the directory tree from the project root, as will the project's virtual environment (<code>.venv</code>).</p>
<p>Other command-line arguments (such as relative paths) will be resolved relative to the current working directory.</p>
<p>See <code>--directory</code> to change the working directory entirely.</p>
<p>This setting has no effect when used in the <code>uv pip</code> interface.</p>
<p>May also be set with the <code>UV_PROJECT</code> environment variable.</p></dd><dt id="uv-tool-update-shell--quiet"><a href="#uv-tool-update-shell--quiet"><code>--quiet</code></a>, <code>-q</code></dt><dd><p>Use quiet output.</p>
<p>Repeating this option, e.g., <code>-qq</code>, will enable a silent mode in which uv will write no output to stdout.</p>
</dd><dt id="uv-tool-update-shell--verbose"><a href="#uv-tool-update-shell--verbose"><code>--verbose</code></a>, <code>-v</code></dt><dd><p>Use verbose output.</p>
<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (<a href="https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives">https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives</a>)</p>
</dd></dl>

### uv tool dir

Show the path to the uv tools directory.

The tools directory is used to store environments and metadata for installed tools.

By default, tools are stored in the uv data directory at `$XDG_DATA_HOME/uv/tools` or `$HOME/.local/share/uv/tools` on Unix and `%APPDATA%\uv\data\tools` on Windows.

The tool installation directory may be overridden with `$UV_TOOL_DIR`.

To instead view the directory uv installs executables into, use the `--bin` flag.

<h3 class="cli-reference">Usage</h3>

```
uv tool dir [OPTIONS]
```

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt id="uv-tool-dir--allow-insecure-host"><a href="#uv-tool-dir--allow-insecure-host"><code>--allow-insecure-host</code></a>, <code>--trusted-host</code> <i>allow-insecure-host</i></dt><dd><p>Allow insecure connections to a host.</p>
<p>Can be provided multiple times.</p>
<p>Expects to receive either a hostname (e.g., <code>localhost</code>), a host-port pair (e.g., <code>localhost:8080</code>), or a URL (e.g., <code>https://localhost</code>).</p>
<p>WARNING: Hosts included in this list will not be verified against the system's certificate store. Only use <code>--allow-insecure-host</code> in a secure network with verified sources, as it bypasses SSL verification and could expose you to MITM attacks.</p>
<p>May also be set with the <code>UV_INSECURE_HOST</code> environment variable.</p></dd><dt id="uv-tool-dir--bin"><a href="#uv-tool-dir--bin"><code>--bin</code></a></dt><dd><p>Show the directory into which <code>uv tool</code> will install executables.</p>
<p>By default, <code>uv tool dir</code> shows the directory into which the tool Python environments
themselves are installed, rather than the directory containing the linked executables.</p>
<p>The tool executable directory is determined according to the XDG standard and is derived
from the following environment variables, in order of preference:</p>
<ul>
<li><code>$UV_TOOL_BIN_DIR</code></li>
<li><code>$XDG_BIN_HOME</code></li>
<li><code>$XDG_DATA_HOME/../bin</code></li>
<li><code>$HOME/.local/bin</code></li>
</ul>
</dd><dt id="uv-tool-dir--cache-dir"><a href="#uv-tool-dir--cache-dir"><code>--cache-dir</code></a> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>
<p>Defaults to <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on macOS and Linux, and <code>%LOCALAPPDATA%\uv\cache</code> on Windows.</p>
<p>To view the location of the cache directory, run <code>uv cache dir</code>.</p>
<p>May also be set with the <code>UV_CACHE_DIR</code> environment variable.</p></dd><dt id="uv-tool-dir--color"><a href="#uv-tool-dir--color"><code>--color</code></a> <i>color-choice</i></dt><dd><p>Control the use of color in output.</p>
<p>By default, uv will automatically detect support for colors when writing to a terminal.</p>
<p>Possible values:</p>
<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>
<li><code>always</code>:  Enables colored output regardless of the detected environment</li>
<li><code>never</code>:  Disables colored output</li>
</ul></dd><dt id="uv-tool-dir--config-file"><a href="#uv-tool-dir--config-file"><code>--config-file</code></a> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>
<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>
<p>May also be set with the <code>UV_CONFIG_FILE</code> environment variable.</p></dd><dt id="uv-tool-dir--directory"><a href="#uv-tool-dir--directory"><code>--directory</code></a> <i>directory</i></dt><dd><p>Change to the given directory prior to running the command.</p>
<p>Relative paths are resolved with the given directory as the base.</p>
<p>See <code>--project</code> to only change the project root directory.</p>
</dd><dt id="uv-tool-dir--help"><a href="#uv-tool-dir--help"><code>--help</code></a>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>
</dd><dt id="uv-tool-dir--managed-python"><a href="#uv-tool-dir--managed-python"><code>--managed-python</code></a></dt><dd><p>Require use of uv-managed Python versions.</p>
<p>By default, uv prefers using Python versions it manages. However, it will use system Python versions if a uv-managed Python is not installed. This option disables use of system Python versions.</p>
<p>May also be set with the <code>UV_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-tool-dir--native-tls"><a href="#uv-tool-dir--native-tls"><code>--native-tls</code></a></dt><dd><p>Whether to load TLS certificates from the platform's native certificate store.</p>
<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>
<p>However, in some cases, you may want to use the platform's native certificate store, especially if you're relying on a corporate trust root (e.g., for a mandatory proxy) that's included in your system's certificate store.</p>
<p>May also be set with the <code>UV_NATIVE_TLS</code> environment variable.</p></dd><dt id="uv-tool-dir--no-cache"><a href="#uv-tool-dir--no-cache"><code>--no-cache</code></a>, <code>--no-cache-dir</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>
<p>May also be set with the <code>UV_NO_CACHE</code> environment variable.</p></dd><dt id="uv-tool-dir--no-config"><a href="#uv-tool-dir--no-config"><code>--no-config</code></a></dt><dd><p>Avoid discovering configuration files (<code>pyproject.toml</code>, <code>uv.toml</code>).</p>
<p>Normally, configuration files are discovered in the current directory, parent directories, or user configuration directories.</p>
<p>May also be set with the <code>UV_NO_CONFIG</code> environment variable.</p></dd><dt id="uv-tool-dir--no-managed-python"><a href="#uv-tool-dir--no-managed-python"><code>--no-managed-python</code></a></dt><dd><p>Disable use of uv-managed Python versions.</p>
<p>Instead, uv will search for a suitable Python version on the system.</p>
<p>May also be set with the <code>UV_NO_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-tool-dir--no-progress"><a href="#uv-tool-dir--no-progress"><code>--no-progress</code></a></dt><dd><p>Hide all progress outputs.</p>
<p>For example, spinners or progress bars.</p>
<p>May also be set with the <code>UV_NO_PROGRESS</code> environment variable.</p></dd><dt id="uv-tool-dir--no-python-downloads"><a href="#uv-tool-dir--no-python-downloads"><code>--no-python-downloads</code></a></dt><dd><p>Disable automatic downloads of Python.</p>
</dd><dt id="uv-tool-dir--offline"><a href="#uv-tool-dir--offline"><code>--offline</code></a></dt><dd><p>Disable network access.</p>
<p>When disabled, uv will only use locally cached data and locally available files.</p>
<p>May also be set with the <code>UV_OFFLINE</code> environment variable.</p></dd><dt id="uv-tool-dir--project"><a href="#uv-tool-dir--project"><code>--project</code></a> <i>project</i></dt><dd><p>Run the command within the given project directory.</p>
<p>All <code>pyproject.toml</code>, <code>uv.toml</code>, and <code>.python-version</code> files will be discovered by walking up the directory tree from the project root, as will the project's virtual environment (<code>.venv</code>).</p>
<p>Other command-line arguments (such as relative paths) will be resolved relative to the current working directory.</p>
<p>See <code>--directory</code> to change the working directory entirely.</p>
<p>This setting has no effect when used in the <code>uv pip</code> interface.</p>
<p>May also be set with the <code>UV_PROJECT</code> environment variable.</p></dd><dt id="uv-tool-dir--quiet"><a href="#uv-tool-dir--quiet"><code>--quiet</code></a>, <code>-q</code></dt><dd><p>Use quiet output.</p>
<p>Repeating this option, e.g., <code>-qq</code>, will enable a silent mode in which uv will write no output to stdout.</p>
</dd><dt id="uv-tool-dir--verbose"><a href="#uv-tool-dir--verbose"><code>--verbose</code></a>, <code>-v</code></dt><dd><p>Use verbose output.</p>
<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (<a href="https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives">https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives</a>)</p>
</dd></dl>

## uv python

Manage Python versions and installations

Generally, uv first searches for Python in a virtual environment, either active or in a
`.venv` directory in the current working directory or any parent directory. If a virtual
environment is not required, uv will then search for a Python interpreter. Python
interpreters are found by searching for Python executables in the `PATH` environment
variable.

On Windows, the registry is also searched for Python executables.

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

<h3 class="cli-reference">Usage</h3>

```
uv python [OPTIONS] <COMMAND>
```

<h3 class="cli-reference">Commands</h3>

<dl class="cli-reference"><dt><a href="#uv-python-list"><code>uv python list</code></a></dt><dd><p>List the available Python installations</p></dd>
<dt><a href="#uv-python-install"><code>uv python install</code></a></dt><dd><p>Download and install Python versions</p></dd>
<dt><a href="#uv-python-upgrade"><code>uv python upgrade</code></a></dt><dd><p>Upgrade installed Python versions to the latest supported patch release</p></dd>
<dt><a href="#uv-python-find"><code>uv python find</code></a></dt><dd><p>Search for a Python installation</p></dd>
<dt><a href="#uv-python-pin"><code>uv python pin</code></a></dt><dd><p>Pin to a specific Python version</p></dd>
<dt><a href="#uv-python-dir"><code>uv python dir</code></a></dt><dd><p>Show the uv Python installation directory</p></dd>
<dt><a href="#uv-python-uninstall"><code>uv python uninstall</code></a></dt><dd><p>Uninstall Python versions</p></dd>
</dl>

### uv python list

List the available Python installations.

By default, installed Python versions and the downloads for latest available patch version of each supported Python major version are shown.

Use `--managed-python` to view only managed Python versions.

Use `--no-managed-python` to omit managed Python versions.

Use `--all-versions` to view all available patch versions.

Use `--only-installed` to omit available downloads.

<h3 class="cli-reference">Usage</h3>

```
uv python list [OPTIONS] [REQUEST]
```

<h3 class="cli-reference">Arguments</h3>

<dl class="cli-reference"><dt id="uv-python-list--request"><a href="#uv-python-list--request"<code>REQUEST</code></a></dt><dd><p>A Python request to filter by.</p>
<p>See <a href="#uv-python">uv python</a> to view supported request formats.</p>
</dd></dl>

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt id="uv-python-list--all-arches"><a href="#uv-python-list--all-arches"><code>--all-arches</code></a>, <code>--all_architectures</code></dt><dd><p>List Python downloads for all architectures.</p>
<p>By default, only downloads for the current architecture are shown.</p>
</dd><dt id="uv-python-list--all-platforms"><a href="#uv-python-list--all-platforms"><code>--all-platforms</code></a></dt><dd><p>List Python downloads for all platforms.</p>
<p>By default, only downloads for the current platform are shown.</p>
</dd><dt id="uv-python-list--all-versions"><a href="#uv-python-list--all-versions"><code>--all-versions</code></a></dt><dd><p>List all Python versions, including old patch versions.</p>
<p>By default, only the latest patch version is shown for each minor version.</p>
</dd><dt id="uv-python-list--allow-insecure-host"><a href="#uv-python-list--allow-insecure-host"><code>--allow-insecure-host</code></a>, <code>--trusted-host</code> <i>allow-insecure-host</i></dt><dd><p>Allow insecure connections to a host.</p>
<p>Can be provided multiple times.</p>
<p>Expects to receive either a hostname (e.g., <code>localhost</code>), a host-port pair (e.g., <code>localhost:8080</code>), or a URL (e.g., <code>https://localhost</code>).</p>
<p>WARNING: Hosts included in this list will not be verified against the system's certificate store. Only use <code>--allow-insecure-host</code> in a secure network with verified sources, as it bypasses SSL verification and could expose you to MITM attacks.</p>
<p>May also be set with the <code>UV_INSECURE_HOST</code> environment variable.</p></dd><dt id="uv-python-list--cache-dir"><a href="#uv-python-list--cache-dir"><code>--cache-dir</code></a> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>
<p>Defaults to <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on macOS and Linux, and <code>%LOCALAPPDATA%\uv\cache</code> on Windows.</p>
<p>To view the location of the cache directory, run <code>uv cache dir</code>.</p>
<p>May also be set with the <code>UV_CACHE_DIR</code> environment variable.</p></dd><dt id="uv-python-list--color"><a href="#uv-python-list--color"><code>--color</code></a> <i>color-choice</i></dt><dd><p>Control the use of color in output.</p>
<p>By default, uv will automatically detect support for colors when writing to a terminal.</p>
<p>Possible values:</p>
<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>
<li><code>always</code>:  Enables colored output regardless of the detected environment</li>
<li><code>never</code>:  Disables colored output</li>
</ul></dd><dt id="uv-python-list--config-file"><a href="#uv-python-list--config-file"><code>--config-file</code></a> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>
<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>
<p>May also be set with the <code>UV_CONFIG_FILE</code> environment variable.</p></dd><dt id="uv-python-list--directory"><a href="#uv-python-list--directory"><code>--directory</code></a> <i>directory</i></dt><dd><p>Change to the given directory prior to running the command.</p>
<p>Relative paths are resolved with the given directory as the base.</p>
<p>See <code>--project</code> to only change the project root directory.</p>
</dd><dt id="uv-python-list--help"><a href="#uv-python-list--help"><code>--help</code></a>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>
</dd><dt id="uv-python-list--managed-python"><a href="#uv-python-list--managed-python"><code>--managed-python</code></a></dt><dd><p>Require use of uv-managed Python versions.</p>
<p>By default, uv prefers using Python versions it manages. However, it will use system Python versions if a uv-managed Python is not installed. This option disables use of system Python versions.</p>
<p>May also be set with the <code>UV_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-python-list--native-tls"><a href="#uv-python-list--native-tls"><code>--native-tls</code></a></dt><dd><p>Whether to load TLS certificates from the platform's native certificate store.</p>
<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>
<p>However, in some cases, you may want to use the platform's native certificate store, especially if you're relying on a corporate trust root (e.g., for a mandatory proxy) that's included in your system's certificate store.</p>
<p>May also be set with the <code>UV_NATIVE_TLS</code> environment variable.</p></dd><dt id="uv-python-list--no-cache"><a href="#uv-python-list--no-cache"><code>--no-cache</code></a>, <code>--no-cache-dir</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>
<p>May also be set with the <code>UV_NO_CACHE</code> environment variable.</p></dd><dt id="uv-python-list--no-config"><a href="#uv-python-list--no-config"><code>--no-config</code></a></dt><dd><p>Avoid discovering configuration files (<code>pyproject.toml</code>, <code>uv.toml</code>).</p>
<p>Normally, configuration files are discovered in the current directory, parent directories, or user configuration directories.</p>
<p>May also be set with the <code>UV_NO_CONFIG</code> environment variable.</p></dd><dt id="uv-python-list--no-managed-python"><a href="#uv-python-list--no-managed-python"><code>--no-managed-python</code></a></dt><dd><p>Disable use of uv-managed Python versions.</p>
<p>Instead, uv will search for a suitable Python version on the system.</p>
<p>May also be set with the <code>UV_NO_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-python-list--no-progress"><a href="#uv-python-list--no-progress"><code>--no-progress</code></a></dt><dd><p>Hide all progress outputs.</p>
<p>For example, spinners or progress bars.</p>
<p>May also be set with the <code>UV_NO_PROGRESS</code> environment variable.</p></dd><dt id="uv-python-list--no-python-downloads"><a href="#uv-python-list--no-python-downloads"><code>--no-python-downloads</code></a></dt><dd><p>Disable automatic downloads of Python.</p>
</dd><dt id="uv-python-list--offline"><a href="#uv-python-list--offline"><code>--offline</code></a></dt><dd><p>Disable network access.</p>
<p>When disabled, uv will only use locally cached data and locally available files.</p>
<p>May also be set with the <code>UV_OFFLINE</code> environment variable.</p></dd><dt id="uv-python-list--only-downloads"><a href="#uv-python-list--only-downloads"><code>--only-downloads</code></a></dt><dd><p>Only show available Python downloads.</p>
<p>By default, installed distributions and available downloads for the current platform are shown.</p>
</dd><dt id="uv-python-list--only-installed"><a href="#uv-python-list--only-installed"><code>--only-installed</code></a></dt><dd><p>Only show installed Python versions.</p>
<p>By default, installed distributions and available downloads for the current platform are shown.</p>
</dd><dt id="uv-python-list--output-format"><a href="#uv-python-list--output-format"><code>--output-format</code></a> <i>output-format</i></dt><dd><p>Select the output format</p>
<p>[default: text]</p><p>Possible values:</p>
<ul>
<li><code>text</code>:  Plain text (for humans)</li>
<li><code>json</code>:  JSON (for computers)</li>
</ul></dd><dt id="uv-python-list--project"><a href="#uv-python-list--project"><code>--project</code></a> <i>project</i></dt><dd><p>Run the command within the given project directory.</p>
<p>All <code>pyproject.toml</code>, <code>uv.toml</code>, and <code>.python-version</code> files will be discovered by walking up the directory tree from the project root, as will the project's virtual environment (<code>.venv</code>).</p>
<p>Other command-line arguments (such as relative paths) will be resolved relative to the current working directory.</p>
<p>See <code>--directory</code> to change the working directory entirely.</p>
<p>This setting has no effect when used in the <code>uv pip</code> interface.</p>
<p>May also be set with the <code>UV_PROJECT</code> environment variable.</p></dd><dt id="uv-python-list--python-downloads-json-url"><a href="#uv-python-list--python-downloads-json-url"><code>--python-downloads-json-url</code></a> <i>python-downloads-json-url</i></dt><dd><p>URL pointing to JSON of custom Python installations.</p>
<p>Note that currently, only local paths are supported.</p>
<p>May also be set with the <code>UV_PYTHON_DOWNLOADS_JSON_URL</code> environment variable.</p></dd><dt id="uv-python-list--quiet"><a href="#uv-python-list--quiet"><code>--quiet</code></a>, <code>-q</code></dt><dd><p>Use quiet output.</p>
<p>Repeating this option, e.g., <code>-qq</code>, will enable a silent mode in which uv will write no output to stdout.</p>
</dd><dt id="uv-python-list--show-urls"><a href="#uv-python-list--show-urls"><code>--show-urls</code></a></dt><dd><p>Show the URLs of available Python downloads.</p>
<p>By default, these display as <code>&lt;download available&gt;</code>.</p>
</dd><dt id="uv-python-list--verbose"><a href="#uv-python-list--verbose"><code>--verbose</code></a>, <code>-v</code></dt><dd><p>Use verbose output.</p>
<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (<a href="https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives">https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives</a>)</p>
</dd></dl>

### uv python install

Download and install Python versions.

Supports CPython and PyPy. CPython distributions are downloaded from the Astral `python-build-standalone` project. PyPy distributions are downloaded from `python.org`. The available Python versions are bundled with each uv release. To install new Python versions, you may need upgrade uv.

Python versions are installed into the uv Python directory, which can be retrieved with `uv python dir`.

A `python` executable is not made globally available, managed Python versions are only used in uv commands or in active virtual environments. There is experimental support for adding Python executables to the `PATH` — use the `--preview` flag to enable this behavior.

Multiple Python versions may be requested.

See `uv help python` to view supported request formats.

<h3 class="cli-reference">Usage</h3>

```
uv python install [OPTIONS] [TARGETS]...
```

<h3 class="cli-reference">Arguments</h3>

<dl class="cli-reference"><dt id="uv-python-install--targets"><a href="#uv-python-install--targets"<code>TARGETS</code></a></dt><dd><p>The Python version(s) to install.</p>
<p>If not provided, the requested Python version(s) will be read from the <code>UV_PYTHON</code> environment variable then <code>.python-versions</code> or <code>.python-version</code> files. If none of the above are present, uv will check if it has installed any Python versions. If not, it will install the latest stable version of Python.</p>
<p>See <a href="#uv-python">uv python</a> to view supported request formats.</p>
</dd></dl>

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt id="uv-python-install--allow-insecure-host"><a href="#uv-python-install--allow-insecure-host"><code>--allow-insecure-host</code></a>, <code>--trusted-host</code> <i>allow-insecure-host</i></dt><dd><p>Allow insecure connections to a host.</p>
<p>Can be provided multiple times.</p>
<p>Expects to receive either a hostname (e.g., <code>localhost</code>), a host-port pair (e.g., <code>localhost:8080</code>), or a URL (e.g., <code>https://localhost</code>).</p>
<p>WARNING: Hosts included in this list will not be verified against the system's certificate store. Only use <code>--allow-insecure-host</code> in a secure network with verified sources, as it bypasses SSL verification and could expose you to MITM attacks.</p>
<p>May also be set with the <code>UV_INSECURE_HOST</code> environment variable.</p></dd><dt id="uv-python-install--cache-dir"><a href="#uv-python-install--cache-dir"><code>--cache-dir</code></a> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>
<p>Defaults to <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on macOS and Linux, and <code>%LOCALAPPDATA%\uv\cache</code> on Windows.</p>
<p>To view the location of the cache directory, run <code>uv cache dir</code>.</p>
<p>May also be set with the <code>UV_CACHE_DIR</code> environment variable.</p></dd><dt id="uv-python-install--color"><a href="#uv-python-install--color"><code>--color</code></a> <i>color-choice</i></dt><dd><p>Control the use of color in output.</p>
<p>By default, uv will automatically detect support for colors when writing to a terminal.</p>
<p>Possible values:</p>
<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>
<li><code>always</code>:  Enables colored output regardless of the detected environment</li>
<li><code>never</code>:  Disables colored output</li>
</ul></dd><dt id="uv-python-install--config-file"><a href="#uv-python-install--config-file"><code>--config-file</code></a> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>
<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>
<p>May also be set with the <code>UV_CONFIG_FILE</code> environment variable.</p></dd><dt id="uv-python-install--default"><a href="#uv-python-install--default"><code>--default</code></a></dt><dd><p>Use as the default Python version.</p>
<p>By default, only a <code>python{major}.{minor}</code> executable is installed, e.g., <code>python3.10</code>. When the <code>--default</code> flag is used, <code>python{major}</code>, e.g., <code>python3</code>, and <code>python</code> executables are also installed.</p>
<p>Alternative Python variants will still include their tag. For example, installing 3.13+freethreaded with <code>--default</code> will include in <code>python3t</code> and <code>pythont</code>, not <code>python3</code> and <code>python</code>.</p>
<p>If multiple Python versions are requested, uv will exit with an error.</p>
</dd><dt id="uv-python-install--directory"><a href="#uv-python-install--directory"><code>--directory</code></a> <i>directory</i></dt><dd><p>Change to the given directory prior to running the command.</p>
<p>Relative paths are resolved with the given directory as the base.</p>
<p>See <code>--project</code> to only change the project root directory.</p>
</dd><dt id="uv-python-install--force"><a href="#uv-python-install--force"><code>--force</code></a>, <code>-f</code></dt><dd><p>Replace existing Python executables during installation.</p>
<p>By default, uv will refuse to replace executables that it does not manage.</p>
<p>Implies <code>--reinstall</code>.</p>
</dd><dt id="uv-python-install--help"><a href="#uv-python-install--help"><code>--help</code></a>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>
</dd><dt id="uv-python-install--install-dir"><a href="#uv-python-install--install-dir"><code>--install-dir</code></a>, <code>-i</code> <i>install-dir</i></dt><dd><p>The directory to store the Python installation in.</p>
<p>If provided, <code>UV_PYTHON_INSTALL_DIR</code> will need to be set for subsequent operations for uv to discover the Python installation.</p>
<p>See <code>uv python dir</code> to view the current Python installation directory. Defaults to <code>~/.local/share/uv/python</code>.</p>
<p>May also be set with the <code>UV_PYTHON_INSTALL_DIR</code> environment variable.</p></dd><dt id="uv-python-install--managed-python"><a href="#uv-python-install--managed-python"><code>--managed-python</code></a></dt><dd><p>Require use of uv-managed Python versions.</p>
<p>By default, uv prefers using Python versions it manages. However, it will use system Python versions if a uv-managed Python is not installed. This option disables use of system Python versions.</p>
<p>May also be set with the <code>UV_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-python-install--mirror"><a href="#uv-python-install--mirror"><code>--mirror</code></a> <i>mirror</i></dt><dd><p>Set the URL to use as the source for downloading Python installations.</p>
<p>The provided URL will replace <code>https://github.com/astral-sh/python-build-standalone/releases/download</code> in, e.g., <code>https://github.com/astral-sh/python-build-standalone/releases/download/20240713/cpython-3.12.4%2B20240713-aarch64-apple-darwin-install_only.tar.gz</code>.</p>
<p>Distributions can be read from a local directory by using the <code>file://</code> URL scheme.</p>
<p>May also be set with the <code>UV_PYTHON_INSTALL_MIRROR</code> environment variable.</p></dd><dt id="uv-python-install--native-tls"><a href="#uv-python-install--native-tls"><code>--native-tls</code></a></dt><dd><p>Whether to load TLS certificates from the platform's native certificate store.</p>
<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>
<p>However, in some cases, you may want to use the platform's native certificate store, especially if you're relying on a corporate trust root (e.g., for a mandatory proxy) that's included in your system's certificate store.</p>
<p>May also be set with the <code>UV_NATIVE_TLS</code> environment variable.</p></dd><dt id="uv-python-install--no-cache"><a href="#uv-python-install--no-cache"><code>--no-cache</code></a>, <code>--no-cache-dir</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>
<p>May also be set with the <code>UV_NO_CACHE</code> environment variable.</p></dd><dt id="uv-python-install--no-config"><a href="#uv-python-install--no-config"><code>--no-config</code></a></dt><dd><p>Avoid discovering configuration files (<code>pyproject.toml</code>, <code>uv.toml</code>).</p>
<p>Normally, configuration files are discovered in the current directory, parent directories, or user configuration directories.</p>
<p>May also be set with the <code>UV_NO_CONFIG</code> environment variable.</p></dd><dt id="uv-python-install--no-managed-python"><a href="#uv-python-install--no-managed-python"><code>--no-managed-python</code></a></dt><dd><p>Disable use of uv-managed Python versions.</p>
<p>Instead, uv will search for a suitable Python version on the system.</p>
<p>May also be set with the <code>UV_NO_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-python-install--no-progress"><a href="#uv-python-install--no-progress"><code>--no-progress</code></a></dt><dd><p>Hide all progress outputs.</p>
<p>For example, spinners or progress bars.</p>
<p>May also be set with the <code>UV_NO_PROGRESS</code> environment variable.</p></dd><dt id="uv-python-install--no-python-downloads"><a href="#uv-python-install--no-python-downloads"><code>--no-python-downloads</code></a></dt><dd><p>Disable automatic downloads of Python.</p>
</dd><dt id="uv-python-install--offline"><a href="#uv-python-install--offline"><code>--offline</code></a></dt><dd><p>Disable network access.</p>
<p>When disabled, uv will only use locally cached data and locally available files.</p>
<p>May also be set with the <code>UV_OFFLINE</code> environment variable.</p></dd><dt id="uv-python-install--project"><a href="#uv-python-install--project"><code>--project</code></a> <i>project</i></dt><dd><p>Run the command within the given project directory.</p>
<p>All <code>pyproject.toml</code>, <code>uv.toml</code>, and <code>.python-version</code> files will be discovered by walking up the directory tree from the project root, as will the project's virtual environment (<code>.venv</code>).</p>
<p>Other command-line arguments (such as relative paths) will be resolved relative to the current working directory.</p>
<p>See <code>--directory</code> to change the working directory entirely.</p>
<p>This setting has no effect when used in the <code>uv pip</code> interface.</p>
<p>May also be set with the <code>UV_PROJECT</code> environment variable.</p></dd><dt id="uv-python-install--pypy-mirror"><a href="#uv-python-install--pypy-mirror"><code>--pypy-mirror</code></a> <i>pypy-mirror</i></dt><dd><p>Set the URL to use as the source for downloading PyPy installations.</p>
<p>The provided URL will replace <code>https://downloads.python.org/pypy</code> in, e.g., <code>https://downloads.python.org/pypy/pypy3.8-v7.3.7-osx64.tar.bz2</code>.</p>
<p>Distributions can be read from a local directory by using the <code>file://</code> URL scheme.</p>
<p>May also be set with the <code>UV_PYPY_INSTALL_MIRROR</code> environment variable.</p></dd><dt id="uv-python-install--python-downloads-json-url"><a href="#uv-python-install--python-downloads-json-url"><code>--python-downloads-json-url</code></a> <i>python-downloads-json-url</i></dt><dd><p>URL pointing to JSON of custom Python installations.</p>
<p>Note that currently, only local paths are supported.</p>
<p>May also be set with the <code>UV_PYTHON_DOWNLOADS_JSON_URL</code> environment variable.</p></dd><dt id="uv-python-install--quiet"><a href="#uv-python-install--quiet"><code>--quiet</code></a>, <code>-q</code></dt><dd><p>Use quiet output.</p>
<p>Repeating this option, e.g., <code>-qq</code>, will enable a silent mode in which uv will write no output to stdout.</p>
</dd><dt id="uv-python-install--reinstall"><a href="#uv-python-install--reinstall"><code>--reinstall</code></a>, <code>-r</code></dt><dd><p>Reinstall the requested Python version, if it's already installed.</p>
<p>By default, uv will exit successfully if the version is already installed.</p>
</dd><dt id="uv-python-install--verbose"><a href="#uv-python-install--verbose"><code>--verbose</code></a>, <code>-v</code></dt><dd><p>Use verbose output.</p>
<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (<a href="https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives">https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives</a>)</p>
</dd></dl>

### uv python upgrade

Upgrade installed Python versions to the latest supported patch release.

A target Python minor version to upgrade may be provided, e.g., `3.13`. Multiple versions may be provided to perform more than one upgrade.

If no target version is provided, then uv will upgrade all managed CPython versions.

During an upgrade, uv will not uninstall outdated patch versions.

When an upgrade is performed, virtual environments created by uv will automatically use the new version. However, if the virtual environment was created before the upgrade functionality was added, it will continue to use the old Python version; to enable upgrades, the environment must be recreated.

Upgrades are not yet supported for alternative implementations, like PyPy.

<h3 class="cli-reference">Usage</h3>

```
uv python upgrade [OPTIONS] [TARGETS]...
```

<h3 class="cli-reference">Arguments</h3>

<dl class="cli-reference"><dt id="uv-python-upgrade--targets"><a href="#uv-python-upgrade--targets"<code>TARGETS</code></a></dt><dd><p>The Python minor version(s) to upgrade.</p>
<p>If no target version is provided, then uv will upgrade all managed CPython versions.</p>
</dd></dl>

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt id="uv-python-upgrade--allow-insecure-host"><a href="#uv-python-upgrade--allow-insecure-host"><code>--allow-insecure-host</code></a>, <code>--trusted-host</code> <i>allow-insecure-host</i></dt><dd><p>Allow insecure connections to a host.</p>
<p>Can be provided multiple times.</p>
<p>Expects to receive either a hostname (e.g., <code>localhost</code>), a host-port pair (e.g., <code>localhost:8080</code>), or a URL (e.g., <code>https://localhost</code>).</p>
<p>WARNING: Hosts included in this list will not be verified against the system's certificate store. Only use <code>--allow-insecure-host</code> in a secure network with verified sources, as it bypasses SSL verification and could expose you to MITM attacks.</p>
<p>May also be set with the <code>UV_INSECURE_HOST</code> environment variable.</p></dd><dt id="uv-python-upgrade--cache-dir"><a href="#uv-python-upgrade--cache-dir"><code>--cache-dir</code></a> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>
<p>Defaults to <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on macOS and Linux, and <code>%LOCALAPPDATA%\uv\cache</code> on Windows.</p>
<p>To view the location of the cache directory, run <code>uv cache dir</code>.</p>
<p>May also be set with the <code>UV_CACHE_DIR</code> environment variable.</p></dd><dt id="uv-python-upgrade--color"><a href="#uv-python-upgrade--color"><code>--color</code></a> <i>color-choice</i></dt><dd><p>Control the use of color in output.</p>
<p>By default, uv will automatically detect support for colors when writing to a terminal.</p>
<p>Possible values:</p>
<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>
<li><code>always</code>:  Enables colored output regardless of the detected environment</li>
<li><code>never</code>:  Disables colored output</li>
</ul></dd><dt id="uv-python-upgrade--config-file"><a href="#uv-python-upgrade--config-file"><code>--config-file</code></a> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>
<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>
<p>May also be set with the <code>UV_CONFIG_FILE</code> environment variable.</p></dd><dt id="uv-python-upgrade--directory"><a href="#uv-python-upgrade--directory"><code>--directory</code></a> <i>directory</i></dt><dd><p>Change to the given directory prior to running the command.</p>
<p>Relative paths are resolved with the given directory as the base.</p>
<p>See <code>--project</code> to only change the project root directory.</p>
</dd><dt id="uv-python-upgrade--help"><a href="#uv-python-upgrade--help"><code>--help</code></a>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>
</dd><dt id="uv-python-upgrade--install-dir"><a href="#uv-python-upgrade--install-dir"><code>--install-dir</code></a>, <code>-i</code> <i>install-dir</i></dt><dd><p>The directory Python installations are stored in.</p>
<p>If provided, <code>UV_PYTHON_INSTALL_DIR</code> will need to be set for subsequent operations for uv to discover the Python installation.</p>
<p>See <code>uv python dir</code> to view the current Python installation directory. Defaults to <code>~/.local/share/uv/python</code>.</p>
<p>May also be set with the <code>UV_PYTHON_INSTALL_DIR</code> environment variable.</p></dd><dt id="uv-python-upgrade--managed-python"><a href="#uv-python-upgrade--managed-python"><code>--managed-python</code></a></dt><dd><p>Require use of uv-managed Python versions.</p>
<p>By default, uv prefers using Python versions it manages. However, it will use system Python versions if a uv-managed Python is not installed. This option disables use of system Python versions.</p>
<p>May also be set with the <code>UV_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-python-upgrade--mirror"><a href="#uv-python-upgrade--mirror"><code>--mirror</code></a> <i>mirror</i></dt><dd><p>Set the URL to use as the source for downloading Python installations.</p>
<p>The provided URL will replace <code>https://github.com/astral-sh/python-build-standalone/releases/download</code> in, e.g., <code>https://github.com/astral-sh/python-build-standalone/releases/download/20240713/cpython-3.12.4%2B20240713-aarch64-apple-darwin-install_only.tar.gz</code>.</p>
<p>Distributions can be read from a local directory by using the <code>file://</code> URL scheme.</p>
<p>May also be set with the <code>UV_PYTHON_INSTALL_MIRROR</code> environment variable.</p></dd><dt id="uv-python-upgrade--native-tls"><a href="#uv-python-upgrade--native-tls"><code>--native-tls</code></a></dt><dd><p>Whether to load TLS certificates from the platform's native certificate store.</p>
<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>
<p>However, in some cases, you may want to use the platform's native certificate store, especially if you're relying on a corporate trust root (e.g., for a mandatory proxy) that's included in your system's certificate store.</p>
<p>May also be set with the <code>UV_NATIVE_TLS</code> environment variable.</p></dd><dt id="uv-python-upgrade--no-cache"><a href="#uv-python-upgrade--no-cache"><code>--no-cache</code></a>, <code>--no-cache-dir</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>
<p>May also be set with the <code>UV_NO_CACHE</code> environment variable.</p></dd><dt id="uv-python-upgrade--no-config"><a href="#uv-python-upgrade--no-config"><code>--no-config</code></a></dt><dd><p>Avoid discovering configuration files (<code>pyproject.toml</code>, <code>uv.toml</code>).</p>
<p>Normally, configuration files are discovered in the current directory, parent directories, or user configuration directories.</p>
<p>May also be set with the <code>UV_NO_CONFIG</code> environment variable.</p></dd><dt id="uv-python-upgrade--no-managed-python"><a href="#uv-python-upgrade--no-managed-python"><code>--no-managed-python</code></a></dt><dd><p>Disable use of uv-managed Python versions.</p>
<p>Instead, uv will search for a suitable Python version on the system.</p>
<p>May also be set with the <code>UV_NO_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-python-upgrade--no-progress"><a href="#uv-python-upgrade--no-progress"><code>--no-progress</code></a></dt><dd><p>Hide all progress outputs.</p>
<p>For example, spinners or progress bars.</p>
<p>May also be set with the <code>UV_NO_PROGRESS</code> environment variable.</p></dd><dt id="uv-python-upgrade--no-python-downloads"><a href="#uv-python-upgrade--no-python-downloads"><code>--no-python-downloads</code></a></dt><dd><p>Disable automatic downloads of Python.</p>
</dd><dt id="uv-python-upgrade--offline"><a href="#uv-python-upgrade--offline"><code>--offline</code></a></dt><dd><p>Disable network access.</p>
<p>When disabled, uv will only use locally cached data and locally available files.</p>
<p>May also be set with the <code>UV_OFFLINE</code> environment variable.</p></dd><dt id="uv-python-upgrade--project"><a href="#uv-python-upgrade--project"><code>--project</code></a> <i>project</i></dt><dd><p>Run the command within the given project directory.</p>
<p>All <code>pyproject.toml</code>, <code>uv.toml</code>, and <code>.python-version</code> files will be discovered by walking up the directory tree from the project root, as will the project's virtual environment (<code>.venv</code>).</p>
<p>Other command-line arguments (such as relative paths) will be resolved relative to the current working directory.</p>
<p>See <code>--directory</code> to change the working directory entirely.</p>
<p>This setting has no effect when used in the <code>uv pip</code> interface.</p>
<p>May also be set with the <code>UV_PROJECT</code> environment variable.</p></dd><dt id="uv-python-upgrade--pypy-mirror"><a href="#uv-python-upgrade--pypy-mirror"><code>--pypy-mirror</code></a> <i>pypy-mirror</i></dt><dd><p>Set the URL to use as the source for downloading PyPy installations.</p>
<p>The provided URL will replace <code>https://downloads.python.org/pypy</code> in, e.g., <code>https://downloads.python.org/pypy/pypy3.8-v7.3.7-osx64.tar.bz2</code>.</p>
<p>Distributions can be read from a local directory by using the <code>file://</code> URL scheme.</p>
<p>May also be set with the <code>UV_PYPY_INSTALL_MIRROR</code> environment variable.</p></dd><dt id="uv-python-upgrade--python-downloads-json-url"><a href="#uv-python-upgrade--python-downloads-json-url"><code>--python-downloads-json-url</code></a> <i>python-downloads-json-url</i></dt><dd><p>URL pointing to JSON of custom Python installations.</p>
<p>Note that currently, only local paths are supported.</p>
<p>May also be set with the <code>UV_PYTHON_DOWNLOADS_JSON_URL</code> environment variable.</p></dd><dt id="uv-python-upgrade--quiet"><a href="#uv-python-upgrade--quiet"><code>--quiet</code></a>, <code>-q</code></dt><dd><p>Use quiet output.</p>
<p>Repeating this option, e.g., <code>-qq</code>, will enable a silent mode in which uv will write no output to stdout.</p>
</dd><dt id="uv-python-upgrade--verbose"><a href="#uv-python-upgrade--verbose"><code>--verbose</code></a>, <code>-v</code></dt><dd><p>Use verbose output.</p>
<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (<a href="https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives">https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives</a>)</p>
</dd></dl>

### uv python find

Search for a Python installation.

Displays the path to the Python executable.

See `uv help python` to view supported request formats and details on discovery behavior.

<h3 class="cli-reference">Usage</h3>

```
uv python find [OPTIONS] [REQUEST]
```

<h3 class="cli-reference">Arguments</h3>

<dl class="cli-reference"><dt id="uv-python-find--request"><a href="#uv-python-find--request"<code>REQUEST</code></a></dt><dd><p>The Python request.</p>
<p>See <a href="#uv-python">uv python</a> to view supported request formats.</p>
</dd></dl>

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt id="uv-python-find--allow-insecure-host"><a href="#uv-python-find--allow-insecure-host"><code>--allow-insecure-host</code></a>, <code>--trusted-host</code> <i>allow-insecure-host</i></dt><dd><p>Allow insecure connections to a host.</p>
<p>Can be provided multiple times.</p>
<p>Expects to receive either a hostname (e.g., <code>localhost</code>), a host-port pair (e.g., <code>localhost:8080</code>), or a URL (e.g., <code>https://localhost</code>).</p>
<p>WARNING: Hosts included in this list will not be verified against the system's certificate store. Only use <code>--allow-insecure-host</code> in a secure network with verified sources, as it bypasses SSL verification and could expose you to MITM attacks.</p>
<p>May also be set with the <code>UV_INSECURE_HOST</code> environment variable.</p></dd><dt id="uv-python-find--cache-dir"><a href="#uv-python-find--cache-dir"><code>--cache-dir</code></a> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>
<p>Defaults to <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on macOS and Linux, and <code>%LOCALAPPDATA%\uv\cache</code> on Windows.</p>
<p>To view the location of the cache directory, run <code>uv cache dir</code>.</p>
<p>May also be set with the <code>UV_CACHE_DIR</code> environment variable.</p></dd><dt id="uv-python-find--color"><a href="#uv-python-find--color"><code>--color</code></a> <i>color-choice</i></dt><dd><p>Control the use of color in output.</p>
<p>By default, uv will automatically detect support for colors when writing to a terminal.</p>
<p>Possible values:</p>
<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>
<li><code>always</code>:  Enables colored output regardless of the detected environment</li>
<li><code>never</code>:  Disables colored output</li>
</ul></dd><dt id="uv-python-find--config-file"><a href="#uv-python-find--config-file"><code>--config-file</code></a> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>
<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>
<p>May also be set with the <code>UV_CONFIG_FILE</code> environment variable.</p></dd><dt id="uv-python-find--directory"><a href="#uv-python-find--directory"><code>--directory</code></a> <i>directory</i></dt><dd><p>Change to the given directory prior to running the command.</p>
<p>Relative paths are resolved with the given directory as the base.</p>
<p>See <code>--project</code> to only change the project root directory.</p>
</dd><dt id="uv-python-find--help"><a href="#uv-python-find--help"><code>--help</code></a>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>
</dd><dt id="uv-python-find--managed-python"><a href="#uv-python-find--managed-python"><code>--managed-python</code></a></dt><dd><p>Require use of uv-managed Python versions.</p>
<p>By default, uv prefers using Python versions it manages. However, it will use system Python versions if a uv-managed Python is not installed. This option disables use of system Python versions.</p>
<p>May also be set with the <code>UV_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-python-find--native-tls"><a href="#uv-python-find--native-tls"><code>--native-tls</code></a></dt><dd><p>Whether to load TLS certificates from the platform's native certificate store.</p>
<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>
<p>However, in some cases, you may want to use the platform's native certificate store, especially if you're relying on a corporate trust root (e.g., for a mandatory proxy) that's included in your system's certificate store.</p>
<p>May also be set with the <code>UV_NATIVE_TLS</code> environment variable.</p></dd><dt id="uv-python-find--no-cache"><a href="#uv-python-find--no-cache"><code>--no-cache</code></a>, <code>--no-cache-dir</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>
<p>May also be set with the <code>UV_NO_CACHE</code> environment variable.</p></dd><dt id="uv-python-find--no-config"><a href="#uv-python-find--no-config"><code>--no-config</code></a></dt><dd><p>Avoid discovering configuration files (<code>pyproject.toml</code>, <code>uv.toml</code>).</p>
<p>Normally, configuration files are discovered in the current directory, parent directories, or user configuration directories.</p>
<p>May also be set with the <code>UV_NO_CONFIG</code> environment variable.</p></dd><dt id="uv-python-find--no-managed-python"><a href="#uv-python-find--no-managed-python"><code>--no-managed-python</code></a></dt><dd><p>Disable use of uv-managed Python versions.</p>
<p>Instead, uv will search for a suitable Python version on the system.</p>
<p>May also be set with the <code>UV_NO_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-python-find--no-progress"><a href="#uv-python-find--no-progress"><code>--no-progress</code></a></dt><dd><p>Hide all progress outputs.</p>
<p>For example, spinners or progress bars.</p>
<p>May also be set with the <code>UV_NO_PROGRESS</code> environment variable.</p></dd><dt id="uv-python-find--no-project"><a href="#uv-python-find--no-project"><code>--no-project</code></a>, <code>--no_workspace</code></dt><dd><p>Avoid discovering a project or workspace.</p>
<p>Otherwise, when no request is provided, the Python requirement of a project in the current directory or parent directories will be used.</p>
</dd><dt id="uv-python-find--no-python-downloads"><a href="#uv-python-find--no-python-downloads"><code>--no-python-downloads</code></a></dt><dd><p>Disable automatic downloads of Python.</p>
</dd><dt id="uv-python-find--offline"><a href="#uv-python-find--offline"><code>--offline</code></a></dt><dd><p>Disable network access.</p>
<p>When disabled, uv will only use locally cached data and locally available files.</p>
<p>May also be set with the <code>UV_OFFLINE</code> environment variable.</p></dd><dt id="uv-python-find--project"><a href="#uv-python-find--project"><code>--project</code></a> <i>project</i></dt><dd><p>Run the command within the given project directory.</p>
<p>All <code>pyproject.toml</code>, <code>uv.toml</code>, and <code>.python-version</code> files will be discovered by walking up the directory tree from the project root, as will the project's virtual environment (<code>.venv</code>).</p>
<p>Other command-line arguments (such as relative paths) will be resolved relative to the current working directory.</p>
<p>See <code>--directory</code> to change the working directory entirely.</p>
<p>This setting has no effect when used in the <code>uv pip</code> interface.</p>
<p>May also be set with the <code>UV_PROJECT</code> environment variable.</p></dd><dt id="uv-python-find--quiet"><a href="#uv-python-find--quiet"><code>--quiet</code></a>, <code>-q</code></dt><dd><p>Use quiet output.</p>
<p>Repeating this option, e.g., <code>-qq</code>, will enable a silent mode in which uv will write no output to stdout.</p>
</dd><dt id="uv-python-find--script"><a href="#uv-python-find--script"><code>--script</code></a> <i>script</i></dt><dd><p>Find the environment for a Python script, rather than the current project</p>
</dd><dt id="uv-python-find--show-version"><a href="#uv-python-find--show-version"><code>--show-version</code></a></dt><dd><p>Show the Python version that would be used instead of the path to the interpreter</p>
</dd><dt id="uv-python-find--system"><a href="#uv-python-find--system"><code>--system</code></a></dt><dd><p>Only find system Python interpreters.</p>
<p>By default, uv will report the first Python interpreter it would use, including those in an active virtual environment or a virtual environment in the current working directory or any parent directory.</p>
<p>The <code>--system</code> option instructs uv to skip virtual environment Python interpreters and restrict its search to the system path.</p>
<p>May also be set with the <code>UV_SYSTEM_PYTHON</code> environment variable.</p></dd><dt id="uv-python-find--verbose"><a href="#uv-python-find--verbose"><code>--verbose</code></a>, <code>-v</code></dt><dd><p>Use verbose output.</p>
<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (<a href="https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives">https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives</a>)</p>
</dd></dl>

### uv python pin

Pin to a specific Python version.

Writes the pinned Python version to a `.python-version` file, which is used by other uv commands to determine the required Python version.

If no version is provided, uv will look for an existing `.python-version` file and display the currently pinned version. If no `.python-version` file is found, uv will exit with an error.

See `uv help python` to view supported request formats.

<h3 class="cli-reference">Usage</h3>

```
uv python pin [OPTIONS] [REQUEST]
```

<h3 class="cli-reference">Arguments</h3>

<dl class="cli-reference"><dt id="uv-python-pin--request"><a href="#uv-python-pin--request"<code>REQUEST</code></a></dt><dd><p>The Python version request.</p>
<p>uv supports more formats than other tools that read <code>.python-version</code> files, i.e., <code>pyenv</code>. If compatibility with those tools is needed, only use version numbers instead of complex requests such as <code>cpython@3.10</code>.</p>
<p>If no request is provided, the currently pinned version will be shown.</p>
<p>See <a href="#uv-python">uv python</a> to view supported request formats.</p>
</dd></dl>

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt id="uv-python-pin--allow-insecure-host"><a href="#uv-python-pin--allow-insecure-host"><code>--allow-insecure-host</code></a>, <code>--trusted-host</code> <i>allow-insecure-host</i></dt><dd><p>Allow insecure connections to a host.</p>
<p>Can be provided multiple times.</p>
<p>Expects to receive either a hostname (e.g., <code>localhost</code>), a host-port pair (e.g., <code>localhost:8080</code>), or a URL (e.g., <code>https://localhost</code>).</p>
<p>WARNING: Hosts included in this list will not be verified against the system's certificate store. Only use <code>--allow-insecure-host</code> in a secure network with verified sources, as it bypasses SSL verification and could expose you to MITM attacks.</p>
<p>May also be set with the <code>UV_INSECURE_HOST</code> environment variable.</p></dd><dt id="uv-python-pin--cache-dir"><a href="#uv-python-pin--cache-dir"><code>--cache-dir</code></a> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>
<p>Defaults to <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on macOS and Linux, and <code>%LOCALAPPDATA%\uv\cache</code> on Windows.</p>
<p>To view the location of the cache directory, run <code>uv cache dir</code>.</p>
<p>May also be set with the <code>UV_CACHE_DIR</code> environment variable.</p></dd><dt id="uv-python-pin--color"><a href="#uv-python-pin--color"><code>--color</code></a> <i>color-choice</i></dt><dd><p>Control the use of color in output.</p>
<p>By default, uv will automatically detect support for colors when writing to a terminal.</p>
<p>Possible values:</p>
<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>
<li><code>always</code>:  Enables colored output regardless of the detected environment</li>
<li><code>never</code>:  Disables colored output</li>
</ul></dd><dt id="uv-python-pin--config-file"><a href="#uv-python-pin--config-file"><code>--config-file</code></a> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>
<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>
<p>May also be set with the <code>UV_CONFIG_FILE</code> environment variable.</p></dd><dt id="uv-python-pin--directory"><a href="#uv-python-pin--directory"><code>--directory</code></a> <i>directory</i></dt><dd><p>Change to the given directory prior to running the command.</p>
<p>Relative paths are resolved with the given directory as the base.</p>
<p>See <code>--project</code> to only change the project root directory.</p>
</dd><dt id="uv-python-pin--global"><a href="#uv-python-pin--global"><code>--global</code></a></dt><dd><p>Update the global Python version pin.</p>
<p>Writes the pinned Python version to a <code>.python-version</code> file in the uv user configuration directory: <code>XDG_CONFIG_HOME/uv</code> on Linux/macOS and <code>%APPDATA%/uv</code> on Windows.</p>
<p>When a local Python version pin is not found in the working directory or an ancestor directory, this version will be used instead.</p>
</dd><dt id="uv-python-pin--help"><a href="#uv-python-pin--help"><code>--help</code></a>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>
</dd><dt id="uv-python-pin--managed-python"><a href="#uv-python-pin--managed-python"><code>--managed-python</code></a></dt><dd><p>Require use of uv-managed Python versions.</p>
<p>By default, uv prefers using Python versions it manages. However, it will use system Python versions if a uv-managed Python is not installed. This option disables use of system Python versions.</p>
<p>May also be set with the <code>UV_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-python-pin--native-tls"><a href="#uv-python-pin--native-tls"><code>--native-tls</code></a></dt><dd><p>Whether to load TLS certificates from the platform's native certificate store.</p>
<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>
<p>However, in some cases, you may want to use the platform's native certificate store, especially if you're relying on a corporate trust root (e.g., for a mandatory proxy) that's included in your system's certificate store.</p>
<p>May also be set with the <code>UV_NATIVE_TLS</code> environment variable.</p></dd><dt id="uv-python-pin--no-cache"><a href="#uv-python-pin--no-cache"><code>--no-cache</code></a>, <code>--no-cache-dir</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>
<p>May also be set with the <code>UV_NO_CACHE</code> environment variable.</p></dd><dt id="uv-python-pin--no-config"><a href="#uv-python-pin--no-config"><code>--no-config</code></a></dt><dd><p>Avoid discovering configuration files (<code>pyproject.toml</code>, <code>uv.toml</code>).</p>
<p>Normally, configuration files are discovered in the current directory, parent directories, or user configuration directories.</p>
<p>May also be set with the <code>UV_NO_CONFIG</code> environment variable.</p></dd><dt id="uv-python-pin--no-managed-python"><a href="#uv-python-pin--no-managed-python"><code>--no-managed-python</code></a></dt><dd><p>Disable use of uv-managed Python versions.</p>
<p>Instead, uv will search for a suitable Python version on the system.</p>
<p>May also be set with the <code>UV_NO_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-python-pin--no-progress"><a href="#uv-python-pin--no-progress"><code>--no-progress</code></a></dt><dd><p>Hide all progress outputs.</p>
<p>For example, spinners or progress bars.</p>
<p>May also be set with the <code>UV_NO_PROGRESS</code> environment variable.</p></dd><dt id="uv-python-pin--no-project"><a href="#uv-python-pin--no-project"><code>--no-project</code></a>, <code>--no-workspace</code></dt><dd><p>Avoid validating the Python pin is compatible with the project or workspace.</p>
<p>By default, a project or workspace is discovered in the current directory or any parent directory. If a workspace is found, the Python pin is validated against the workspace's <code>requires-python</code> constraint.</p>
</dd><dt id="uv-python-pin--no-python-downloads"><a href="#uv-python-pin--no-python-downloads"><code>--no-python-downloads</code></a></dt><dd><p>Disable automatic downloads of Python.</p>
</dd><dt id="uv-python-pin--offline"><a href="#uv-python-pin--offline"><code>--offline</code></a></dt><dd><p>Disable network access.</p>
<p>When disabled, uv will only use locally cached data and locally available files.</p>
<p>May also be set with the <code>UV_OFFLINE</code> environment variable.</p></dd><dt id="uv-python-pin--project"><a href="#uv-python-pin--project"><code>--project</code></a> <i>project</i></dt><dd><p>Run the command within the given project directory.</p>
<p>All <code>pyproject.toml</code>, <code>uv.toml</code>, and <code>.python-version</code> files will be discovered by walking up the directory tree from the project root, as will the project's virtual environment (<code>.venv</code>).</p>
<p>Other command-line arguments (such as relative paths) will be resolved relative to the current working directory.</p>
<p>See <code>--directory</code> to change the working directory entirely.</p>
<p>This setting has no effect when used in the <code>uv pip</code> interface.</p>
<p>May also be set with the <code>UV_PROJECT</code> environment variable.</p></dd><dt id="uv-python-pin--quiet"><a href="#uv-python-pin--quiet"><code>--quiet</code></a>, <code>-q</code></dt><dd><p>Use quiet output.</p>
<p>Repeating this option, e.g., <code>-qq</code>, will enable a silent mode in which uv will write no output to stdout.</p>
</dd><dt id="uv-python-pin--resolved"><a href="#uv-python-pin--resolved"><code>--resolved</code></a></dt><dd><p>Write the resolved Python interpreter path instead of the request.</p>
<p>Ensures that the exact same interpreter is used.</p>
<p>This option is usually not safe to use when committing the <code>.python-version</code> file to version control.</p>
</dd><dt id="uv-python-pin--verbose"><a href="#uv-python-pin--verbose"><code>--verbose</code></a>, <code>-v</code></dt><dd><p>Use verbose output.</p>
<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (<a href="https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives">https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives</a>)</p>
</dd></dl>

### uv python dir

Show the uv Python installation directory.

By default, Python installations are stored in the uv data directory at `$XDG_DATA_HOME/uv/python` or `$HOME/.local/share/uv/python` on Unix and `%APPDATA%\uv\data\python` on Windows.

The Python installation directory may be overridden with `$UV_PYTHON_INSTALL_DIR`.

To view the directory where uv installs Python executables instead, use the `--bin` flag. Note that Python executables are only installed when preview mode is enabled.

<h3 class="cli-reference">Usage</h3>

```
uv python dir [OPTIONS]
```

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt id="uv-python-dir--allow-insecure-host"><a href="#uv-python-dir--allow-insecure-host"><code>--allow-insecure-host</code></a>, <code>--trusted-host</code> <i>allow-insecure-host</i></dt><dd><p>Allow insecure connections to a host.</p>
<p>Can be provided multiple times.</p>
<p>Expects to receive either a hostname (e.g., <code>localhost</code>), a host-port pair (e.g., <code>localhost:8080</code>), or a URL (e.g., <code>https://localhost</code>).</p>
<p>WARNING: Hosts included in this list will not be verified against the system's certificate store. Only use <code>--allow-insecure-host</code> in a secure network with verified sources, as it bypasses SSL verification and could expose you to MITM attacks.</p>
<p>May also be set with the <code>UV_INSECURE_HOST</code> environment variable.</p></dd><dt id="uv-python-dir--bin"><a href="#uv-python-dir--bin"><code>--bin</code></a></dt><dd><p>Show the directory into which <code>uv python</code> will install Python executables.</p>
<p>Note that this directory is only used when installing Python with preview mode enabled.</p>
<p>The Python executable directory is determined according to the XDG standard and is derived
from the following environment variables, in order of preference:</p>
<ul>
<li><code>$UV_PYTHON_BIN_DIR</code></li>
<li><code>$XDG_BIN_HOME</code></li>
<li><code>$XDG_DATA_HOME/../bin</code></li>
<li><code>$HOME/.local/bin</code></li>
</ul>
</dd><dt id="uv-python-dir--cache-dir"><a href="#uv-python-dir--cache-dir"><code>--cache-dir</code></a> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>
<p>Defaults to <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on macOS and Linux, and <code>%LOCALAPPDATA%\uv\cache</code> on Windows.</p>
<p>To view the location of the cache directory, run <code>uv cache dir</code>.</p>
<p>May also be set with the <code>UV_CACHE_DIR</code> environment variable.</p></dd><dt id="uv-python-dir--color"><a href="#uv-python-dir--color"><code>--color</code></a> <i>color-choice</i></dt><dd><p>Control the use of color in output.</p>
<p>By default, uv will automatically detect support for colors when writing to a terminal.</p>
<p>Possible values:</p>
<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>
<li><code>always</code>:  Enables colored output regardless of the detected environment</li>
<li><code>never</code>:  Disables colored output</li>
</ul></dd><dt id="uv-python-dir--config-file"><a href="#uv-python-dir--config-file"><code>--config-file</code></a> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>
<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>
<p>May also be set with the <code>UV_CONFIG_FILE</code> environment variable.</p></dd><dt id="uv-python-dir--directory"><a href="#uv-python-dir--directory"><code>--directory</code></a> <i>directory</i></dt><dd><p>Change to the given directory prior to running the command.</p>
<p>Relative paths are resolved with the given directory as the base.</p>
<p>See <code>--project</code> to only change the project root directory.</p>
</dd><dt id="uv-python-dir--help"><a href="#uv-python-dir--help"><code>--help</code></a>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>
</dd><dt id="uv-python-dir--managed-python"><a href="#uv-python-dir--managed-python"><code>--managed-python</code></a></dt><dd><p>Require use of uv-managed Python versions.</p>
<p>By default, uv prefers using Python versions it manages. However, it will use system Python versions if a uv-managed Python is not installed. This option disables use of system Python versions.</p>
<p>May also be set with the <code>UV_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-python-dir--native-tls"><a href="#uv-python-dir--native-tls"><code>--native-tls</code></a></dt><dd><p>Whether to load TLS certificates from the platform's native certificate store.</p>
<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>
<p>However, in some cases, you may want to use the platform's native certificate store, especially if you're relying on a corporate trust root (e.g., for a mandatory proxy) that's included in your system's certificate store.</p>
<p>May also be set with the <code>UV_NATIVE_TLS</code> environment variable.</p></dd><dt id="uv-python-dir--no-cache"><a href="#uv-python-dir--no-cache"><code>--no-cache</code></a>, <code>--no-cache-dir</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>
<p>May also be set with the <code>UV_NO_CACHE</code> environment variable.</p></dd><dt id="uv-python-dir--no-config"><a href="#uv-python-dir--no-config"><code>--no-config</code></a></dt><dd><p>Avoid discovering configuration files (<code>pyproject.toml</code>, <code>uv.toml</code>).</p>
<p>Normally, configuration files are discovered in the current directory, parent directories, or user configuration directories.</p>
<p>May also be set with the <code>UV_NO_CONFIG</code> environment variable.</p></dd><dt id="uv-python-dir--no-managed-python"><a href="#uv-python-dir--no-managed-python"><code>--no-managed-python</code></a></dt><dd><p>Disable use of uv-managed Python versions.</p>
<p>Instead, uv will search for a suitable Python version on the system.</p>
<p>May also be set with the <code>UV_NO_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-python-dir--no-progress"><a href="#uv-python-dir--no-progress"><code>--no-progress</code></a></dt><dd><p>Hide all progress outputs.</p>
<p>For example, spinners or progress bars.</p>
<p>May also be set with the <code>UV_NO_PROGRESS</code> environment variable.</p></dd><dt id="uv-python-dir--no-python-downloads"><a href="#uv-python-dir--no-python-downloads"><code>--no-python-downloads</code></a></dt><dd><p>Disable automatic downloads of Python.</p>
</dd><dt id="uv-python-dir--offline"><a href="#uv-python-dir--offline"><code>--offline</code></a></dt><dd><p>Disable network access.</p>
<p>When disabled, uv will only use locally cached data and locally available files.</p>
<p>May also be set with the <code>UV_OFFLINE</code> environment variable.</p></dd><dt id="uv-python-dir--project"><a href="#uv-python-dir--project"><code>--project</code></a> <i>project</i></dt><dd><p>Run the command within the given project directory.</p>
<p>All <code>pyproject.toml</code>, <code>uv.toml</code>, and <code>.python-version</code> files will be discovered by walking up the directory tree from the project root, as will the project's virtual environment (<code>.venv</code>).</p>
<p>Other command-line arguments (such as relative paths) will be resolved relative to the current working directory.</p>
<p>See <code>--directory</code> to change the working directory entirely.</p>
<p>This setting has no effect when used in the <code>uv pip</code> interface.</p>
<p>May also be set with the <code>UV_PROJECT</code> environment variable.</p></dd><dt id="uv-python-dir--quiet"><a href="#uv-python-dir--quiet"><code>--quiet</code></a>, <code>-q</code></dt><dd><p>Use quiet output.</p>
<p>Repeating this option, e.g., <code>-qq</code>, will enable a silent mode in which uv will write no output to stdout.</p>
</dd><dt id="uv-python-dir--verbose"><a href="#uv-python-dir--verbose"><code>--verbose</code></a>, <code>-v</code></dt><dd><p>Use verbose output.</p>
<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (<a href="https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives">https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives</a>)</p>
</dd></dl>

### uv python uninstall

Uninstall Python versions

<h3 class="cli-reference">Usage</h3>

```
uv python uninstall [OPTIONS] <TARGETS>...
```

<h3 class="cli-reference">Arguments</h3>

<dl class="cli-reference"><dt id="uv-python-uninstall--targets"><a href="#uv-python-uninstall--targets"<code>TARGETS</code></a></dt><dd><p>The Python version(s) to uninstall.</p>
<p>See <a href="#uv-python">uv python</a> to view supported request formats.</p>
</dd></dl>

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt id="uv-python-uninstall--all"><a href="#uv-python-uninstall--all"><code>--all</code></a></dt><dd><p>Uninstall all managed Python versions</p>
</dd><dt id="uv-python-uninstall--allow-insecure-host"><a href="#uv-python-uninstall--allow-insecure-host"><code>--allow-insecure-host</code></a>, <code>--trusted-host</code> <i>allow-insecure-host</i></dt><dd><p>Allow insecure connections to a host.</p>
<p>Can be provided multiple times.</p>
<p>Expects to receive either a hostname (e.g., <code>localhost</code>), a host-port pair (e.g., <code>localhost:8080</code>), or a URL (e.g., <code>https://localhost</code>).</p>
<p>WARNING: Hosts included in this list will not be verified against the system's certificate store. Only use <code>--allow-insecure-host</code> in a secure network with verified sources, as it bypasses SSL verification and could expose you to MITM attacks.</p>
<p>May also be set with the <code>UV_INSECURE_HOST</code> environment variable.</p></dd><dt id="uv-python-uninstall--cache-dir"><a href="#uv-python-uninstall--cache-dir"><code>--cache-dir</code></a> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>
<p>Defaults to <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on macOS and Linux, and <code>%LOCALAPPDATA%\uv\cache</code> on Windows.</p>
<p>To view the location of the cache directory, run <code>uv cache dir</code>.</p>
<p>May also be set with the <code>UV_CACHE_DIR</code> environment variable.</p></dd><dt id="uv-python-uninstall--color"><a href="#uv-python-uninstall--color"><code>--color</code></a> <i>color-choice</i></dt><dd><p>Control the use of color in output.</p>
<p>By default, uv will automatically detect support for colors when writing to a terminal.</p>
<p>Possible values:</p>
<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>
<li><code>always</code>:  Enables colored output regardless of the detected environment</li>
<li><code>never</code>:  Disables colored output</li>
</ul></dd><dt id="uv-python-uninstall--config-file"><a href="#uv-python-uninstall--config-file"><code>--config-file</code></a> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>
<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>
<p>May also be set with the <code>UV_CONFIG_FILE</code> environment variable.</p></dd><dt id="uv-python-uninstall--directory"><a href="#uv-python-uninstall--directory"><code>--directory</code></a> <i>directory</i></dt><dd><p>Change to the given directory prior to running the command.</p>
<p>Relative paths are resolved with the given directory as the base.</p>
<p>See <code>--project</code> to only change the project root directory.</p>
</dd><dt id="uv-python-uninstall--help"><a href="#uv-python-uninstall--help"><code>--help</code></a>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>
</dd><dt id="uv-python-uninstall--install-dir"><a href="#uv-python-uninstall--install-dir"><code>--install-dir</code></a>, <code>-i</code> <i>install-dir</i></dt><dd><p>The directory where the Python was installed</p>
<p>May also be set with the <code>UV_PYTHON_INSTALL_DIR</code> environment variable.</p></dd><dt id="uv-python-uninstall--managed-python"><a href="#uv-python-uninstall--managed-python"><code>--managed-python</code></a></dt><dd><p>Require use of uv-managed Python versions.</p>
<p>By default, uv prefers using Python versions it manages. However, it will use system Python versions if a uv-managed Python is not installed. This option disables use of system Python versions.</p>
<p>May also be set with the <code>UV_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-python-uninstall--native-tls"><a href="#uv-python-uninstall--native-tls"><code>--native-tls</code></a></dt><dd><p>Whether to load TLS certificates from the platform's native certificate store.</p>
<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>
<p>However, in some cases, you may want to use the platform's native certificate store, especially if you're relying on a corporate trust root (e.g., for a mandatory proxy) that's included in your system's certificate store.</p>
<p>May also be set with the <code>UV_NATIVE_TLS</code> environment variable.</p></dd><dt id="uv-python-uninstall--no-cache"><a href="#uv-python-uninstall--no-cache"><code>--no-cache</code></a>, <code>--no-cache-dir</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>
<p>May also be set with the <code>UV_NO_CACHE</code> environment variable.</p></dd><dt id="uv-python-uninstall--no-config"><a href="#uv-python-uninstall--no-config"><code>--no-config</code></a></dt><dd><p>Avoid discovering configuration files (<code>pyproject.toml</code>, <code>uv.toml</code>).</p>
<p>Normally, configuration files are discovered in the current directory, parent directories, or user configuration directories.</p>
<p>May also be set with the <code>UV_NO_CONFIG</code> environment variable.</p></dd><dt id="uv-python-uninstall--no-managed-python"><a href="#uv-python-uninstall--no-managed-python"><code>--no-managed-python</code></a></dt><dd><p>Disable use of uv-managed Python versions.</p>
<p>Instead, uv will search for a suitable Python version on the system.</p>
<p>May also be set with the <code>UV_NO_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-python-uninstall--no-progress"><a href="#uv-python-uninstall--no-progress"><code>--no-progress</code></a></dt><dd><p>Hide all progress outputs.</p>
<p>For example, spinners or progress bars.</p>
<p>May also be set with the <code>UV_NO_PROGRESS</code> environment variable.</p></dd><dt id="uv-python-uninstall--no-python-downloads"><a href="#uv-python-uninstall--no-python-downloads"><code>--no-python-downloads</code></a></dt><dd><p>Disable automatic downloads of Python.</p>
</dd><dt id="uv-python-uninstall--offline"><a href="#uv-python-uninstall--offline"><code>--offline</code></a></dt><dd><p>Disable network access.</p>
<p>When disabled, uv will only use locally cached data and locally available files.</p>
<p>May also be set with the <code>UV_OFFLINE</code> environment variable.</p></dd><dt id="uv-python-uninstall--project"><a href="#uv-python-uninstall--project"><code>--project</code></a> <i>project</i></dt><dd><p>Run the command within the given project directory.</p>
<p>All <code>pyproject.toml</code>, <code>uv.toml</code>, and <code>.python-version</code> files will be discovered by walking up the directory tree from the project root, as will the project's virtual environment (<code>.venv</code>).</p>
<p>Other command-line arguments (such as relative paths) will be resolved relative to the current working directory.</p>
<p>See <code>--directory</code> to change the working directory entirely.</p>
<p>This setting has no effect when used in the <code>uv pip</code> interface.</p>
<p>May also be set with the <code>UV_PROJECT</code> environment variable.</p></dd><dt id="uv-python-uninstall--quiet"><a href="#uv-python-uninstall--quiet"><code>--quiet</code></a>, <code>-q</code></dt><dd><p>Use quiet output.</p>
<p>Repeating this option, e.g., <code>-qq</code>, will enable a silent mode in which uv will write no output to stdout.</p>
</dd><dt id="uv-python-uninstall--verbose"><a href="#uv-python-uninstall--verbose"><code>--verbose</code></a>, <code>-v</code></dt><dd><p>Use verbose output.</p>
<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (<a href="https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives">https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives</a>)</p>
</dd></dl>

## uv pip

Manage Python packages with a pip-compatible interface

<h3 class="cli-reference">Usage</h3>

```
uv pip [OPTIONS] <COMMAND>
```

<h3 class="cli-reference">Commands</h3>

<dl class="cli-reference"><dt><a href="#uv-pip-compile"><code>uv pip compile</code></a></dt><dd><p>Compile a <code>requirements.in</code> file to a <code>requirements.txt</code> or <code>pylock.toml</code> file</p></dd>
<dt><a href="#uv-pip-sync"><code>uv pip sync</code></a></dt><dd><p>Sync an environment with a <code>requirements.txt</code> or <code>pylock.toml</code> file</p></dd>
<dt><a href="#uv-pip-install"><code>uv pip install</code></a></dt><dd><p>Install packages into an environment</p></dd>
<dt><a href="#uv-pip-uninstall"><code>uv pip uninstall</code></a></dt><dd><p>Uninstall packages from an environment</p></dd>
<dt><a href="#uv-pip-freeze"><code>uv pip freeze</code></a></dt><dd><p>List, in requirements format, packages installed in an environment</p></dd>
<dt><a href="#uv-pip-list"><code>uv pip list</code></a></dt><dd><p>List, in tabular format, packages installed in an environment</p></dd>
<dt><a href="#uv-pip-show"><code>uv pip show</code></a></dt><dd><p>Show information about one or more installed packages</p></dd>
<dt><a href="#uv-pip-tree"><code>uv pip tree</code></a></dt><dd><p>Display the dependency tree for an environment</p></dd>
<dt><a href="#uv-pip-check"><code>uv pip check</code></a></dt><dd><p>Verify installed packages have compatible dependencies</p></dd>
</dl>

### uv pip compile

Compile a `requirements.in` file to a `requirements.txt` or `pylock.toml` file

<h3 class="cli-reference">Usage</h3>

```
uv pip compile [OPTIONS] <SRC_FILE|--group <GROUP>>
```

<h3 class="cli-reference">Arguments</h3>

<dl class="cli-reference"><dt id="uv-pip-compile--src_file"><a href="#uv-pip-compile--src_file"<code>SRC_FILE</code></a></dt><dd><p>Include all packages listed in the given <code>requirements.in</code> files.</p>
<p>If a <code>pyproject.toml</code>, <code>setup.py</code>, or <code>setup.cfg</code> file is provided, uv will extract the requirements for the relevant project.</p>
<p>If <code>-</code> is provided, then requirements will be read from stdin.</p>
<p>The order of the requirements files and the requirements in them is used to determine priority during resolution.</p>
</dd></dl>

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt id="uv-pip-compile--all-extras"><a href="#uv-pip-compile--all-extras"><code>--all-extras</code></a></dt><dd><p>Include all optional dependencies.</p>
<p>Only applies to <code>pyproject.toml</code>, <code>setup.py</code>, and <code>setup.cfg</code> sources.</p>
</dd><dt id="uv-pip-compile--allow-insecure-host"><a href="#uv-pip-compile--allow-insecure-host"><code>--allow-insecure-host</code></a>, <code>--trusted-host</code> <i>allow-insecure-host</i></dt><dd><p>Allow insecure connections to a host.</p>
<p>Can be provided multiple times.</p>
<p>Expects to receive either a hostname (e.g., <code>localhost</code>), a host-port pair (e.g., <code>localhost:8080</code>), or a URL (e.g., <code>https://localhost</code>).</p>
<p>WARNING: Hosts included in this list will not be verified against the system's certificate store. Only use <code>--allow-insecure-host</code> in a secure network with verified sources, as it bypasses SSL verification and could expose you to MITM attacks.</p>
<p>May also be set with the <code>UV_INSECURE_HOST</code> environment variable.</p></dd><dt id="uv-pip-compile--annotation-style"><a href="#uv-pip-compile--annotation-style"><code>--annotation-style</code></a> <i>annotation-style</i></dt><dd><p>The style of the annotation comments included in the output file, used to indicate the source of each package.</p>
<p>Defaults to <code>split</code>.</p>
<p>Possible values:</p>
<ul>
<li><code>line</code>:  Render the annotations on a single, comma-separated line</li>
<li><code>split</code>:  Render each annotation on its own line</li>
</ul></dd><dt id="uv-pip-compile--build-constraints"><a href="#uv-pip-compile--build-constraints"><code>--build-constraints</code></a>, <code>--build-constraint</code>, <code>-b</code> <i>build-constraints</i></dt><dd><p>Constrain build dependencies using the given requirements files when building source distributions.</p>
<p>Constraints files are <code>requirements.txt</code>-like files that only control the <em>version</em> of a requirement that's installed. However, including a package in a constraints file will <em>not</em> trigger the installation of that package.</p>
<p>May also be set with the <code>UV_BUILD_CONSTRAINT</code> environment variable.</p></dd><dt id="uv-pip-compile--cache-dir"><a href="#uv-pip-compile--cache-dir"><code>--cache-dir</code></a> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>
<p>Defaults to <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on macOS and Linux, and <code>%LOCALAPPDATA%\uv\cache</code> on Windows.</p>
<p>To view the location of the cache directory, run <code>uv cache dir</code>.</p>
<p>May also be set with the <code>UV_CACHE_DIR</code> environment variable.</p></dd><dt id="uv-pip-compile--color"><a href="#uv-pip-compile--color"><code>--color</code></a> <i>color-choice</i></dt><dd><p>Control the use of color in output.</p>
<p>By default, uv will automatically detect support for colors when writing to a terminal.</p>
<p>Possible values:</p>
<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>
<li><code>always</code>:  Enables colored output regardless of the detected environment</li>
<li><code>never</code>:  Disables colored output</li>
</ul></dd><dt id="uv-pip-compile--config-file"><a href="#uv-pip-compile--config-file"><code>--config-file</code></a> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>
<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>
<p>May also be set with the <code>UV_CONFIG_FILE</code> environment variable.</p></dd><dt id="uv-pip-compile--config-setting"><a href="#uv-pip-compile--config-setting"><code>--config-setting</code></a>, <code>--config-settings</code>, <code>-C</code> <i>config-setting</i></dt><dd><p>Settings to pass to the PEP 517 build backend, specified as <code>KEY=VALUE</code> pairs</p>
</dd><dt id="uv-pip-compile--constraints"><a href="#uv-pip-compile--constraints"><code>--constraints</code></a>, <code>--constraint</code>, <code>-c</code> <i>constraints</i></dt><dd><p>Constrain versions using the given requirements files.</p>
<p>Constraints files are <code>requirements.txt</code>-like files that only control the <em>version</em> of a requirement that's installed. However, including a package in a constraints file will <em>not</em> trigger the installation of that package.</p>
<p>This is equivalent to pip's <code>--constraint</code> option.</p>
<p>May also be set with the <code>UV_CONSTRAINT</code> environment variable.</p></dd><dt id="uv-pip-compile--custom-compile-command"><a href="#uv-pip-compile--custom-compile-command"><code>--custom-compile-command</code></a> <i>custom-compile-command</i></dt><dd><p>The header comment to include at the top of the output file generated by <code>uv pip compile</code>.</p>
<p>Used to reflect custom build scripts and commands that wrap <code>uv pip compile</code>.</p>
<p>May also be set with the <code>UV_CUSTOM_COMPILE_COMMAND</code> environment variable.</p></dd><dt id="uv-pip-compile--default-index"><a href="#uv-pip-compile--default-index"><code>--default-index</code></a> <i>default-index</i></dt><dd><p>The URL of the default package index (by default: <a href="https://pypi.org/simple">https://pypi.org/simple</a>).</p>
<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>
<p>The index given by this flag is given lower priority than all other indexes specified via the <code>--index</code> flag.</p>
<p>May also be set with the <code>UV_DEFAULT_INDEX</code> environment variable.</p></dd><dt id="uv-pip-compile--directory"><a href="#uv-pip-compile--directory"><code>--directory</code></a> <i>directory</i></dt><dd><p>Change to the given directory prior to running the command.</p>
<p>Relative paths are resolved with the given directory as the base.</p>
<p>See <code>--project</code> to only change the project root directory.</p>
</dd><dt id="uv-pip-compile--emit-build-options"><a href="#uv-pip-compile--emit-build-options"><code>--emit-build-options</code></a></dt><dd><p>Include <code>--no-binary</code> and <code>--only-binary</code> entries in the generated output file</p>
</dd><dt id="uv-pip-compile--emit-find-links"><a href="#uv-pip-compile--emit-find-links"><code>--emit-find-links</code></a></dt><dd><p>Include <code>--find-links</code> entries in the generated output file</p>
</dd><dt id="uv-pip-compile--emit-index-annotation"><a href="#uv-pip-compile--emit-index-annotation"><code>--emit-index-annotation</code></a></dt><dd><p>Include comment annotations indicating the index used to resolve each package (e.g., <code># from https://pypi.org/simple</code>)</p>
</dd><dt id="uv-pip-compile--emit-index-url"><a href="#uv-pip-compile--emit-index-url"><code>--emit-index-url</code></a></dt><dd><p>Include <code>--index-url</code> and <code>--extra-index-url</code> entries in the generated output file</p>
</dd><dt id="uv-pip-compile--exclude-newer"><a href="#uv-pip-compile--exclude-newer"><code>--exclude-newer</code></a> <i>exclude-newer</i></dt><dd><p>Limit candidate packages to those that were uploaded prior to the given date.</p>
<p>Accepts both RFC 3339 timestamps (e.g., <code>2006-12-02T02:07:43Z</code>) and local dates in the same format (e.g., <code>2006-12-02</code>) in your system's configured time zone.</p>
<p>May also be set with the <code>UV_EXCLUDE_NEWER</code> environment variable.</p></dd><dt id="uv-pip-compile--extra"><a href="#uv-pip-compile--extra"><code>--extra</code></a> <i>extra</i></dt><dd><p>Include optional dependencies from the specified extra name; may be provided more than once.</p>
<p>Only applies to <code>pyproject.toml</code>, <code>setup.py</code>, and <code>setup.cfg</code> sources.</p>
</dd><dt id="uv-pip-compile--extra-index-url"><a href="#uv-pip-compile--extra-index-url"><code>--extra-index-url</code></a> <i>extra-index-url</i></dt><dd><p>(Deprecated: use <code>--index</code> instead) Extra URLs of package indexes to use, in addition to <code>--index-url</code>.</p>
<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>
<p>All indexes provided via this flag take priority over the index specified by <code>--index-url</code> (which defaults to PyPI). When multiple <code>--extra-index-url</code> flags are provided, earlier values take priority.</p>
<p>May also be set with the <code>UV_EXTRA_INDEX_URL</code> environment variable.</p></dd><dt id="uv-pip-compile--find-links"><a href="#uv-pip-compile--find-links"><code>--find-links</code></a>, <code>-f</code> <i>find-links</i></dt><dd><p>Locations to search for candidate distributions, in addition to those found in the registry indexes.</p>
<p>If a path, the target must be a directory that contains packages as wheel files (<code>.whl</code>) or source distributions (e.g., <code>.tar.gz</code> or <code>.zip</code>) at the top level.</p>
<p>If a URL, the page must contain a flat list of links to package files adhering to the formats described above.</p>
<p>May also be set with the <code>UV_FIND_LINKS</code> environment variable.</p></dd><dt id="uv-pip-compile--fork-strategy"><a href="#uv-pip-compile--fork-strategy"><code>--fork-strategy</code></a> <i>fork-strategy</i></dt><dd><p>The strategy to use when selecting multiple versions of a given package across Python versions and platforms.</p>
<p>By default, uv will optimize for selecting the latest version of each package for each supported Python version (<code>requires-python</code>), while minimizing the number of selected versions across platforms.</p>
<p>Under <code>fewest</code>, uv will minimize the number of selected versions for each package, preferring older versions that are compatible with a wider range of supported Python versions or platforms.</p>
<p>May also be set with the <code>UV_FORK_STRATEGY</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>fewest</code>:  Optimize for selecting the fewest number of versions for each package. Older versions may be preferred if they are compatible with a wider range of supported Python versions or platforms</li>
<li><code>requires-python</code>:  Optimize for selecting latest supported version of each package, for each supported Python version</li>
</ul></dd><dt id="uv-pip-compile--format"><a href="#uv-pip-compile--format"><code>--format</code></a> <i>format</i></dt><dd><p>The format in which the resolution should be output.</p>
<p>Supports both <code>requirements.txt</code> and <code>pylock.toml</code> (PEP 751) output formats.</p>
<p>uv will infer the output format from the file extension of the output file, if provided. Otherwise, defaults to <code>requirements.txt</code>.</p>
<p>Possible values:</p>
<ul>
<li><code>requirements.txt</code>:  Export in <code>requirements.txt</code> format</li>
<li><code>pylock.toml</code>:  Export in <code>pylock.toml</code> format</li>
</ul></dd><dt id="uv-pip-compile--generate-hashes"><a href="#uv-pip-compile--generate-hashes"><code>--generate-hashes</code></a></dt><dd><p>Include distribution hashes in the output file</p>
</dd><dt id="uv-pip-compile--group"><a href="#uv-pip-compile--group"><code>--group</code></a> <i>group</i></dt><dd><p>Install the specified dependency group from a <code>pyproject.toml</code>.</p>
<p>If no path is provided, the <code>pyproject.toml</code> in the working directory is used.</p>
<p>May be provided multiple times.</p>
</dd><dt id="uv-pip-compile--help"><a href="#uv-pip-compile--help"><code>--help</code></a>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>
</dd><dt id="uv-pip-compile--index"><a href="#uv-pip-compile--index"><code>--index</code></a> <i>index</i></dt><dd><p>The URLs to use when resolving dependencies, in addition to the default index.</p>
<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>
<p>All indexes provided via this flag take priority over the index specified by <code>--default-index</code> (which defaults to PyPI). When multiple <code>--index</code> flags are provided, earlier values take priority.</p>
<p>May also be set with the <code>UV_INDEX</code> environment variable.</p></dd><dt id="uv-pip-compile--index-strategy"><a href="#uv-pip-compile--index-strategy"><code>--index-strategy</code></a> <i>index-strategy</i></dt><dd><p>The strategy to use when resolving against multiple index URLs.</p>
<p>By default, uv will stop at the first index on which a given package is available, and limit resolutions to those present on that first index (<code>first-index</code>). This prevents &quot;dependency confusion&quot; attacks, whereby an attacker can upload a malicious package under the same name to an alternate index.</p>
<p>May also be set with the <code>UV_INDEX_STRATEGY</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>first-index</code>:  Only use results from the first index that returns a match for a given package name</li>
<li><code>unsafe-first-match</code>:  Search for every package name across all indexes, exhausting the versions from the first index before moving on to the next</li>
<li><code>unsafe-best-match</code>:  Search for every package name across all indexes, preferring the &quot;best&quot; version found. If a package version is in multiple indexes, only look at the entry for the first index</li>
</ul></dd><dt id="uv-pip-compile--index-url"><a href="#uv-pip-compile--index-url"><code>--index-url</code></a>, <code>-i</code> <i>index-url</i></dt><dd><p>(Deprecated: use <code>--default-index</code> instead) The URL of the Python package index (by default: <a href="https://pypi.org/simple">https://pypi.org/simple</a>).</p>
<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>
<p>The index given by this flag is given lower priority than all other indexes specified via the <code>--extra-index-url</code> flag.</p>
<p>May also be set with the <code>UV_INDEX_URL</code> environment variable.</p></dd><dt id="uv-pip-compile--keyring-provider"><a href="#uv-pip-compile--keyring-provider"><code>--keyring-provider</code></a> <i>keyring-provider</i></dt><dd><p>Attempt to use <code>keyring</code> for authentication for index URLs.</p>
<p>At present, only <code>--keyring-provider subprocess</code> is supported, which configures uv to use the <code>keyring</code> CLI to handle authentication.</p>
<p>Defaults to <code>disabled</code>.</p>
<p>May also be set with the <code>UV_KEYRING_PROVIDER</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>disabled</code>:  Do not use keyring for credential lookup</li>
<li><code>subprocess</code>:  Use the <code>keyring</code> command for credential lookup</li>
</ul></dd><dt id="uv-pip-compile--link-mode"><a href="#uv-pip-compile--link-mode"><code>--link-mode</code></a> <i>link-mode</i></dt><dd><p>The method to use when installing packages from the global cache.</p>
<p>This option is only used when building source distributions.</p>
<p>Defaults to <code>clone</code> (also known as Copy-on-Write) on macOS, and <code>hardlink</code> on Linux and Windows.</p>
<p>May also be set with the <code>UV_LINK_MODE</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>clone</code>:  Clone (i.e., copy-on-write) packages from the wheel into the <code>site-packages</code> directory</li>
<li><code>copy</code>:  Copy packages from the wheel into the <code>site-packages</code> directory</li>
<li><code>hardlink</code>:  Hard link packages from the wheel into the <code>site-packages</code> directory</li>
<li><code>symlink</code>:  Symbolically link packages from the wheel into the <code>site-packages</code> directory</li>
</ul></dd><dt id="uv-pip-compile--managed-python"><a href="#uv-pip-compile--managed-python"><code>--managed-python</code></a></dt><dd><p>Require use of uv-managed Python versions.</p>
<p>By default, uv prefers using Python versions it manages. However, it will use system Python versions if a uv-managed Python is not installed. This option disables use of system Python versions.</p>
<p>May also be set with the <code>UV_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-pip-compile--native-tls"><a href="#uv-pip-compile--native-tls"><code>--native-tls</code></a></dt><dd><p>Whether to load TLS certificates from the platform's native certificate store.</p>
<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>
<p>However, in some cases, you may want to use the platform's native certificate store, especially if you're relying on a corporate trust root (e.g., for a mandatory proxy) that's included in your system's certificate store.</p>
<p>May also be set with the <code>UV_NATIVE_TLS</code> environment variable.</p></dd><dt id="uv-pip-compile--no-annotate"><a href="#uv-pip-compile--no-annotate"><code>--no-annotate</code></a></dt><dd><p>Exclude comment annotations indicating the source of each package</p>
</dd><dt id="uv-pip-compile--no-binary"><a href="#uv-pip-compile--no-binary"><code>--no-binary</code></a> <i>no-binary</i></dt><dd><p>Don't install pre-built wheels.</p>
<p>The given packages will be built and installed from source. The resolver will still use pre-built wheels to extract package metadata, if available.</p>
<p>Multiple packages may be provided. Disable binaries for all packages with <code>:all:</code>. Clear previously specified packages with <code>:none:</code>.</p>
</dd><dt id="uv-pip-compile--no-build"><a href="#uv-pip-compile--no-build"><code>--no-build</code></a></dt><dd><p>Don't build source distributions.</p>
<p>When enabled, resolving will not run arbitrary Python code. The cached wheels of already-built source distributions will be reused, but operations that require building distributions will exit with an error.</p>
<p>Alias for <code>--only-binary :all:</code>.</p>
</dd><dt id="uv-pip-compile--no-build-isolation"><a href="#uv-pip-compile--no-build-isolation"><code>--no-build-isolation</code></a></dt><dd><p>Disable isolation when building source distributions.</p>
<p>Assumes that build dependencies specified by PEP 518 are already installed.</p>
<p>May also be set with the <code>UV_NO_BUILD_ISOLATION</code> environment variable.</p></dd><dt id="uv-pip-compile--no-build-isolation-package"><a href="#uv-pip-compile--no-build-isolation-package"><code>--no-build-isolation-package</code></a> <i>no-build-isolation-package</i></dt><dd><p>Disable isolation when building source distributions for a specific package.</p>
<p>Assumes that the packages' build dependencies specified by PEP 518 are already installed.</p>
</dd><dt id="uv-pip-compile--no-cache"><a href="#uv-pip-compile--no-cache"><code>--no-cache</code></a>, <code>--no-cache-dir</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>
<p>May also be set with the <code>UV_NO_CACHE</code> environment variable.</p></dd><dt id="uv-pip-compile--no-deps"><a href="#uv-pip-compile--no-deps"><code>--no-deps</code></a></dt><dd><p>Ignore package dependencies, instead only add those packages explicitly listed on the command line to the resulting requirements file</p>
</dd><dt id="uv-pip-compile--no-emit-package"><a href="#uv-pip-compile--no-emit-package"><code>--no-emit-package</code></a>, <code>--unsafe-package</code> <i>no-emit-package</i></dt><dd><p>Specify a package to omit from the output resolution. Its dependencies will still be included in the resolution. Equivalent to pip-compile's <code>--unsafe-package</code> option</p>
</dd><dt id="uv-pip-compile--no-header"><a href="#uv-pip-compile--no-header"><code>--no-header</code></a></dt><dd><p>Exclude the comment header at the top of the generated output file</p>
</dd><dt id="uv-pip-compile--no-index"><a href="#uv-pip-compile--no-index"><code>--no-index</code></a></dt><dd><p>Ignore the registry index (e.g., PyPI), instead relying on direct URL dependencies and those provided via <code>--find-links</code></p>
</dd><dt id="uv-pip-compile--no-managed-python"><a href="#uv-pip-compile--no-managed-python"><code>--no-managed-python</code></a></dt><dd><p>Disable use of uv-managed Python versions.</p>
<p>Instead, uv will search for a suitable Python version on the system.</p>
<p>May also be set with the <code>UV_NO_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-pip-compile--no-progress"><a href="#uv-pip-compile--no-progress"><code>--no-progress</code></a></dt><dd><p>Hide all progress outputs.</p>
<p>For example, spinners or progress bars.</p>
<p>May also be set with the <code>UV_NO_PROGRESS</code> environment variable.</p></dd><dt id="uv-pip-compile--no-python-downloads"><a href="#uv-pip-compile--no-python-downloads"><code>--no-python-downloads</code></a></dt><dd><p>Disable automatic downloads of Python.</p>
</dd><dt id="uv-pip-compile--no-sources"><a href="#uv-pip-compile--no-sources"><code>--no-sources</code></a></dt><dd><p>Ignore the <code>tool.uv.sources</code> table when resolving dependencies. Used to lock against the standards-compliant, publishable package metadata, as opposed to using any workspace, Git, URL, or local path sources</p>
</dd><dt id="uv-pip-compile--no-strip-extras"><a href="#uv-pip-compile--no-strip-extras"><code>--no-strip-extras</code></a></dt><dd><p>Include extras in the output file.</p>
<p>By default, uv strips extras, as any packages pulled in by the extras are already included as dependencies in the output file directly. Further, output files generated with <code>--no-strip-extras</code> cannot be used as constraints files in <code>install</code> and <code>sync</code> invocations.</p>
</dd><dt id="uv-pip-compile--no-strip-markers"><a href="#uv-pip-compile--no-strip-markers"><code>--no-strip-markers</code></a></dt><dd><p>Include environment markers in the output file.</p>
<p>By default, uv strips environment markers, as the resolution generated by <code>compile</code> is only guaranteed to be correct for the target environment.</p>
</dd><dt id="uv-pip-compile--offline"><a href="#uv-pip-compile--offline"><code>--offline</code></a></dt><dd><p>Disable network access.</p>
<p>When disabled, uv will only use locally cached data and locally available files.</p>
<p>May also be set with the <code>UV_OFFLINE</code> environment variable.</p></dd><dt id="uv-pip-compile--only-binary"><a href="#uv-pip-compile--only-binary"><code>--only-binary</code></a> <i>only-binary</i></dt><dd><p>Only use pre-built wheels; don't build source distributions.</p>
<p>When enabled, resolving will not run code from the given packages. The cached wheels of already-built source distributions will be reused, but operations that require building distributions will exit with an error.</p>
<p>Multiple packages may be provided. Disable binaries for all packages with <code>:all:</code>. Clear previously specified packages with <code>:none:</code>.</p>
</dd><dt id="uv-pip-compile--output-file"><a href="#uv-pip-compile--output-file"><code>--output-file</code></a>, <code>-o</code> <i>output-file</i></dt><dd><p>Write the compiled requirements to the given <code>requirements.txt</code> or <code>pylock.toml</code> file.</p>
<p>If the file already exists, the existing versions will be preferred when resolving dependencies, unless <code>--upgrade</code> is also specified.</p>
</dd><dt id="uv-pip-compile--overrides"><a href="#uv-pip-compile--overrides"><code>--overrides</code></a>, <code>--override</code> <i>overrides</i></dt><dd><p>Override versions using the given requirements files.</p>
<p>Overrides files are <code>requirements.txt</code>-like files that force a specific version of a requirement to be installed, regardless of the requirements declared by any constituent package, and regardless of whether this would be considered an invalid resolution.</p>
<p>While constraints are <em>additive</em>, in that they're combined with the requirements of the constituent packages, overrides are <em>absolute</em>, in that they completely replace the requirements of the constituent packages.</p>
<p>May also be set with the <code>UV_OVERRIDE</code> environment variable.</p></dd><dt id="uv-pip-compile--prerelease"><a href="#uv-pip-compile--prerelease"><code>--prerelease</code></a> <i>prerelease</i></dt><dd><p>The strategy to use when considering pre-release versions.</p>
<p>By default, uv will accept pre-releases for packages that <em>only</em> publish pre-releases, along with first-party requirements that contain an explicit pre-release marker in the declared specifiers (<code>if-necessary-or-explicit</code>).</p>
<p>May also be set with the <code>UV_PRERELEASE</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>disallow</code>:  Disallow all pre-release versions</li>
<li><code>allow</code>:  Allow all pre-release versions</li>
<li><code>if-necessary</code>:  Allow pre-release versions if all versions of a package are pre-release</li>
<li><code>explicit</code>:  Allow pre-release versions for first-party packages with explicit pre-release markers in their version requirements</li>
<li><code>if-necessary-or-explicit</code>:  Allow pre-release versions if all versions of a package are pre-release, or if the package has an explicit pre-release marker in its version requirements</li>
</ul></dd><dt id="uv-pip-compile--project"><a href="#uv-pip-compile--project"><code>--project</code></a> <i>project</i></dt><dd><p>Run the command within the given project directory.</p>
<p>All <code>pyproject.toml</code>, <code>uv.toml</code>, and <code>.python-version</code> files will be discovered by walking up the directory tree from the project root, as will the project's virtual environment (<code>.venv</code>).</p>
<p>Other command-line arguments (such as relative paths) will be resolved relative to the current working directory.</p>
<p>See <code>--directory</code> to change the working directory entirely.</p>
<p>This setting has no effect when used in the <code>uv pip</code> interface.</p>
<p>May also be set with the <code>UV_PROJECT</code> environment variable.</p></dd><dt id="uv-pip-compile--python"><a href="#uv-pip-compile--python"><code>--python</code></a>, <code>-p</code> <i>python</i></dt><dd><p>The Python interpreter to use during resolution.</p>
<p>A Python interpreter is required for building source distributions to determine package
metadata when there are not wheels.</p>
<p>The interpreter is also used to determine the default minimum Python version, unless
<code>--python-version</code> is provided.</p>
<p>This option respects <code>UV_PYTHON</code>, but when set via environment variable, it is overridden
by <code>--python-version</code>.</p>
<p>See <a href="#uv-python">uv python</a> for details on Python discovery and supported request formats.</p>
</dd><dt id="uv-pip-compile--python-platform"><a href="#uv-pip-compile--python-platform"><code>--python-platform</code></a> <i>python-platform</i></dt><dd><p>The platform for which requirements should be resolved.</p>
<p>Represented as a &quot;target triple&quot;, a string that describes the target platform in terms of its CPU, vendor, and operating system name, like <code>x86_64-unknown-linux-gnu</code> or <code>aarch64-apple-darwin</code>.</p>
<p>When targeting macOS (Darwin), the default minimum version is <code>12.0</code>. Use <code>MACOSX_DEPLOYMENT_TARGET</code> to specify a different minimum version, e.g., <code>13.0</code>.</p>
<p>Possible values:</p>
<ul>
<li><code>windows</code>:  An alias for <code>x86_64-pc-windows-msvc</code>, the default target for Windows</li>
<li><code>linux</code>:  An alias for <code>x86_64-unknown-linux-gnu</code>, the default target for Linux</li>
<li><code>macos</code>:  An alias for <code>aarch64-apple-darwin</code>, the default target for macOS</li>
<li><code>x86_64-pc-windows-msvc</code>:  A 64-bit x86 Windows target</li>
<li><code>i686-pc-windows-msvc</code>:  A 32-bit x86 Windows target</li>
<li><code>x86_64-unknown-linux-gnu</code>:  An x86 Linux target. Equivalent to <code>x86_64-manylinux_2_17</code></li>
<li><code>aarch64-apple-darwin</code>:  An ARM-based macOS target, as seen on Apple Silicon devices</li>
<li><code>x86_64-apple-darwin</code>:  An x86 macOS target</li>
<li><code>aarch64-unknown-linux-gnu</code>:  An ARM64 Linux target. Equivalent to <code>aarch64-manylinux_2_17</code></li>
<li><code>aarch64-unknown-linux-musl</code>:  An ARM64 Linux target</li>
<li><code>x86_64-unknown-linux-musl</code>:  An <code>x86_64</code> Linux target</li>
<li><code>x86_64-manylinux2014</code>:  An <code>x86_64</code> target for the <code>manylinux2014</code> platform. Equivalent to <code>x86_64-manylinux_2_17</code></li>
<li><code>x86_64-manylinux_2_17</code>:  An <code>x86_64</code> target for the <code>manylinux_2_17</code> platform</li>
<li><code>x86_64-manylinux_2_28</code>:  An <code>x86_64</code> target for the <code>manylinux_2_28</code> platform</li>
<li><code>x86_64-manylinux_2_31</code>:  An <code>x86_64</code> target for the <code>manylinux_2_31</code> platform</li>
<li><code>x86_64-manylinux_2_32</code>:  An <code>x86_64</code> target for the <code>manylinux_2_32</code> platform</li>
<li><code>x86_64-manylinux_2_33</code>:  An <code>x86_64</code> target for the <code>manylinux_2_33</code> platform</li>
<li><code>x86_64-manylinux_2_34</code>:  An <code>x86_64</code> target for the <code>manylinux_2_34</code> platform</li>
<li><code>x86_64-manylinux_2_35</code>:  An <code>x86_64</code> target for the <code>manylinux_2_35</code> platform</li>
<li><code>x86_64-manylinux_2_36</code>:  An <code>x86_64</code> target for the <code>manylinux_2_36</code> platform</li>
<li><code>x86_64-manylinux_2_37</code>:  An <code>x86_64</code> target for the <code>manylinux_2_37</code> platform</li>
<li><code>x86_64-manylinux_2_38</code>:  An <code>x86_64</code> target for the <code>manylinux_2_38</code> platform</li>
<li><code>x86_64-manylinux_2_39</code>:  An <code>x86_64</code> target for the <code>manylinux_2_39</code> platform</li>
<li><code>x86_64-manylinux_2_40</code>:  An <code>x86_64</code> target for the <code>manylinux_2_40</code> platform</li>
<li><code>aarch64-manylinux2014</code>:  An ARM64 target for the <code>manylinux2014</code> platform. Equivalent to <code>aarch64-manylinux_2_17</code></li>
<li><code>aarch64-manylinux_2_17</code>:  An ARM64 target for the <code>manylinux_2_17</code> platform</li>
<li><code>aarch64-manylinux_2_28</code>:  An ARM64 target for the <code>manylinux_2_28</code> platform</li>
<li><code>aarch64-manylinux_2_31</code>:  An ARM64 target for the <code>manylinux_2_31</code> platform</li>
<li><code>aarch64-manylinux_2_32</code>:  An ARM64 target for the <code>manylinux_2_32</code> platform</li>
<li><code>aarch64-manylinux_2_33</code>:  An ARM64 target for the <code>manylinux_2_33</code> platform</li>
<li><code>aarch64-manylinux_2_34</code>:  An ARM64 target for the <code>manylinux_2_34</code> platform</li>
<li><code>aarch64-manylinux_2_35</code>:  An ARM64 target for the <code>manylinux_2_35</code> platform</li>
<li><code>aarch64-manylinux_2_36</code>:  An ARM64 target for the <code>manylinux_2_36</code> platform</li>
<li><code>aarch64-manylinux_2_37</code>:  An ARM64 target for the <code>manylinux_2_37</code> platform</li>
<li><code>aarch64-manylinux_2_38</code>:  An ARM64 target for the <code>manylinux_2_38</code> platform</li>
<li><code>aarch64-manylinux_2_39</code>:  An ARM64 target for the <code>manylinux_2_39</code> platform</li>
<li><code>aarch64-manylinux_2_40</code>:  An ARM64 target for the <code>manylinux_2_40</code> platform</li>
</ul></dd><dt id="uv-pip-compile--python-version"><a href="#uv-pip-compile--python-version"><code>--python-version</code></a> <i>python-version</i></dt><dd><p>The Python version to use for resolution.</p>
<p>For example, <code>3.8</code> or <code>3.8.17</code>.</p>
<p>Defaults to the version of the Python interpreter used for resolution.</p>
<p>Defines the minimum Python version that must be supported by the resolved requirements.</p>
<p>If a patch version is omitted, the minimum patch version is assumed. For example, <code>3.8</code> is mapped to <code>3.8.0</code>.</p>
</dd><dt id="uv-pip-compile--quiet"><a href="#uv-pip-compile--quiet"><code>--quiet</code></a>, <code>-q</code></dt><dd><p>Use quiet output.</p>
<p>Repeating this option, e.g., <code>-qq</code>, will enable a silent mode in which uv will write no output to stdout.</p>
</dd><dt id="uv-pip-compile--refresh"><a href="#uv-pip-compile--refresh"><code>--refresh</code></a></dt><dd><p>Refresh all cached data</p>
</dd><dt id="uv-pip-compile--refresh-package"><a href="#uv-pip-compile--refresh-package"><code>--refresh-package</code></a> <i>refresh-package</i></dt><dd><p>Refresh cached data for a specific package</p>
</dd><dt id="uv-pip-compile--resolution"><a href="#uv-pip-compile--resolution"><code>--resolution</code></a> <i>resolution</i></dt><dd><p>The strategy to use when selecting between the different compatible versions for a given package requirement.</p>
<p>By default, uv will use the latest compatible version of each package (<code>highest</code>).</p>
<p>May also be set with the <code>UV_RESOLUTION</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>highest</code>:  Resolve the highest compatible version of each package</li>
<li><code>lowest</code>:  Resolve the lowest compatible version of each package</li>
<li><code>lowest-direct</code>:  Resolve the lowest compatible version of any direct dependencies, and the highest compatible version of any transitive dependencies</li>
</ul></dd><dt id="uv-pip-compile--system"><a href="#uv-pip-compile--system"><code>--system</code></a></dt><dd><p>Install packages into the system Python environment.</p>
<p>By default, uv uses the virtual environment in the current working directory or any parent directory, falling back to searching for a Python executable in <code>PATH</code>. The <code>--system</code> option instructs uv to avoid using a virtual environment Python and restrict its search to the system path.</p>
<p>May also be set with the <code>UV_SYSTEM_PYTHON</code> environment variable.</p></dd><dt id="uv-pip-compile--torch-backend"><a href="#uv-pip-compile--torch-backend"><code>--torch-backend</code></a> <i>torch-backend</i></dt><dd><p>The backend to use when fetching packages in the PyTorch ecosystem (e.g., <code>cpu</code>, <code>cu126</code>, or <code>auto</code>).</p>
<p>When set, uv will ignore the configured index URLs for packages in the PyTorch ecosystem, and will instead use the defined backend.</p>
<p>For example, when set to <code>cpu</code>, uv will use the CPU-only PyTorch index; when set to <code>cu126</code>, uv will use the PyTorch index for CUDA 12.6.</p>
<p>The <code>auto</code> mode will attempt to detect the appropriate PyTorch index based on the currently installed CUDA drivers.</p>
<p>This option is in preview and may change in any future release.</p>
<p>May also be set with the <code>UV_TORCH_BACKEND</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>auto</code>:  Select the appropriate PyTorch index based on the operating system and CUDA driver version</li>
<li><code>cpu</code>:  Use the CPU-only PyTorch index</li>
<li><code>cu128</code>:  Use the PyTorch index for CUDA 12.8</li>
<li><code>cu126</code>:  Use the PyTorch index for CUDA 12.6</li>
<li><code>cu125</code>:  Use the PyTorch index for CUDA 12.5</li>
<li><code>cu124</code>:  Use the PyTorch index for CUDA 12.4</li>
<li><code>cu123</code>:  Use the PyTorch index for CUDA 12.3</li>
<li><code>cu122</code>:  Use the PyTorch index for CUDA 12.2</li>
<li><code>cu121</code>:  Use the PyTorch index for CUDA 12.1</li>
<li><code>cu120</code>:  Use the PyTorch index for CUDA 12.0</li>
<li><code>cu118</code>:  Use the PyTorch index for CUDA 11.8</li>
<li><code>cu117</code>:  Use the PyTorch index for CUDA 11.7</li>
<li><code>cu116</code>:  Use the PyTorch index for CUDA 11.6</li>
<li><code>cu115</code>:  Use the PyTorch index for CUDA 11.5</li>
<li><code>cu114</code>:  Use the PyTorch index for CUDA 11.4</li>
<li><code>cu113</code>:  Use the PyTorch index for CUDA 11.3</li>
<li><code>cu112</code>:  Use the PyTorch index for CUDA 11.2</li>
<li><code>cu111</code>:  Use the PyTorch index for CUDA 11.1</li>
<li><code>cu110</code>:  Use the PyTorch index for CUDA 11.0</li>
<li><code>cu102</code>:  Use the PyTorch index for CUDA 10.2</li>
<li><code>cu101</code>:  Use the PyTorch index for CUDA 10.1</li>
<li><code>cu100</code>:  Use the PyTorch index for CUDA 10.0</li>
<li><code>cu92</code>:  Use the PyTorch index for CUDA 9.2</li>
<li><code>cu91</code>:  Use the PyTorch index for CUDA 9.1</li>
<li><code>cu90</code>:  Use the PyTorch index for CUDA 9.0</li>
<li><code>cu80</code>:  Use the PyTorch index for CUDA 8.0</li>
</ul></dd><dt id="uv-pip-compile--universal"><a href="#uv-pip-compile--universal"><code>--universal</code></a></dt><dd><p>Perform a universal resolution, attempting to generate a single <code>requirements.txt</code> output file that is compatible with all operating systems, architectures, and Python implementations.</p>
<p>In universal mode, the current Python version (or user-provided <code>--python-version</code>) will be treated as a lower bound. For example, <code>--universal --python-version 3.7</code> would produce a universal resolution for Python 3.7 and later.</p>
<p>Implies <code>--no-strip-markers</code>.</p>
</dd><dt id="uv-pip-compile--upgrade"><a href="#uv-pip-compile--upgrade"><code>--upgrade</code></a>, <code>-U</code></dt><dd><p>Allow package upgrades, ignoring pinned versions in any existing output file. Implies <code>--refresh</code></p>
</dd><dt id="uv-pip-compile--upgrade-package"><a href="#uv-pip-compile--upgrade-package"><code>--upgrade-package</code></a>, <code>-P</code> <i>upgrade-package</i></dt><dd><p>Allow upgrades for a specific package, ignoring pinned versions in any existing output file. Implies <code>--refresh-package</code></p>
</dd><dt id="uv-pip-compile--verbose"><a href="#uv-pip-compile--verbose"><code>--verbose</code></a>, <code>-v</code></dt><dd><p>Use verbose output.</p>
<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (<a href="https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives">https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives</a>)</p>
</dd></dl>

### uv pip sync

Sync an environment with a `requirements.txt` or `pylock.toml` file.

When syncing an environment, any packages not listed in the `requirements.txt` or `pylock.toml` file will be removed. To retain extraneous packages, use `uv pip install` instead.

The input file is presumed to be the output of a `pip compile` or `uv export` operation, in which it will include all transitive dependencies. If transitive dependencies are not present in the file, they will not be installed. Use `--strict` to warn if any transitive dependencies are missing.

<h3 class="cli-reference">Usage</h3>

```
uv pip sync [OPTIONS] <SRC_FILE>...
```

<h3 class="cli-reference">Arguments</h3>

<dl class="cli-reference"><dt id="uv-pip-sync--src_file"><a href="#uv-pip-sync--src_file"<code>SRC_FILE</code></a></dt><dd><p>Include all packages listed in the given <code>requirements.txt</code> files.</p>
<p>If a <code>pyproject.toml</code>, <code>setup.py</code>, or <code>setup.cfg</code> file is provided, uv will extract the requirements for the relevant project.</p>
<p>If <code>-</code> is provided, then requirements will be read from stdin.</p>
</dd></dl>

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt id="uv-pip-sync--allow-empty-requirements"><a href="#uv-pip-sync--allow-empty-requirements"><code>--allow-empty-requirements</code></a></dt><dd><p>Allow sync of empty requirements, which will clear the environment of all packages</p>
</dd><dt id="uv-pip-sync--allow-insecure-host"><a href="#uv-pip-sync--allow-insecure-host"><code>--allow-insecure-host</code></a>, <code>--trusted-host</code> <i>allow-insecure-host</i></dt><dd><p>Allow insecure connections to a host.</p>
<p>Can be provided multiple times.</p>
<p>Expects to receive either a hostname (e.g., <code>localhost</code>), a host-port pair (e.g., <code>localhost:8080</code>), or a URL (e.g., <code>https://localhost</code>).</p>
<p>WARNING: Hosts included in this list will not be verified against the system's certificate store. Only use <code>--allow-insecure-host</code> in a secure network with verified sources, as it bypasses SSL verification and could expose you to MITM attacks.</p>
<p>May also be set with the <code>UV_INSECURE_HOST</code> environment variable.</p></dd><dt id="uv-pip-sync--break-system-packages"><a href="#uv-pip-sync--break-system-packages"><code>--break-system-packages</code></a></dt><dd><p>Allow uv to modify an <code>EXTERNALLY-MANAGED</code> Python installation.</p>
<p>WARNING: <code>--break-system-packages</code> is intended for use in continuous integration (CI) environments, when installing into Python installations that are managed by an external package manager, like <code>apt</code>. It should be used with caution, as such Python installations explicitly recommend against modifications by other package managers (like uv or <code>pip</code>).</p>
<p>May also be set with the <code>UV_BREAK_SYSTEM_PACKAGES</code> environment variable.</p></dd><dt id="uv-pip-sync--build-constraints"><a href="#uv-pip-sync--build-constraints"><code>--build-constraints</code></a>, <code>--build-constraint</code>, <code>-b</code> <i>build-constraints</i></dt><dd><p>Constrain build dependencies using the given requirements files when building source distributions.</p>
<p>Constraints files are <code>requirements.txt</code>-like files that only control the <em>version</em> of a requirement that's installed. However, including a package in a constraints file will <em>not</em> trigger the installation of that package.</p>
<p>May also be set with the <code>UV_BUILD_CONSTRAINT</code> environment variable.</p></dd><dt id="uv-pip-sync--cache-dir"><a href="#uv-pip-sync--cache-dir"><code>--cache-dir</code></a> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>
<p>Defaults to <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on macOS and Linux, and <code>%LOCALAPPDATA%\uv\cache</code> on Windows.</p>
<p>To view the location of the cache directory, run <code>uv cache dir</code>.</p>
<p>May also be set with the <code>UV_CACHE_DIR</code> environment variable.</p></dd><dt id="uv-pip-sync--color"><a href="#uv-pip-sync--color"><code>--color</code></a> <i>color-choice</i></dt><dd><p>Control the use of color in output.</p>
<p>By default, uv will automatically detect support for colors when writing to a terminal.</p>
<p>Possible values:</p>
<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>
<li><code>always</code>:  Enables colored output regardless of the detected environment</li>
<li><code>never</code>:  Disables colored output</li>
</ul></dd><dt id="uv-pip-sync--compile-bytecode"><a href="#uv-pip-sync--compile-bytecode"><code>--compile-bytecode</code></a>, <code>--compile</code></dt><dd><p>Compile Python files to bytecode after installation.</p>
<p>By default, uv does not compile Python (<code>.py</code>) files to bytecode (<code>__pycache__/*.pyc</code>); instead, compilation is performed lazily the first time a module is imported. For use-cases in which start time is critical, such as CLI applications and Docker containers, this option can be enabled to trade longer installation times for faster start times.</p>
<p>When enabled, uv will process the entire site-packages directory (including packages that are not being modified by the current operation) for consistency. Like pip, it will also ignore errors.</p>
<p>May also be set with the <code>UV_COMPILE_BYTECODE</code> environment variable.</p></dd><dt id="uv-pip-sync--config-file"><a href="#uv-pip-sync--config-file"><code>--config-file</code></a> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>
<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>
<p>May also be set with the <code>UV_CONFIG_FILE</code> environment variable.</p></dd><dt id="uv-pip-sync--config-setting"><a href="#uv-pip-sync--config-setting"><code>--config-setting</code></a>, <code>--config-settings</code>, <code>-C</code> <i>config-setting</i></dt><dd><p>Settings to pass to the PEP 517 build backend, specified as <code>KEY=VALUE</code> pairs</p>
</dd><dt id="uv-pip-sync--constraints"><a href="#uv-pip-sync--constraints"><code>--constraints</code></a>, <code>--constraint</code>, <code>-c</code> <i>constraints</i></dt><dd><p>Constrain versions using the given requirements files.</p>
<p>Constraints files are <code>requirements.txt</code>-like files that only control the <em>version</em> of a requirement that's installed. However, including a package in a constraints file will <em>not</em> trigger the installation of that package.</p>
<p>This is equivalent to pip's <code>--constraint</code> option.</p>
<p>May also be set with the <code>UV_CONSTRAINT</code> environment variable.</p></dd><dt id="uv-pip-sync--default-index"><a href="#uv-pip-sync--default-index"><code>--default-index</code></a> <i>default-index</i></dt><dd><p>The URL of the default package index (by default: <a href="https://pypi.org/simple">https://pypi.org/simple</a>).</p>
<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>
<p>The index given by this flag is given lower priority than all other indexes specified via the <code>--index</code> flag.</p>
<p>May also be set with the <code>UV_DEFAULT_INDEX</code> environment variable.</p></dd><dt id="uv-pip-sync--directory"><a href="#uv-pip-sync--directory"><code>--directory</code></a> <i>directory</i></dt><dd><p>Change to the given directory prior to running the command.</p>
<p>Relative paths are resolved with the given directory as the base.</p>
<p>See <code>--project</code> to only change the project root directory.</p>
</dd><dt id="uv-pip-sync--dry-run"><a href="#uv-pip-sync--dry-run"><code>--dry-run</code></a></dt><dd><p>Perform a dry run, i.e., don't actually install anything but resolve the dependencies and print the resulting plan</p>
</dd><dt id="uv-pip-sync--exclude-newer"><a href="#uv-pip-sync--exclude-newer"><code>--exclude-newer</code></a> <i>exclude-newer</i></dt><dd><p>Limit candidate packages to those that were uploaded prior to the given date.</p>
<p>Accepts both RFC 3339 timestamps (e.g., <code>2006-12-02T02:07:43Z</code>) and local dates in the same format (e.g., <code>2006-12-02</code>) in your system's configured time zone.</p>
<p>May also be set with the <code>UV_EXCLUDE_NEWER</code> environment variable.</p></dd><dt id="uv-pip-sync--extra-index-url"><a href="#uv-pip-sync--extra-index-url"><code>--extra-index-url</code></a> <i>extra-index-url</i></dt><dd><p>(Deprecated: use <code>--index</code> instead) Extra URLs of package indexes to use, in addition to <code>--index-url</code>.</p>
<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>
<p>All indexes provided via this flag take priority over the index specified by <code>--index-url</code> (which defaults to PyPI). When multiple <code>--extra-index-url</code> flags are provided, earlier values take priority.</p>
<p>May also be set with the <code>UV_EXTRA_INDEX_URL</code> environment variable.</p></dd><dt id="uv-pip-sync--find-links"><a href="#uv-pip-sync--find-links"><code>--find-links</code></a>, <code>-f</code> <i>find-links</i></dt><dd><p>Locations to search for candidate distributions, in addition to those found in the registry indexes.</p>
<p>If a path, the target must be a directory that contains packages as wheel files (<code>.whl</code>) or source distributions (e.g., <code>.tar.gz</code> or <code>.zip</code>) at the top level.</p>
<p>If a URL, the page must contain a flat list of links to package files adhering to the formats described above.</p>
<p>May also be set with the <code>UV_FIND_LINKS</code> environment variable.</p></dd><dt id="uv-pip-sync--help"><a href="#uv-pip-sync--help"><code>--help</code></a>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>
</dd><dt id="uv-pip-sync--index"><a href="#uv-pip-sync--index"><code>--index</code></a> <i>index</i></dt><dd><p>The URLs to use when resolving dependencies, in addition to the default index.</p>
<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>
<p>All indexes provided via this flag take priority over the index specified by <code>--default-index</code> (which defaults to PyPI). When multiple <code>--index</code> flags are provided, earlier values take priority.</p>
<p>May also be set with the <code>UV_INDEX</code> environment variable.</p></dd><dt id="uv-pip-sync--index-strategy"><a href="#uv-pip-sync--index-strategy"><code>--index-strategy</code></a> <i>index-strategy</i></dt><dd><p>The strategy to use when resolving against multiple index URLs.</p>
<p>By default, uv will stop at the first index on which a given package is available, and limit resolutions to those present on that first index (<code>first-index</code>). This prevents &quot;dependency confusion&quot; attacks, whereby an attacker can upload a malicious package under the same name to an alternate index.</p>
<p>May also be set with the <code>UV_INDEX_STRATEGY</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>first-index</code>:  Only use results from the first index that returns a match for a given package name</li>
<li><code>unsafe-first-match</code>:  Search for every package name across all indexes, exhausting the versions from the first index before moving on to the next</li>
<li><code>unsafe-best-match</code>:  Search for every package name across all indexes, preferring the &quot;best&quot; version found. If a package version is in multiple indexes, only look at the entry for the first index</li>
</ul></dd><dt id="uv-pip-sync--index-url"><a href="#uv-pip-sync--index-url"><code>--index-url</code></a>, <code>-i</code> <i>index-url</i></dt><dd><p>(Deprecated: use <code>--default-index</code> instead) The URL of the Python package index (by default: <a href="https://pypi.org/simple">https://pypi.org/simple</a>).</p>
<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>
<p>The index given by this flag is given lower priority than all other indexes specified via the <code>--extra-index-url</code> flag.</p>
<p>May also be set with the <code>UV_INDEX_URL</code> environment variable.</p></dd><dt id="uv-pip-sync--keyring-provider"><a href="#uv-pip-sync--keyring-provider"><code>--keyring-provider</code></a> <i>keyring-provider</i></dt><dd><p>Attempt to use <code>keyring</code> for authentication for index URLs.</p>
<p>At present, only <code>--keyring-provider subprocess</code> is supported, which configures uv to use the <code>keyring</code> CLI to handle authentication.</p>
<p>Defaults to <code>disabled</code>.</p>
<p>May also be set with the <code>UV_KEYRING_PROVIDER</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>disabled</code>:  Do not use keyring for credential lookup</li>
<li><code>subprocess</code>:  Use the <code>keyring</code> command for credential lookup</li>
</ul></dd><dt id="uv-pip-sync--link-mode"><a href="#uv-pip-sync--link-mode"><code>--link-mode</code></a> <i>link-mode</i></dt><dd><p>The method to use when installing packages from the global cache.</p>
<p>Defaults to <code>clone</code> (also known as Copy-on-Write) on macOS, and <code>hardlink</code> on Linux and Windows.</p>
<p>May also be set with the <code>UV_LINK_MODE</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>clone</code>:  Clone (i.e., copy-on-write) packages from the wheel into the <code>site-packages</code> directory</li>
<li><code>copy</code>:  Copy packages from the wheel into the <code>site-packages</code> directory</li>
<li><code>hardlink</code>:  Hard link packages from the wheel into the <code>site-packages</code> directory</li>
<li><code>symlink</code>:  Symbolically link packages from the wheel into the <code>site-packages</code> directory</li>
</ul></dd><dt id="uv-pip-sync--managed-python"><a href="#uv-pip-sync--managed-python"><code>--managed-python</code></a></dt><dd><p>Require use of uv-managed Python versions.</p>
<p>By default, uv prefers using Python versions it manages. However, it will use system Python versions if a uv-managed Python is not installed. This option disables use of system Python versions.</p>
<p>May also be set with the <code>UV_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-pip-sync--native-tls"><a href="#uv-pip-sync--native-tls"><code>--native-tls</code></a></dt><dd><p>Whether to load TLS certificates from the platform's native certificate store.</p>
<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>
<p>However, in some cases, you may want to use the platform's native certificate store, especially if you're relying on a corporate trust root (e.g., for a mandatory proxy) that's included in your system's certificate store.</p>
<p>May also be set with the <code>UV_NATIVE_TLS</code> environment variable.</p></dd><dt id="uv-pip-sync--no-allow-empty-requirements"><a href="#uv-pip-sync--no-allow-empty-requirements"><code>--no-allow-empty-requirements</code></a></dt><dt id="uv-pip-sync--no-binary"><a href="#uv-pip-sync--no-binary"><code>--no-binary</code></a> <i>no-binary</i></dt><dd><p>Don't install pre-built wheels.</p>
<p>The given packages will be built and installed from source. The resolver will still use pre-built wheels to extract package metadata, if available.</p>
<p>Multiple packages may be provided. Disable binaries for all packages with <code>:all:</code>. Clear previously specified packages with <code>:none:</code>.</p>
</dd><dt id="uv-pip-sync--no-break-system-packages"><a href="#uv-pip-sync--no-break-system-packages"><code>--no-break-system-packages</code></a></dt><dt id="uv-pip-sync--no-build"><a href="#uv-pip-sync--no-build"><code>--no-build</code></a></dt><dd><p>Don't build source distributions.</p>
<p>When enabled, resolving will not run arbitrary Python code. The cached wheels of already-built source distributions will be reused, but operations that require building distributions will exit with an error.</p>
<p>Alias for <code>--only-binary :all:</code>.</p>
</dd><dt id="uv-pip-sync--no-build-isolation"><a href="#uv-pip-sync--no-build-isolation"><code>--no-build-isolation</code></a></dt><dd><p>Disable isolation when building source distributions.</p>
<p>Assumes that build dependencies specified by PEP 518 are already installed.</p>
<p>May also be set with the <code>UV_NO_BUILD_ISOLATION</code> environment variable.</p></dd><dt id="uv-pip-sync--no-cache"><a href="#uv-pip-sync--no-cache"><code>--no-cache</code></a>, <code>--no-cache-dir</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>
<p>May also be set with the <code>UV_NO_CACHE</code> environment variable.</p></dd><dt id="uv-pip-sync--no-index"><a href="#uv-pip-sync--no-index"><code>--no-index</code></a></dt><dd><p>Ignore the registry index (e.g., PyPI), instead relying on direct URL dependencies and those provided via <code>--find-links</code></p>
</dd><dt id="uv-pip-sync--no-managed-python"><a href="#uv-pip-sync--no-managed-python"><code>--no-managed-python</code></a></dt><dd><p>Disable use of uv-managed Python versions.</p>
<p>Instead, uv will search for a suitable Python version on the system.</p>
<p>May also be set with the <code>UV_NO_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-pip-sync--no-progress"><a href="#uv-pip-sync--no-progress"><code>--no-progress</code></a></dt><dd><p>Hide all progress outputs.</p>
<p>For example, spinners or progress bars.</p>
<p>May also be set with the <code>UV_NO_PROGRESS</code> environment variable.</p></dd><dt id="uv-pip-sync--no-python-downloads"><a href="#uv-pip-sync--no-python-downloads"><code>--no-python-downloads</code></a></dt><dd><p>Disable automatic downloads of Python.</p>
</dd><dt id="uv-pip-sync--no-sources"><a href="#uv-pip-sync--no-sources"><code>--no-sources</code></a></dt><dd><p>Ignore the <code>tool.uv.sources</code> table when resolving dependencies. Used to lock against the standards-compliant, publishable package metadata, as opposed to using any workspace, Git, URL, or local path sources</p>
</dd><dt id="uv-pip-sync--no-verify-hashes"><a href="#uv-pip-sync--no-verify-hashes"><code>--no-verify-hashes</code></a></dt><dd><p>Disable validation of hashes in the requirements file.</p>
<p>By default, uv will verify any available hashes in the requirements file, but will not require that all requirements have an associated hash. To enforce hash validation, use <code>--require-hashes</code>.</p>
<p>May also be set with the <code>UV_NO_VERIFY_HASHES</code> environment variable.</p></dd><dt id="uv-pip-sync--offline"><a href="#uv-pip-sync--offline"><code>--offline</code></a></dt><dd><p>Disable network access.</p>
<p>When disabled, uv will only use locally cached data and locally available files.</p>
<p>May also be set with the <code>UV_OFFLINE</code> environment variable.</p></dd><dt id="uv-pip-sync--only-binary"><a href="#uv-pip-sync--only-binary"><code>--only-binary</code></a> <i>only-binary</i></dt><dd><p>Only use pre-built wheels; don't build source distributions.</p>
<p>When enabled, resolving will not run code from the given packages. The cached wheels of already-built source distributions will be reused, but operations that require building distributions will exit with an error.</p>
<p>Multiple packages may be provided. Disable binaries for all packages with <code>:all:</code>. Clear previously specified packages with <code>:none:</code>.</p>
</dd><dt id="uv-pip-sync--prefix"><a href="#uv-pip-sync--prefix"><code>--prefix</code></a> <i>prefix</i></dt><dd><p>Install packages into <code>lib</code>, <code>bin</code>, and other top-level folders under the specified directory, as if a virtual environment were present at that location.</p>
<p>In general, prefer the use of <code>--python</code> to install into an alternate environment, as scripts and other artifacts installed via <code>--prefix</code> will reference the installing interpreter, rather than any interpreter added to the <code>--prefix</code> directory, rendering them non-portable.</p>
</dd><dt id="uv-pip-sync--project"><a href="#uv-pip-sync--project"><code>--project</code></a> <i>project</i></dt><dd><p>Run the command within the given project directory.</p>
<p>All <code>pyproject.toml</code>, <code>uv.toml</code>, and <code>.python-version</code> files will be discovered by walking up the directory tree from the project root, as will the project's virtual environment (<code>.venv</code>).</p>
<p>Other command-line arguments (such as relative paths) will be resolved relative to the current working directory.</p>
<p>See <code>--directory</code> to change the working directory entirely.</p>
<p>This setting has no effect when used in the <code>uv pip</code> interface.</p>
<p>May also be set with the <code>UV_PROJECT</code> environment variable.</p></dd><dt id="uv-pip-sync--python"><a href="#uv-pip-sync--python"><code>--python</code></a>, <code>-p</code> <i>python</i></dt><dd><p>The Python interpreter into which packages should be installed.</p>
<p>By default, syncing requires a virtual environment. A path to an alternative Python can be
provided, but it is only recommended in continuous integration (CI) environments and should
be used with caution, as it can modify the system Python installation.</p>
<p>See <a href="#uv-python">uv python</a> for details on Python discovery and supported request formats.</p>
<p>May also be set with the <code>UV_PYTHON</code> environment variable.</p></dd><dt id="uv-pip-sync--python-platform"><a href="#uv-pip-sync--python-platform"><code>--python-platform</code></a> <i>python-platform</i></dt><dd><p>The platform for which requirements should be installed.</p>
<p>Represented as a &quot;target triple&quot;, a string that describes the target platform in terms of its CPU, vendor, and operating system name, like <code>x86_64-unknown-linux-gnu</code> or <code>aarch64-apple-darwin</code>.</p>
<p>When targeting macOS (Darwin), the default minimum version is <code>12.0</code>. Use <code>MACOSX_DEPLOYMENT_TARGET</code> to specify a different minimum version, e.g., <code>13.0</code>.</p>
<p>WARNING: When specified, uv will select wheels that are compatible with the <em>target</em> platform; as a result, the installed distributions may not be compatible with the <em>current</em> platform. Conversely, any distributions that are built from source may be incompatible with the <em>target</em> platform, as they will be built for the <em>current</em> platform. The <code>--python-platform</code> option is intended for advanced use cases.</p>
<p>Possible values:</p>
<ul>
<li><code>windows</code>:  An alias for <code>x86_64-pc-windows-msvc</code>, the default target for Windows</li>
<li><code>linux</code>:  An alias for <code>x86_64-unknown-linux-gnu</code>, the default target for Linux</li>
<li><code>macos</code>:  An alias for <code>aarch64-apple-darwin</code>, the default target for macOS</li>
<li><code>x86_64-pc-windows-msvc</code>:  A 64-bit x86 Windows target</li>
<li><code>i686-pc-windows-msvc</code>:  A 32-bit x86 Windows target</li>
<li><code>x86_64-unknown-linux-gnu</code>:  An x86 Linux target. Equivalent to <code>x86_64-manylinux_2_17</code></li>
<li><code>aarch64-apple-darwin</code>:  An ARM-based macOS target, as seen on Apple Silicon devices</li>
<li><code>x86_64-apple-darwin</code>:  An x86 macOS target</li>
<li><code>aarch64-unknown-linux-gnu</code>:  An ARM64 Linux target. Equivalent to <code>aarch64-manylinux_2_17</code></li>
<li><code>aarch64-unknown-linux-musl</code>:  An ARM64 Linux target</li>
<li><code>x86_64-unknown-linux-musl</code>:  An <code>x86_64</code> Linux target</li>
<li><code>x86_64-manylinux2014</code>:  An <code>x86_64</code> target for the <code>manylinux2014</code> platform. Equivalent to <code>x86_64-manylinux_2_17</code></li>
<li><code>x86_64-manylinux_2_17</code>:  An <code>x86_64</code> target for the <code>manylinux_2_17</code> platform</li>
<li><code>x86_64-manylinux_2_28</code>:  An <code>x86_64</code> target for the <code>manylinux_2_28</code> platform</li>
<li><code>x86_64-manylinux_2_31</code>:  An <code>x86_64</code> target for the <code>manylinux_2_31</code> platform</li>
<li><code>x86_64-manylinux_2_32</code>:  An <code>x86_64</code> target for the <code>manylinux_2_32</code> platform</li>
<li><code>x86_64-manylinux_2_33</code>:  An <code>x86_64</code> target for the <code>manylinux_2_33</code> platform</li>
<li><code>x86_64-manylinux_2_34</code>:  An <code>x86_64</code> target for the <code>manylinux_2_34</code> platform</li>
<li><code>x86_64-manylinux_2_35</code>:  An <code>x86_64</code> target for the <code>manylinux_2_35</code> platform</li>
<li><code>x86_64-manylinux_2_36</code>:  An <code>x86_64</code> target for the <code>manylinux_2_36</code> platform</li>
<li><code>x86_64-manylinux_2_37</code>:  An <code>x86_64</code> target for the <code>manylinux_2_37</code> platform</li>
<li><code>x86_64-manylinux_2_38</code>:  An <code>x86_64</code> target for the <code>manylinux_2_38</code> platform</li>
<li><code>x86_64-manylinux_2_39</code>:  An <code>x86_64</code> target for the <code>manylinux_2_39</code> platform</li>
<li><code>x86_64-manylinux_2_40</code>:  An <code>x86_64</code> target for the <code>manylinux_2_40</code> platform</li>
<li><code>aarch64-manylinux2014</code>:  An ARM64 target for the <code>manylinux2014</code> platform. Equivalent to <code>aarch64-manylinux_2_17</code></li>
<li><code>aarch64-manylinux_2_17</code>:  An ARM64 target for the <code>manylinux_2_17</code> platform</li>
<li><code>aarch64-manylinux_2_28</code>:  An ARM64 target for the <code>manylinux_2_28</code> platform</li>
<li><code>aarch64-manylinux_2_31</code>:  An ARM64 target for the <code>manylinux_2_31</code> platform</li>
<li><code>aarch64-manylinux_2_32</code>:  An ARM64 target for the <code>manylinux_2_32</code> platform</li>
<li><code>aarch64-manylinux_2_33</code>:  An ARM64 target for the <code>manylinux_2_33</code> platform</li>
<li><code>aarch64-manylinux_2_34</code>:  An ARM64 target for the <code>manylinux_2_34</code> platform</li>
<li><code>aarch64-manylinux_2_35</code>:  An ARM64 target for the <code>manylinux_2_35</code> platform</li>
<li><code>aarch64-manylinux_2_36</code>:  An ARM64 target for the <code>manylinux_2_36</code> platform</li>
<li><code>aarch64-manylinux_2_37</code>:  An ARM64 target for the <code>manylinux_2_37</code> platform</li>
<li><code>aarch64-manylinux_2_38</code>:  An ARM64 target for the <code>manylinux_2_38</code> platform</li>
<li><code>aarch64-manylinux_2_39</code>:  An ARM64 target for the <code>manylinux_2_39</code> platform</li>
<li><code>aarch64-manylinux_2_40</code>:  An ARM64 target for the <code>manylinux_2_40</code> platform</li>
</ul></dd><dt id="uv-pip-sync--python-version"><a href="#uv-pip-sync--python-version"><code>--python-version</code></a> <i>python-version</i></dt><dd><p>The minimum Python version that should be supported by the requirements (e.g., <code>3.7</code> or <code>3.7.9</code>).</p>
<p>If a patch version is omitted, the minimum patch version is assumed. For example, <code>3.7</code> is mapped to <code>3.7.0</code>.</p>
</dd><dt id="uv-pip-sync--quiet"><a href="#uv-pip-sync--quiet"><code>--quiet</code></a>, <code>-q</code></dt><dd><p>Use quiet output.</p>
<p>Repeating this option, e.g., <code>-qq</code>, will enable a silent mode in which uv will write no output to stdout.</p>
</dd><dt id="uv-pip-sync--refresh"><a href="#uv-pip-sync--refresh"><code>--refresh</code></a></dt><dd><p>Refresh all cached data</p>
</dd><dt id="uv-pip-sync--refresh-package"><a href="#uv-pip-sync--refresh-package"><code>--refresh-package</code></a> <i>refresh-package</i></dt><dd><p>Refresh cached data for a specific package</p>
</dd><dt id="uv-pip-sync--reinstall"><a href="#uv-pip-sync--reinstall"><code>--reinstall</code></a>, <code>--force-reinstall</code></dt><dd><p>Reinstall all packages, regardless of whether they're already installed. Implies <code>--refresh</code></p>
</dd><dt id="uv-pip-sync--reinstall-package"><a href="#uv-pip-sync--reinstall-package"><code>--reinstall-package</code></a> <i>reinstall-package</i></dt><dd><p>Reinstall a specific package, regardless of whether it's already installed. Implies <code>--refresh-package</code></p>
</dd><dt id="uv-pip-sync--require-hashes"><a href="#uv-pip-sync--require-hashes"><code>--require-hashes</code></a></dt><dd><p>Require a matching hash for each requirement.</p>
<p>By default, uv will verify any available hashes in the requirements file, but will not require that all requirements have an associated hash.</p>
<p>When <code>--require-hashes</code> is enabled, <em>all</em> requirements must include a hash or set of hashes, and <em>all</em> requirements must either be pinned to exact versions (e.g., <code>==1.0.0</code>), or be specified via direct URL.</p>
<p>Hash-checking mode introduces a number of additional constraints:</p>
<ul>
<li>Git dependencies are not supported. - Editable installs are not supported. - Local dependencies are not supported, unless they point to a specific wheel (<code>.whl</code>) or source archive (<code>.zip</code>, <code>.tar.gz</code>), as opposed to a directory.</li>
</ul>
<p>May also be set with the <code>UV_REQUIRE_HASHES</code> environment variable.</p></dd><dt id="uv-pip-sync--strict"><a href="#uv-pip-sync--strict"><code>--strict</code></a></dt><dd><p>Validate the Python environment after completing the installation, to detect packages with missing dependencies or other issues</p>
</dd><dt id="uv-pip-sync--system"><a href="#uv-pip-sync--system"><code>--system</code></a></dt><dd><p>Install packages into the system Python environment.</p>
<p>By default, uv installs into the virtual environment in the current working directory or any parent directory. The <code>--system</code> option instructs uv to instead use the first Python found in the system <code>PATH</code>.</p>
<p>WARNING: <code>--system</code> is intended for use in continuous integration (CI) environments and should be used with caution, as it can modify the system Python installation.</p>
<p>May also be set with the <code>UV_SYSTEM_PYTHON</code> environment variable.</p></dd><dt id="uv-pip-sync--target"><a href="#uv-pip-sync--target"><code>--target</code></a> <i>target</i></dt><dd><p>Install packages into the specified directory, rather than into the virtual or system Python environment. The packages will be installed at the top-level of the directory</p>
</dd><dt id="uv-pip-sync--torch-backend"><a href="#uv-pip-sync--torch-backend"><code>--torch-backend</code></a> <i>torch-backend</i></dt><dd><p>The backend to use when fetching packages in the PyTorch ecosystem (e.g., <code>cpu</code>, <code>cu126</code>, or <code>auto</code>).</p>
<p>When set, uv will ignore the configured index URLs for packages in the PyTorch ecosystem, and will instead use the defined backend.</p>
<p>For example, when set to <code>cpu</code>, uv will use the CPU-only PyTorch index; when set to <code>cu126</code>, uv will use the PyTorch index for CUDA 12.6.</p>
<p>The <code>auto</code> mode will attempt to detect the appropriate PyTorch index based on the currently installed CUDA drivers.</p>
<p>This option is in preview and may change in any future release.</p>
<p>May also be set with the <code>UV_TORCH_BACKEND</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>auto</code>:  Select the appropriate PyTorch index based on the operating system and CUDA driver version</li>
<li><code>cpu</code>:  Use the CPU-only PyTorch index</li>
<li><code>cu128</code>:  Use the PyTorch index for CUDA 12.8</li>
<li><code>cu126</code>:  Use the PyTorch index for CUDA 12.6</li>
<li><code>cu125</code>:  Use the PyTorch index for CUDA 12.5</li>
<li><code>cu124</code>:  Use the PyTorch index for CUDA 12.4</li>
<li><code>cu123</code>:  Use the PyTorch index for CUDA 12.3</li>
<li><code>cu122</code>:  Use the PyTorch index for CUDA 12.2</li>
<li><code>cu121</code>:  Use the PyTorch index for CUDA 12.1</li>
<li><code>cu120</code>:  Use the PyTorch index for CUDA 12.0</li>
<li><code>cu118</code>:  Use the PyTorch index for CUDA 11.8</li>
<li><code>cu117</code>:  Use the PyTorch index for CUDA 11.7</li>
<li><code>cu116</code>:  Use the PyTorch index for CUDA 11.6</li>
<li><code>cu115</code>:  Use the PyTorch index for CUDA 11.5</li>
<li><code>cu114</code>:  Use the PyTorch index for CUDA 11.4</li>
<li><code>cu113</code>:  Use the PyTorch index for CUDA 11.3</li>
<li><code>cu112</code>:  Use the PyTorch index for CUDA 11.2</li>
<li><code>cu111</code>:  Use the PyTorch index for CUDA 11.1</li>
<li><code>cu110</code>:  Use the PyTorch index for CUDA 11.0</li>
<li><code>cu102</code>:  Use the PyTorch index for CUDA 10.2</li>
<li><code>cu101</code>:  Use the PyTorch index for CUDA 10.1</li>
<li><code>cu100</code>:  Use the PyTorch index for CUDA 10.0</li>
<li><code>cu92</code>:  Use the PyTorch index for CUDA 9.2</li>
<li><code>cu91</code>:  Use the PyTorch index for CUDA 9.1</li>
<li><code>cu90</code>:  Use the PyTorch index for CUDA 9.0</li>
<li><code>cu80</code>:  Use the PyTorch index for CUDA 8.0</li>
</ul></dd><dt id="uv-pip-sync--verbose"><a href="#uv-pip-sync--verbose"><code>--verbose</code></a>, <code>-v</code></dt><dd><p>Use verbose output.</p>
<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (<a href="https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives">https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives</a>)</p>
</dd></dl>

### uv pip install

Install packages into an environment

<h3 class="cli-reference">Usage</h3>

```
uv pip install [OPTIONS] <PACKAGE|--requirements <REQUIREMENTS>|--editable <EDITABLE>|--group <GROUP>>
```

<h3 class="cli-reference">Arguments</h3>

<dl class="cli-reference"><dt id="uv-pip-install--package"><a href="#uv-pip-install--package"<code>PACKAGE</code></a></dt><dd><p>Install all listed packages.</p>
<p>The order of the packages is used to determine priority during resolution.</p>
</dd></dl>

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt id="uv-pip-install--all-extras"><a href="#uv-pip-install--all-extras"><code>--all-extras</code></a></dt><dd><p>Include all optional dependencies.</p>
<p>Only applies to <code>pyproject.toml</code>, <code>setup.py</code>, and <code>setup.cfg</code> sources.</p>
</dd><dt id="uv-pip-install--allow-insecure-host"><a href="#uv-pip-install--allow-insecure-host"><code>--allow-insecure-host</code></a>, <code>--trusted-host</code> <i>allow-insecure-host</i></dt><dd><p>Allow insecure connections to a host.</p>
<p>Can be provided multiple times.</p>
<p>Expects to receive either a hostname (e.g., <code>localhost</code>), a host-port pair (e.g., <code>localhost:8080</code>), or a URL (e.g., <code>https://localhost</code>).</p>
<p>WARNING: Hosts included in this list will not be verified against the system's certificate store. Only use <code>--allow-insecure-host</code> in a secure network with verified sources, as it bypasses SSL verification and could expose you to MITM attacks.</p>
<p>May also be set with the <code>UV_INSECURE_HOST</code> environment variable.</p></dd><dt id="uv-pip-install--break-system-packages"><a href="#uv-pip-install--break-system-packages"><code>--break-system-packages</code></a></dt><dd><p>Allow uv to modify an <code>EXTERNALLY-MANAGED</code> Python installation.</p>
<p>WARNING: <code>--break-system-packages</code> is intended for use in continuous integration (CI) environments, when installing into Python installations that are managed by an external package manager, like <code>apt</code>. It should be used with caution, as such Python installations explicitly recommend against modifications by other package managers (like uv or <code>pip</code>).</p>
<p>May also be set with the <code>UV_BREAK_SYSTEM_PACKAGES</code> environment variable.</p></dd><dt id="uv-pip-install--build-constraints"><a href="#uv-pip-install--build-constraints"><code>--build-constraints</code></a>, <code>--build-constraint</code>, <code>-b</code> <i>build-constraints</i></dt><dd><p>Constrain build dependencies using the given requirements files when building source distributions.</p>
<p>Constraints files are <code>requirements.txt</code>-like files that only control the <em>version</em> of a requirement that's installed. However, including a package in a constraints file will <em>not</em> trigger the installation of that package.</p>
<p>May also be set with the <code>UV_BUILD_CONSTRAINT</code> environment variable.</p></dd><dt id="uv-pip-install--cache-dir"><a href="#uv-pip-install--cache-dir"><code>--cache-dir</code></a> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>
<p>Defaults to <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on macOS and Linux, and <code>%LOCALAPPDATA%\uv\cache</code> on Windows.</p>
<p>To view the location of the cache directory, run <code>uv cache dir</code>.</p>
<p>May also be set with the <code>UV_CACHE_DIR</code> environment variable.</p></dd><dt id="uv-pip-install--color"><a href="#uv-pip-install--color"><code>--color</code></a> <i>color-choice</i></dt><dd><p>Control the use of color in output.</p>
<p>By default, uv will automatically detect support for colors when writing to a terminal.</p>
<p>Possible values:</p>
<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>
<li><code>always</code>:  Enables colored output regardless of the detected environment</li>
<li><code>never</code>:  Disables colored output</li>
</ul></dd><dt id="uv-pip-install--compile-bytecode"><a href="#uv-pip-install--compile-bytecode"><code>--compile-bytecode</code></a>, <code>--compile</code></dt><dd><p>Compile Python files to bytecode after installation.</p>
<p>By default, uv does not compile Python (<code>.py</code>) files to bytecode (<code>__pycache__/*.pyc</code>); instead, compilation is performed lazily the first time a module is imported. For use-cases in which start time is critical, such as CLI applications and Docker containers, this option can be enabled to trade longer installation times for faster start times.</p>
<p>When enabled, uv will process the entire site-packages directory (including packages that are not being modified by the current operation) for consistency. Like pip, it will also ignore errors.</p>
<p>May also be set with the <code>UV_COMPILE_BYTECODE</code> environment variable.</p></dd><dt id="uv-pip-install--config-file"><a href="#uv-pip-install--config-file"><code>--config-file</code></a> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>
<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>
<p>May also be set with the <code>UV_CONFIG_FILE</code> environment variable.</p></dd><dt id="uv-pip-install--config-setting"><a href="#uv-pip-install--config-setting"><code>--config-setting</code></a>, <code>--config-settings</code>, <code>-C</code> <i>config-setting</i></dt><dd><p>Settings to pass to the PEP 517 build backend, specified as <code>KEY=VALUE</code> pairs</p>
</dd><dt id="uv-pip-install--constraints"><a href="#uv-pip-install--constraints"><code>--constraints</code></a>, <code>--constraint</code>, <code>-c</code> <i>constraints</i></dt><dd><p>Constrain versions using the given requirements files.</p>
<p>Constraints files are <code>requirements.txt</code>-like files that only control the <em>version</em> of a requirement that's installed. However, including a package in a constraints file will <em>not</em> trigger the installation of that package.</p>
<p>This is equivalent to pip's <code>--constraint</code> option.</p>
<p>May also be set with the <code>UV_CONSTRAINT</code> environment variable.</p></dd><dt id="uv-pip-install--default-index"><a href="#uv-pip-install--default-index"><code>--default-index</code></a> <i>default-index</i></dt><dd><p>The URL of the default package index (by default: <a href="https://pypi.org/simple">https://pypi.org/simple</a>).</p>
<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>
<p>The index given by this flag is given lower priority than all other indexes specified via the <code>--index</code> flag.</p>
<p>May also be set with the <code>UV_DEFAULT_INDEX</code> environment variable.</p></dd><dt id="uv-pip-install--directory"><a href="#uv-pip-install--directory"><code>--directory</code></a> <i>directory</i></dt><dd><p>Change to the given directory prior to running the command.</p>
<p>Relative paths are resolved with the given directory as the base.</p>
<p>See <code>--project</code> to only change the project root directory.</p>
</dd><dt id="uv-pip-install--dry-run"><a href="#uv-pip-install--dry-run"><code>--dry-run</code></a></dt><dd><p>Perform a dry run, i.e., don't actually install anything but resolve the dependencies and print the resulting plan</p>
</dd><dt id="uv-pip-install--editable"><a href="#uv-pip-install--editable"><code>--editable</code></a>, <code>-e</code> <i>editable</i></dt><dd><p>Install the editable package based on the provided local file path</p>
</dd><dt id="uv-pip-install--exact"><a href="#uv-pip-install--exact"><code>--exact</code></a></dt><dd><p>Perform an exact sync, removing extraneous packages.</p>
<p>By default, installing will make the minimum necessary changes to satisfy the requirements. When enabled, uv will update the environment to exactly match the requirements, removing packages that are not included in the requirements.</p>
</dd><dt id="uv-pip-install--exclude-newer"><a href="#uv-pip-install--exclude-newer"><code>--exclude-newer</code></a> <i>exclude-newer</i></dt><dd><p>Limit candidate packages to those that were uploaded prior to the given date.</p>
<p>Accepts both RFC 3339 timestamps (e.g., <code>2006-12-02T02:07:43Z</code>) and local dates in the same format (e.g., <code>2006-12-02</code>) in your system's configured time zone.</p>
<p>May also be set with the <code>UV_EXCLUDE_NEWER</code> environment variable.</p></dd><dt id="uv-pip-install--extra"><a href="#uv-pip-install--extra"><code>--extra</code></a> <i>extra</i></dt><dd><p>Include optional dependencies from the specified extra name; may be provided more than once.</p>
<p>Only applies to <code>pyproject.toml</code>, <code>setup.py</code>, and <code>setup.cfg</code> sources.</p>
</dd><dt id="uv-pip-install--extra-index-url"><a href="#uv-pip-install--extra-index-url"><code>--extra-index-url</code></a> <i>extra-index-url</i></dt><dd><p>(Deprecated: use <code>--index</code> instead) Extra URLs of package indexes to use, in addition to <code>--index-url</code>.</p>
<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>
<p>All indexes provided via this flag take priority over the index specified by <code>--index-url</code> (which defaults to PyPI). When multiple <code>--extra-index-url</code> flags are provided, earlier values take priority.</p>
<p>May also be set with the <code>UV_EXTRA_INDEX_URL</code> environment variable.</p></dd><dt id="uv-pip-install--find-links"><a href="#uv-pip-install--find-links"><code>--find-links</code></a>, <code>-f</code> <i>find-links</i></dt><dd><p>Locations to search for candidate distributions, in addition to those found in the registry indexes.</p>
<p>If a path, the target must be a directory that contains packages as wheel files (<code>.whl</code>) or source distributions (e.g., <code>.tar.gz</code> or <code>.zip</code>) at the top level.</p>
<p>If a URL, the page must contain a flat list of links to package files adhering to the formats described above.</p>
<p>May also be set with the <code>UV_FIND_LINKS</code> environment variable.</p></dd><dt id="uv-pip-install--fork-strategy"><a href="#uv-pip-install--fork-strategy"><code>--fork-strategy</code></a> <i>fork-strategy</i></dt><dd><p>The strategy to use when selecting multiple versions of a given package across Python versions and platforms.</p>
<p>By default, uv will optimize for selecting the latest version of each package for each supported Python version (<code>requires-python</code>), while minimizing the number of selected versions across platforms.</p>
<p>Under <code>fewest</code>, uv will minimize the number of selected versions for each package, preferring older versions that are compatible with a wider range of supported Python versions or platforms.</p>
<p>May also be set with the <code>UV_FORK_STRATEGY</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>fewest</code>:  Optimize for selecting the fewest number of versions for each package. Older versions may be preferred if they are compatible with a wider range of supported Python versions or platforms</li>
<li><code>requires-python</code>:  Optimize for selecting latest supported version of each package, for each supported Python version</li>
</ul></dd><dt id="uv-pip-install--group"><a href="#uv-pip-install--group"><code>--group</code></a> <i>group</i></dt><dd><p>Install the specified dependency group from a <code>pyproject.toml</code>.</p>
<p>If no path is provided, the <code>pyproject.toml</code> in the working directory is used.</p>
<p>May be provided multiple times.</p>
</dd><dt id="uv-pip-install--help"><a href="#uv-pip-install--help"><code>--help</code></a>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>
</dd><dt id="uv-pip-install--index"><a href="#uv-pip-install--index"><code>--index</code></a> <i>index</i></dt><dd><p>The URLs to use when resolving dependencies, in addition to the default index.</p>
<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>
<p>All indexes provided via this flag take priority over the index specified by <code>--default-index</code> (which defaults to PyPI). When multiple <code>--index</code> flags are provided, earlier values take priority.</p>
<p>May also be set with the <code>UV_INDEX</code> environment variable.</p></dd><dt id="uv-pip-install--index-strategy"><a href="#uv-pip-install--index-strategy"><code>--index-strategy</code></a> <i>index-strategy</i></dt><dd><p>The strategy to use when resolving against multiple index URLs.</p>
<p>By default, uv will stop at the first index on which a given package is available, and limit resolutions to those present on that first index (<code>first-index</code>). This prevents &quot;dependency confusion&quot; attacks, whereby an attacker can upload a malicious package under the same name to an alternate index.</p>
<p>May also be set with the <code>UV_INDEX_STRATEGY</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>first-index</code>:  Only use results from the first index that returns a match for a given package name</li>
<li><code>unsafe-first-match</code>:  Search for every package name across all indexes, exhausting the versions from the first index before moving on to the next</li>
<li><code>unsafe-best-match</code>:  Search for every package name across all indexes, preferring the &quot;best&quot; version found. If a package version is in multiple indexes, only look at the entry for the first index</li>
</ul></dd><dt id="uv-pip-install--index-url"><a href="#uv-pip-install--index-url"><code>--index-url</code></a>, <code>-i</code> <i>index-url</i></dt><dd><p>(Deprecated: use <code>--default-index</code> instead) The URL of the Python package index (by default: <a href="https://pypi.org/simple">https://pypi.org/simple</a>).</p>
<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>
<p>The index given by this flag is given lower priority than all other indexes specified via the <code>--extra-index-url</code> flag.</p>
<p>May also be set with the <code>UV_INDEX_URL</code> environment variable.</p></dd><dt id="uv-pip-install--keyring-provider"><a href="#uv-pip-install--keyring-provider"><code>--keyring-provider</code></a> <i>keyring-provider</i></dt><dd><p>Attempt to use <code>keyring</code> for authentication for index URLs.</p>
<p>At present, only <code>--keyring-provider subprocess</code> is supported, which configures uv to use the <code>keyring</code> CLI to handle authentication.</p>
<p>Defaults to <code>disabled</code>.</p>
<p>May also be set with the <code>UV_KEYRING_PROVIDER</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>disabled</code>:  Do not use keyring for credential lookup</li>
<li><code>subprocess</code>:  Use the <code>keyring</code> command for credential lookup</li>
</ul></dd><dt id="uv-pip-install--link-mode"><a href="#uv-pip-install--link-mode"><code>--link-mode</code></a> <i>link-mode</i></dt><dd><p>The method to use when installing packages from the global cache.</p>
<p>Defaults to <code>clone</code> (also known as Copy-on-Write) on macOS, and <code>hardlink</code> on Linux and Windows.</p>
<p>May also be set with the <code>UV_LINK_MODE</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>clone</code>:  Clone (i.e., copy-on-write) packages from the wheel into the <code>site-packages</code> directory</li>
<li><code>copy</code>:  Copy packages from the wheel into the <code>site-packages</code> directory</li>
<li><code>hardlink</code>:  Hard link packages from the wheel into the <code>site-packages</code> directory</li>
<li><code>symlink</code>:  Symbolically link packages from the wheel into the <code>site-packages</code> directory</li>
</ul></dd><dt id="uv-pip-install--managed-python"><a href="#uv-pip-install--managed-python"><code>--managed-python</code></a></dt><dd><p>Require use of uv-managed Python versions.</p>
<p>By default, uv prefers using Python versions it manages. However, it will use system Python versions if a uv-managed Python is not installed. This option disables use of system Python versions.</p>
<p>May also be set with the <code>UV_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-pip-install--native-tls"><a href="#uv-pip-install--native-tls"><code>--native-tls</code></a></dt><dd><p>Whether to load TLS certificates from the platform's native certificate store.</p>
<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>
<p>However, in some cases, you may want to use the platform's native certificate store, especially if you're relying on a corporate trust root (e.g., for a mandatory proxy) that's included in your system's certificate store.</p>
<p>May also be set with the <code>UV_NATIVE_TLS</code> environment variable.</p></dd><dt id="uv-pip-install--no-binary"><a href="#uv-pip-install--no-binary"><code>--no-binary</code></a> <i>no-binary</i></dt><dd><p>Don't install pre-built wheels.</p>
<p>The given packages will be built and installed from source. The resolver will still use pre-built wheels to extract package metadata, if available.</p>
<p>Multiple packages may be provided. Disable binaries for all packages with <code>:all:</code>. Clear previously specified packages with <code>:none:</code>.</p>
</dd><dt id="uv-pip-install--no-break-system-packages"><a href="#uv-pip-install--no-break-system-packages"><code>--no-break-system-packages</code></a></dt><dt id="uv-pip-install--no-build"><a href="#uv-pip-install--no-build"><code>--no-build</code></a></dt><dd><p>Don't build source distributions.</p>
<p>When enabled, resolving will not run arbitrary Python code. The cached wheels of already-built source distributions will be reused, but operations that require building distributions will exit with an error.</p>
<p>Alias for <code>--only-binary :all:</code>.</p>
</dd><dt id="uv-pip-install--no-build-isolation"><a href="#uv-pip-install--no-build-isolation"><code>--no-build-isolation</code></a></dt><dd><p>Disable isolation when building source distributions.</p>
<p>Assumes that build dependencies specified by PEP 518 are already installed.</p>
<p>May also be set with the <code>UV_NO_BUILD_ISOLATION</code> environment variable.</p></dd><dt id="uv-pip-install--no-build-isolation-package"><a href="#uv-pip-install--no-build-isolation-package"><code>--no-build-isolation-package</code></a> <i>no-build-isolation-package</i></dt><dd><p>Disable isolation when building source distributions for a specific package.</p>
<p>Assumes that the packages' build dependencies specified by PEP 518 are already installed.</p>
</dd><dt id="uv-pip-install--no-cache"><a href="#uv-pip-install--no-cache"><code>--no-cache</code></a>, <code>--no-cache-dir</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>
<p>May also be set with the <code>UV_NO_CACHE</code> environment variable.</p></dd><dt id="uv-pip-install--no-config"><a href="#uv-pip-install--no-config"><code>--no-config</code></a></dt><dd><p>Avoid discovering configuration files (<code>pyproject.toml</code>, <code>uv.toml</code>).</p>
<p>Normally, configuration files are discovered in the current directory, parent directories, or user configuration directories.</p>
<p>May also be set with the <code>UV_NO_CONFIG</code> environment variable.</p></dd><dt id="uv-pip-install--no-deps"><a href="#uv-pip-install--no-deps"><code>--no-deps</code></a></dt><dd><p>Ignore package dependencies, instead only installing those packages explicitly listed on the command line or in the requirements files</p>
</dd><dt id="uv-pip-install--no-index"><a href="#uv-pip-install--no-index"><code>--no-index</code></a></dt><dd><p>Ignore the registry index (e.g., PyPI), instead relying on direct URL dependencies and those provided via <code>--find-links</code></p>
</dd><dt id="uv-pip-install--no-managed-python"><a href="#uv-pip-install--no-managed-python"><code>--no-managed-python</code></a></dt><dd><p>Disable use of uv-managed Python versions.</p>
<p>Instead, uv will search for a suitable Python version on the system.</p>
<p>May also be set with the <code>UV_NO_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-pip-install--no-progress"><a href="#uv-pip-install--no-progress"><code>--no-progress</code></a></dt><dd><p>Hide all progress outputs.</p>
<p>For example, spinners or progress bars.</p>
<p>May also be set with the <code>UV_NO_PROGRESS</code> environment variable.</p></dd><dt id="uv-pip-install--no-python-downloads"><a href="#uv-pip-install--no-python-downloads"><code>--no-python-downloads</code></a></dt><dd><p>Disable automatic downloads of Python.</p>
</dd><dt id="uv-pip-install--no-sources"><a href="#uv-pip-install--no-sources"><code>--no-sources</code></a></dt><dd><p>Ignore the <code>tool.uv.sources</code> table when resolving dependencies. Used to lock against the standards-compliant, publishable package metadata, as opposed to using any workspace, Git, URL, or local path sources</p>
</dd><dt id="uv-pip-install--no-verify-hashes"><a href="#uv-pip-install--no-verify-hashes"><code>--no-verify-hashes</code></a></dt><dd><p>Disable validation of hashes in the requirements file.</p>
<p>By default, uv will verify any available hashes in the requirements file, but will not require that all requirements have an associated hash. To enforce hash validation, use <code>--require-hashes</code>.</p>
<p>May also be set with the <code>UV_NO_VERIFY_HASHES</code> environment variable.</p></dd><dt id="uv-pip-install--offline"><a href="#uv-pip-install--offline"><code>--offline</code></a></dt><dd><p>Disable network access.</p>
<p>When disabled, uv will only use locally cached data and locally available files.</p>
<p>May also be set with the <code>UV_OFFLINE</code> environment variable.</p></dd><dt id="uv-pip-install--only-binary"><a href="#uv-pip-install--only-binary"><code>--only-binary</code></a> <i>only-binary</i></dt><dd><p>Only use pre-built wheels; don't build source distributions.</p>
<p>When enabled, resolving will not run code from the given packages. The cached wheels of already-built source distributions will be reused, but operations that require building distributions will exit with an error.</p>
<p>Multiple packages may be provided. Disable binaries for all packages with <code>:all:</code>. Clear previously specified packages with <code>:none:</code>.</p>
</dd><dt id="uv-pip-install--overrides"><a href="#uv-pip-install--overrides"><code>--overrides</code></a>, <code>--override</code> <i>overrides</i></dt><dd><p>Override versions using the given requirements files.</p>
<p>Overrides files are <code>requirements.txt</code>-like files that force a specific version of a requirement to be installed, regardless of the requirements declared by any constituent package, and regardless of whether this would be considered an invalid resolution.</p>
<p>While constraints are <em>additive</em>, in that they're combined with the requirements of the constituent packages, overrides are <em>absolute</em>, in that they completely replace the requirements of the constituent packages.</p>
<p>May also be set with the <code>UV_OVERRIDE</code> environment variable.</p></dd><dt id="uv-pip-install--prefix"><a href="#uv-pip-install--prefix"><code>--prefix</code></a> <i>prefix</i></dt><dd><p>Install packages into <code>lib</code>, <code>bin</code>, and other top-level folders under the specified directory, as if a virtual environment were present at that location.</p>
<p>In general, prefer the use of <code>--python</code> to install into an alternate environment, as scripts and other artifacts installed via <code>--prefix</code> will reference the installing interpreter, rather than any interpreter added to the <code>--prefix</code> directory, rendering them non-portable.</p>
</dd><dt id="uv-pip-install--prerelease"><a href="#uv-pip-install--prerelease"><code>--prerelease</code></a> <i>prerelease</i></dt><dd><p>The strategy to use when considering pre-release versions.</p>
<p>By default, uv will accept pre-releases for packages that <em>only</em> publish pre-releases, along with first-party requirements that contain an explicit pre-release marker in the declared specifiers (<code>if-necessary-or-explicit</code>).</p>
<p>May also be set with the <code>UV_PRERELEASE</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>disallow</code>:  Disallow all pre-release versions</li>
<li><code>allow</code>:  Allow all pre-release versions</li>
<li><code>if-necessary</code>:  Allow pre-release versions if all versions of a package are pre-release</li>
<li><code>explicit</code>:  Allow pre-release versions for first-party packages with explicit pre-release markers in their version requirements</li>
<li><code>if-necessary-or-explicit</code>:  Allow pre-release versions if all versions of a package are pre-release, or if the package has an explicit pre-release marker in its version requirements</li>
</ul></dd><dt id="uv-pip-install--project"><a href="#uv-pip-install--project"><code>--project</code></a> <i>project</i></dt><dd><p>Run the command within the given project directory.</p>
<p>All <code>pyproject.toml</code>, <code>uv.toml</code>, and <code>.python-version</code> files will be discovered by walking up the directory tree from the project root, as will the project's virtual environment (<code>.venv</code>).</p>
<p>Other command-line arguments (such as relative paths) will be resolved relative to the current working directory.</p>
<p>See <code>--directory</code> to change the working directory entirely.</p>
<p>This setting has no effect when used in the <code>uv pip</code> interface.</p>
<p>May also be set with the <code>UV_PROJECT</code> environment variable.</p></dd><dt id="uv-pip-install--python"><a href="#uv-pip-install--python"><code>--python</code></a>, <code>-p</code> <i>python</i></dt><dd><p>The Python interpreter into which packages should be installed.</p>
<p>By default, installation requires a virtual environment. A path to an alternative Python can
be provided, but it is only recommended in continuous integration (CI) environments and
should be used with caution, as it can modify the system Python installation.</p>
<p>See <a href="#uv-python">uv python</a> for details on Python discovery and supported request formats.</p>
<p>May also be set with the <code>UV_PYTHON</code> environment variable.</p></dd><dt id="uv-pip-install--python-platform"><a href="#uv-pip-install--python-platform"><code>--python-platform</code></a> <i>python-platform</i></dt><dd><p>The platform for which requirements should be installed.</p>
<p>Represented as a &quot;target triple&quot;, a string that describes the target platform in terms of its CPU, vendor, and operating system name, like <code>x86_64-unknown-linux-gnu</code> or <code>aarch64-apple-darwin</code>.</p>
<p>When targeting macOS (Darwin), the default minimum version is <code>12.0</code>. Use <code>MACOSX_DEPLOYMENT_TARGET</code> to specify a different minimum version, e.g., <code>13.0</code>.</p>
<p>WARNING: When specified, uv will select wheels that are compatible with the <em>target</em> platform; as a result, the installed distributions may not be compatible with the <em>current</em> platform. Conversely, any distributions that are built from source may be incompatible with the <em>target</em> platform, as they will be built for the <em>current</em> platform. The <code>--python-platform</code> option is intended for advanced use cases.</p>
<p>Possible values:</p>
<ul>
<li><code>windows</code>:  An alias for <code>x86_64-pc-windows-msvc</code>, the default target for Windows</li>
<li><code>linux</code>:  An alias for <code>x86_64-unknown-linux-gnu</code>, the default target for Linux</li>
<li><code>macos</code>:  An alias for <code>aarch64-apple-darwin</code>, the default target for macOS</li>
<li><code>x86_64-pc-windows-msvc</code>:  A 64-bit x86 Windows target</li>
<li><code>i686-pc-windows-msvc</code>:  A 32-bit x86 Windows target</li>
<li><code>x86_64-unknown-linux-gnu</code>:  An x86 Linux target. Equivalent to <code>x86_64-manylinux_2_17</code></li>
<li><code>aarch64-apple-darwin</code>:  An ARM-based macOS target, as seen on Apple Silicon devices</li>
<li><code>x86_64-apple-darwin</code>:  An x86 macOS target</li>
<li><code>aarch64-unknown-linux-gnu</code>:  An ARM64 Linux target. Equivalent to <code>aarch64-manylinux_2_17</code></li>
<li><code>aarch64-unknown-linux-musl</code>:  An ARM64 Linux target</li>
<li><code>x86_64-unknown-linux-musl</code>:  An <code>x86_64</code> Linux target</li>
<li><code>x86_64-manylinux2014</code>:  An <code>x86_64</code> target for the <code>manylinux2014</code> platform. Equivalent to <code>x86_64-manylinux_2_17</code></li>
<li><code>x86_64-manylinux_2_17</code>:  An <code>x86_64</code> target for the <code>manylinux_2_17</code> platform</li>
<li><code>x86_64-manylinux_2_28</code>:  An <code>x86_64</code> target for the <code>manylinux_2_28</code> platform</li>
<li><code>x86_64-manylinux_2_31</code>:  An <code>x86_64</code> target for the <code>manylinux_2_31</code> platform</li>
<li><code>x86_64-manylinux_2_32</code>:  An <code>x86_64</code> target for the <code>manylinux_2_32</code> platform</li>
<li><code>x86_64-manylinux_2_33</code>:  An <code>x86_64</code> target for the <code>manylinux_2_33</code> platform</li>
<li><code>x86_64-manylinux_2_34</code>:  An <code>x86_64</code> target for the <code>manylinux_2_34</code> platform</li>
<li><code>x86_64-manylinux_2_35</code>:  An <code>x86_64</code> target for the <code>manylinux_2_35</code> platform</li>
<li><code>x86_64-manylinux_2_36</code>:  An <code>x86_64</code> target for the <code>manylinux_2_36</code> platform</li>
<li><code>x86_64-manylinux_2_37</code>:  An <code>x86_64</code> target for the <code>manylinux_2_37</code> platform</li>
<li><code>x86_64-manylinux_2_38</code>:  An <code>x86_64</code> target for the <code>manylinux_2_38</code> platform</li>
<li><code>x86_64-manylinux_2_39</code>:  An <code>x86_64</code> target for the <code>manylinux_2_39</code> platform</li>
<li><code>x86_64-manylinux_2_40</code>:  An <code>x86_64</code> target for the <code>manylinux_2_40</code> platform</li>
<li><code>aarch64-manylinux2014</code>:  An ARM64 target for the <code>manylinux2014</code> platform. Equivalent to <code>aarch64-manylinux_2_17</code></li>
<li><code>aarch64-manylinux_2_17</code>:  An ARM64 target for the <code>manylinux_2_17</code> platform</li>
<li><code>aarch64-manylinux_2_28</code>:  An ARM64 target for the <code>manylinux_2_28</code> platform</li>
<li><code>aarch64-manylinux_2_31</code>:  An ARM64 target for the <code>manylinux_2_31</code> platform</li>
<li><code>aarch64-manylinux_2_32</code>:  An ARM64 target for the <code>manylinux_2_32</code> platform</li>
<li><code>aarch64-manylinux_2_33</code>:  An ARM64 target for the <code>manylinux_2_33</code> platform</li>
<li><code>aarch64-manylinux_2_34</code>:  An ARM64 target for the <code>manylinux_2_34</code> platform</li>
<li><code>aarch64-manylinux_2_35</code>:  An ARM64 target for the <code>manylinux_2_35</code> platform</li>
<li><code>aarch64-manylinux_2_36</code>:  An ARM64 target for the <code>manylinux_2_36</code> platform</li>
<li><code>aarch64-manylinux_2_37</code>:  An ARM64 target for the <code>manylinux_2_37</code> platform</li>
<li><code>aarch64-manylinux_2_38</code>:  An ARM64 target for the <code>manylinux_2_38</code> platform</li>
<li><code>aarch64-manylinux_2_39</code>:  An ARM64 target for the <code>manylinux_2_39</code> platform</li>
<li><code>aarch64-manylinux_2_40</code>:  An ARM64 target for the <code>manylinux_2_40</code> platform</li>
</ul></dd><dt id="uv-pip-install--python-version"><a href="#uv-pip-install--python-version"><code>--python-version</code></a> <i>python-version</i></dt><dd><p>The minimum Python version that should be supported by the requirements (e.g., <code>3.7</code> or <code>3.7.9</code>).</p>
<p>If a patch version is omitted, the minimum patch version is assumed. For example, <code>3.7</code> is mapped to <code>3.7.0</code>.</p>
</dd><dt id="uv-pip-install--quiet"><a href="#uv-pip-install--quiet"><code>--quiet</code></a>, <code>-q</code></dt><dd><p>Use quiet output.</p>
<p>Repeating this option, e.g., <code>-qq</code>, will enable a silent mode in which uv will write no output to stdout.</p>
</dd><dt id="uv-pip-install--refresh"><a href="#uv-pip-install--refresh"><code>--refresh</code></a></dt><dd><p>Refresh all cached data</p>
</dd><dt id="uv-pip-install--refresh-package"><a href="#uv-pip-install--refresh-package"><code>--refresh-package</code></a> <i>refresh-package</i></dt><dd><p>Refresh cached data for a specific package</p>
</dd><dt id="uv-pip-install--reinstall"><a href="#uv-pip-install--reinstall"><code>--reinstall</code></a>, <code>--force-reinstall</code></dt><dd><p>Reinstall all packages, regardless of whether they're already installed. Implies <code>--refresh</code></p>
</dd><dt id="uv-pip-install--reinstall-package"><a href="#uv-pip-install--reinstall-package"><code>--reinstall-package</code></a> <i>reinstall-package</i></dt><dd><p>Reinstall a specific package, regardless of whether it's already installed. Implies <code>--refresh-package</code></p>
</dd><dt id="uv-pip-install--require-hashes"><a href="#uv-pip-install--require-hashes"><code>--require-hashes</code></a></dt><dd><p>Require a matching hash for each requirement.</p>
<p>By default, uv will verify any available hashes in the requirements file, but will not require that all requirements have an associated hash.</p>
<p>When <code>--require-hashes</code> is enabled, <em>all</em> requirements must include a hash or set of hashes, and <em>all</em> requirements must either be pinned to exact versions (e.g., <code>==1.0.0</code>), or be specified via direct URL.</p>
<p>Hash-checking mode introduces a number of additional constraints:</p>
<ul>
<li>Git dependencies are not supported. - Editable installs are not supported. - Local dependencies are not supported, unless they point to a specific wheel (<code>.whl</code>) or source archive (<code>.zip</code>, <code>.tar.gz</code>), as opposed to a directory.</li>
</ul>
<p>May also be set with the <code>UV_REQUIRE_HASHES</code> environment variable.</p></dd><dt id="uv-pip-install--requirements"><a href="#uv-pip-install--requirements"><code>--requirements</code></a>, <code>--requirement</code>, <code>-r</code> <i>requirements</i></dt><dd><p>Install all packages listed in the given <code>requirements.txt</code> or <code>pylock.toml</code> files.</p>
<p>If a <code>pyproject.toml</code>, <code>setup.py</code>, or <code>setup.cfg</code> file is provided, uv will extract the requirements for the relevant project.</p>
<p>If <code>-</code> is provided, then requirements will be read from stdin.</p>
</dd><dt id="uv-pip-install--resolution"><a href="#uv-pip-install--resolution"><code>--resolution</code></a> <i>resolution</i></dt><dd><p>The strategy to use when selecting between the different compatible versions for a given package requirement.</p>
<p>By default, uv will use the latest compatible version of each package (<code>highest</code>).</p>
<p>May also be set with the <code>UV_RESOLUTION</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>highest</code>:  Resolve the highest compatible version of each package</li>
<li><code>lowest</code>:  Resolve the lowest compatible version of each package</li>
<li><code>lowest-direct</code>:  Resolve the lowest compatible version of any direct dependencies, and the highest compatible version of any transitive dependencies</li>
</ul></dd><dt id="uv-pip-install--strict"><a href="#uv-pip-install--strict"><code>--strict</code></a></dt><dd><p>Validate the Python environment after completing the installation, to detect packages with missing dependencies or other issues</p>
</dd><dt id="uv-pip-install--system"><a href="#uv-pip-install--system"><code>--system</code></a></dt><dd><p>Install packages into the system Python environment.</p>
<p>By default, uv installs into the virtual environment in the current working directory or any parent directory. The <code>--system</code> option instructs uv to instead use the first Python found in the system <code>PATH</code>.</p>
<p>WARNING: <code>--system</code> is intended for use in continuous integration (CI) environments and should be used with caution, as it can modify the system Python installation.</p>
<p>May also be set with the <code>UV_SYSTEM_PYTHON</code> environment variable.</p></dd><dt id="uv-pip-install--target"><a href="#uv-pip-install--target"><code>--target</code></a> <i>target</i></dt><dd><p>Install packages into the specified directory, rather than into the virtual or system Python environment. The packages will be installed at the top-level of the directory</p>
</dd><dt id="uv-pip-install--torch-backend"><a href="#uv-pip-install--torch-backend"><code>--torch-backend</code></a> <i>torch-backend</i></dt><dd><p>The backend to use when fetching packages in the PyTorch ecosystem (e.g., <code>cpu</code>, <code>cu126</code>, or <code>auto</code>)</p>
<p>When set, uv will ignore the configured index URLs for packages in the PyTorch ecosystem, and will instead use the defined backend.</p>
<p>For example, when set to <code>cpu</code>, uv will use the CPU-only PyTorch index; when set to <code>cu126</code>, uv will use the PyTorch index for CUDA 12.6.</p>
<p>The <code>auto</code> mode will attempt to detect the appropriate PyTorch index based on the currently installed CUDA drivers.</p>
<p>This option is in preview and may change in any future release.</p>
<p>May also be set with the <code>UV_TORCH_BACKEND</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>auto</code>:  Select the appropriate PyTorch index based on the operating system and CUDA driver version</li>
<li><code>cpu</code>:  Use the CPU-only PyTorch index</li>
<li><code>cu128</code>:  Use the PyTorch index for CUDA 12.8</li>
<li><code>cu126</code>:  Use the PyTorch index for CUDA 12.6</li>
<li><code>cu125</code>:  Use the PyTorch index for CUDA 12.5</li>
<li><code>cu124</code>:  Use the PyTorch index for CUDA 12.4</li>
<li><code>cu123</code>:  Use the PyTorch index for CUDA 12.3</li>
<li><code>cu122</code>:  Use the PyTorch index for CUDA 12.2</li>
<li><code>cu121</code>:  Use the PyTorch index for CUDA 12.1</li>
<li><code>cu120</code>:  Use the PyTorch index for CUDA 12.0</li>
<li><code>cu118</code>:  Use the PyTorch index for CUDA 11.8</li>
<li><code>cu117</code>:  Use the PyTorch index for CUDA 11.7</li>
<li><code>cu116</code>:  Use the PyTorch index for CUDA 11.6</li>
<li><code>cu115</code>:  Use the PyTorch index for CUDA 11.5</li>
<li><code>cu114</code>:  Use the PyTorch index for CUDA 11.4</li>
<li><code>cu113</code>:  Use the PyTorch index for CUDA 11.3</li>
<li><code>cu112</code>:  Use the PyTorch index for CUDA 11.2</li>
<li><code>cu111</code>:  Use the PyTorch index for CUDA 11.1</li>
<li><code>cu110</code>:  Use the PyTorch index for CUDA 11.0</li>
<li><code>cu102</code>:  Use the PyTorch index for CUDA 10.2</li>
<li><code>cu101</code>:  Use the PyTorch index for CUDA 10.1</li>
<li><code>cu100</code>:  Use the PyTorch index for CUDA 10.0</li>
<li><code>cu92</code>:  Use the PyTorch index for CUDA 9.2</li>
<li><code>cu91</code>:  Use the PyTorch index for CUDA 9.1</li>
<li><code>cu90</code>:  Use the PyTorch index for CUDA 9.0</li>
<li><code>cu80</code>:  Use the PyTorch index for CUDA 8.0</li>
</ul></dd><dt id="uv-pip-install--upgrade"><a href="#uv-pip-install--upgrade"><code>--upgrade</code></a>, <code>-U</code></dt><dd><p>Allow package upgrades, ignoring pinned versions in any existing output file. Implies <code>--refresh</code></p>
</dd><dt id="uv-pip-install--upgrade-package"><a href="#uv-pip-install--upgrade-package"><code>--upgrade-package</code></a>, <code>-P</code> <i>upgrade-package</i></dt><dd><p>Allow upgrades for a specific package, ignoring pinned versions in any existing output file. Implies <code>--refresh-package</code></p>
</dd><dt id="uv-pip-install--user"><a href="#uv-pip-install--user"><code>--user</code></a></dt><dt id="uv-pip-install--verbose"><a href="#uv-pip-install--verbose"><code>--verbose</code></a>, <code>-v</code></dt><dd><p>Use verbose output.</p>
<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (<a href="https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives">https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives</a>)</p>
</dd></dl>

### uv pip uninstall

Uninstall packages from an environment

<h3 class="cli-reference">Usage</h3>

```
uv pip uninstall [OPTIONS] <PACKAGE|--requirements <REQUIREMENTS>>
```

<h3 class="cli-reference">Arguments</h3>

<dl class="cli-reference"><dt id="uv-pip-uninstall--package"><a href="#uv-pip-uninstall--package"<code>PACKAGE</code></a></dt><dd><p>Uninstall all listed packages</p>
</dd></dl>

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt id="uv-pip-uninstall--allow-insecure-host"><a href="#uv-pip-uninstall--allow-insecure-host"><code>--allow-insecure-host</code></a>, <code>--trusted-host</code> <i>allow-insecure-host</i></dt><dd><p>Allow insecure connections to a host.</p>
<p>Can be provided multiple times.</p>
<p>Expects to receive either a hostname (e.g., <code>localhost</code>), a host-port pair (e.g., <code>localhost:8080</code>), or a URL (e.g., <code>https://localhost</code>).</p>
<p>WARNING: Hosts included in this list will not be verified against the system's certificate store. Only use <code>--allow-insecure-host</code> in a secure network with verified sources, as it bypasses SSL verification and could expose you to MITM attacks.</p>
<p>May also be set with the <code>UV_INSECURE_HOST</code> environment variable.</p></dd><dt id="uv-pip-uninstall--break-system-packages"><a href="#uv-pip-uninstall--break-system-packages"><code>--break-system-packages</code></a></dt><dd><p>Allow uv to modify an <code>EXTERNALLY-MANAGED</code> Python installation.</p>
<p>WARNING: <code>--break-system-packages</code> is intended for use in continuous integration (CI) environments, when installing into Python installations that are managed by an external package manager, like <code>apt</code>. It should be used with caution, as such Python installations explicitly recommend against modifications by other package managers (like uv or <code>pip</code>).</p>
<p>May also be set with the <code>UV_BREAK_SYSTEM_PACKAGES</code> environment variable.</p></dd><dt id="uv-pip-uninstall--cache-dir"><a href="#uv-pip-uninstall--cache-dir"><code>--cache-dir</code></a> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>
<p>Defaults to <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on macOS and Linux, and <code>%LOCALAPPDATA%\uv\cache</code> on Windows.</p>
<p>To view the location of the cache directory, run <code>uv cache dir</code>.</p>
<p>May also be set with the <code>UV_CACHE_DIR</code> environment variable.</p></dd><dt id="uv-pip-uninstall--color"><a href="#uv-pip-uninstall--color"><code>--color</code></a> <i>color-choice</i></dt><dd><p>Control the use of color in output.</p>
<p>By default, uv will automatically detect support for colors when writing to a terminal.</p>
<p>Possible values:</p>
<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>
<li><code>always</code>:  Enables colored output regardless of the detected environment</li>
<li><code>never</code>:  Disables colored output</li>
</ul></dd><dt id="uv-pip-uninstall--config-file"><a href="#uv-pip-uninstall--config-file"><code>--config-file</code></a> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>
<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>
<p>May also be set with the <code>UV_CONFIG_FILE</code> environment variable.</p></dd><dt id="uv-pip-uninstall--directory"><a href="#uv-pip-uninstall--directory"><code>--directory</code></a> <i>directory</i></dt><dd><p>Change to the given directory prior to running the command.</p>
<p>Relative paths are resolved with the given directory as the base.</p>
<p>See <code>--project</code> to only change the project root directory.</p>
</dd><dt id="uv-pip-uninstall--dry-run"><a href="#uv-pip-uninstall--dry-run"><code>--dry-run</code></a></dt><dd><p>Perform a dry run, i.e., don't actually uninstall anything but print the resulting plan</p>
</dd><dt id="uv-pip-uninstall--help"><a href="#uv-pip-uninstall--help"><code>--help</code></a>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>
</dd><dt id="uv-pip-uninstall--keyring-provider"><a href="#uv-pip-uninstall--keyring-provider"><code>--keyring-provider</code></a> <i>keyring-provider</i></dt><dd><p>Attempt to use <code>keyring</code> for authentication for remote requirements files.</p>
<p>At present, only <code>--keyring-provider subprocess</code> is supported, which configures uv to use the <code>keyring</code> CLI to handle authentication.</p>
<p>Defaults to <code>disabled</code>.</p>
<p>May also be set with the <code>UV_KEYRING_PROVIDER</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>disabled</code>:  Do not use keyring for credential lookup</li>
<li><code>subprocess</code>:  Use the <code>keyring</code> command for credential lookup</li>
</ul></dd><dt id="uv-pip-uninstall--managed-python"><a href="#uv-pip-uninstall--managed-python"><code>--managed-python</code></a></dt><dd><p>Require use of uv-managed Python versions.</p>
<p>By default, uv prefers using Python versions it manages. However, it will use system Python versions if a uv-managed Python is not installed. This option disables use of system Python versions.</p>
<p>May also be set with the <code>UV_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-pip-uninstall--native-tls"><a href="#uv-pip-uninstall--native-tls"><code>--native-tls</code></a></dt><dd><p>Whether to load TLS certificates from the platform's native certificate store.</p>
<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>
<p>However, in some cases, you may want to use the platform's native certificate store, especially if you're relying on a corporate trust root (e.g., for a mandatory proxy) that's included in your system's certificate store.</p>
<p>May also be set with the <code>UV_NATIVE_TLS</code> environment variable.</p></dd><dt id="uv-pip-uninstall--no-break-system-packages"><a href="#uv-pip-uninstall--no-break-system-packages"><code>--no-break-system-packages</code></a></dt><dt id="uv-pip-uninstall--no-cache"><a href="#uv-pip-uninstall--no-cache"><code>--no-cache</code></a>, <code>--no-cache-dir</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>
<p>May also be set with the <code>UV_NO_CACHE</code> environment variable.</p></dd><dt id="uv-pip-uninstall--no-config"><a href="#uv-pip-uninstall--no-config"><code>--no-config</code></a></dt><dd><p>Avoid discovering configuration files (<code>pyproject.toml</code>, <code>uv.toml</code>).</p>
<p>Normally, configuration files are discovered in the current directory, parent directories, or user configuration directories.</p>
<p>May also be set with the <code>UV_NO_CONFIG</code> environment variable.</p></dd><dt id="uv-pip-uninstall--no-managed-python"><a href="#uv-pip-uninstall--no-managed-python"><code>--no-managed-python</code></a></dt><dd><p>Disable use of uv-managed Python versions.</p>
<p>Instead, uv will search for a suitable Python version on the system.</p>
<p>May also be set with the <code>UV_NO_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-pip-uninstall--no-progress"><a href="#uv-pip-uninstall--no-progress"><code>--no-progress</code></a></dt><dd><p>Hide all progress outputs.</p>
<p>For example, spinners or progress bars.</p>
<p>May also be set with the <code>UV_NO_PROGRESS</code> environment variable.</p></dd><dt id="uv-pip-uninstall--no-python-downloads"><a href="#uv-pip-uninstall--no-python-downloads"><code>--no-python-downloads</code></a></dt><dd><p>Disable automatic downloads of Python.</p>
</dd><dt id="uv-pip-uninstall--offline"><a href="#uv-pip-uninstall--offline"><code>--offline</code></a></dt><dd><p>Disable network access.</p>
<p>When disabled, uv will only use locally cached data and locally available files.</p>
<p>May also be set with the <code>UV_OFFLINE</code> environment variable.</p></dd><dt id="uv-pip-uninstall--prefix"><a href="#uv-pip-uninstall--prefix"><code>--prefix</code></a> <i>prefix</i></dt><dd><p>Uninstall packages from the specified <code>--prefix</code> directory</p>
</dd><dt id="uv-pip-uninstall--project"><a href="#uv-pip-uninstall--project"><code>--project</code></a> <i>project</i></dt><dd><p>Run the command within the given project directory.</p>
<p>All <code>pyproject.toml</code>, <code>uv.toml</code>, and <code>.python-version</code> files will be discovered by walking up the directory tree from the project root, as will the project's virtual environment (<code>.venv</code>).</p>
<p>Other command-line arguments (such as relative paths) will be resolved relative to the current working directory.</p>
<p>See <code>--directory</code> to change the working directory entirely.</p>
<p>This setting has no effect when used in the <code>uv pip</code> interface.</p>
<p>May also be set with the <code>UV_PROJECT</code> environment variable.</p></dd><dt id="uv-pip-uninstall--python"><a href="#uv-pip-uninstall--python"><code>--python</code></a>, <code>-p</code> <i>python</i></dt><dd><p>The Python interpreter from which packages should be uninstalled.</p>
<p>By default, uninstallation requires a virtual environment. A path to an alternative Python
can be provided, but it is only recommended in continuous integration (CI) environments and
should be used with caution, as it can modify the system Python installation.</p>
<p>See <a href="#uv-python">uv python</a> for details on Python discovery and supported request formats.</p>
<p>May also be set with the <code>UV_PYTHON</code> environment variable.</p></dd><dt id="uv-pip-uninstall--quiet"><a href="#uv-pip-uninstall--quiet"><code>--quiet</code></a>, <code>-q</code></dt><dd><p>Use quiet output.</p>
<p>Repeating this option, e.g., <code>-qq</code>, will enable a silent mode in which uv will write no output to stdout.</p>
</dd><dt id="uv-pip-uninstall--requirements"><a href="#uv-pip-uninstall--requirements"><code>--requirements</code></a>, <code>--requirement</code>, <code>-r</code> <i>requirements</i></dt><dd><p>Uninstall all packages listed in the given requirements files</p>
</dd><dt id="uv-pip-uninstall--system"><a href="#uv-pip-uninstall--system"><code>--system</code></a></dt><dd><p>Use the system Python to uninstall packages.</p>
<p>By default, uv uninstalls from the virtual environment in the current working directory or any parent directory. The <code>--system</code> option instructs uv to instead use the first Python found in the system <code>PATH</code>.</p>
<p>WARNING: <code>--system</code> is intended for use in continuous integration (CI) environments and should be used with caution, as it can modify the system Python installation.</p>
<p>May also be set with the <code>UV_SYSTEM_PYTHON</code> environment variable.</p></dd><dt id="uv-pip-uninstall--target"><a href="#uv-pip-uninstall--target"><code>--target</code></a> <i>target</i></dt><dd><p>Uninstall packages from the specified <code>--target</code> directory</p>
</dd><dt id="uv-pip-uninstall--verbose"><a href="#uv-pip-uninstall--verbose"><code>--verbose</code></a>, <code>-v</code></dt><dd><p>Use verbose output.</p>
<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (<a href="https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives">https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives</a>)</p>
</dd></dl>

### uv pip freeze

List, in requirements format, packages installed in an environment

<h3 class="cli-reference">Usage</h3>

```
uv pip freeze [OPTIONS]
```

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt id="uv-pip-freeze--allow-insecure-host"><a href="#uv-pip-freeze--allow-insecure-host"><code>--allow-insecure-host</code></a>, <code>--trusted-host</code> <i>allow-insecure-host</i></dt><dd><p>Allow insecure connections to a host.</p>
<p>Can be provided multiple times.</p>
<p>Expects to receive either a hostname (e.g., <code>localhost</code>), a host-port pair (e.g., <code>localhost:8080</code>), or a URL (e.g., <code>https://localhost</code>).</p>
<p>WARNING: Hosts included in this list will not be verified against the system's certificate store. Only use <code>--allow-insecure-host</code> in a secure network with verified sources, as it bypasses SSL verification and could expose you to MITM attacks.</p>
<p>May also be set with the <code>UV_INSECURE_HOST</code> environment variable.</p></dd><dt id="uv-pip-freeze--cache-dir"><a href="#uv-pip-freeze--cache-dir"><code>--cache-dir</code></a> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>
<p>Defaults to <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on macOS and Linux, and <code>%LOCALAPPDATA%\uv\cache</code> on Windows.</p>
<p>To view the location of the cache directory, run <code>uv cache dir</code>.</p>
<p>May also be set with the <code>UV_CACHE_DIR</code> environment variable.</p></dd><dt id="uv-pip-freeze--color"><a href="#uv-pip-freeze--color"><code>--color</code></a> <i>color-choice</i></dt><dd><p>Control the use of color in output.</p>
<p>By default, uv will automatically detect support for colors when writing to a terminal.</p>
<p>Possible values:</p>
<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>
<li><code>always</code>:  Enables colored output regardless of the detected environment</li>
<li><code>never</code>:  Disables colored output</li>
</ul></dd><dt id="uv-pip-freeze--config-file"><a href="#uv-pip-freeze--config-file"><code>--config-file</code></a> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>
<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>
<p>May also be set with the <code>UV_CONFIG_FILE</code> environment variable.</p></dd><dt id="uv-pip-freeze--directory"><a href="#uv-pip-freeze--directory"><code>--directory</code></a> <i>directory</i></dt><dd><p>Change to the given directory prior to running the command.</p>
<p>Relative paths are resolved with the given directory as the base.</p>
<p>See <code>--project</code> to only change the project root directory.</p>
</dd><dt id="uv-pip-freeze--exclude-editable"><a href="#uv-pip-freeze--exclude-editable"><code>--exclude-editable</code></a></dt><dd><p>Exclude any editable packages from output</p>
</dd><dt id="uv-pip-freeze--help"><a href="#uv-pip-freeze--help"><code>--help</code></a>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>
</dd><dt id="uv-pip-freeze--managed-python"><a href="#uv-pip-freeze--managed-python"><code>--managed-python</code></a></dt><dd><p>Require use of uv-managed Python versions.</p>
<p>By default, uv prefers using Python versions it manages. However, it will use system Python versions if a uv-managed Python is not installed. This option disables use of system Python versions.</p>
<p>May also be set with the <code>UV_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-pip-freeze--native-tls"><a href="#uv-pip-freeze--native-tls"><code>--native-tls</code></a></dt><dd><p>Whether to load TLS certificates from the platform's native certificate store.</p>
<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>
<p>However, in some cases, you may want to use the platform's native certificate store, especially if you're relying on a corporate trust root (e.g., for a mandatory proxy) that's included in your system's certificate store.</p>
<p>May also be set with the <code>UV_NATIVE_TLS</code> environment variable.</p></dd><dt id="uv-pip-freeze--no-cache"><a href="#uv-pip-freeze--no-cache"><code>--no-cache</code></a>, <code>--no-cache-dir</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>
<p>May also be set with the <code>UV_NO_CACHE</code> environment variable.</p></dd><dt id="uv-pip-freeze--no-config"><a href="#uv-pip-freeze--no-config"><code>--no-config</code></a></dt><dd><p>Avoid discovering configuration files (<code>pyproject.toml</code>, <code>uv.toml</code>).</p>
<p>Normally, configuration files are discovered in the current directory, parent directories, or user configuration directories.</p>
<p>May also be set with the <code>UV_NO_CONFIG</code> environment variable.</p></dd><dt id="uv-pip-freeze--no-managed-python"><a href="#uv-pip-freeze--no-managed-python"><code>--no-managed-python</code></a></dt><dd><p>Disable use of uv-managed Python versions.</p>
<p>Instead, uv will search for a suitable Python version on the system.</p>
<p>May also be set with the <code>UV_NO_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-pip-freeze--no-progress"><a href="#uv-pip-freeze--no-progress"><code>--no-progress</code></a></dt><dd><p>Hide all progress outputs.</p>
<p>For example, spinners or progress bars.</p>
<p>May also be set with the <code>UV_NO_PROGRESS</code> environment variable.</p></dd><dt id="uv-pip-freeze--no-python-downloads"><a href="#uv-pip-freeze--no-python-downloads"><code>--no-python-downloads</code></a></dt><dd><p>Disable automatic downloads of Python.</p>
</dd><dt id="uv-pip-freeze--offline"><a href="#uv-pip-freeze--offline"><code>--offline</code></a></dt><dd><p>Disable network access.</p>
<p>When disabled, uv will only use locally cached data and locally available files.</p>
<p>May also be set with the <code>UV_OFFLINE</code> environment variable.</p></dd><dt id="uv-pip-freeze--path"><a href="#uv-pip-freeze--path"><code>--path</code></a> <i>paths</i></dt><dd><p>Restrict to the specified installation path for listing packages (can be used multiple times)</p>
</dd><dt id="uv-pip-freeze--project"><a href="#uv-pip-freeze--project"><code>--project</code></a> <i>project</i></dt><dd><p>Run the command within the given project directory.</p>
<p>All <code>pyproject.toml</code>, <code>uv.toml</code>, and <code>.python-version</code> files will be discovered by walking up the directory tree from the project root, as will the project's virtual environment (<code>.venv</code>).</p>
<p>Other command-line arguments (such as relative paths) will be resolved relative to the current working directory.</p>
<p>See <code>--directory</code> to change the working directory entirely.</p>
<p>This setting has no effect when used in the <code>uv pip</code> interface.</p>
<p>May also be set with the <code>UV_PROJECT</code> environment variable.</p></dd><dt id="uv-pip-freeze--python"><a href="#uv-pip-freeze--python"><code>--python</code></a>, <code>-p</code> <i>python</i></dt><dd><p>The Python interpreter for which packages should be listed.</p>
<p>By default, uv lists packages in a virtual environment but will show packages in a system
Python environment if no virtual environment is found.</p>
<p>See <a href="#uv-python">uv python</a> for details on Python discovery and supported request formats.</p>
<p>May also be set with the <code>UV_PYTHON</code> environment variable.</p></dd><dt id="uv-pip-freeze--quiet"><a href="#uv-pip-freeze--quiet"><code>--quiet</code></a>, <code>-q</code></dt><dd><p>Use quiet output.</p>
<p>Repeating this option, e.g., <code>-qq</code>, will enable a silent mode in which uv will write no output to stdout.</p>
</dd><dt id="uv-pip-freeze--strict"><a href="#uv-pip-freeze--strict"><code>--strict</code></a></dt><dd><p>Validate the Python environment, to detect packages with missing dependencies and other issues</p>
</dd><dt id="uv-pip-freeze--system"><a href="#uv-pip-freeze--system"><code>--system</code></a></dt><dd><p>List packages in the system Python environment.</p>
<p>Disables discovery of virtual environments.</p>
<p>See <a href="#uv-python">uv python</a> for details on Python discovery.</p>
<p>May also be set with the <code>UV_SYSTEM_PYTHON</code> environment variable.</p></dd><dt id="uv-pip-freeze--verbose"><a href="#uv-pip-freeze--verbose"><code>--verbose</code></a>, <code>-v</code></dt><dd><p>Use verbose output.</p>
<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (<a href="https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives">https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives</a>)</p>
</dd></dl>

### uv pip list

List, in tabular format, packages installed in an environment

<h3 class="cli-reference">Usage</h3>

```
uv pip list [OPTIONS]
```

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt id="uv-pip-list--allow-insecure-host"><a href="#uv-pip-list--allow-insecure-host"><code>--allow-insecure-host</code></a>, <code>--trusted-host</code> <i>allow-insecure-host</i></dt><dd><p>Allow insecure connections to a host.</p>
<p>Can be provided multiple times.</p>
<p>Expects to receive either a hostname (e.g., <code>localhost</code>), a host-port pair (e.g., <code>localhost:8080</code>), or a URL (e.g., <code>https://localhost</code>).</p>
<p>WARNING: Hosts included in this list will not be verified against the system's certificate store. Only use <code>--allow-insecure-host</code> in a secure network with verified sources, as it bypasses SSL verification and could expose you to MITM attacks.</p>
<p>May also be set with the <code>UV_INSECURE_HOST</code> environment variable.</p></dd><dt id="uv-pip-list--cache-dir"><a href="#uv-pip-list--cache-dir"><code>--cache-dir</code></a> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>
<p>Defaults to <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on macOS and Linux, and <code>%LOCALAPPDATA%\uv\cache</code> on Windows.</p>
<p>To view the location of the cache directory, run <code>uv cache dir</code>.</p>
<p>May also be set with the <code>UV_CACHE_DIR</code> environment variable.</p></dd><dt id="uv-pip-list--color"><a href="#uv-pip-list--color"><code>--color</code></a> <i>color-choice</i></dt><dd><p>Control the use of color in output.</p>
<p>By default, uv will automatically detect support for colors when writing to a terminal.</p>
<p>Possible values:</p>
<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>
<li><code>always</code>:  Enables colored output regardless of the detected environment</li>
<li><code>never</code>:  Disables colored output</li>
</ul></dd><dt id="uv-pip-list--config-file"><a href="#uv-pip-list--config-file"><code>--config-file</code></a> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>
<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>
<p>May also be set with the <code>UV_CONFIG_FILE</code> environment variable.</p></dd><dt id="uv-pip-list--default-index"><a href="#uv-pip-list--default-index"><code>--default-index</code></a> <i>default-index</i></dt><dd><p>The URL of the default package index (by default: <a href="https://pypi.org/simple">https://pypi.org/simple</a>).</p>
<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>
<p>The index given by this flag is given lower priority than all other indexes specified via the <code>--index</code> flag.</p>
<p>May also be set with the <code>UV_DEFAULT_INDEX</code> environment variable.</p></dd><dt id="uv-pip-list--directory"><a href="#uv-pip-list--directory"><code>--directory</code></a> <i>directory</i></dt><dd><p>Change to the given directory prior to running the command.</p>
<p>Relative paths are resolved with the given directory as the base.</p>
<p>See <code>--project</code> to only change the project root directory.</p>
</dd><dt id="uv-pip-list--editable"><a href="#uv-pip-list--editable"><code>--editable</code></a>, <code>-e</code></dt><dd><p>Only include editable projects</p>
</dd><dt id="uv-pip-list--exclude"><a href="#uv-pip-list--exclude"><code>--exclude</code></a> <i>exclude</i></dt><dd><p>Exclude the specified package(s) from the output</p>
</dd><dt id="uv-pip-list--exclude-editable"><a href="#uv-pip-list--exclude-editable"><code>--exclude-editable</code></a></dt><dd><p>Exclude any editable packages from output</p>
</dd><dt id="uv-pip-list--exclude-newer"><a href="#uv-pip-list--exclude-newer"><code>--exclude-newer</code></a> <i>exclude-newer</i></dt><dd><p>Limit candidate packages to those that were uploaded prior to the given date.</p>
<p>Accepts both RFC 3339 timestamps (e.g., <code>2006-12-02T02:07:43Z</code>) and local dates in the same format (e.g., <code>2006-12-02</code>) in your system's configured time zone.</p>
<p>May also be set with the <code>UV_EXCLUDE_NEWER</code> environment variable.</p></dd><dt id="uv-pip-list--extra-index-url"><a href="#uv-pip-list--extra-index-url"><code>--extra-index-url</code></a> <i>extra-index-url</i></dt><dd><p>(Deprecated: use <code>--index</code> instead) Extra URLs of package indexes to use, in addition to <code>--index-url</code>.</p>
<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>
<p>All indexes provided via this flag take priority over the index specified by <code>--index-url</code> (which defaults to PyPI). When multiple <code>--extra-index-url</code> flags are provided, earlier values take priority.</p>
<p>May also be set with the <code>UV_EXTRA_INDEX_URL</code> environment variable.</p></dd><dt id="uv-pip-list--find-links"><a href="#uv-pip-list--find-links"><code>--find-links</code></a>, <code>-f</code> <i>find-links</i></dt><dd><p>Locations to search for candidate distributions, in addition to those found in the registry indexes.</p>
<p>If a path, the target must be a directory that contains packages as wheel files (<code>.whl</code>) or source distributions (e.g., <code>.tar.gz</code> or <code>.zip</code>) at the top level.</p>
<p>If a URL, the page must contain a flat list of links to package files adhering to the formats described above.</p>
<p>May also be set with the <code>UV_FIND_LINKS</code> environment variable.</p></dd><dt id="uv-pip-list--format"><a href="#uv-pip-list--format"><code>--format</code></a> <i>format</i></dt><dd><p>Select the output format</p>
<p>[default: columns]</p><p>Possible values:</p>
<ul>
<li><code>columns</code>:  Display the list of packages in a human-readable table</li>
<li><code>freeze</code>:  Display the list of packages in a <code>pip freeze</code>-like format, with one package per line alongside its version</li>
<li><code>json</code>:  Display the list of packages in a machine-readable JSON format</li>
</ul></dd><dt id="uv-pip-list--help"><a href="#uv-pip-list--help"><code>--help</code></a>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>
</dd><dt id="uv-pip-list--index"><a href="#uv-pip-list--index"><code>--index</code></a> <i>index</i></dt><dd><p>The URLs to use when resolving dependencies, in addition to the default index.</p>
<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>
<p>All indexes provided via this flag take priority over the index specified by <code>--default-index</code> (which defaults to PyPI). When multiple <code>--index</code> flags are provided, earlier values take priority.</p>
<p>May also be set with the <code>UV_INDEX</code> environment variable.</p></dd><dt id="uv-pip-list--index-strategy"><a href="#uv-pip-list--index-strategy"><code>--index-strategy</code></a> <i>index-strategy</i></dt><dd><p>The strategy to use when resolving against multiple index URLs.</p>
<p>By default, uv will stop at the first index on which a given package is available, and limit resolutions to those present on that first index (<code>first-index</code>). This prevents &quot;dependency confusion&quot; attacks, whereby an attacker can upload a malicious package under the same name to an alternate index.</p>
<p>May also be set with the <code>UV_INDEX_STRATEGY</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>first-index</code>:  Only use results from the first index that returns a match for a given package name</li>
<li><code>unsafe-first-match</code>:  Search for every package name across all indexes, exhausting the versions from the first index before moving on to the next</li>
<li><code>unsafe-best-match</code>:  Search for every package name across all indexes, preferring the &quot;best&quot; version found. If a package version is in multiple indexes, only look at the entry for the first index</li>
</ul></dd><dt id="uv-pip-list--index-url"><a href="#uv-pip-list--index-url"><code>--index-url</code></a>, <code>-i</code> <i>index-url</i></dt><dd><p>(Deprecated: use <code>--default-index</code> instead) The URL of the Python package index (by default: <a href="https://pypi.org/simple">https://pypi.org/simple</a>).</p>
<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>
<p>The index given by this flag is given lower priority than all other indexes specified via the <code>--extra-index-url</code> flag.</p>
<p>May also be set with the <code>UV_INDEX_URL</code> environment variable.</p></dd><dt id="uv-pip-list--keyring-provider"><a href="#uv-pip-list--keyring-provider"><code>--keyring-provider</code></a> <i>keyring-provider</i></dt><dd><p>Attempt to use <code>keyring</code> for authentication for index URLs.</p>
<p>At present, only <code>--keyring-provider subprocess</code> is supported, which configures uv to use the <code>keyring</code> CLI to handle authentication.</p>
<p>Defaults to <code>disabled</code>.</p>
<p>May also be set with the <code>UV_KEYRING_PROVIDER</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>disabled</code>:  Do not use keyring for credential lookup</li>
<li><code>subprocess</code>:  Use the <code>keyring</code> command for credential lookup</li>
</ul></dd><dt id="uv-pip-list--managed-python"><a href="#uv-pip-list--managed-python"><code>--managed-python</code></a></dt><dd><p>Require use of uv-managed Python versions.</p>
<p>By default, uv prefers using Python versions it manages. However, it will use system Python versions if a uv-managed Python is not installed. This option disables use of system Python versions.</p>
<p>May also be set with the <code>UV_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-pip-list--native-tls"><a href="#uv-pip-list--native-tls"><code>--native-tls</code></a></dt><dd><p>Whether to load TLS certificates from the platform's native certificate store.</p>
<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>
<p>However, in some cases, you may want to use the platform's native certificate store, especially if you're relying on a corporate trust root (e.g., for a mandatory proxy) that's included in your system's certificate store.</p>
<p>May also be set with the <code>UV_NATIVE_TLS</code> environment variable.</p></dd><dt id="uv-pip-list--no-cache"><a href="#uv-pip-list--no-cache"><code>--no-cache</code></a>, <code>--no-cache-dir</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>
<p>May also be set with the <code>UV_NO_CACHE</code> environment variable.</p></dd><dt id="uv-pip-list--no-config"><a href="#uv-pip-list--no-config"><code>--no-config</code></a></dt><dd><p>Avoid discovering configuration files (<code>pyproject.toml</code>, <code>uv.toml</code>).</p>
<p>Normally, configuration files are discovered in the current directory, parent directories, or user configuration directories.</p>
<p>May also be set with the <code>UV_NO_CONFIG</code> environment variable.</p></dd><dt id="uv-pip-list--no-index"><a href="#uv-pip-list--no-index"><code>--no-index</code></a></dt><dd><p>Ignore the registry index (e.g., PyPI), instead relying on direct URL dependencies and those provided via <code>--find-links</code></p>
</dd><dt id="uv-pip-list--no-managed-python"><a href="#uv-pip-list--no-managed-python"><code>--no-managed-python</code></a></dt><dd><p>Disable use of uv-managed Python versions.</p>
<p>Instead, uv will search for a suitable Python version on the system.</p>
<p>May also be set with the <code>UV_NO_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-pip-list--no-progress"><a href="#uv-pip-list--no-progress"><code>--no-progress</code></a></dt><dd><p>Hide all progress outputs.</p>
<p>For example, spinners or progress bars.</p>
<p>May also be set with the <code>UV_NO_PROGRESS</code> environment variable.</p></dd><dt id="uv-pip-list--no-python-downloads"><a href="#uv-pip-list--no-python-downloads"><code>--no-python-downloads</code></a></dt><dd><p>Disable automatic downloads of Python.</p>
</dd><dt id="uv-pip-list--offline"><a href="#uv-pip-list--offline"><code>--offline</code></a></dt><dd><p>Disable network access.</p>
<p>When disabled, uv will only use locally cached data and locally available files.</p>
<p>May also be set with the <code>UV_OFFLINE</code> environment variable.</p></dd><dt id="uv-pip-list--outdated"><a href="#uv-pip-list--outdated"><code>--outdated</code></a></dt><dd><p>List outdated packages.</p>
<p>The latest version of each package will be shown alongside the installed version. Up-to-date packages will be omitted from the output.</p>
</dd><dt id="uv-pip-list--project"><a href="#uv-pip-list--project"><code>--project</code></a> <i>project</i></dt><dd><p>Run the command within the given project directory.</p>
<p>All <code>pyproject.toml</code>, <code>uv.toml</code>, and <code>.python-version</code> files will be discovered by walking up the directory tree from the project root, as will the project's virtual environment (<code>.venv</code>).</p>
<p>Other command-line arguments (such as relative paths) will be resolved relative to the current working directory.</p>
<p>See <code>--directory</code> to change the working directory entirely.</p>
<p>This setting has no effect when used in the <code>uv pip</code> interface.</p>
<p>May also be set with the <code>UV_PROJECT</code> environment variable.</p></dd><dt id="uv-pip-list--python"><a href="#uv-pip-list--python"><code>--python</code></a>, <code>-p</code> <i>python</i></dt><dd><p>The Python interpreter for which packages should be listed.</p>
<p>By default, uv lists packages in a virtual environment but will show packages in a system
Python environment if no virtual environment is found.</p>
<p>See <a href="#uv-python">uv python</a> for details on Python discovery and supported request formats.</p>
<p>May also be set with the <code>UV_PYTHON</code> environment variable.</p></dd><dt id="uv-pip-list--quiet"><a href="#uv-pip-list--quiet"><code>--quiet</code></a>, <code>-q</code></dt><dd><p>Use quiet output.</p>
<p>Repeating this option, e.g., <code>-qq</code>, will enable a silent mode in which uv will write no output to stdout.</p>
</dd><dt id="uv-pip-list--strict"><a href="#uv-pip-list--strict"><code>--strict</code></a></dt><dd><p>Validate the Python environment, to detect packages with missing dependencies and other issues</p>
</dd><dt id="uv-pip-list--system"><a href="#uv-pip-list--system"><code>--system</code></a></dt><dd><p>List packages in the system Python environment.</p>
<p>Disables discovery of virtual environments.</p>
<p>See <a href="#uv-python">uv python</a> for details on Python discovery.</p>
<p>May also be set with the <code>UV_SYSTEM_PYTHON</code> environment variable.</p></dd><dt id="uv-pip-list--verbose"><a href="#uv-pip-list--verbose"><code>--verbose</code></a>, <code>-v</code></dt><dd><p>Use verbose output.</p>
<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (<a href="https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives">https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives</a>)</p>
</dd></dl>

### uv pip show

Show information about one or more installed packages

<h3 class="cli-reference">Usage</h3>

```
uv pip show [OPTIONS] [PACKAGE]...
```

<h3 class="cli-reference">Arguments</h3>

<dl class="cli-reference"><dt id="uv-pip-show--package"><a href="#uv-pip-show--package"<code>PACKAGE</code></a></dt><dd><p>The package(s) to display</p>
</dd></dl>

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt id="uv-pip-show--allow-insecure-host"><a href="#uv-pip-show--allow-insecure-host"><code>--allow-insecure-host</code></a>, <code>--trusted-host</code> <i>allow-insecure-host</i></dt><dd><p>Allow insecure connections to a host.</p>
<p>Can be provided multiple times.</p>
<p>Expects to receive either a hostname (e.g., <code>localhost</code>), a host-port pair (e.g., <code>localhost:8080</code>), or a URL (e.g., <code>https://localhost</code>).</p>
<p>WARNING: Hosts included in this list will not be verified against the system's certificate store. Only use <code>--allow-insecure-host</code> in a secure network with verified sources, as it bypasses SSL verification and could expose you to MITM attacks.</p>
<p>May also be set with the <code>UV_INSECURE_HOST</code> environment variable.</p></dd><dt id="uv-pip-show--cache-dir"><a href="#uv-pip-show--cache-dir"><code>--cache-dir</code></a> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>
<p>Defaults to <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on macOS and Linux, and <code>%LOCALAPPDATA%\uv\cache</code> on Windows.</p>
<p>To view the location of the cache directory, run <code>uv cache dir</code>.</p>
<p>May also be set with the <code>UV_CACHE_DIR</code> environment variable.</p></dd><dt id="uv-pip-show--color"><a href="#uv-pip-show--color"><code>--color</code></a> <i>color-choice</i></dt><dd><p>Control the use of color in output.</p>
<p>By default, uv will automatically detect support for colors when writing to a terminal.</p>
<p>Possible values:</p>
<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>
<li><code>always</code>:  Enables colored output regardless of the detected environment</li>
<li><code>never</code>:  Disables colored output</li>
</ul></dd><dt id="uv-pip-show--config-file"><a href="#uv-pip-show--config-file"><code>--config-file</code></a> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>
<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>
<p>May also be set with the <code>UV_CONFIG_FILE</code> environment variable.</p></dd><dt id="uv-pip-show--directory"><a href="#uv-pip-show--directory"><code>--directory</code></a> <i>directory</i></dt><dd><p>Change to the given directory prior to running the command.</p>
<p>Relative paths are resolved with the given directory as the base.</p>
<p>See <code>--project</code> to only change the project root directory.</p>
</dd><dt id="uv-pip-show--files"><a href="#uv-pip-show--files"><code>--files</code></a>, <code>-f</code></dt><dd><p>Show the full list of installed files for each package</p>
</dd><dt id="uv-pip-show--help"><a href="#uv-pip-show--help"><code>--help</code></a>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>
</dd><dt id="uv-pip-show--managed-python"><a href="#uv-pip-show--managed-python"><code>--managed-python</code></a></dt><dd><p>Require use of uv-managed Python versions.</p>
<p>By default, uv prefers using Python versions it manages. However, it will use system Python versions if a uv-managed Python is not installed. This option disables use of system Python versions.</p>
<p>May also be set with the <code>UV_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-pip-show--native-tls"><a href="#uv-pip-show--native-tls"><code>--native-tls</code></a></dt><dd><p>Whether to load TLS certificates from the platform's native certificate store.</p>
<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>
<p>However, in some cases, you may want to use the platform's native certificate store, especially if you're relying on a corporate trust root (e.g., for a mandatory proxy) that's included in your system's certificate store.</p>
<p>May also be set with the <code>UV_NATIVE_TLS</code> environment variable.</p></dd><dt id="uv-pip-show--no-cache"><a href="#uv-pip-show--no-cache"><code>--no-cache</code></a>, <code>--no-cache-dir</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>
<p>May also be set with the <code>UV_NO_CACHE</code> environment variable.</p></dd><dt id="uv-pip-show--no-config"><a href="#uv-pip-show--no-config"><code>--no-config</code></a></dt><dd><p>Avoid discovering configuration files (<code>pyproject.toml</code>, <code>uv.toml</code>).</p>
<p>Normally, configuration files are discovered in the current directory, parent directories, or user configuration directories.</p>
<p>May also be set with the <code>UV_NO_CONFIG</code> environment variable.</p></dd><dt id="uv-pip-show--no-managed-python"><a href="#uv-pip-show--no-managed-python"><code>--no-managed-python</code></a></dt><dd><p>Disable use of uv-managed Python versions.</p>
<p>Instead, uv will search for a suitable Python version on the system.</p>
<p>May also be set with the <code>UV_NO_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-pip-show--no-progress"><a href="#uv-pip-show--no-progress"><code>--no-progress</code></a></dt><dd><p>Hide all progress outputs.</p>
<p>For example, spinners or progress bars.</p>
<p>May also be set with the <code>UV_NO_PROGRESS</code> environment variable.</p></dd><dt id="uv-pip-show--no-python-downloads"><a href="#uv-pip-show--no-python-downloads"><code>--no-python-downloads</code></a></dt><dd><p>Disable automatic downloads of Python.</p>
</dd><dt id="uv-pip-show--offline"><a href="#uv-pip-show--offline"><code>--offline</code></a></dt><dd><p>Disable network access.</p>
<p>When disabled, uv will only use locally cached data and locally available files.</p>
<p>May also be set with the <code>UV_OFFLINE</code> environment variable.</p></dd><dt id="uv-pip-show--project"><a href="#uv-pip-show--project"><code>--project</code></a> <i>project</i></dt><dd><p>Run the command within the given project directory.</p>
<p>All <code>pyproject.toml</code>, <code>uv.toml</code>, and <code>.python-version</code> files will be discovered by walking up the directory tree from the project root, as will the project's virtual environment (<code>.venv</code>).</p>
<p>Other command-line arguments (such as relative paths) will be resolved relative to the current working directory.</p>
<p>See <code>--directory</code> to change the working directory entirely.</p>
<p>This setting has no effect when used in the <code>uv pip</code> interface.</p>
<p>May also be set with the <code>UV_PROJECT</code> environment variable.</p></dd><dt id="uv-pip-show--python"><a href="#uv-pip-show--python"><code>--python</code></a>, <code>-p</code> <i>python</i></dt><dd><p>The Python interpreter to find the package in.</p>
<p>By default, uv looks for packages in a virtual environment but will look for packages in a
system Python environment if no virtual environment is found.</p>
<p>See <a href="#uv-python">uv python</a> for details on Python discovery and supported request formats.</p>
<p>May also be set with the <code>UV_PYTHON</code> environment variable.</p></dd><dt id="uv-pip-show--quiet"><a href="#uv-pip-show--quiet"><code>--quiet</code></a>, <code>-q</code></dt><dd><p>Use quiet output.</p>
<p>Repeating this option, e.g., <code>-qq</code>, will enable a silent mode in which uv will write no output to stdout.</p>
</dd><dt id="uv-pip-show--strict"><a href="#uv-pip-show--strict"><code>--strict</code></a></dt><dd><p>Validate the Python environment, to detect packages with missing dependencies and other issues</p>
</dd><dt id="uv-pip-show--system"><a href="#uv-pip-show--system"><code>--system</code></a></dt><dd><p>Show a package in the system Python environment.</p>
<p>Disables discovery of virtual environments.</p>
<p>See <a href="#uv-python">uv python</a> for details on Python discovery.</p>
<p>May also be set with the <code>UV_SYSTEM_PYTHON</code> environment variable.</p></dd><dt id="uv-pip-show--verbose"><a href="#uv-pip-show--verbose"><code>--verbose</code></a>, <code>-v</code></dt><dd><p>Use verbose output.</p>
<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (<a href="https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives">https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives</a>)</p>
</dd></dl>

### uv pip tree

Display the dependency tree for an environment

<h3 class="cli-reference">Usage</h3>

```
uv pip tree [OPTIONS]
```

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt id="uv-pip-tree--allow-insecure-host"><a href="#uv-pip-tree--allow-insecure-host"><code>--allow-insecure-host</code></a>, <code>--trusted-host</code> <i>allow-insecure-host</i></dt><dd><p>Allow insecure connections to a host.</p>
<p>Can be provided multiple times.</p>
<p>Expects to receive either a hostname (e.g., <code>localhost</code>), a host-port pair (e.g., <code>localhost:8080</code>), or a URL (e.g., <code>https://localhost</code>).</p>
<p>WARNING: Hosts included in this list will not be verified against the system's certificate store. Only use <code>--allow-insecure-host</code> in a secure network with verified sources, as it bypasses SSL verification and could expose you to MITM attacks.</p>
<p>May also be set with the <code>UV_INSECURE_HOST</code> environment variable.</p></dd><dt id="uv-pip-tree--cache-dir"><a href="#uv-pip-tree--cache-dir"><code>--cache-dir</code></a> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>
<p>Defaults to <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on macOS and Linux, and <code>%LOCALAPPDATA%\uv\cache</code> on Windows.</p>
<p>To view the location of the cache directory, run <code>uv cache dir</code>.</p>
<p>May also be set with the <code>UV_CACHE_DIR</code> environment variable.</p></dd><dt id="uv-pip-tree--color"><a href="#uv-pip-tree--color"><code>--color</code></a> <i>color-choice</i></dt><dd><p>Control the use of color in output.</p>
<p>By default, uv will automatically detect support for colors when writing to a terminal.</p>
<p>Possible values:</p>
<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>
<li><code>always</code>:  Enables colored output regardless of the detected environment</li>
<li><code>never</code>:  Disables colored output</li>
</ul></dd><dt id="uv-pip-tree--config-file"><a href="#uv-pip-tree--config-file"><code>--config-file</code></a> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>
<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>
<p>May also be set with the <code>UV_CONFIG_FILE</code> environment variable.</p></dd><dt id="uv-pip-tree--default-index"><a href="#uv-pip-tree--default-index"><code>--default-index</code></a> <i>default-index</i></dt><dd><p>The URL of the default package index (by default: <a href="https://pypi.org/simple">https://pypi.org/simple</a>).</p>
<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>
<p>The index given by this flag is given lower priority than all other indexes specified via the <code>--index</code> flag.</p>
<p>May also be set with the <code>UV_DEFAULT_INDEX</code> environment variable.</p></dd><dt id="uv-pip-tree--depth"><a href="#uv-pip-tree--depth"><code>--depth</code></a>, <code>-d</code> <i>depth</i></dt><dd><p>Maximum display depth of the dependency tree</p>
<p>[default: 255]</p></dd><dt id="uv-pip-tree--directory"><a href="#uv-pip-tree--directory"><code>--directory</code></a> <i>directory</i></dt><dd><p>Change to the given directory prior to running the command.</p>
<p>Relative paths are resolved with the given directory as the base.</p>
<p>See <code>--project</code> to only change the project root directory.</p>
</dd><dt id="uv-pip-tree--exclude-newer"><a href="#uv-pip-tree--exclude-newer"><code>--exclude-newer</code></a> <i>exclude-newer</i></dt><dd><p>Limit candidate packages to those that were uploaded prior to the given date.</p>
<p>Accepts both RFC 3339 timestamps (e.g., <code>2006-12-02T02:07:43Z</code>) and local dates in the same format (e.g., <code>2006-12-02</code>) in your system's configured time zone.</p>
<p>May also be set with the <code>UV_EXCLUDE_NEWER</code> environment variable.</p></dd><dt id="uv-pip-tree--extra-index-url"><a href="#uv-pip-tree--extra-index-url"><code>--extra-index-url</code></a> <i>extra-index-url</i></dt><dd><p>(Deprecated: use <code>--index</code> instead) Extra URLs of package indexes to use, in addition to <code>--index-url</code>.</p>
<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>
<p>All indexes provided via this flag take priority over the index specified by <code>--index-url</code> (which defaults to PyPI). When multiple <code>--extra-index-url</code> flags are provided, earlier values take priority.</p>
<p>May also be set with the <code>UV_EXTRA_INDEX_URL</code> environment variable.</p></dd><dt id="uv-pip-tree--find-links"><a href="#uv-pip-tree--find-links"><code>--find-links</code></a>, <code>-f</code> <i>find-links</i></dt><dd><p>Locations to search for candidate distributions, in addition to those found in the registry indexes.</p>
<p>If a path, the target must be a directory that contains packages as wheel files (<code>.whl</code>) or source distributions (e.g., <code>.tar.gz</code> or <code>.zip</code>) at the top level.</p>
<p>If a URL, the page must contain a flat list of links to package files adhering to the formats described above.</p>
<p>May also be set with the <code>UV_FIND_LINKS</code> environment variable.</p></dd><dt id="uv-pip-tree--help"><a href="#uv-pip-tree--help"><code>--help</code></a>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>
</dd><dt id="uv-pip-tree--index"><a href="#uv-pip-tree--index"><code>--index</code></a> <i>index</i></dt><dd><p>The URLs to use when resolving dependencies, in addition to the default index.</p>
<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>
<p>All indexes provided via this flag take priority over the index specified by <code>--default-index</code> (which defaults to PyPI). When multiple <code>--index</code> flags are provided, earlier values take priority.</p>
<p>May also be set with the <code>UV_INDEX</code> environment variable.</p></dd><dt id="uv-pip-tree--index-strategy"><a href="#uv-pip-tree--index-strategy"><code>--index-strategy</code></a> <i>index-strategy</i></dt><dd><p>The strategy to use when resolving against multiple index URLs.</p>
<p>By default, uv will stop at the first index on which a given package is available, and limit resolutions to those present on that first index (<code>first-index</code>). This prevents &quot;dependency confusion&quot; attacks, whereby an attacker can upload a malicious package under the same name to an alternate index.</p>
<p>May also be set with the <code>UV_INDEX_STRATEGY</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>first-index</code>:  Only use results from the first index that returns a match for a given package name</li>
<li><code>unsafe-first-match</code>:  Search for every package name across all indexes, exhausting the versions from the first index before moving on to the next</li>
<li><code>unsafe-best-match</code>:  Search for every package name across all indexes, preferring the &quot;best&quot; version found. If a package version is in multiple indexes, only look at the entry for the first index</li>
</ul></dd><dt id="uv-pip-tree--index-url"><a href="#uv-pip-tree--index-url"><code>--index-url</code></a>, <code>-i</code> <i>index-url</i></dt><dd><p>(Deprecated: use <code>--default-index</code> instead) The URL of the Python package index (by default: <a href="https://pypi.org/simple">https://pypi.org/simple</a>).</p>
<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>
<p>The index given by this flag is given lower priority than all other indexes specified via the <code>--extra-index-url</code> flag.</p>
<p>May also be set with the <code>UV_INDEX_URL</code> environment variable.</p></dd><dt id="uv-pip-tree--invert"><a href="#uv-pip-tree--invert"><code>--invert</code></a>, <code>--reverse</code></dt><dd><p>Show the reverse dependencies for the given package. This flag will invert the tree and display the packages that depend on the given package</p>
</dd><dt id="uv-pip-tree--keyring-provider"><a href="#uv-pip-tree--keyring-provider"><code>--keyring-provider</code></a> <i>keyring-provider</i></dt><dd><p>Attempt to use <code>keyring</code> for authentication for index URLs.</p>
<p>At present, only <code>--keyring-provider subprocess</code> is supported, which configures uv to use the <code>keyring</code> CLI to handle authentication.</p>
<p>Defaults to <code>disabled</code>.</p>
<p>May also be set with the <code>UV_KEYRING_PROVIDER</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>disabled</code>:  Do not use keyring for credential lookup</li>
<li><code>subprocess</code>:  Use the <code>keyring</code> command for credential lookup</li>
</ul></dd><dt id="uv-pip-tree--managed-python"><a href="#uv-pip-tree--managed-python"><code>--managed-python</code></a></dt><dd><p>Require use of uv-managed Python versions.</p>
<p>By default, uv prefers using Python versions it manages. However, it will use system Python versions if a uv-managed Python is not installed. This option disables use of system Python versions.</p>
<p>May also be set with the <code>UV_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-pip-tree--native-tls"><a href="#uv-pip-tree--native-tls"><code>--native-tls</code></a></dt><dd><p>Whether to load TLS certificates from the platform's native certificate store.</p>
<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>
<p>However, in some cases, you may want to use the platform's native certificate store, especially if you're relying on a corporate trust root (e.g., for a mandatory proxy) that's included in your system's certificate store.</p>
<p>May also be set with the <code>UV_NATIVE_TLS</code> environment variable.</p></dd><dt id="uv-pip-tree--no-cache"><a href="#uv-pip-tree--no-cache"><code>--no-cache</code></a>, <code>--no-cache-dir</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>
<p>May also be set with the <code>UV_NO_CACHE</code> environment variable.</p></dd><dt id="uv-pip-tree--no-config"><a href="#uv-pip-tree--no-config"><code>--no-config</code></a></dt><dd><p>Avoid discovering configuration files (<code>pyproject.toml</code>, <code>uv.toml</code>).</p>
<p>Normally, configuration files are discovered in the current directory, parent directories, or user configuration directories.</p>
<p>May also be set with the <code>UV_NO_CONFIG</code> environment variable.</p></dd><dt id="uv-pip-tree--no-dedupe"><a href="#uv-pip-tree--no-dedupe"><code>--no-dedupe</code></a></dt><dd><p>Do not de-duplicate repeated dependencies. Usually, when a package has already displayed its dependencies, further occurrences will not re-display its dependencies, and will include a (*) to indicate it has already been shown. This flag will cause those duplicates to be repeated</p>
</dd><dt id="uv-pip-tree--no-index"><a href="#uv-pip-tree--no-index"><code>--no-index</code></a></dt><dd><p>Ignore the registry index (e.g., PyPI), instead relying on direct URL dependencies and those provided via <code>--find-links</code></p>
</dd><dt id="uv-pip-tree--no-managed-python"><a href="#uv-pip-tree--no-managed-python"><code>--no-managed-python</code></a></dt><dd><p>Disable use of uv-managed Python versions.</p>
<p>Instead, uv will search for a suitable Python version on the system.</p>
<p>May also be set with the <code>UV_NO_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-pip-tree--no-progress"><a href="#uv-pip-tree--no-progress"><code>--no-progress</code></a></dt><dd><p>Hide all progress outputs.</p>
<p>For example, spinners or progress bars.</p>
<p>May also be set with the <code>UV_NO_PROGRESS</code> environment variable.</p></dd><dt id="uv-pip-tree--no-python-downloads"><a href="#uv-pip-tree--no-python-downloads"><code>--no-python-downloads</code></a></dt><dd><p>Disable automatic downloads of Python.</p>
</dd><dt id="uv-pip-tree--offline"><a href="#uv-pip-tree--offline"><code>--offline</code></a></dt><dd><p>Disable network access.</p>
<p>When disabled, uv will only use locally cached data and locally available files.</p>
<p>May also be set with the <code>UV_OFFLINE</code> environment variable.</p></dd><dt id="uv-pip-tree--outdated"><a href="#uv-pip-tree--outdated"><code>--outdated</code></a></dt><dd><p>Show the latest available version of each package in the tree</p>
</dd><dt id="uv-pip-tree--package"><a href="#uv-pip-tree--package"><code>--package</code></a> <i>package</i></dt><dd><p>Display only the specified packages</p>
</dd><dt id="uv-pip-tree--project"><a href="#uv-pip-tree--project"><code>--project</code></a> <i>project</i></dt><dd><p>Run the command within the given project directory.</p>
<p>All <code>pyproject.toml</code>, <code>uv.toml</code>, and <code>.python-version</code> files will be discovered by walking up the directory tree from the project root, as will the project's virtual environment (<code>.venv</code>).</p>
<p>Other command-line arguments (such as relative paths) will be resolved relative to the current working directory.</p>
<p>See <code>--directory</code> to change the working directory entirely.</p>
<p>This setting has no effect when used in the <code>uv pip</code> interface.</p>
<p>May also be set with the <code>UV_PROJECT</code> environment variable.</p></dd><dt id="uv-pip-tree--prune"><a href="#uv-pip-tree--prune"><code>--prune</code></a> <i>prune</i></dt><dd><p>Prune the given package from the display of the dependency tree</p>
</dd><dt id="uv-pip-tree--python"><a href="#uv-pip-tree--python"><code>--python</code></a>, <code>-p</code> <i>python</i></dt><dd><p>The Python interpreter for which packages should be listed.</p>
<p>By default, uv lists packages in a virtual environment but will show packages in a system
Python environment if no virtual environment is found.</p>
<p>See <a href="#uv-python">uv python</a> for details on Python discovery and supported request formats.</p>
<p>May also be set with the <code>UV_PYTHON</code> environment variable.</p></dd><dt id="uv-pip-tree--quiet"><a href="#uv-pip-tree--quiet"><code>--quiet</code></a>, <code>-q</code></dt><dd><p>Use quiet output.</p>
<p>Repeating this option, e.g., <code>-qq</code>, will enable a silent mode in which uv will write no output to stdout.</p>
</dd><dt id="uv-pip-tree--show-version-specifiers"><a href="#uv-pip-tree--show-version-specifiers"><code>--show-version-specifiers</code></a></dt><dd><p>Show the version constraint(s) imposed on each package</p>
</dd><dt id="uv-pip-tree--strict"><a href="#uv-pip-tree--strict"><code>--strict</code></a></dt><dd><p>Validate the Python environment, to detect packages with missing dependencies and other issues</p>
</dd><dt id="uv-pip-tree--system"><a href="#uv-pip-tree--system"><code>--system</code></a></dt><dd><p>List packages in the system Python environment.</p>
<p>Disables discovery of virtual environments.</p>
<p>See <a href="#uv-python">uv python</a> for details on Python discovery.</p>
<p>May also be set with the <code>UV_SYSTEM_PYTHON</code> environment variable.</p></dd><dt id="uv-pip-tree--verbose"><a href="#uv-pip-tree--verbose"><code>--verbose</code></a>, <code>-v</code></dt><dd><p>Use verbose output.</p>
<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (<a href="https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives">https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives</a>)</p>
</dd></dl>

### uv pip check

Verify installed packages have compatible dependencies

<h3 class="cli-reference">Usage</h3>

```
uv pip check [OPTIONS]
```

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt id="uv-pip-check--allow-insecure-host"><a href="#uv-pip-check--allow-insecure-host"><code>--allow-insecure-host</code></a>, <code>--trusted-host</code> <i>allow-insecure-host</i></dt><dd><p>Allow insecure connections to a host.</p>
<p>Can be provided multiple times.</p>
<p>Expects to receive either a hostname (e.g., <code>localhost</code>), a host-port pair (e.g., <code>localhost:8080</code>), or a URL (e.g., <code>https://localhost</code>).</p>
<p>WARNING: Hosts included in this list will not be verified against the system's certificate store. Only use <code>--allow-insecure-host</code> in a secure network with verified sources, as it bypasses SSL verification and could expose you to MITM attacks.</p>
<p>May also be set with the <code>UV_INSECURE_HOST</code> environment variable.</p></dd><dt id="uv-pip-check--cache-dir"><a href="#uv-pip-check--cache-dir"><code>--cache-dir</code></a> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>
<p>Defaults to <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on macOS and Linux, and <code>%LOCALAPPDATA%\uv\cache</code> on Windows.</p>
<p>To view the location of the cache directory, run <code>uv cache dir</code>.</p>
<p>May also be set with the <code>UV_CACHE_DIR</code> environment variable.</p></dd><dt id="uv-pip-check--color"><a href="#uv-pip-check--color"><code>--color</code></a> <i>color-choice</i></dt><dd><p>Control the use of color in output.</p>
<p>By default, uv will automatically detect support for colors when writing to a terminal.</p>
<p>Possible values:</p>
<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>
<li><code>always</code>:  Enables colored output regardless of the detected environment</li>
<li><code>never</code>:  Disables colored output</li>
</ul></dd><dt id="uv-pip-check--config-file"><a href="#uv-pip-check--config-file"><code>--config-file</code></a> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>
<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>
<p>May also be set with the <code>UV_CONFIG_FILE</code> environment variable.</p></dd><dt id="uv-pip-check--directory"><a href="#uv-pip-check--directory"><code>--directory</code></a> <i>directory</i></dt><dd><p>Change to the given directory prior to running the command.</p>
<p>Relative paths are resolved with the given directory as the base.</p>
<p>See <code>--project</code> to only change the project root directory.</p>
</dd><dt id="uv-pip-check--help"><a href="#uv-pip-check--help"><code>--help</code></a>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>
</dd><dt id="uv-pip-check--managed-python"><a href="#uv-pip-check--managed-python"><code>--managed-python</code></a></dt><dd><p>Require use of uv-managed Python versions.</p>
<p>By default, uv prefers using Python versions it manages. However, it will use system Python versions if a uv-managed Python is not installed. This option disables use of system Python versions.</p>
<p>May also be set with the <code>UV_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-pip-check--native-tls"><a href="#uv-pip-check--native-tls"><code>--native-tls</code></a></dt><dd><p>Whether to load TLS certificates from the platform's native certificate store.</p>
<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>
<p>However, in some cases, you may want to use the platform's native certificate store, especially if you're relying on a corporate trust root (e.g., for a mandatory proxy) that's included in your system's certificate store.</p>
<p>May also be set with the <code>UV_NATIVE_TLS</code> environment variable.</p></dd><dt id="uv-pip-check--no-cache"><a href="#uv-pip-check--no-cache"><code>--no-cache</code></a>, <code>--no-cache-dir</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>
<p>May also be set with the <code>UV_NO_CACHE</code> environment variable.</p></dd><dt id="uv-pip-check--no-config"><a href="#uv-pip-check--no-config"><code>--no-config</code></a></dt><dd><p>Avoid discovering configuration files (<code>pyproject.toml</code>, <code>uv.toml</code>).</p>
<p>Normally, configuration files are discovered in the current directory, parent directories, or user configuration directories.</p>
<p>May also be set with the <code>UV_NO_CONFIG</code> environment variable.</p></dd><dt id="uv-pip-check--no-managed-python"><a href="#uv-pip-check--no-managed-python"><code>--no-managed-python</code></a></dt><dd><p>Disable use of uv-managed Python versions.</p>
<p>Instead, uv will search for a suitable Python version on the system.</p>
<p>May also be set with the <code>UV_NO_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-pip-check--no-progress"><a href="#uv-pip-check--no-progress"><code>--no-progress</code></a></dt><dd><p>Hide all progress outputs.</p>
<p>For example, spinners or progress bars.</p>
<p>May also be set with the <code>UV_NO_PROGRESS</code> environment variable.</p></dd><dt id="uv-pip-check--no-python-downloads"><a href="#uv-pip-check--no-python-downloads"><code>--no-python-downloads</code></a></dt><dd><p>Disable automatic downloads of Python.</p>
</dd><dt id="uv-pip-check--offline"><a href="#uv-pip-check--offline"><code>--offline</code></a></dt><dd><p>Disable network access.</p>
<p>When disabled, uv will only use locally cached data and locally available files.</p>
<p>May also be set with the <code>UV_OFFLINE</code> environment variable.</p></dd><dt id="uv-pip-check--project"><a href="#uv-pip-check--project"><code>--project</code></a> <i>project</i></dt><dd><p>Run the command within the given project directory.</p>
<p>All <code>pyproject.toml</code>, <code>uv.toml</code>, and <code>.python-version</code> files will be discovered by walking up the directory tree from the project root, as will the project's virtual environment (<code>.venv</code>).</p>
<p>Other command-line arguments (such as relative paths) will be resolved relative to the current working directory.</p>
<p>See <code>--directory</code> to change the working directory entirely.</p>
<p>This setting has no effect when used in the <code>uv pip</code> interface.</p>
<p>May also be set with the <code>UV_PROJECT</code> environment variable.</p></dd><dt id="uv-pip-check--python"><a href="#uv-pip-check--python"><code>--python</code></a>, <code>-p</code> <i>python</i></dt><dd><p>The Python interpreter for which packages should be checked.</p>
<p>By default, uv checks packages in a virtual environment but will check packages in a system
Python environment if no virtual environment is found.</p>
<p>See <a href="#uv-python">uv python</a> for details on Python discovery and supported request formats.</p>
<p>May also be set with the <code>UV_PYTHON</code> environment variable.</p></dd><dt id="uv-pip-check--quiet"><a href="#uv-pip-check--quiet"><code>--quiet</code></a>, <code>-q</code></dt><dd><p>Use quiet output.</p>
<p>Repeating this option, e.g., <code>-qq</code>, will enable a silent mode in which uv will write no output to stdout.</p>
</dd><dt id="uv-pip-check--system"><a href="#uv-pip-check--system"><code>--system</code></a></dt><dd><p>Check packages in the system Python environment.</p>
<p>Disables discovery of virtual environments.</p>
<p>See <a href="#uv-python">uv python</a> for details on Python discovery.</p>
<p>May also be set with the <code>UV_SYSTEM_PYTHON</code> environment variable.</p></dd><dt id="uv-pip-check--verbose"><a href="#uv-pip-check--verbose"><code>--verbose</code></a>, <code>-v</code></dt><dd><p>Use verbose output.</p>
<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (<a href="https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives">https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives</a>)</p>
</dd></dl>

## uv venv

Create a virtual environment.

By default, creates a virtual environment named `.venv` in the working directory. An alternative path may be provided positionally.

If in a project, the default environment name can be changed with the `UV_PROJECT_ENVIRONMENT` environment variable; this only applies when run from the project root directory.

If a virtual environment exists at the target path, it will be removed and a new, empty virtual environment will be created.

When using uv, the virtual environment does not need to be activated. uv will find a virtual environment (named `.venv`) in the working directory or any parent directories.

<h3 class="cli-reference">Usage</h3>

```
uv venv [OPTIONS] [PATH]
```

<h3 class="cli-reference">Arguments</h3>

<dl class="cli-reference"><dt id="uv-venv--path"><a href="#uv-venv--path"<code>PATH</code></a></dt><dd><p>The path to the virtual environment to create.</p>
<p>Default to <code>.venv</code> in the working directory.</p>
<p>Relative paths are resolved relative to the working directory.</p>
</dd></dl>

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt id="uv-venv--allow-existing"><a href="#uv-venv--allow-existing"><code>--allow-existing</code></a></dt><dd><p>Preserve any existing files or directories at the target path.</p>
<p>By default, <code>uv venv</code> will remove an existing virtual environment at the given path, and exit with an error if the path is non-empty but <em>not</em> a virtual environment. The <code>--allow-existing</code> option will instead write to the given path, regardless of its contents, and without clearing it beforehand.</p>
<p>WARNING: This option can lead to unexpected behavior if the existing virtual environment and the newly-created virtual environment are linked to different Python interpreters.</p>
</dd><dt id="uv-venv--allow-insecure-host"><a href="#uv-venv--allow-insecure-host"><code>--allow-insecure-host</code></a>, <code>--trusted-host</code> <i>allow-insecure-host</i></dt><dd><p>Allow insecure connections to a host.</p>
<p>Can be provided multiple times.</p>
<p>Expects to receive either a hostname (e.g., <code>localhost</code>), a host-port pair (e.g., <code>localhost:8080</code>), or a URL (e.g., <code>https://localhost</code>).</p>
<p>WARNING: Hosts included in this list will not be verified against the system's certificate store. Only use <code>--allow-insecure-host</code> in a secure network with verified sources, as it bypasses SSL verification and could expose you to MITM attacks.</p>
<p>May also be set with the <code>UV_INSECURE_HOST</code> environment variable.</p></dd><dt id="uv-venv--cache-dir"><a href="#uv-venv--cache-dir"><code>--cache-dir</code></a> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>
<p>Defaults to <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on macOS and Linux, and <code>%LOCALAPPDATA%\uv\cache</code> on Windows.</p>
<p>To view the location of the cache directory, run <code>uv cache dir</code>.</p>
<p>May also be set with the <code>UV_CACHE_DIR</code> environment variable.</p></dd><dt id="uv-venv--color"><a href="#uv-venv--color"><code>--color</code></a> <i>color-choice</i></dt><dd><p>Control the use of color in output.</p>
<p>By default, uv will automatically detect support for colors when writing to a terminal.</p>
<p>Possible values:</p>
<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>
<li><code>always</code>:  Enables colored output regardless of the detected environment</li>
<li><code>never</code>:  Disables colored output</li>
</ul></dd><dt id="uv-venv--config-file"><a href="#uv-venv--config-file"><code>--config-file</code></a> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>
<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>
<p>May also be set with the <code>UV_CONFIG_FILE</code> environment variable.</p></dd><dt id="uv-venv--default-index"><a href="#uv-venv--default-index"><code>--default-index</code></a> <i>default-index</i></dt><dd><p>The URL of the default package index (by default: <a href="https://pypi.org/simple">https://pypi.org/simple</a>).</p>
<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>
<p>The index given by this flag is given lower priority than all other indexes specified via the <code>--index</code> flag.</p>
<p>May also be set with the <code>UV_DEFAULT_INDEX</code> environment variable.</p></dd><dt id="uv-venv--directory"><a href="#uv-venv--directory"><code>--directory</code></a> <i>directory</i></dt><dd><p>Change to the given directory prior to running the command.</p>
<p>Relative paths are resolved with the given directory as the base.</p>
<p>See <code>--project</code> to only change the project root directory.</p>
</dd><dt id="uv-venv--exclude-newer"><a href="#uv-venv--exclude-newer"><code>--exclude-newer</code></a> <i>exclude-newer</i></dt><dd><p>Limit candidate packages to those that were uploaded prior to the given date.</p>
<p>Accepts both RFC 3339 timestamps (e.g., <code>2006-12-02T02:07:43Z</code>) and local dates in the same format (e.g., <code>2006-12-02</code>) in your system's configured time zone.</p>
<p>May also be set with the <code>UV_EXCLUDE_NEWER</code> environment variable.</p></dd><dt id="uv-venv--extra-index-url"><a href="#uv-venv--extra-index-url"><code>--extra-index-url</code></a> <i>extra-index-url</i></dt><dd><p>(Deprecated: use <code>--index</code> instead) Extra URLs of package indexes to use, in addition to <code>--index-url</code>.</p>
<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>
<p>All indexes provided via this flag take priority over the index specified by <code>--index-url</code> (which defaults to PyPI). When multiple <code>--extra-index-url</code> flags are provided, earlier values take priority.</p>
<p>May also be set with the <code>UV_EXTRA_INDEX_URL</code> environment variable.</p></dd><dt id="uv-venv--find-links"><a href="#uv-venv--find-links"><code>--find-links</code></a>, <code>-f</code> <i>find-links</i></dt><dd><p>Locations to search for candidate distributions, in addition to those found in the registry indexes.</p>
<p>If a path, the target must be a directory that contains packages as wheel files (<code>.whl</code>) or source distributions (e.g., <code>.tar.gz</code> or <code>.zip</code>) at the top level.</p>
<p>If a URL, the page must contain a flat list of links to package files adhering to the formats described above.</p>
<p>May also be set with the <code>UV_FIND_LINKS</code> environment variable.</p></dd><dt id="uv-venv--help"><a href="#uv-venv--help"><code>--help</code></a>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>
</dd><dt id="uv-venv--index"><a href="#uv-venv--index"><code>--index</code></a> <i>index</i></dt><dd><p>The URLs to use when resolving dependencies, in addition to the default index.</p>
<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>
<p>All indexes provided via this flag take priority over the index specified by <code>--default-index</code> (which defaults to PyPI). When multiple <code>--index</code> flags are provided, earlier values take priority.</p>
<p>May also be set with the <code>UV_INDEX</code> environment variable.</p></dd><dt id="uv-venv--index-strategy"><a href="#uv-venv--index-strategy"><code>--index-strategy</code></a> <i>index-strategy</i></dt><dd><p>The strategy to use when resolving against multiple index URLs.</p>
<p>By default, uv will stop at the first index on which a given package is available, and limit resolutions to those present on that first index (<code>first-index</code>). This prevents &quot;dependency confusion&quot; attacks, whereby an attacker can upload a malicious package under the same name to an alternate index.</p>
<p>May also be set with the <code>UV_INDEX_STRATEGY</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>first-index</code>:  Only use results from the first index that returns a match for a given package name</li>
<li><code>unsafe-first-match</code>:  Search for every package name across all indexes, exhausting the versions from the first index before moving on to the next</li>
<li><code>unsafe-best-match</code>:  Search for every package name across all indexes, preferring the &quot;best&quot; version found. If a package version is in multiple indexes, only look at the entry for the first index</li>
</ul></dd><dt id="uv-venv--index-url"><a href="#uv-venv--index-url"><code>--index-url</code></a>, <code>-i</code> <i>index-url</i></dt><dd><p>(Deprecated: use <code>--default-index</code> instead) The URL of the Python package index (by default: <a href="https://pypi.org/simple">https://pypi.org/simple</a>).</p>
<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>
<p>The index given by this flag is given lower priority than all other indexes specified via the <code>--extra-index-url</code> flag.</p>
<p>May also be set with the <code>UV_INDEX_URL</code> environment variable.</p></dd><dt id="uv-venv--keyring-provider"><a href="#uv-venv--keyring-provider"><code>--keyring-provider</code></a> <i>keyring-provider</i></dt><dd><p>Attempt to use <code>keyring</code> for authentication for index URLs.</p>
<p>At present, only <code>--keyring-provider subprocess</code> is supported, which configures uv to use the <code>keyring</code> CLI to handle authentication.</p>
<p>Defaults to <code>disabled</code>.</p>
<p>May also be set with the <code>UV_KEYRING_PROVIDER</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>disabled</code>:  Do not use keyring for credential lookup</li>
<li><code>subprocess</code>:  Use the <code>keyring</code> command for credential lookup</li>
</ul></dd><dt id="uv-venv--link-mode"><a href="#uv-venv--link-mode"><code>--link-mode</code></a> <i>link-mode</i></dt><dd><p>The method to use when installing packages from the global cache.</p>
<p>This option is only used for installing seed packages.</p>
<p>Defaults to <code>clone</code> (also known as Copy-on-Write) on macOS, and <code>hardlink</code> on Linux and Windows.</p>
<p>May also be set with the <code>UV_LINK_MODE</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>clone</code>:  Clone (i.e., copy-on-write) packages from the wheel into the <code>site-packages</code> directory</li>
<li><code>copy</code>:  Copy packages from the wheel into the <code>site-packages</code> directory</li>
<li><code>hardlink</code>:  Hard link packages from the wheel into the <code>site-packages</code> directory</li>
<li><code>symlink</code>:  Symbolically link packages from the wheel into the <code>site-packages</code> directory</li>
</ul></dd><dt id="uv-venv--managed-python"><a href="#uv-venv--managed-python"><code>--managed-python</code></a></dt><dd><p>Require use of uv-managed Python versions.</p>
<p>By default, uv prefers using Python versions it manages. However, it will use system Python versions if a uv-managed Python is not installed. This option disables use of system Python versions.</p>
<p>May also be set with the <code>UV_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-venv--native-tls"><a href="#uv-venv--native-tls"><code>--native-tls</code></a></dt><dd><p>Whether to load TLS certificates from the platform's native certificate store.</p>
<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>
<p>However, in some cases, you may want to use the platform's native certificate store, especially if you're relying on a corporate trust root (e.g., for a mandatory proxy) that's included in your system's certificate store.</p>
<p>May also be set with the <code>UV_NATIVE_TLS</code> environment variable.</p></dd><dt id="uv-venv--no-cache"><a href="#uv-venv--no-cache"><code>--no-cache</code></a>, <code>--no-cache-dir</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>
<p>May also be set with the <code>UV_NO_CACHE</code> environment variable.</p></dd><dt id="uv-venv--no-config"><a href="#uv-venv--no-config"><code>--no-config</code></a></dt><dd><p>Avoid discovering configuration files (<code>pyproject.toml</code>, <code>uv.toml</code>).</p>
<p>Normally, configuration files are discovered in the current directory, parent directories, or user configuration directories.</p>
<p>May also be set with the <code>UV_NO_CONFIG</code> environment variable.</p></dd><dt id="uv-venv--no-index"><a href="#uv-venv--no-index"><code>--no-index</code></a></dt><dd><p>Ignore the registry index (e.g., PyPI), instead relying on direct URL dependencies and those provided via <code>--find-links</code></p>
</dd><dt id="uv-venv--no-managed-python"><a href="#uv-venv--no-managed-python"><code>--no-managed-python</code></a></dt><dd><p>Disable use of uv-managed Python versions.</p>
<p>Instead, uv will search for a suitable Python version on the system.</p>
<p>May also be set with the <code>UV_NO_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-venv--no-progress"><a href="#uv-venv--no-progress"><code>--no-progress</code></a></dt><dd><p>Hide all progress outputs.</p>
<p>For example, spinners or progress bars.</p>
<p>May also be set with the <code>UV_NO_PROGRESS</code> environment variable.</p></dd><dt id="uv-venv--no-project"><a href="#uv-venv--no-project"><code>--no-project</code></a>, <code>--no-workspace</code></dt><dd><p>Avoid discovering a project or workspace.</p>
<p>By default, uv searches for projects in the current directory or any parent directory to determine the default path of the virtual environment and check for Python version constraints, if any.</p>
</dd><dt id="uv-venv--no-python-downloads"><a href="#uv-venv--no-python-downloads"><code>--no-python-downloads</code></a></dt><dd><p>Disable automatic downloads of Python.</p>
</dd><dt id="uv-venv--offline"><a href="#uv-venv--offline"><code>--offline</code></a></dt><dd><p>Disable network access.</p>
<p>When disabled, uv will only use locally cached data and locally available files.</p>
<p>May also be set with the <code>UV_OFFLINE</code> environment variable.</p></dd><dt id="uv-venv--project"><a href="#uv-venv--project"><code>--project</code></a> <i>project</i></dt><dd><p>Run the command within the given project directory.</p>
<p>All <code>pyproject.toml</code>, <code>uv.toml</code>, and <code>.python-version</code> files will be discovered by walking up the directory tree from the project root, as will the project's virtual environment (<code>.venv</code>).</p>
<p>Other command-line arguments (such as relative paths) will be resolved relative to the current working directory.</p>
<p>See <code>--directory</code> to change the working directory entirely.</p>
<p>This setting has no effect when used in the <code>uv pip</code> interface.</p>
<p>May also be set with the <code>UV_PROJECT</code> environment variable.</p></dd><dt id="uv-venv--prompt"><a href="#uv-venv--prompt"><code>--prompt</code></a> <i>prompt</i></dt><dd><p>Provide an alternative prompt prefix for the virtual environment.</p>
<p>By default, the prompt is dependent on whether a path was provided to <code>uv venv</code>. If provided
(e.g, <code>uv venv project</code>), the prompt is set to the directory name. If not provided
(<code>uv venv</code>), the prompt is set to the current directory's name.</p>
<p>If &quot;.&quot; is provided, the current directory name will be used regardless of whether a path was
provided to <code>uv venv</code>.</p>
</dd><dt id="uv-venv--python"><a href="#uv-venv--python"><code>--python</code></a>, <code>-p</code> <i>python</i></dt><dd><p>The Python interpreter to use for the virtual environment.</p>
<p>During virtual environment creation, uv will not look for Python interpreters in virtual
environments.</p>
<p>See <a href="#uv-python">uv python</a> for details on Python discovery and supported request formats.</p>
<p>May also be set with the <code>UV_PYTHON</code> environment variable.</p></dd><dt id="uv-venv--quiet"><a href="#uv-venv--quiet"><code>--quiet</code></a>, <code>-q</code></dt><dd><p>Use quiet output.</p>
<p>Repeating this option, e.g., <code>-qq</code>, will enable a silent mode in which uv will write no output to stdout.</p>
</dd><dt id="uv-venv--refresh"><a href="#uv-venv--refresh"><code>--refresh</code></a></dt><dd><p>Refresh all cached data</p>
</dd><dt id="uv-venv--refresh-package"><a href="#uv-venv--refresh-package"><code>--refresh-package</code></a> <i>refresh-package</i></dt><dd><p>Refresh cached data for a specific package</p>
</dd><dt id="uv-venv--relocatable"><a href="#uv-venv--relocatable"><code>--relocatable</code></a></dt><dd><p>Make the virtual environment relocatable.</p>
<p>A relocatable virtual environment can be moved around and redistributed without invalidating its associated entrypoint and activation scripts.</p>
<p>Note that this can only be guaranteed for standard <code>console_scripts</code> and <code>gui_scripts</code>. Other scripts may be adjusted if they ship with a generic <code>#!python[w]</code> shebang, and binaries are left as-is.</p>
<p>As a result of making the environment relocatable (by way of writing relative, rather than absolute paths), the entrypoints and scripts themselves will <em>not</em> be relocatable. In other words, copying those entrypoints and scripts to a location outside the environment will not work, as they reference paths relative to the environment itself.</p>
</dd><dt id="uv-venv--seed"><a href="#uv-venv--seed"><code>--seed</code></a></dt><dd><p>Install seed packages (one or more of: <code>pip</code>, <code>setuptools</code>, and <code>wheel</code>) into the virtual environment.</p>
<p>Note that <code>setuptools</code> and <code>wheel</code> are not included in Python 3.12+ environments.</p>
<p>May also be set with the <code>UV_VENV_SEED</code> environment variable.</p></dd><dt id="uv-venv--system-site-packages"><a href="#uv-venv--system-site-packages"><code>--system-site-packages</code></a></dt><dd><p>Give the virtual environment access to the system site packages directory.</p>
<p>Unlike <code>pip</code>, when a virtual environment is created with <code>--system-site-packages</code>, uv will <em>not</em> take system site packages into account when running commands like <code>uv pip list</code> or <code>uv pip install</code>. The <code>--system-site-packages</code> flag will provide the virtual environment with access to the system site packages directory at runtime, but will not affect the behavior of uv commands.</p>
</dd><dt id="uv-venv--verbose"><a href="#uv-venv--verbose"><code>--verbose</code></a>, <code>-v</code></dt><dd><p>Use verbose output.</p>
<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (<a href="https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives">https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives</a>)</p>
</dd></dl>

## uv build

Build Python packages into source distributions and wheels.

`uv build` accepts a path to a directory or source distribution, which defaults to the current working directory.

By default, if passed a directory, `uv build` will build a source distribution ("sdist") from the source directory, and a binary distribution ("wheel") from the source distribution.

`uv build --sdist` can be used to build only the source distribution, `uv build --wheel` can be used to build only the binary distribution, and `uv build --sdist --wheel` can be used to build both distributions from source.

If passed a source distribution, `uv build --wheel` will build a wheel from the source distribution.

<h3 class="cli-reference">Usage</h3>

```
uv build [OPTIONS] [SRC]
```

<h3 class="cli-reference">Arguments</h3>

<dl class="cli-reference"><dt id="uv-build--src"><a href="#uv-build--src"<code>SRC</code></a></dt><dd><p>The directory from which distributions should be built, or a source distribution archive to build into a wheel.</p>
<p>Defaults to the current working directory.</p>
</dd></dl>

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt id="uv-build--all-packages"><a href="#uv-build--all-packages"><code>--all-packages</code></a>, <code>--all</code></dt><dd><p>Builds all packages in the workspace.</p>
<p>The workspace will be discovered from the provided source directory, or the current directory if no source directory is provided.</p>
<p>If the workspace member does not exist, uv will exit with an error.</p>
</dd><dt id="uv-build--allow-insecure-host"><a href="#uv-build--allow-insecure-host"><code>--allow-insecure-host</code></a>, <code>--trusted-host</code> <i>allow-insecure-host</i></dt><dd><p>Allow insecure connections to a host.</p>
<p>Can be provided multiple times.</p>
<p>Expects to receive either a hostname (e.g., <code>localhost</code>), a host-port pair (e.g., <code>localhost:8080</code>), or a URL (e.g., <code>https://localhost</code>).</p>
<p>WARNING: Hosts included in this list will not be verified against the system's certificate store. Only use <code>--allow-insecure-host</code> in a secure network with verified sources, as it bypasses SSL verification and could expose you to MITM attacks.</p>
<p>May also be set with the <code>UV_INSECURE_HOST</code> environment variable.</p></dd><dt id="uv-build--build-constraints"><a href="#uv-build--build-constraints"><code>--build-constraints</code></a>, <code>--build-constraint</code>, <code>-b</code> <i>build-constraints</i></dt><dd><p>Constrain build dependencies using the given requirements files when building distributions.</p>
<p>Constraints files are <code>requirements.txt</code>-like files that only control the <em>version</em> of a build dependency that's installed. However, including a package in a constraints file will <em>not</em> trigger the inclusion of that package on its own.</p>
<p>May also be set with the <code>UV_BUILD_CONSTRAINT</code> environment variable.</p></dd><dt id="uv-build--cache-dir"><a href="#uv-build--cache-dir"><code>--cache-dir</code></a> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>
<p>Defaults to <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on macOS and Linux, and <code>%LOCALAPPDATA%\uv\cache</code> on Windows.</p>
<p>To view the location of the cache directory, run <code>uv cache dir</code>.</p>
<p>May also be set with the <code>UV_CACHE_DIR</code> environment variable.</p></dd><dt id="uv-build--color"><a href="#uv-build--color"><code>--color</code></a> <i>color-choice</i></dt><dd><p>Control the use of color in output.</p>
<p>By default, uv will automatically detect support for colors when writing to a terminal.</p>
<p>Possible values:</p>
<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>
<li><code>always</code>:  Enables colored output regardless of the detected environment</li>
<li><code>never</code>:  Disables colored output</li>
</ul></dd><dt id="uv-build--config-file"><a href="#uv-build--config-file"><code>--config-file</code></a> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>
<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>
<p>May also be set with the <code>UV_CONFIG_FILE</code> environment variable.</p></dd><dt id="uv-build--config-setting"><a href="#uv-build--config-setting"><code>--config-setting</code></a>, <code>--config-settings</code>, <code>-C</code> <i>config-setting</i></dt><dd><p>Settings to pass to the PEP 517 build backend, specified as <code>KEY=VALUE</code> pairs</p>
</dd><dt id="uv-build--default-index"><a href="#uv-build--default-index"><code>--default-index</code></a> <i>default-index</i></dt><dd><p>The URL of the default package index (by default: <a href="https://pypi.org/simple">https://pypi.org/simple</a>).</p>
<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>
<p>The index given by this flag is given lower priority than all other indexes specified via the <code>--index</code> flag.</p>
<p>May also be set with the <code>UV_DEFAULT_INDEX</code> environment variable.</p></dd><dt id="uv-build--directory"><a href="#uv-build--directory"><code>--directory</code></a> <i>directory</i></dt><dd><p>Change to the given directory prior to running the command.</p>
<p>Relative paths are resolved with the given directory as the base.</p>
<p>See <code>--project</code> to only change the project root directory.</p>
</dd><dt id="uv-build--exclude-newer"><a href="#uv-build--exclude-newer"><code>--exclude-newer</code></a> <i>exclude-newer</i></dt><dd><p>Limit candidate packages to those that were uploaded prior to the given date.</p>
<p>Accepts both RFC 3339 timestamps (e.g., <code>2006-12-02T02:07:43Z</code>) and local dates in the same format (e.g., <code>2006-12-02</code>) in your system's configured time zone.</p>
<p>May also be set with the <code>UV_EXCLUDE_NEWER</code> environment variable.</p></dd><dt id="uv-build--extra-index-url"><a href="#uv-build--extra-index-url"><code>--extra-index-url</code></a> <i>extra-index-url</i></dt><dd><p>(Deprecated: use <code>--index</code> instead) Extra URLs of package indexes to use, in addition to <code>--index-url</code>.</p>
<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>
<p>All indexes provided via this flag take priority over the index specified by <code>--index-url</code> (which defaults to PyPI). When multiple <code>--extra-index-url</code> flags are provided, earlier values take priority.</p>
<p>May also be set with the <code>UV_EXTRA_INDEX_URL</code> environment variable.</p></dd><dt id="uv-build--find-links"><a href="#uv-build--find-links"><code>--find-links</code></a>, <code>-f</code> <i>find-links</i></dt><dd><p>Locations to search for candidate distributions, in addition to those found in the registry indexes.</p>
<p>If a path, the target must be a directory that contains packages as wheel files (<code>.whl</code>) or source distributions (e.g., <code>.tar.gz</code> or <code>.zip</code>) at the top level.</p>
<p>If a URL, the page must contain a flat list of links to package files adhering to the formats described above.</p>
<p>May also be set with the <code>UV_FIND_LINKS</code> environment variable.</p></dd><dt id="uv-build--force-pep517"><a href="#uv-build--force-pep517"><code>--force-pep517</code></a></dt><dd><p>Always build through PEP 517, don't use the fast path for the uv build backend.</p>
<p>By default, uv won't create a PEP 517 build environment for packages using the uv build backend, but use a fast path that calls into the build backend directly. This option forces always using PEP 517.</p>
</dd><dt id="uv-build--fork-strategy"><a href="#uv-build--fork-strategy"><code>--fork-strategy</code></a> <i>fork-strategy</i></dt><dd><p>The strategy to use when selecting multiple versions of a given package across Python versions and platforms.</p>
<p>By default, uv will optimize for selecting the latest version of each package for each supported Python version (<code>requires-python</code>), while minimizing the number of selected versions across platforms.</p>
<p>Under <code>fewest</code>, uv will minimize the number of selected versions for each package, preferring older versions that are compatible with a wider range of supported Python versions or platforms.</p>
<p>May also be set with the <code>UV_FORK_STRATEGY</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>fewest</code>:  Optimize for selecting the fewest number of versions for each package. Older versions may be preferred if they are compatible with a wider range of supported Python versions or platforms</li>
<li><code>requires-python</code>:  Optimize for selecting latest supported version of each package, for each supported Python version</li>
</ul></dd><dt id="uv-build--help"><a href="#uv-build--help"><code>--help</code></a>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>
</dd><dt id="uv-build--index"><a href="#uv-build--index"><code>--index</code></a> <i>index</i></dt><dd><p>The URLs to use when resolving dependencies, in addition to the default index.</p>
<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>
<p>All indexes provided via this flag take priority over the index specified by <code>--default-index</code> (which defaults to PyPI). When multiple <code>--index</code> flags are provided, earlier values take priority.</p>
<p>May also be set with the <code>UV_INDEX</code> environment variable.</p></dd><dt id="uv-build--index-strategy"><a href="#uv-build--index-strategy"><code>--index-strategy</code></a> <i>index-strategy</i></dt><dd><p>The strategy to use when resolving against multiple index URLs.</p>
<p>By default, uv will stop at the first index on which a given package is available, and limit resolutions to those present on that first index (<code>first-index</code>). This prevents &quot;dependency confusion&quot; attacks, whereby an attacker can upload a malicious package under the same name to an alternate index.</p>
<p>May also be set with the <code>UV_INDEX_STRATEGY</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>first-index</code>:  Only use results from the first index that returns a match for a given package name</li>
<li><code>unsafe-first-match</code>:  Search for every package name across all indexes, exhausting the versions from the first index before moving on to the next</li>
<li><code>unsafe-best-match</code>:  Search for every package name across all indexes, preferring the &quot;best&quot; version found. If a package version is in multiple indexes, only look at the entry for the first index</li>
</ul></dd><dt id="uv-build--index-url"><a href="#uv-build--index-url"><code>--index-url</code></a>, <code>-i</code> <i>index-url</i></dt><dd><p>(Deprecated: use <code>--default-index</code> instead) The URL of the Python package index (by default: <a href="https://pypi.org/simple">https://pypi.org/simple</a>).</p>
<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>
<p>The index given by this flag is given lower priority than all other indexes specified via the <code>--extra-index-url</code> flag.</p>
<p>May also be set with the <code>UV_INDEX_URL</code> environment variable.</p></dd><dt id="uv-build--keyring-provider"><a href="#uv-build--keyring-provider"><code>--keyring-provider</code></a> <i>keyring-provider</i></dt><dd><p>Attempt to use <code>keyring</code> for authentication for index URLs.</p>
<p>At present, only <code>--keyring-provider subprocess</code> is supported, which configures uv to use the <code>keyring</code> CLI to handle authentication.</p>
<p>Defaults to <code>disabled</code>.</p>
<p>May also be set with the <code>UV_KEYRING_PROVIDER</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>disabled</code>:  Do not use keyring for credential lookup</li>
<li><code>subprocess</code>:  Use the <code>keyring</code> command for credential lookup</li>
</ul></dd><dt id="uv-build--link-mode"><a href="#uv-build--link-mode"><code>--link-mode</code></a> <i>link-mode</i></dt><dd><p>The method to use when installing packages from the global cache.</p>
<p>This option is only used when building source distributions.</p>
<p>Defaults to <code>clone</code> (also known as Copy-on-Write) on macOS, and <code>hardlink</code> on Linux and Windows.</p>
<p>May also be set with the <code>UV_LINK_MODE</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>clone</code>:  Clone (i.e., copy-on-write) packages from the wheel into the <code>site-packages</code> directory</li>
<li><code>copy</code>:  Copy packages from the wheel into the <code>site-packages</code> directory</li>
<li><code>hardlink</code>:  Hard link packages from the wheel into the <code>site-packages</code> directory</li>
<li><code>symlink</code>:  Symbolically link packages from the wheel into the <code>site-packages</code> directory</li>
</ul></dd><dt id="uv-build--managed-python"><a href="#uv-build--managed-python"><code>--managed-python</code></a></dt><dd><p>Require use of uv-managed Python versions.</p>
<p>By default, uv prefers using Python versions it manages. However, it will use system Python versions if a uv-managed Python is not installed. This option disables use of system Python versions.</p>
<p>May also be set with the <code>UV_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-build--native-tls"><a href="#uv-build--native-tls"><code>--native-tls</code></a></dt><dd><p>Whether to load TLS certificates from the platform's native certificate store.</p>
<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>
<p>However, in some cases, you may want to use the platform's native certificate store, especially if you're relying on a corporate trust root (e.g., for a mandatory proxy) that's included in your system's certificate store.</p>
<p>May also be set with the <code>UV_NATIVE_TLS</code> environment variable.</p></dd><dt id="uv-build--no-binary"><a href="#uv-build--no-binary"><code>--no-binary</code></a></dt><dd><p>Don't install pre-built wheels.</p>
<p>The given packages will be built and installed from source. The resolver will still use pre-built wheels to extract package metadata, if available.</p>
<p>May also be set with the <code>UV_NO_BINARY</code> environment variable.</p></dd><dt id="uv-build--no-binary-package"><a href="#uv-build--no-binary-package"><code>--no-binary-package</code></a> <i>no-binary-package</i></dt><dd><p>Don't install pre-built wheels for a specific package</p>
<p>May also be set with the <code>UV_NO_BINARY_PACKAGE</code> environment variable.</p></dd><dt id="uv-build--no-build"><a href="#uv-build--no-build"><code>--no-build</code></a></dt><dd><p>Don't build source distributions.</p>
<p>When enabled, resolving will not run arbitrary Python code. The cached wheels of already-built source distributions will be reused, but operations that require building distributions will exit with an error.</p>
<p>May also be set with the <code>UV_NO_BUILD</code> environment variable.</p></dd><dt id="uv-build--no-build-isolation"><a href="#uv-build--no-build-isolation"><code>--no-build-isolation</code></a></dt><dd><p>Disable isolation when building source distributions.</p>
<p>Assumes that build dependencies specified by PEP 518 are already installed.</p>
<p>May also be set with the <code>UV_NO_BUILD_ISOLATION</code> environment variable.</p></dd><dt id="uv-build--no-build-isolation-package"><a href="#uv-build--no-build-isolation-package"><code>--no-build-isolation-package</code></a> <i>no-build-isolation-package</i></dt><dd><p>Disable isolation when building source distributions for a specific package.</p>
<p>Assumes that the packages' build dependencies specified by PEP 518 are already installed.</p>
</dd><dt id="uv-build--no-build-logs"><a href="#uv-build--no-build-logs"><code>--no-build-logs</code></a></dt><dd><p>Hide logs from the build backend</p>
</dd><dt id="uv-build--no-build-package"><a href="#uv-build--no-build-package"><code>--no-build-package</code></a> <i>no-build-package</i></dt><dd><p>Don't build source distributions for a specific package</p>
<p>May also be set with the <code>UV_NO_BUILD_PACKAGE</code> environment variable.</p></dd><dt id="uv-build--no-cache"><a href="#uv-build--no-cache"><code>--no-cache</code></a>, <code>--no-cache-dir</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>
<p>May also be set with the <code>UV_NO_CACHE</code> environment variable.</p></dd><dt id="uv-build--no-config"><a href="#uv-build--no-config"><code>--no-config</code></a></dt><dd><p>Avoid discovering configuration files (<code>pyproject.toml</code>, <code>uv.toml</code>).</p>
<p>Normally, configuration files are discovered in the current directory, parent directories, or user configuration directories.</p>
<p>May also be set with the <code>UV_NO_CONFIG</code> environment variable.</p></dd><dt id="uv-build--no-index"><a href="#uv-build--no-index"><code>--no-index</code></a></dt><dd><p>Ignore the registry index (e.g., PyPI), instead relying on direct URL dependencies and those provided via <code>--find-links</code></p>
</dd><dt id="uv-build--no-managed-python"><a href="#uv-build--no-managed-python"><code>--no-managed-python</code></a></dt><dd><p>Disable use of uv-managed Python versions.</p>
<p>Instead, uv will search for a suitable Python version on the system.</p>
<p>May also be set with the <code>UV_NO_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-build--no-progress"><a href="#uv-build--no-progress"><code>--no-progress</code></a></dt><dd><p>Hide all progress outputs.</p>
<p>For example, spinners or progress bars.</p>
<p>May also be set with the <code>UV_NO_PROGRESS</code> environment variable.</p></dd><dt id="uv-build--no-python-downloads"><a href="#uv-build--no-python-downloads"><code>--no-python-downloads</code></a></dt><dd><p>Disable automatic downloads of Python.</p>
</dd><dt id="uv-build--no-sources"><a href="#uv-build--no-sources"><code>--no-sources</code></a></dt><dd><p>Ignore the <code>tool.uv.sources</code> table when resolving dependencies. Used to lock against the standards-compliant, publishable package metadata, as opposed to using any workspace, Git, URL, or local path sources</p>
</dd><dt id="uv-build--no-verify-hashes"><a href="#uv-build--no-verify-hashes"><code>--no-verify-hashes</code></a></dt><dd><p>Disable validation of hashes in the requirements file.</p>
<p>By default, uv will verify any available hashes in the requirements file, but will not require that all requirements have an associated hash. To enforce hash validation, use <code>--require-hashes</code>.</p>
<p>May also be set with the <code>UV_NO_VERIFY_HASHES</code> environment variable.</p></dd><dt id="uv-build--offline"><a href="#uv-build--offline"><code>--offline</code></a></dt><dd><p>Disable network access.</p>
<p>When disabled, uv will only use locally cached data and locally available files.</p>
<p>May also be set with the <code>UV_OFFLINE</code> environment variable.</p></dd><dt id="uv-build--out-dir"><a href="#uv-build--out-dir"><code>--out-dir</code></a>, <code>-o</code> <i>out-dir</i></dt><dd><p>The output directory to which distributions should be written.</p>
<p>Defaults to the <code>dist</code> subdirectory within the source directory, or the directory containing the source distribution archive.</p>
</dd><dt id="uv-build--package"><a href="#uv-build--package"><code>--package</code></a> <i>package</i></dt><dd><p>Build a specific package in the workspace.</p>
<p>The workspace will be discovered from the provided source directory, or the current directory if no source directory is provided.</p>
<p>If the workspace member does not exist, uv will exit with an error.</p>
</dd><dt id="uv-build--prerelease"><a href="#uv-build--prerelease"><code>--prerelease</code></a> <i>prerelease</i></dt><dd><p>The strategy to use when considering pre-release versions.</p>
<p>By default, uv will accept pre-releases for packages that <em>only</em> publish pre-releases, along with first-party requirements that contain an explicit pre-release marker in the declared specifiers (<code>if-necessary-or-explicit</code>).</p>
<p>May also be set with the <code>UV_PRERELEASE</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>disallow</code>:  Disallow all pre-release versions</li>
<li><code>allow</code>:  Allow all pre-release versions</li>
<li><code>if-necessary</code>:  Allow pre-release versions if all versions of a package are pre-release</li>
<li><code>explicit</code>:  Allow pre-release versions for first-party packages with explicit pre-release markers in their version requirements</li>
<li><code>if-necessary-or-explicit</code>:  Allow pre-release versions if all versions of a package are pre-release, or if the package has an explicit pre-release marker in its version requirements</li>
</ul></dd><dt id="uv-build--project"><a href="#uv-build--project"><code>--project</code></a> <i>project</i></dt><dd><p>Run the command within the given project directory.</p>
<p>All <code>pyproject.toml</code>, <code>uv.toml</code>, and <code>.python-version</code> files will be discovered by walking up the directory tree from the project root, as will the project's virtual environment (<code>.venv</code>).</p>
<p>Other command-line arguments (such as relative paths) will be resolved relative to the current working directory.</p>
<p>See <code>--directory</code> to change the working directory entirely.</p>
<p>This setting has no effect when used in the <code>uv pip</code> interface.</p>
<p>May also be set with the <code>UV_PROJECT</code> environment variable.</p></dd><dt id="uv-build--python"><a href="#uv-build--python"><code>--python</code></a>, <code>-p</code> <i>python</i></dt><dd><p>The Python interpreter to use for the build environment.</p>
<p>By default, builds are executed in isolated virtual environments. The discovered interpreter
will be used to create those environments, and will be symlinked or copied in depending on
the platform.</p>
<p>See <a href="#uv-python">uv python</a> to view supported request formats.</p>
<p>May also be set with the <code>UV_PYTHON</code> environment variable.</p></dd><dt id="uv-build--quiet"><a href="#uv-build--quiet"><code>--quiet</code></a>, <code>-q</code></dt><dd><p>Use quiet output.</p>
<p>Repeating this option, e.g., <code>-qq</code>, will enable a silent mode in which uv will write no output to stdout.</p>
</dd><dt id="uv-build--refresh"><a href="#uv-build--refresh"><code>--refresh</code></a></dt><dd><p>Refresh all cached data</p>
</dd><dt id="uv-build--refresh-package"><a href="#uv-build--refresh-package"><code>--refresh-package</code></a> <i>refresh-package</i></dt><dd><p>Refresh cached data for a specific package</p>
</dd><dt id="uv-build--require-hashes"><a href="#uv-build--require-hashes"><code>--require-hashes</code></a></dt><dd><p>Require a matching hash for each requirement.</p>
<p>By default, uv will verify any available hashes in the requirements file, but will not require that all requirements have an associated hash.</p>
<p>When <code>--require-hashes</code> is enabled, <em>all</em> requirements must include a hash or set of hashes, and <em>all</em> requirements must either be pinned to exact versions (e.g., <code>==1.0.0</code>), or be specified via direct URL.</p>
<p>Hash-checking mode introduces a number of additional constraints:</p>
<ul>
<li>Git dependencies are not supported. - Editable installs are not supported. - Local dependencies are not supported, unless they point to a specific wheel (<code>.whl</code>) or source archive (<code>.zip</code>, <code>.tar.gz</code>), as opposed to a directory.</li>
</ul>
<p>May also be set with the <code>UV_REQUIRE_HASHES</code> environment variable.</p></dd><dt id="uv-build--resolution"><a href="#uv-build--resolution"><code>--resolution</code></a> <i>resolution</i></dt><dd><p>The strategy to use when selecting between the different compatible versions for a given package requirement.</p>
<p>By default, uv will use the latest compatible version of each package (<code>highest</code>).</p>
<p>May also be set with the <code>UV_RESOLUTION</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>highest</code>:  Resolve the highest compatible version of each package</li>
<li><code>lowest</code>:  Resolve the lowest compatible version of each package</li>
<li><code>lowest-direct</code>:  Resolve the lowest compatible version of any direct dependencies, and the highest compatible version of any transitive dependencies</li>
</ul></dd><dt id="uv-build--sdist"><a href="#uv-build--sdist"><code>--sdist</code></a></dt><dd><p>Build a source distribution (&quot;sdist&quot;) from the given directory</p>
</dd><dt id="uv-build--upgrade"><a href="#uv-build--upgrade"><code>--upgrade</code></a>, <code>-U</code></dt><dd><p>Allow package upgrades, ignoring pinned versions in any existing output file. Implies <code>--refresh</code></p>
</dd><dt id="uv-build--upgrade-package"><a href="#uv-build--upgrade-package"><code>--upgrade-package</code></a>, <code>-P</code> <i>upgrade-package</i></dt><dd><p>Allow upgrades for a specific package, ignoring pinned versions in any existing output file. Implies <code>--refresh-package</code></p>
</dd><dt id="uv-build--verbose"><a href="#uv-build--verbose"><code>--verbose</code></a>, <code>-v</code></dt><dd><p>Use verbose output.</p>
<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (<a href="https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives">https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives</a>)</p>
</dd><dt id="uv-build--wheel"><a href="#uv-build--wheel"><code>--wheel</code></a></dt><dd><p>Build a binary distribution (&quot;wheel&quot;) from the given directory</p>
</dd></dl>

## uv publish

Upload distributions to an index

<h3 class="cli-reference">Usage</h3>

```
uv publish [OPTIONS] [FILES]...
```

<h3 class="cli-reference">Arguments</h3>

<dl class="cli-reference"><dt id="uv-publish--files"><a href="#uv-publish--files"<code>FILES</code></a></dt><dd><p>Paths to the files to upload. Accepts glob expressions.</p>
<p>Defaults to the <code>dist</code> directory. Selects only wheels and source distributions, while ignoring other files.</p>
</dd></dl>

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt id="uv-publish--allow-insecure-host"><a href="#uv-publish--allow-insecure-host"><code>--allow-insecure-host</code></a>, <code>--trusted-host</code> <i>allow-insecure-host</i></dt><dd><p>Allow insecure connections to a host.</p>
<p>Can be provided multiple times.</p>
<p>Expects to receive either a hostname (e.g., <code>localhost</code>), a host-port pair (e.g., <code>localhost:8080</code>), or a URL (e.g., <code>https://localhost</code>).</p>
<p>WARNING: Hosts included in this list will not be verified against the system's certificate store. Only use <code>--allow-insecure-host</code> in a secure network with verified sources, as it bypasses SSL verification and could expose you to MITM attacks.</p>
<p>May also be set with the <code>UV_INSECURE_HOST</code> environment variable.</p></dd><dt id="uv-publish--cache-dir"><a href="#uv-publish--cache-dir"><code>--cache-dir</code></a> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>
<p>Defaults to <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on macOS and Linux, and <code>%LOCALAPPDATA%\uv\cache</code> on Windows.</p>
<p>To view the location of the cache directory, run <code>uv cache dir</code>.</p>
<p>May also be set with the <code>UV_CACHE_DIR</code> environment variable.</p></dd><dt id="uv-publish--check-url"><a href="#uv-publish--check-url"><code>--check-url</code></a> <i>check-url</i></dt><dd><p>Check an index URL for existing files to skip duplicate uploads.</p>
<p>This option allows retrying publishing that failed after only some, but not all files have been uploaded, and handles error due to parallel uploads of the same file.</p>
<p>Before uploading, the index is checked. If the exact same file already exists in the index, the file will not be uploaded. If an error occurred during the upload, the index is checked again, to handle cases where the identical file was uploaded twice in parallel.</p>
<p>The exact behavior will vary based on the index. When uploading to PyPI, uploading the same file succeeds even without <code>--check-url</code>, while most other indexes error.</p>
<p>The index must provide one of the supported hashes (SHA-256, SHA-384, or SHA-512).</p>
<p>May also be set with the <code>UV_PUBLISH_CHECK_URL</code> environment variable.</p></dd><dt id="uv-publish--color"><a href="#uv-publish--color"><code>--color</code></a> <i>color-choice</i></dt><dd><p>Control the use of color in output.</p>
<p>By default, uv will automatically detect support for colors when writing to a terminal.</p>
<p>Possible values:</p>
<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>
<li><code>always</code>:  Enables colored output regardless of the detected environment</li>
<li><code>never</code>:  Disables colored output</li>
</ul></dd><dt id="uv-publish--config-file"><a href="#uv-publish--config-file"><code>--config-file</code></a> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>
<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>
<p>May also be set with the <code>UV_CONFIG_FILE</code> environment variable.</p></dd><dt id="uv-publish--directory"><a href="#uv-publish--directory"><code>--directory</code></a> <i>directory</i></dt><dd><p>Change to the given directory prior to running the command.</p>
<p>Relative paths are resolved with the given directory as the base.</p>
<p>See <code>--project</code> to only change the project root directory.</p>
</dd><dt id="uv-publish--help"><a href="#uv-publish--help"><code>--help</code></a>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>
</dd><dt id="uv-publish--index"><a href="#uv-publish--index"><code>--index</code></a> <i>index</i></dt><dd><p>The name of an index in the configuration to use for publishing.</p>
<p>The index must have a <code>publish-url</code> setting, for example:</p>
<pre><code class="language-toml">[[tool.uv.index]]
name = &quot;pypi&quot;
url = &quot;https://pypi.org/simple&quot;
publish-url = &quot;https://upload.pypi.org/legacy/&quot;
</code></pre>
<p>The index <code>url</code> will be used to check for existing files to skip duplicate uploads.</p>
<p>With these settings, the following two calls are equivalent:</p>
<pre><code class="language-shell">uv publish --index pypi
uv publish --publish-url https://upload.pypi.org/legacy/ --check-url https://pypi.org/simple
</code></pre>
<p>May also be set with the <code>UV_PUBLISH_INDEX</code> environment variable.</p></dd><dt id="uv-publish--keyring-provider"><a href="#uv-publish--keyring-provider"><code>--keyring-provider</code></a> <i>keyring-provider</i></dt><dd><p>Attempt to use <code>keyring</code> for authentication for remote requirements files.</p>
<p>At present, only <code>--keyring-provider subprocess</code> is supported, which configures uv to use the <code>keyring</code> CLI to handle authentication.</p>
<p>Defaults to <code>disabled</code>.</p>
<p>May also be set with the <code>UV_KEYRING_PROVIDER</code> environment variable.</p><p>Possible values:</p>
<ul>
<li><code>disabled</code>:  Do not use keyring for credential lookup</li>
<li><code>subprocess</code>:  Use the <code>keyring</code> command for credential lookup</li>
</ul></dd><dt id="uv-publish--managed-python"><a href="#uv-publish--managed-python"><code>--managed-python</code></a></dt><dd><p>Require use of uv-managed Python versions.</p>
<p>By default, uv prefers using Python versions it manages. However, it will use system Python versions if a uv-managed Python is not installed. This option disables use of system Python versions.</p>
<p>May also be set with the <code>UV_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-publish--native-tls"><a href="#uv-publish--native-tls"><code>--native-tls</code></a></dt><dd><p>Whether to load TLS certificates from the platform's native certificate store.</p>
<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>
<p>However, in some cases, you may want to use the platform's native certificate store, especially if you're relying on a corporate trust root (e.g., for a mandatory proxy) that's included in your system's certificate store.</p>
<p>May also be set with the <code>UV_NATIVE_TLS</code> environment variable.</p></dd><dt id="uv-publish--no-cache"><a href="#uv-publish--no-cache"><code>--no-cache</code></a>, <code>--no-cache-dir</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>
<p>May also be set with the <code>UV_NO_CACHE</code> environment variable.</p></dd><dt id="uv-publish--no-config"><a href="#uv-publish--no-config"><code>--no-config</code></a></dt><dd><p>Avoid discovering configuration files (<code>pyproject.toml</code>, <code>uv.toml</code>).</p>
<p>Normally, configuration files are discovered in the current directory, parent directories, or user configuration directories.</p>
<p>May also be set with the <code>UV_NO_CONFIG</code> environment variable.</p></dd><dt id="uv-publish--no-managed-python"><a href="#uv-publish--no-managed-python"><code>--no-managed-python</code></a></dt><dd><p>Disable use of uv-managed Python versions.</p>
<p>Instead, uv will search for a suitable Python version on the system.</p>
<p>May also be set with the <code>UV_NO_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-publish--no-progress"><a href="#uv-publish--no-progress"><code>--no-progress</code></a></dt><dd><p>Hide all progress outputs.</p>
<p>For example, spinners or progress bars.</p>
<p>May also be set with the <code>UV_NO_PROGRESS</code> environment variable.</p></dd><dt id="uv-publish--no-python-downloads"><a href="#uv-publish--no-python-downloads"><code>--no-python-downloads</code></a></dt><dd><p>Disable automatic downloads of Python.</p>
</dd><dt id="uv-publish--offline"><a href="#uv-publish--offline"><code>--offline</code></a></dt><dd><p>Disable network access.</p>
<p>When disabled, uv will only use locally cached data and locally available files.</p>
<p>May also be set with the <code>UV_OFFLINE</code> environment variable.</p></dd><dt id="uv-publish--password"><a href="#uv-publish--password"><code>--password</code></a>, <code>-p</code> <i>password</i></dt><dd><p>The password for the upload</p>
<p>May also be set with the <code>UV_PUBLISH_PASSWORD</code> environment variable.</p></dd><dt id="uv-publish--project"><a href="#uv-publish--project"><code>--project</code></a> <i>project</i></dt><dd><p>Run the command within the given project directory.</p>
<p>All <code>pyproject.toml</code>, <code>uv.toml</code>, and <code>.python-version</code> files will be discovered by walking up the directory tree from the project root, as will the project's virtual environment (<code>.venv</code>).</p>
<p>Other command-line arguments (such as relative paths) will be resolved relative to the current working directory.</p>
<p>See <code>--directory</code> to change the working directory entirely.</p>
<p>This setting has no effect when used in the <code>uv pip</code> interface.</p>
<p>May also be set with the <code>UV_PROJECT</code> environment variable.</p></dd><dt id="uv-publish--publish-url"><a href="#uv-publish--publish-url"><code>--publish-url</code></a> <i>publish-url</i></dt><dd><p>The URL of the upload endpoint (not the index URL).</p>
<p>Note that there are typically different URLs for index access (e.g., <code>https:://.../simple</code>) and index upload.</p>
<p>Defaults to PyPI's publish URL (<a href="https://upload.pypi.org/legacy/">https://upload.pypi.org/legacy/</a>).</p>
<p>May also be set with the <code>UV_PUBLISH_URL</code> environment variable.</p></dd><dt id="uv-publish--quiet"><a href="#uv-publish--quiet"><code>--quiet</code></a>, <code>-q</code></dt><dd><p>Use quiet output.</p>
<p>Repeating this option, e.g., <code>-qq</code>, will enable a silent mode in which uv will write no output to stdout.</p>
</dd><dt id="uv-publish--token"><a href="#uv-publish--token"><code>--token</code></a>, <code>-t</code> <i>token</i></dt><dd><p>The token for the upload.</p>
<p>Using a token is equivalent to passing <code>__token__</code> as <code>--username</code> and the token as <code>--password</code> password.</p>
<p>May also be set with the <code>UV_PUBLISH_TOKEN</code> environment variable.</p></dd><dt id="uv-publish--trusted-publishing"><a href="#uv-publish--trusted-publishing"><code>--trusted-publishing</code></a> <i>trusted-publishing</i></dt><dd><p>Configure using trusted publishing through GitHub Actions.</p>
<p>By default, uv checks for trusted publishing when running in GitHub Actions, but ignores it if it isn't configured or the workflow doesn't have enough permissions (e.g., a pull request from a fork).</p>
<p>Possible values:</p>
<ul>
<li><code>automatic</code>:  Try trusted publishing when we're already in GitHub Actions, continue if that fails</li>
<li><code>always</code></li>
<li><code>never</code></li>
</ul></dd><dt id="uv-publish--username"><a href="#uv-publish--username"><code>--username</code></a>, <code>-u</code> <i>username</i></dt><dd><p>The username for the upload</p>
<p>May also be set with the <code>UV_PUBLISH_USERNAME</code> environment variable.</p></dd><dt id="uv-publish--verbose"><a href="#uv-publish--verbose"><code>--verbose</code></a>, <code>-v</code></dt><dd><p>Use verbose output.</p>
<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (<a href="https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives">https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives</a>)</p>
</dd></dl>

## uv cache

Manage uv's cache

<h3 class="cli-reference">Usage</h3>

```
uv cache [OPTIONS] <COMMAND>
```

<h3 class="cli-reference">Commands</h3>

<dl class="cli-reference"><dt><a href="#uv-cache-clean"><code>uv cache clean</code></a></dt><dd><p>Clear the cache, removing all entries or those linked to specific packages</p></dd>
<dt><a href="#uv-cache-prune"><code>uv cache prune</code></a></dt><dd><p>Prune all unreachable objects from the cache</p></dd>
<dt><a href="#uv-cache-dir"><code>uv cache dir</code></a></dt><dd><p>Show the cache directory</p></dd>
</dl>

### uv cache clean

Clear the cache, removing all entries or those linked to specific packages

<h3 class="cli-reference">Usage</h3>

```
uv cache clean [OPTIONS] [PACKAGE]...
```

<h3 class="cli-reference">Arguments</h3>

<dl class="cli-reference"><dt id="uv-cache-clean--package"><a href="#uv-cache-clean--package"<code>PACKAGE</code></a></dt><dd><p>The packages to remove from the cache</p>
</dd></dl>

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt id="uv-cache-clean--allow-insecure-host"><a href="#uv-cache-clean--allow-insecure-host"><code>--allow-insecure-host</code></a>, <code>--trusted-host</code> <i>allow-insecure-host</i></dt><dd><p>Allow insecure connections to a host.</p>
<p>Can be provided multiple times.</p>
<p>Expects to receive either a hostname (e.g., <code>localhost</code>), a host-port pair (e.g., <code>localhost:8080</code>), or a URL (e.g., <code>https://localhost</code>).</p>
<p>WARNING: Hosts included in this list will not be verified against the system's certificate store. Only use <code>--allow-insecure-host</code> in a secure network with verified sources, as it bypasses SSL verification and could expose you to MITM attacks.</p>
<p>May also be set with the <code>UV_INSECURE_HOST</code> environment variable.</p></dd><dt id="uv-cache-clean--cache-dir"><a href="#uv-cache-clean--cache-dir"><code>--cache-dir</code></a> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>
<p>Defaults to <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on macOS and Linux, and <code>%LOCALAPPDATA%\uv\cache</code> on Windows.</p>
<p>To view the location of the cache directory, run <code>uv cache dir</code>.</p>
<p>May also be set with the <code>UV_CACHE_DIR</code> environment variable.</p></dd><dt id="uv-cache-clean--color"><a href="#uv-cache-clean--color"><code>--color</code></a> <i>color-choice</i></dt><dd><p>Control the use of color in output.</p>
<p>By default, uv will automatically detect support for colors when writing to a terminal.</p>
<p>Possible values:</p>
<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>
<li><code>always</code>:  Enables colored output regardless of the detected environment</li>
<li><code>never</code>:  Disables colored output</li>
</ul></dd><dt id="uv-cache-clean--config-file"><a href="#uv-cache-clean--config-file"><code>--config-file</code></a> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>
<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>
<p>May also be set with the <code>UV_CONFIG_FILE</code> environment variable.</p></dd><dt id="uv-cache-clean--directory"><a href="#uv-cache-clean--directory"><code>--directory</code></a> <i>directory</i></dt><dd><p>Change to the given directory prior to running the command.</p>
<p>Relative paths are resolved with the given directory as the base.</p>
<p>See <code>--project</code> to only change the project root directory.</p>
</dd><dt id="uv-cache-clean--help"><a href="#uv-cache-clean--help"><code>--help</code></a>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>
</dd><dt id="uv-cache-clean--managed-python"><a href="#uv-cache-clean--managed-python"><code>--managed-python</code></a></dt><dd><p>Require use of uv-managed Python versions.</p>
<p>By default, uv prefers using Python versions it manages. However, it will use system Python versions if a uv-managed Python is not installed. This option disables use of system Python versions.</p>
<p>May also be set with the <code>UV_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-cache-clean--native-tls"><a href="#uv-cache-clean--native-tls"><code>--native-tls</code></a></dt><dd><p>Whether to load TLS certificates from the platform's native certificate store.</p>
<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>
<p>However, in some cases, you may want to use the platform's native certificate store, especially if you're relying on a corporate trust root (e.g., for a mandatory proxy) that's included in your system's certificate store.</p>
<p>May also be set with the <code>UV_NATIVE_TLS</code> environment variable.</p></dd><dt id="uv-cache-clean--no-cache"><a href="#uv-cache-clean--no-cache"><code>--no-cache</code></a>, <code>--no-cache-dir</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>
<p>May also be set with the <code>UV_NO_CACHE</code> environment variable.</p></dd><dt id="uv-cache-clean--no-config"><a href="#uv-cache-clean--no-config"><code>--no-config</code></a></dt><dd><p>Avoid discovering configuration files (<code>pyproject.toml</code>, <code>uv.toml</code>).</p>
<p>Normally, configuration files are discovered in the current directory, parent directories, or user configuration directories.</p>
<p>May also be set with the <code>UV_NO_CONFIG</code> environment variable.</p></dd><dt id="uv-cache-clean--no-managed-python"><a href="#uv-cache-clean--no-managed-python"><code>--no-managed-python</code></a></dt><dd><p>Disable use of uv-managed Python versions.</p>
<p>Instead, uv will search for a suitable Python version on the system.</p>
<p>May also be set with the <code>UV_NO_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-cache-clean--no-progress"><a href="#uv-cache-clean--no-progress"><code>--no-progress</code></a></dt><dd><p>Hide all progress outputs.</p>
<p>For example, spinners or progress bars.</p>
<p>May also be set with the <code>UV_NO_PROGRESS</code> environment variable.</p></dd><dt id="uv-cache-clean--no-python-downloads"><a href="#uv-cache-clean--no-python-downloads"><code>--no-python-downloads</code></a></dt><dd><p>Disable automatic downloads of Python.</p>
</dd><dt id="uv-cache-clean--offline"><a href="#uv-cache-clean--offline"><code>--offline</code></a></dt><dd><p>Disable network access.</p>
<p>When disabled, uv will only use locally cached data and locally available files.</p>
<p>May also be set with the <code>UV_OFFLINE</code> environment variable.</p></dd><dt id="uv-cache-clean--project"><a href="#uv-cache-clean--project"><code>--project</code></a> <i>project</i></dt><dd><p>Run the command within the given project directory.</p>
<p>All <code>pyproject.toml</code>, <code>uv.toml</code>, and <code>.python-version</code> files will be discovered by walking up the directory tree from the project root, as will the project's virtual environment (<code>.venv</code>).</p>
<p>Other command-line arguments (such as relative paths) will be resolved relative to the current working directory.</p>
<p>See <code>--directory</code> to change the working directory entirely.</p>
<p>This setting has no effect when used in the <code>uv pip</code> interface.</p>
<p>May also be set with the <code>UV_PROJECT</code> environment variable.</p></dd><dt id="uv-cache-clean--quiet"><a href="#uv-cache-clean--quiet"><code>--quiet</code></a>, <code>-q</code></dt><dd><p>Use quiet output.</p>
<p>Repeating this option, e.g., <code>-qq</code>, will enable a silent mode in which uv will write no output to stdout.</p>
</dd><dt id="uv-cache-clean--verbose"><a href="#uv-cache-clean--verbose"><code>--verbose</code></a>, <code>-v</code></dt><dd><p>Use verbose output.</p>
<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (<a href="https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives">https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives</a>)</p>
</dd></dl>

### uv cache prune

Prune all unreachable objects from the cache

<h3 class="cli-reference">Usage</h3>

```
uv cache prune [OPTIONS]
```

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt id="uv-cache-prune--allow-insecure-host"><a href="#uv-cache-prune--allow-insecure-host"><code>--allow-insecure-host</code></a>, <code>--trusted-host</code> <i>allow-insecure-host</i></dt><dd><p>Allow insecure connections to a host.</p>
<p>Can be provided multiple times.</p>
<p>Expects to receive either a hostname (e.g., <code>localhost</code>), a host-port pair (e.g., <code>localhost:8080</code>), or a URL (e.g., <code>https://localhost</code>).</p>
<p>WARNING: Hosts included in this list will not be verified against the system's certificate store. Only use <code>--allow-insecure-host</code> in a secure network with verified sources, as it bypasses SSL verification and could expose you to MITM attacks.</p>
<p>May also be set with the <code>UV_INSECURE_HOST</code> environment variable.</p></dd><dt id="uv-cache-prune--cache-dir"><a href="#uv-cache-prune--cache-dir"><code>--cache-dir</code></a> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>
<p>Defaults to <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on macOS and Linux, and <code>%LOCALAPPDATA%\uv\cache</code> on Windows.</p>
<p>To view the location of the cache directory, run <code>uv cache dir</code>.</p>
<p>May also be set with the <code>UV_CACHE_DIR</code> environment variable.</p></dd><dt id="uv-cache-prune--ci"><a href="#uv-cache-prune--ci"><code>--ci</code></a></dt><dd><p>Optimize the cache for persistence in a continuous integration environment, like GitHub Actions.</p>
<p>By default, uv caches both the wheels that it builds from source and the pre-built wheels that it downloads directly, to enable high-performance package installation. In some scenarios, though, persisting pre-built wheels may be undesirable. For example, in GitHub Actions, it's faster to omit pre-built wheels from the cache and instead have re-download them on each run. However, it typically <em>is</em> faster to cache wheels that are built from source, since the wheel building process can be expensive, especially for extension modules.</p>
<p>In <code>--ci</code> mode, uv will prune any pre-built wheels from the cache, but retain any wheels that were built from source.</p>
</dd><dt id="uv-cache-prune--color"><a href="#uv-cache-prune--color"><code>--color</code></a> <i>color-choice</i></dt><dd><p>Control the use of color in output.</p>
<p>By default, uv will automatically detect support for colors when writing to a terminal.</p>
<p>Possible values:</p>
<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>
<li><code>always</code>:  Enables colored output regardless of the detected environment</li>
<li><code>never</code>:  Disables colored output</li>
</ul></dd><dt id="uv-cache-prune--config-file"><a href="#uv-cache-prune--config-file"><code>--config-file</code></a> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>
<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>
<p>May also be set with the <code>UV_CONFIG_FILE</code> environment variable.</p></dd><dt id="uv-cache-prune--directory"><a href="#uv-cache-prune--directory"><code>--directory</code></a> <i>directory</i></dt><dd><p>Change to the given directory prior to running the command.</p>
<p>Relative paths are resolved with the given directory as the base.</p>
<p>See <code>--project</code> to only change the project root directory.</p>
</dd><dt id="uv-cache-prune--help"><a href="#uv-cache-prune--help"><code>--help</code></a>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>
</dd><dt id="uv-cache-prune--managed-python"><a href="#uv-cache-prune--managed-python"><code>--managed-python</code></a></dt><dd><p>Require use of uv-managed Python versions.</p>
<p>By default, uv prefers using Python versions it manages. However, it will use system Python versions if a uv-managed Python is not installed. This option disables use of system Python versions.</p>
<p>May also be set with the <code>UV_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-cache-prune--native-tls"><a href="#uv-cache-prune--native-tls"><code>--native-tls</code></a></dt><dd><p>Whether to load TLS certificates from the platform's native certificate store.</p>
<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>
<p>However, in some cases, you may want to use the platform's native certificate store, especially if you're relying on a corporate trust root (e.g., for a mandatory proxy) that's included in your system's certificate store.</p>
<p>May also be set with the <code>UV_NATIVE_TLS</code> environment variable.</p></dd><dt id="uv-cache-prune--no-cache"><a href="#uv-cache-prune--no-cache"><code>--no-cache</code></a>, <code>--no-cache-dir</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>
<p>May also be set with the <code>UV_NO_CACHE</code> environment variable.</p></dd><dt id="uv-cache-prune--no-config"><a href="#uv-cache-prune--no-config"><code>--no-config</code></a></dt><dd><p>Avoid discovering configuration files (<code>pyproject.toml</code>, <code>uv.toml</code>).</p>
<p>Normally, configuration files are discovered in the current directory, parent directories, or user configuration directories.</p>
<p>May also be set with the <code>UV_NO_CONFIG</code> environment variable.</p></dd><dt id="uv-cache-prune--no-managed-python"><a href="#uv-cache-prune--no-managed-python"><code>--no-managed-python</code></a></dt><dd><p>Disable use of uv-managed Python versions.</p>
<p>Instead, uv will search for a suitable Python version on the system.</p>
<p>May also be set with the <code>UV_NO_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-cache-prune--no-progress"><a href="#uv-cache-prune--no-progress"><code>--no-progress</code></a></dt><dd><p>Hide all progress outputs.</p>
<p>For example, spinners or progress bars.</p>
<p>May also be set with the <code>UV_NO_PROGRESS</code> environment variable.</p></dd><dt id="uv-cache-prune--no-python-downloads"><a href="#uv-cache-prune--no-python-downloads"><code>--no-python-downloads</code></a></dt><dd><p>Disable automatic downloads of Python.</p>
</dd><dt id="uv-cache-prune--offline"><a href="#uv-cache-prune--offline"><code>--offline</code></a></dt><dd><p>Disable network access.</p>
<p>When disabled, uv will only use locally cached data and locally available files.</p>
<p>May also be set with the <code>UV_OFFLINE</code> environment variable.</p></dd><dt id="uv-cache-prune--project"><a href="#uv-cache-prune--project"><code>--project</code></a> <i>project</i></dt><dd><p>Run the command within the given project directory.</p>
<p>All <code>pyproject.toml</code>, <code>uv.toml</code>, and <code>.python-version</code> files will be discovered by walking up the directory tree from the project root, as will the project's virtual environment (<code>.venv</code>).</p>
<p>Other command-line arguments (such as relative paths) will be resolved relative to the current working directory.</p>
<p>See <code>--directory</code> to change the working directory entirely.</p>
<p>This setting has no effect when used in the <code>uv pip</code> interface.</p>
<p>May also be set with the <code>UV_PROJECT</code> environment variable.</p></dd><dt id="uv-cache-prune--quiet"><a href="#uv-cache-prune--quiet"><code>--quiet</code></a>, <code>-q</code></dt><dd><p>Use quiet output.</p>
<p>Repeating this option, e.g., <code>-qq</code>, will enable a silent mode in which uv will write no output to stdout.</p>
</dd><dt id="uv-cache-prune--verbose"><a href="#uv-cache-prune--verbose"><code>--verbose</code></a>, <code>-v</code></dt><dd><p>Use verbose output.</p>
<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (<a href="https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives">https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives</a>)</p>
</dd></dl>

### uv cache dir

Show the cache directory.

By default, the cache is stored in `$XDG_CACHE_HOME/uv` or `$HOME/.cache/uv` on Unix and `%LOCALAPPDATA%\uv\cache` on Windows.

When `--no-cache` is used, the cache is stored in a temporary directory and discarded when the process exits.

An alternative cache directory may be specified via the `cache-dir` setting, the `--cache-dir` option, or the `$UV_CACHE_DIR` environment variable.

Note that it is important for performance for the cache directory to be located on the same file system as the Python environment uv is operating on.

<h3 class="cli-reference">Usage</h3>

```
uv cache dir [OPTIONS]
```

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt id="uv-cache-dir--allow-insecure-host"><a href="#uv-cache-dir--allow-insecure-host"><code>--allow-insecure-host</code></a>, <code>--trusted-host</code> <i>allow-insecure-host</i></dt><dd><p>Allow insecure connections to a host.</p>
<p>Can be provided multiple times.</p>
<p>Expects to receive either a hostname (e.g., <code>localhost</code>), a host-port pair (e.g., <code>localhost:8080</code>), or a URL (e.g., <code>https://localhost</code>).</p>
<p>WARNING: Hosts included in this list will not be verified against the system's certificate store. Only use <code>--allow-insecure-host</code> in a secure network with verified sources, as it bypasses SSL verification and could expose you to MITM attacks.</p>
<p>May also be set with the <code>UV_INSECURE_HOST</code> environment variable.</p></dd><dt id="uv-cache-dir--cache-dir"><a href="#uv-cache-dir--cache-dir"><code>--cache-dir</code></a> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>
<p>Defaults to <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on macOS and Linux, and <code>%LOCALAPPDATA%\uv\cache</code> on Windows.</p>
<p>To view the location of the cache directory, run <code>uv cache dir</code>.</p>
<p>May also be set with the <code>UV_CACHE_DIR</code> environment variable.</p></dd><dt id="uv-cache-dir--color"><a href="#uv-cache-dir--color"><code>--color</code></a> <i>color-choice</i></dt><dd><p>Control the use of color in output.</p>
<p>By default, uv will automatically detect support for colors when writing to a terminal.</p>
<p>Possible values:</p>
<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>
<li><code>always</code>:  Enables colored output regardless of the detected environment</li>
<li><code>never</code>:  Disables colored output</li>
</ul></dd><dt id="uv-cache-dir--config-file"><a href="#uv-cache-dir--config-file"><code>--config-file</code></a> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>
<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>
<p>May also be set with the <code>UV_CONFIG_FILE</code> environment variable.</p></dd><dt id="uv-cache-dir--directory"><a href="#uv-cache-dir--directory"><code>--directory</code></a> <i>directory</i></dt><dd><p>Change to the given directory prior to running the command.</p>
<p>Relative paths are resolved with the given directory as the base.</p>
<p>See <code>--project</code> to only change the project root directory.</p>
</dd><dt id="uv-cache-dir--help"><a href="#uv-cache-dir--help"><code>--help</code></a>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>
</dd><dt id="uv-cache-dir--managed-python"><a href="#uv-cache-dir--managed-python"><code>--managed-python</code></a></dt><dd><p>Require use of uv-managed Python versions.</p>
<p>By default, uv prefers using Python versions it manages. However, it will use system Python versions if a uv-managed Python is not installed. This option disables use of system Python versions.</p>
<p>May also be set with the <code>UV_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-cache-dir--native-tls"><a href="#uv-cache-dir--native-tls"><code>--native-tls</code></a></dt><dd><p>Whether to load TLS certificates from the platform's native certificate store.</p>
<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>
<p>However, in some cases, you may want to use the platform's native certificate store, especially if you're relying on a corporate trust root (e.g., for a mandatory proxy) that's included in your system's certificate store.</p>
<p>May also be set with the <code>UV_NATIVE_TLS</code> environment variable.</p></dd><dt id="uv-cache-dir--no-cache"><a href="#uv-cache-dir--no-cache"><code>--no-cache</code></a>, <code>--no-cache-dir</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>
<p>May also be set with the <code>UV_NO_CACHE</code> environment variable.</p></dd><dt id="uv-cache-dir--no-config"><a href="#uv-cache-dir--no-config"><code>--no-config</code></a></dt><dd><p>Avoid discovering configuration files (<code>pyproject.toml</code>, <code>uv.toml</code>).</p>
<p>Normally, configuration files are discovered in the current directory, parent directories, or user configuration directories.</p>
<p>May also be set with the <code>UV_NO_CONFIG</code> environment variable.</p></dd><dt id="uv-cache-dir--no-managed-python"><a href="#uv-cache-dir--no-managed-python"><code>--no-managed-python</code></a></dt><dd><p>Disable use of uv-managed Python versions.</p>
<p>Instead, uv will search for a suitable Python version on the system.</p>
<p>May also be set with the <code>UV_NO_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-cache-dir--no-progress"><a href="#uv-cache-dir--no-progress"><code>--no-progress</code></a></dt><dd><p>Hide all progress outputs.</p>
<p>For example, spinners or progress bars.</p>
<p>May also be set with the <code>UV_NO_PROGRESS</code> environment variable.</p></dd><dt id="uv-cache-dir--no-python-downloads"><a href="#uv-cache-dir--no-python-downloads"><code>--no-python-downloads</code></a></dt><dd><p>Disable automatic downloads of Python.</p>
</dd><dt id="uv-cache-dir--offline"><a href="#uv-cache-dir--offline"><code>--offline</code></a></dt><dd><p>Disable network access.</p>
<p>When disabled, uv will only use locally cached data and locally available files.</p>
<p>May also be set with the <code>UV_OFFLINE</code> environment variable.</p></dd><dt id="uv-cache-dir--project"><a href="#uv-cache-dir--project"><code>--project</code></a> <i>project</i></dt><dd><p>Run the command within the given project directory.</p>
<p>All <code>pyproject.toml</code>, <code>uv.toml</code>, and <code>.python-version</code> files will be discovered by walking up the directory tree from the project root, as will the project's virtual environment (<code>.venv</code>).</p>
<p>Other command-line arguments (such as relative paths) will be resolved relative to the current working directory.</p>
<p>See <code>--directory</code> to change the working directory entirely.</p>
<p>This setting has no effect when used in the <code>uv pip</code> interface.</p>
<p>May also be set with the <code>UV_PROJECT</code> environment variable.</p></dd><dt id="uv-cache-dir--quiet"><a href="#uv-cache-dir--quiet"><code>--quiet</code></a>, <code>-q</code></dt><dd><p>Use quiet output.</p>
<p>Repeating this option, e.g., <code>-qq</code>, will enable a silent mode in which uv will write no output to stdout.</p>
</dd><dt id="uv-cache-dir--verbose"><a href="#uv-cache-dir--verbose"><code>--verbose</code></a>, <code>-v</code></dt><dd><p>Use verbose output.</p>
<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (<a href="https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives">https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives</a>)</p>
</dd></dl>

## uv self

Manage the uv executable

<h3 class="cli-reference">Usage</h3>

```
uv self [OPTIONS] <COMMAND>
```

<h3 class="cli-reference">Commands</h3>

<dl class="cli-reference"><dt><a href="#uv-self-update"><code>uv self update</code></a></dt><dd><p>Update uv</p></dd>
<dt><a href="#uv-self-version"><code>uv self version</code></a></dt><dd><p>Display uv's version</p></dd>
</dl>

### uv self update

Update uv

<h3 class="cli-reference">Usage</h3>

```
uv self update [OPTIONS] [TARGET_VERSION]
```

<h3 class="cli-reference">Arguments</h3>

<dl class="cli-reference"><dt id="uv-self-update--target_version"><a href="#uv-self-update--target_version"<code>TARGET_VERSION</code></a></dt><dd><p>Update to the specified version. If not provided, uv will update to the latest version</p>
</dd></dl>

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt id="uv-self-update--allow-insecure-host"><a href="#uv-self-update--allow-insecure-host"><code>--allow-insecure-host</code></a>, <code>--trusted-host</code> <i>allow-insecure-host</i></dt><dd><p>Allow insecure connections to a host.</p>
<p>Can be provided multiple times.</p>
<p>Expects to receive either a hostname (e.g., <code>localhost</code>), a host-port pair (e.g., <code>localhost:8080</code>), or a URL (e.g., <code>https://localhost</code>).</p>
<p>WARNING: Hosts included in this list will not be verified against the system's certificate store. Only use <code>--allow-insecure-host</code> in a secure network with verified sources, as it bypasses SSL verification and could expose you to MITM attacks.</p>
<p>May also be set with the <code>UV_INSECURE_HOST</code> environment variable.</p></dd><dt id="uv-self-update--cache-dir"><a href="#uv-self-update--cache-dir"><code>--cache-dir</code></a> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>
<p>Defaults to <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on macOS and Linux, and <code>%LOCALAPPDATA%\uv\cache</code> on Windows.</p>
<p>To view the location of the cache directory, run <code>uv cache dir</code>.</p>
<p>May also be set with the <code>UV_CACHE_DIR</code> environment variable.</p></dd><dt id="uv-self-update--color"><a href="#uv-self-update--color"><code>--color</code></a> <i>color-choice</i></dt><dd><p>Control the use of color in output.</p>
<p>By default, uv will automatically detect support for colors when writing to a terminal.</p>
<p>Possible values:</p>
<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>
<li><code>always</code>:  Enables colored output regardless of the detected environment</li>
<li><code>never</code>:  Disables colored output</li>
</ul></dd><dt id="uv-self-update--config-file"><a href="#uv-self-update--config-file"><code>--config-file</code></a> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>
<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>
<p>May also be set with the <code>UV_CONFIG_FILE</code> environment variable.</p></dd><dt id="uv-self-update--directory"><a href="#uv-self-update--directory"><code>--directory</code></a> <i>directory</i></dt><dd><p>Change to the given directory prior to running the command.</p>
<p>Relative paths are resolved with the given directory as the base.</p>
<p>See <code>--project</code> to only change the project root directory.</p>
</dd><dt id="uv-self-update--dry-run"><a href="#uv-self-update--dry-run"><code>--dry-run</code></a></dt><dd><p>Run without performing the update</p>
</dd><dt id="uv-self-update--help"><a href="#uv-self-update--help"><code>--help</code></a>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>
</dd><dt id="uv-self-update--managed-python"><a href="#uv-self-update--managed-python"><code>--managed-python</code></a></dt><dd><p>Require use of uv-managed Python versions.</p>
<p>By default, uv prefers using Python versions it manages. However, it will use system Python versions if a uv-managed Python is not installed. This option disables use of system Python versions.</p>
<p>May also be set with the <code>UV_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-self-update--native-tls"><a href="#uv-self-update--native-tls"><code>--native-tls</code></a></dt><dd><p>Whether to load TLS certificates from the platform's native certificate store.</p>
<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>
<p>However, in some cases, you may want to use the platform's native certificate store, especially if you're relying on a corporate trust root (e.g., for a mandatory proxy) that's included in your system's certificate store.</p>
<p>May also be set with the <code>UV_NATIVE_TLS</code> environment variable.</p></dd><dt id="uv-self-update--no-cache"><a href="#uv-self-update--no-cache"><code>--no-cache</code></a>, <code>--no-cache-dir</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>
<p>May also be set with the <code>UV_NO_CACHE</code> environment variable.</p></dd><dt id="uv-self-update--no-config"><a href="#uv-self-update--no-config"><code>--no-config</code></a></dt><dd><p>Avoid discovering configuration files (<code>pyproject.toml</code>, <code>uv.toml</code>).</p>
<p>Normally, configuration files are discovered in the current directory, parent directories, or user configuration directories.</p>
<p>May also be set with the <code>UV_NO_CONFIG</code> environment variable.</p></dd><dt id="uv-self-update--no-managed-python"><a href="#uv-self-update--no-managed-python"><code>--no-managed-python</code></a></dt><dd><p>Disable use of uv-managed Python versions.</p>
<p>Instead, uv will search for a suitable Python version on the system.</p>
<p>May also be set with the <code>UV_NO_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-self-update--no-progress"><a href="#uv-self-update--no-progress"><code>--no-progress</code></a></dt><dd><p>Hide all progress outputs.</p>
<p>For example, spinners or progress bars.</p>
<p>May also be set with the <code>UV_NO_PROGRESS</code> environment variable.</p></dd><dt id="uv-self-update--no-python-downloads"><a href="#uv-self-update--no-python-downloads"><code>--no-python-downloads</code></a></dt><dd><p>Disable automatic downloads of Python.</p>
</dd><dt id="uv-self-update--offline"><a href="#uv-self-update--offline"><code>--offline</code></a></dt><dd><p>Disable network access.</p>
<p>When disabled, uv will only use locally cached data and locally available files.</p>
<p>May also be set with the <code>UV_OFFLINE</code> environment variable.</p></dd><dt id="uv-self-update--project"><a href="#uv-self-update--project"><code>--project</code></a> <i>project</i></dt><dd><p>Run the command within the given project directory.</p>
<p>All <code>pyproject.toml</code>, <code>uv.toml</code>, and <code>.python-version</code> files will be discovered by walking up the directory tree from the project root, as will the project's virtual environment (<code>.venv</code>).</p>
<p>Other command-line arguments (such as relative paths) will be resolved relative to the current working directory.</p>
<p>See <code>--directory</code> to change the working directory entirely.</p>
<p>This setting has no effect when used in the <code>uv pip</code> interface.</p>
<p>May also be set with the <code>UV_PROJECT</code> environment variable.</p></dd><dt id="uv-self-update--quiet"><a href="#uv-self-update--quiet"><code>--quiet</code></a>, <code>-q</code></dt><dd><p>Use quiet output.</p>
<p>Repeating this option, e.g., <code>-qq</code>, will enable a silent mode in which uv will write no output to stdout.</p>
</dd><dt id="uv-self-update--token"><a href="#uv-self-update--token"><code>--token</code></a> <i>token</i></dt><dd><p>A GitHub token for authentication. A token is not required but can be used to reduce the chance of encountering rate limits</p>
<p>May also be set with the <code>UV_GITHUB_TOKEN</code> environment variable.</p></dd><dt id="uv-self-update--verbose"><a href="#uv-self-update--verbose"><code>--verbose</code></a>, <code>-v</code></dt><dd><p>Use verbose output.</p>
<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (<a href="https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives">https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives</a>)</p>
</dd></dl>

### uv self version

Display uv's version

<h3 class="cli-reference">Usage</h3>

```
uv self version [OPTIONS]
```

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt id="uv-self-version--allow-insecure-host"><a href="#uv-self-version--allow-insecure-host"><code>--allow-insecure-host</code></a>, <code>--trusted-host</code> <i>allow-insecure-host</i></dt><dd><p>Allow insecure connections to a host.</p>
<p>Can be provided multiple times.</p>
<p>Expects to receive either a hostname (e.g., <code>localhost</code>), a host-port pair (e.g., <code>localhost:8080</code>), or a URL (e.g., <code>https://localhost</code>).</p>
<p>WARNING: Hosts included in this list will not be verified against the system's certificate store. Only use <code>--allow-insecure-host</code> in a secure network with verified sources, as it bypasses SSL verification and could expose you to MITM attacks.</p>
<p>May also be set with the <code>UV_INSECURE_HOST</code> environment variable.</p></dd><dt id="uv-self-version--cache-dir"><a href="#uv-self-version--cache-dir"><code>--cache-dir</code></a> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>
<p>Defaults to <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on macOS and Linux, and <code>%LOCALAPPDATA%\uv\cache</code> on Windows.</p>
<p>To view the location of the cache directory, run <code>uv cache dir</code>.</p>
<p>May also be set with the <code>UV_CACHE_DIR</code> environment variable.</p></dd><dt id="uv-self-version--color"><a href="#uv-self-version--color"><code>--color</code></a> <i>color-choice</i></dt><dd><p>Control the use of color in output.</p>
<p>By default, uv will automatically detect support for colors when writing to a terminal.</p>
<p>Possible values:</p>
<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>
<li><code>always</code>:  Enables colored output regardless of the detected environment</li>
<li><code>never</code>:  Disables colored output</li>
</ul></dd><dt id="uv-self-version--config-file"><a href="#uv-self-version--config-file"><code>--config-file</code></a> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>
<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>
<p>May also be set with the <code>UV_CONFIG_FILE</code> environment variable.</p></dd><dt id="uv-self-version--directory"><a href="#uv-self-version--directory"><code>--directory</code></a> <i>directory</i></dt><dd><p>Change to the given directory prior to running the command.</p>
<p>Relative paths are resolved with the given directory as the base.</p>
<p>See <code>--project</code> to only change the project root directory.</p>
</dd><dt id="uv-self-version--help"><a href="#uv-self-version--help"><code>--help</code></a>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>
</dd><dt id="uv-self-version--managed-python"><a href="#uv-self-version--managed-python"><code>--managed-python</code></a></dt><dd><p>Require use of uv-managed Python versions.</p>
<p>By default, uv prefers using Python versions it manages. However, it will use system Python versions if a uv-managed Python is not installed. This option disables use of system Python versions.</p>
<p>May also be set with the <code>UV_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-self-version--native-tls"><a href="#uv-self-version--native-tls"><code>--native-tls</code></a></dt><dd><p>Whether to load TLS certificates from the platform's native certificate store.</p>
<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>
<p>However, in some cases, you may want to use the platform's native certificate store, especially if you're relying on a corporate trust root (e.g., for a mandatory proxy) that's included in your system's certificate store.</p>
<p>May also be set with the <code>UV_NATIVE_TLS</code> environment variable.</p></dd><dt id="uv-self-version--no-cache"><a href="#uv-self-version--no-cache"><code>--no-cache</code></a>, <code>--no-cache-dir</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>
<p>May also be set with the <code>UV_NO_CACHE</code> environment variable.</p></dd><dt id="uv-self-version--no-config"><a href="#uv-self-version--no-config"><code>--no-config</code></a></dt><dd><p>Avoid discovering configuration files (<code>pyproject.toml</code>, <code>uv.toml</code>).</p>
<p>Normally, configuration files are discovered in the current directory, parent directories, or user configuration directories.</p>
<p>May also be set with the <code>UV_NO_CONFIG</code> environment variable.</p></dd><dt id="uv-self-version--no-managed-python"><a href="#uv-self-version--no-managed-python"><code>--no-managed-python</code></a></dt><dd><p>Disable use of uv-managed Python versions.</p>
<p>Instead, uv will search for a suitable Python version on the system.</p>
<p>May also be set with the <code>UV_NO_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-self-version--no-progress"><a href="#uv-self-version--no-progress"><code>--no-progress</code></a></dt><dd><p>Hide all progress outputs.</p>
<p>For example, spinners or progress bars.</p>
<p>May also be set with the <code>UV_NO_PROGRESS</code> environment variable.</p></dd><dt id="uv-self-version--no-python-downloads"><a href="#uv-self-version--no-python-downloads"><code>--no-python-downloads</code></a></dt><dd><p>Disable automatic downloads of Python.</p>
</dd><dt id="uv-self-version--offline"><a href="#uv-self-version--offline"><code>--offline</code></a></dt><dd><p>Disable network access.</p>
<p>When disabled, uv will only use locally cached data and locally available files.</p>
<p>May also be set with the <code>UV_OFFLINE</code> environment variable.</p></dd><dt id="uv-self-version--output-format"><a href="#uv-self-version--output-format"><code>--output-format</code></a> <i>output-format</i></dt><dt id="uv-self-version--project"><a href="#uv-self-version--project"><code>--project</code></a> <i>project</i></dt><dd><p>Run the command within the given project directory.</p>
<p>All <code>pyproject.toml</code>, <code>uv.toml</code>, and <code>.python-version</code> files will be discovered by walking up the directory tree from the project root, as will the project's virtual environment (<code>.venv</code>).</p>
<p>Other command-line arguments (such as relative paths) will be resolved relative to the current working directory.</p>
<p>See <code>--directory</code> to change the working directory entirely.</p>
<p>This setting has no effect when used in the <code>uv pip</code> interface.</p>
<p>May also be set with the <code>UV_PROJECT</code> environment variable.</p></dd><dt id="uv-self-version--quiet"><a href="#uv-self-version--quiet"><code>--quiet</code></a>, <code>-q</code></dt><dd><p>Use quiet output.</p>
<p>Repeating this option, e.g., <code>-qq</code>, will enable a silent mode in which uv will write no output to stdout.</p>
</dd><dt id="uv-self-version--short"><a href="#uv-self-version--short"><code>--short</code></a></dt><dd><p>Only print the version</p>
</dd><dt id="uv-self-version--verbose"><a href="#uv-self-version--verbose"><code>--verbose</code></a>, <code>-v</code></dt><dd><p>Use verbose output.</p>
<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (<a href="https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives">https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives</a>)</p>
</dd></dl>

## uv generate-shell-completion

Generate shell completion

<h3 class="cli-reference">Usage</h3>

```
uv generate-shell-completion [OPTIONS] <SHELL>
```

<h3 class="cli-reference">Arguments</h3>

<dl class="cli-reference"><dt id="uv-generate-shell-completion--shell"><a href="#uv-generate-shell-completion--shell"<code>SHELL</code></a></dt><dd><p>The shell to generate the completion script for</p>
</dd></dl>

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt id="uv-generate-shell-completion--allow-insecure-host"><a href="#uv-generate-shell-completion--allow-insecure-host"><code>--allow-insecure-host</code></a>, <code>--trusted-host</code> <i>allow-insecure-host</i></dt><dd><p>Allow insecure connections to a host.</p>
<p>Can be provided multiple times.</p>
<p>Expects to receive either a hostname (e.g., <code>localhost</code>), a host-port pair (e.g., <code>localhost:8080</code>), or a URL (e.g., <code>https://localhost</code>).</p>
<p>WARNING: Hosts included in this list will not be verified against the system's certificate store. Only use <code>--allow-insecure-host</code> in a secure network with verified sources, as it bypasses SSL verification and could expose you to MITM attacks.</p>
<p>May also be set with the <code>UV_INSECURE_HOST</code> environment variable.</p></dd><dt id="uv-generate-shell-completion--directory"><a href="#uv-generate-shell-completion--directory"><code>--directory</code></a> <i>directory</i></dt><dd><p>Change to the given directory prior to running the command.</p>
<p>Relative paths are resolved with the given directory as the base.</p>
<p>See <code>--project</code> to only change the project root directory.</p>
</dd><dt id="uv-generate-shell-completion--managed-python"><a href="#uv-generate-shell-completion--managed-python"><code>--managed-python</code></a></dt><dd><p>Require use of uv-managed Python versions.</p>
<p>By default, uv prefers using Python versions it manages. However, it will use system Python versions if a uv-managed Python is not installed. This option disables use of system Python versions.</p>
<p>May also be set with the <code>UV_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-generate-shell-completion--no-managed-python"><a href="#uv-generate-shell-completion--no-managed-python"><code>--no-managed-python</code></a></dt><dd><p>Disable use of uv-managed Python versions.</p>
<p>Instead, uv will search for a suitable Python version on the system.</p>
<p>May also be set with the <code>UV_NO_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-generate-shell-completion--project"><a href="#uv-generate-shell-completion--project"><code>--project</code></a> <i>project</i></dt><dd><p>Run the command within the given project directory.</p>
<p>All <code>pyproject.toml</code>, <code>uv.toml</code>, and <code>.python-version</code> files will be discovered by walking up the directory tree from the project root, as will the project's virtual environment (<code>.venv</code>).</p>
<p>Other command-line arguments (such as relative paths) will be resolved relative to the current working directory.</p>
<p>See <code>--directory</code> to change the working directory entirely.</p>
<p>This setting has no effect when used in the <code>uv pip</code> interface.</p>
<p>May also be set with the <code>UV_PROJECT</code> environment variable.</p></dd></dl>

## uv help

Display documentation for a command

<h3 class="cli-reference">Usage</h3>

```
uv help [OPTIONS] [COMMAND]...
```

<h3 class="cli-reference">Arguments</h3>

<dl class="cli-reference"><dt id="uv-help--command"><a href="#uv-help--command"<code>COMMAND</code></a></dt></dl>

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt id="uv-help--allow-insecure-host"><a href="#uv-help--allow-insecure-host"><code>--allow-insecure-host</code></a>, <code>--trusted-host</code> <i>allow-insecure-host</i></dt><dd><p>Allow insecure connections to a host.</p>
<p>Can be provided multiple times.</p>
<p>Expects to receive either a hostname (e.g., <code>localhost</code>), a host-port pair (e.g., <code>localhost:8080</code>), or a URL (e.g., <code>https://localhost</code>).</p>
<p>WARNING: Hosts included in this list will not be verified against the system's certificate store. Only use <code>--allow-insecure-host</code> in a secure network with verified sources, as it bypasses SSL verification and could expose you to MITM attacks.</p>
<p>May also be set with the <code>UV_INSECURE_HOST</code> environment variable.</p></dd><dt id="uv-help--cache-dir"><a href="#uv-help--cache-dir"><code>--cache-dir</code></a> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>
<p>Defaults to <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on macOS and Linux, and <code>%LOCALAPPDATA%\uv\cache</code> on Windows.</p>
<p>To view the location of the cache directory, run <code>uv cache dir</code>.</p>
<p>May also be set with the <code>UV_CACHE_DIR</code> environment variable.</p></dd><dt id="uv-help--color"><a href="#uv-help--color"><code>--color</code></a> <i>color-choice</i></dt><dd><p>Control the use of color in output.</p>
<p>By default, uv will automatically detect support for colors when writing to a terminal.</p>
<p>Possible values:</p>
<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>
<li><code>always</code>:  Enables colored output regardless of the detected environment</li>
<li><code>never</code>:  Disables colored output</li>
</ul></dd><dt id="uv-help--config-file"><a href="#uv-help--config-file"><code>--config-file</code></a> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>
<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>
<p>May also be set with the <code>UV_CONFIG_FILE</code> environment variable.</p></dd><dt id="uv-help--directory"><a href="#uv-help--directory"><code>--directory</code></a> <i>directory</i></dt><dd><p>Change to the given directory prior to running the command.</p>
<p>Relative paths are resolved with the given directory as the base.</p>
<p>See <code>--project</code> to only change the project root directory.</p>
</dd><dt id="uv-help--help"><a href="#uv-help--help"><code>--help</code></a>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>
</dd><dt id="uv-help--managed-python"><a href="#uv-help--managed-python"><code>--managed-python</code></a></dt><dd><p>Require use of uv-managed Python versions.</p>
<p>By default, uv prefers using Python versions it manages. However, it will use system Python versions if a uv-managed Python is not installed. This option disables use of system Python versions.</p>
<p>May also be set with the <code>UV_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-help--native-tls"><a href="#uv-help--native-tls"><code>--native-tls</code></a></dt><dd><p>Whether to load TLS certificates from the platform's native certificate store.</p>
<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>
<p>However, in some cases, you may want to use the platform's native certificate store, especially if you're relying on a corporate trust root (e.g., for a mandatory proxy) that's included in your system's certificate store.</p>
<p>May also be set with the <code>UV_NATIVE_TLS</code> environment variable.</p></dd><dt id="uv-help--no-cache"><a href="#uv-help--no-cache"><code>--no-cache</code></a>, <code>--no-cache-dir</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>
<p>May also be set with the <code>UV_NO_CACHE</code> environment variable.</p></dd><dt id="uv-help--no-config"><a href="#uv-help--no-config"><code>--no-config</code></a></dt><dd><p>Avoid discovering configuration files (<code>pyproject.toml</code>, <code>uv.toml</code>).</p>
<p>Normally, configuration files are discovered in the current directory, parent directories, or user configuration directories.</p>
<p>May also be set with the <code>UV_NO_CONFIG</code> environment variable.</p></dd><dt id="uv-help--no-managed-python"><a href="#uv-help--no-managed-python"><code>--no-managed-python</code></a></dt><dd><p>Disable use of uv-managed Python versions.</p>
<p>Instead, uv will search for a suitable Python version on the system.</p>
<p>May also be set with the <code>UV_NO_MANAGED_PYTHON</code> environment variable.</p></dd><dt id="uv-help--no-pager"><a href="#uv-help--no-pager"><code>--no-pager</code></a></dt><dd><p>Disable pager when printing help</p>
</dd><dt id="uv-help--no-progress"><a href="#uv-help--no-progress"><code>--no-progress</code></a></dt><dd><p>Hide all progress outputs.</p>
<p>For example, spinners or progress bars.</p>
<p>May also be set with the <code>UV_NO_PROGRESS</code> environment variable.</p></dd><dt id="uv-help--no-python-downloads"><a href="#uv-help--no-python-downloads"><code>--no-python-downloads</code></a></dt><dd><p>Disable automatic downloads of Python.</p>
</dd><dt id="uv-help--offline"><a href="#uv-help--offline"><code>--offline</code></a></dt><dd><p>Disable network access.</p>
<p>When disabled, uv will only use locally cached data and locally available files.</p>
<p>May also be set with the <code>UV_OFFLINE</code> environment variable.</p></dd><dt id="uv-help--project"><a href="#uv-help--project"><code>--project</code></a> <i>project</i></dt><dd><p>Run the command within the given project directory.</p>
<p>All <code>pyproject.toml</code>, <code>uv.toml</code>, and <code>.python-version</code> files will be discovered by walking up the directory tree from the project root, as will the project's virtual environment (<code>.venv</code>).</p>
<p>Other command-line arguments (such as relative paths) will be resolved relative to the current working directory.</p>
<p>See <code>--directory</code> to change the working directory entirely.</p>
<p>This setting has no effect when used in the <code>uv pip</code> interface.</p>
<p>May also be set with the <code>UV_PROJECT</code> environment variable.</p></dd><dt id="uv-help--quiet"><a href="#uv-help--quiet"><code>--quiet</code></a>, <code>-q</code></dt><dd><p>Use quiet output.</p>
<p>Repeating this option, e.g., <code>-qq</code>, will enable a silent mode in which uv will write no output to stdout.</p>
</dd><dt id="uv-help--verbose"><a href="#uv-help--verbose"><code>--verbose</code></a>, <code>-v</code></dt><dd><p>Use verbose output.</p>
<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (<a href="https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives">https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives</a>)</p>
</dd></dl>

