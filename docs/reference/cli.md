# CLI Reference

## uv

An extremely fast Python package manager.

<h3 class="cli-reference">Usage</h3>

```
uv [OPTIONS] <COMMAND>
```

<h3 class="cli-reference">Commands</h3>

<dl class="cli-reference"><dt><a href="#uv-run"><code>uv run</code></a></dt><dd><p>Run a command or script (experimental)</p>
</dd>
<dt><a href="#uv-init"><code>uv init</code></a></dt><dd><p>Create a new project (experimental)</p>
</dd>
<dt><a href="#uv-add"><code>uv add</code></a></dt><dd><p>Add dependencies to the project (experimental)</p>
</dd>
<dt><a href="#uv-remove"><code>uv remove</code></a></dt><dd><p>Remove dependencies from the project (experimental)</p>
</dd>
<dt><a href="#uv-sync"><code>uv sync</code></a></dt><dd><p>Update the project&#8217;s environment (experimental)</p>
</dd>
<dt><a href="#uv-lock"><code>uv lock</code></a></dt><dd><p>Update the project&#8217;s lockfile (experimental)</p>
</dd>
<dt><a href="#uv-tree"><code>uv tree</code></a></dt><dd><p>Display the project&#8217;s dependency tree (experimental)</p>
</dd>
<dt><a href="#uv-tool"><code>uv tool</code></a></dt><dd><p>Run and install commands provided by Python packages (experimental)</p>
</dd>
<dt><a href="#uv-python"><code>uv python</code></a></dt><dd><p>Manage Python versions and installations (experimental)</p>
</dd>
<dt><a href="#uv-pip"><code>uv pip</code></a></dt><dd><p>Manage Python packages with a pip-compatible interface</p>
</dd>
<dt><a href="#uv-venv"><code>uv venv</code></a></dt><dd><p>Create a virtual environment</p>
</dd>
<dt><a href="#uv-cache"><code>uv cache</code></a></dt><dd><p>Manage uv&#8217;s cache</p>
</dd>
<dt><a href="#uv-version"><code>uv version</code></a></dt><dd><p>Display uv&#8217;s version</p>
</dd>
<dt><a href="#uv-help"><code>uv help</code></a></dt><dd><p>Display documentation for a command</p>
</dd>
</dl>

## uv run

Run a command or script (experimental).

Ensures that the command runs in a Python environment.

When used with a file ending in `.py`, the file will be treated as a script and run with a Python interpreter, i.e., `uv run file.py` is equivalent to `uv run python file.py`. If the script contains inline dependency metadata, it will be installed into an isolated, ephemeral environment.

When used in a project, the project environment will be created and updated before invoking the command.

When used outside a project, if a virtual environment can be found in the current directory or a parent directory, the command will be run in that environment. Otherwise, the command will be run in the environment of the discovered interpreter.

Arguments following the command (or script) are not interpreted as arguments to uv. All options to uv must be provided before the command, e.g., `uv run --verbose foo`. A `--` can be used to separate the command from uv options for clarity, e.g., `uv run --python 3.12 -- python`.

<h3 class="cli-reference">Usage</h3>

```
uv run [OPTIONS] <COMMAND>
```

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt><code>--all-extras</code></dt><dd><p>Include all optional dependencies.</p>

<p>Optional dependencies are defined via <code>project.optional-dependencies</code> in a <code>pyproject.toml</code>.</p>

<p>This option is only available when running in a project.</p>

</dd><dt><code>--cache-dir</code> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>

<p>Defaults to <code>$HOME/Library/Caches/uv</code> on macOS, <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on Linux, and <code>{FOLDERID_LocalAppData}\uv\cache</code> on Windows.</p>

</dd><dt><code>--color</code> <i>color-choice</i></dt><dd><p>Control colors in output</p>

<p>[default: auto]</p>
<p>Possible values:</p>

<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>

<li><code>always</code>:  Enables colored output regardless of the detected environment</li>

<li><code>never</code>:  Disables colored output</li>
</ul>
</dd><dt><code>--compile-bytecode</code></dt><dd><p>Compile Python files to bytecode after installation.</p>

<p>By default, uv does not compile Python (<code>.py</code>) files to bytecode (<code>__pycache__/*.pyc</code>); instead, compilation is performed lazily the first time a module is imported. For use-cases in which start time is critical, such as CLI applications and Docker containers, this option can be enabled to trade longer installation times for faster start times.</p>

<p>When enabled, uv will process the entire site-packages directory (including packages that are not being modified by the current operation) for consistency. Like pip, it will also ignore errors.</p>

</dd><dt><code>--config-file</code> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>

<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>

</dd><dt><code>--config-setting</code>, <code>-C</code> <i>config-setting</i></dt><dd><p>Settings to pass to the PEP 517 build backend, specified as <code>KEY=VALUE</code> pairs</p>

</dd><dt><code>--exclude-newer</code> <i>exclude-newer</i></dt><dd><p>Limit candidate packages to those that were uploaded prior to the given date.</p>

<p>Accepts both RFC 3339 timestamps (e.g., <code>2006-12-02T02:07:43Z</code>) and UTC dates in the same format (e.g., <code>2006-12-02</code>).</p>

</dd><dt><code>--extra</code> <i>extra</i></dt><dd><p>Include optional dependencies from the extra group name.</p>

<p>May be provided more than once.</p>

<p>Optional dependencies are defined via <code>project.optional-dependencies</code> in a <code>pyproject.toml</code>.</p>

<p>This option is only available when running in a project.</p>

</dd><dt><code>--extra-index-url</code> <i>extra-index-url</i></dt><dd><p>Extra URLs of package indexes to use, in addition to <code>--index-url</code>.</p>

<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>

<p>All indexes provided via this flag take priority over the index specified by <code>--index-url</code> (which defaults to PyPI). When multiple <code>--extra-index-url</code> flags are provided, earlier values take priority.</p>

</dd><dt><code>--find-links</code>, <code>-f</code> <i>find-links</i></dt><dd><p>Locations to search for candidate distributions, in addition to those found in the registry indexes.</p>

<p>If a path, the target must be a directory that contains packages as wheel files (<code>.whl</code>) or source distributions (<code>.tar.gz</code> or <code>.zip</code>) at the top level.</p>

<p>If a URL, the page must contain a flat list of links to package files adhering to the formats described above.</p>

</dd><dt><code>--frozen</code></dt><dd><p>Run without updating the <code>uv.lock</code> file.</p>

<p>Instead of checking if the lockfile is up-to-date, uses the versions in the lockfile as the source of truth. If the lockfile is missing, uv will exit with an error. If the <code>pyproject.toml</code> includes changes to dependencies that have not been included in the lockfile yet, they will not be present in the environment.</p>

</dd><dt><code>--help</code>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>

</dd><dt><code>--index-strategy</code> <i>index-strategy</i></dt><dd><p>The strategy to use when resolving against multiple index URLs.</p>

<p>By default, uv will stop at the first index on which a given package is available, and limit resolutions to those present on that first index (<code>first-match</code>). This prevents &quot;dependency confusion&quot; attacks, whereby an attack can upload a malicious package under the same name to a secondary.</p>

<p>Possible values:</p>

<ul>
<li><code>first-index</code>:  Only use results from the first index that returns a match for a given package name</li>

<li><code>unsafe-first-match</code>:  Search for every package name across all indexes, exhausting the versions from the first index before moving on to the next</li>

<li><code>unsafe-best-match</code>:  Search for every package name across all indexes, preferring the &quot;best&quot; version found. If a package version is in multiple indexes, only look at the entry for the first index</li>
</ul>
</dd><dt><code>--index-url</code>, <code>-i</code> <i>index-url</i></dt><dd><p>The URL of the Python package index (by default: &lt;https://pypi.org/simple&gt;).</p>

<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>

<p>The index given by this flag is given lower priority than all other indexes specified via the <code>--extra-index-url</code> flag.</p>

</dd><dt><code>--isolated</code></dt><dd><p>Run the command in an isolated virtual environment.</p>

<p>Usually, the project environment is reused for performance. This option forces a fresh environment to be used for the project, enforcing strict isolation between dependencies and declaration of requirements.</p>

<p>An editable installation is still used for the project.</p>

<p>When used with <code>--with</code> or <code>--with-requirements</code>, the additional dependencies will still be layered in a second environment.</p>

</dd><dt><code>--keyring-provider</code> <i>keyring-provider</i></dt><dd><p>Attempt to use <code>keyring</code> for authentication for index URLs.</p>

<p>At present, only <code>--keyring-provider subprocess</code> is supported, which configures uv to use the <code>keyring</code> CLI to handle authentication.</p>

<p>Defaults to <code>disabled</code>.</p>

<p>Possible values:</p>

<ul>
<li><code>disabled</code>:  Do not use keyring for credential lookup</li>

<li><code>subprocess</code>:  Use the <code>keyring</code> command for credential lookup</li>
</ul>
</dd><dt><code>--link-mode</code> <i>link-mode</i></dt><dd><p>The method to use when installing packages from the global cache.</p>

<p>Defaults to <code>clone</code> (also known as Copy-on-Write) on macOS, and <code>hardlink</code> on Linux and Windows.</p>

<p>Possible values:</p>

<ul>
<li><code>clone</code>:  Clone (i.e., copy-on-write) packages from the wheel into the <code>site-packages</code> directory</li>

<li><code>copy</code>:  Copy packages from the wheel into the <code>site-packages</code> directory</li>

<li><code>hardlink</code>:  Hard link packages from the wheel into the <code>site-packages</code> directory</li>

<li><code>symlink</code>:  Symbolically link packages from the wheel into the <code>site-packages</code> directory</li>
</ul>
</dd><dt><code>--locked</code></dt><dd><p>Assert that the <code>uv.lock</code> will remain unchanged.</p>

<p>Requires that the lockfile is up-to-date. If the lockfile is missing or needs to be updated, uv will exit with an error.</p>

</dd><dt><code>--native-tls</code></dt><dd><p>Whether to load TLS certificates from the platform&#8217;s native certificate store.</p>

<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>

<p>However, in some cases, you may want to use the platform&#8217;s native certificate store, especially if you&#8217;re relying on a corporate trust root (e.g., for a mandatory proxy) that&#8217;s included in your system&#8217;s certificate store.</p>

</dd><dt><code>--no-binary</code></dt><dd><p>Don&#8217;t install pre-built wheels.</p>

<p>The given packages will be built and installed from source. The resolver will still use pre-built wheels to extract package metadata, if available.</p>

</dd><dt><code>--no-binary-package</code> <i>no-binary-package</i></dt><dd><p>Don&#8217;t install pre-built wheels for a specific package</p>

</dd><dt><code>--no-build</code></dt><dd><p>Don&#8217;t build source distributions.</p>

<p>When enabled, resolving will not run arbitrary Python code. The cached wheels of already-built source distributions will be reused, but operations that require building distributions will exit with an error.</p>

</dd><dt><code>--no-build-isolation</code></dt><dd><p>Disable isolation when building source distributions.</p>

<p>Assumes that build dependencies specified by PEP 518 are already installed.</p>

</dd><dt><code>--no-build-isolation-package</code> <i>no-build-isolation-package</i></dt><dd><p>Disable isolation when building source distributions for a specific package.</p>

<p>Assumes that the packages&#8217; build dependencies specified by PEP 518  are already installed.</p>

</dd><dt><code>--no-build-package</code> <i>no-build-package</i></dt><dd><p>Don&#8217;t build source distributions for a specific package</p>

</dd><dt><code>--no-cache</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>

</dd><dt><code>--no-config</code></dt><dd><p>Avoid discovering configuration files (<code>pyproject.toml</code>, <code>uv.toml</code>).</p>

<p>Normally, configuration files are discovered in the current directory, parent directories, or user configuration directories.</p>

</dd><dt><code>--no-dev</code></dt><dd><p>Omit development dependencies.</p>

<p>This option is only available when running in a project.</p>

</dd><dt><code>--no-index</code></dt><dd><p>Ignore the registry index (e.g., PyPI), instead relying on direct URL dependencies and those provided via <code>--find-links</code></p>

</dd><dt><code>--no-progress</code></dt><dd><p>Hide all progress outputs.</p>

<p>For example, spinners or progress bars.</p>

</dd><dt><code>--no-project</code></dt><dd><p>Avoid discovering the project or workspace.</p>

<p>Instead of searching for projects in the current directory and parent directories, run in an isolated, ephemeral environment populated by the <code>--with</code> requirements.</p>

<p>If a virtual environment is active or found in a current or parent directory, it will be used as if there was no project or workspace.</p>

</dd><dt><code>--no-python-downloads</code></dt><dd><p>Disable automatic downloads of Python</p>

</dd><dt><code>--no-sources</code></dt><dd><p>Ignore the <code>tool.uv.sources</code> table when resolving dependencies. Used to lock against the standards-compliant, publishable package metadata, as opposed to using any local or Git sources</p>

</dd><dt><code>--offline</code></dt><dd><p>Disable network access.</p>

<p>When disabled, uv will only use locally cached data and locally available files.</p>

</dd><dt><code>--package</code> <i>package</i></dt><dd><p>Run the command in a specific package in the workspace.</p>

<p>If not in a workspace, or if the workspace member does not exist, uv will exit with an error.</p>

</dd><dt><code>--prerelease</code> <i>prerelease</i></dt><dd><p>The strategy to use when considering pre-release versions.</p>

<p>By default, uv will accept pre-releases for packages that <em>only</em> publish pre-releases, along with first-party requirements that contain an explicit pre-release marker in the declared specifiers (<code>if-necessary-or-explicit</code>).</p>

<p>Possible values:</p>

<ul>
<li><code>disallow</code>:  Disallow all pre-release versions</li>

<li><code>allow</code>:  Allow all pre-release versions</li>

<li><code>if-necessary</code>:  Allow pre-release versions if all versions of a package are pre-release</li>

<li><code>explicit</code>:  Allow pre-release versions for first-party packages with explicit pre-release markers in their version requirements</li>

<li><code>if-necessary-or-explicit</code>:  Allow pre-release versions if all versions of a package are pre-release, or if the package has an explicit pre-release marker in its version requirements</li>
</ul>
</dd><dt><code>--python</code>, <code>-p</code> <i>python</i></dt><dd><p>The Python interpreter to use for the run environment.</p>

<p>If the interpreter request is satisfied by a discovered environment, the environment will be used.</p>

<p>See <a href="#uv-python">uv python</a> to view supported request formats.</p>

</dd><dt><code>--python-preference</code> <i>python-preference</i></dt><dd><p>Whether to prefer uv-managed or system Python installations.</p>

<p>By default, uv prefers using Python versions it manages. However, it will use system Python installations if a uv-managed Python is not installed. This option allows prioritizing or ignoring system Python installations.</p>

<p>Possible values:</p>

<ul>
<li><code>only-managed</code>:  Only use managed Python installations; never use system Python installations</li>

<li><code>managed</code>:  Prefer managed Python installations over system Python installations</li>

<li><code>system</code>:  Prefer system Python installations over managed Python installations</li>

<li><code>only-system</code>:  Only use system Python installations; never use managed Python installations</li>
</ul>
</dd><dt><code>--quiet</code>, <code>-q</code></dt><dd><p>Do not print any output</p>

</dd><dt><code>--refresh</code></dt><dd><p>Refresh all cached data</p>

</dd><dt><code>--refresh-package</code> <i>refresh-package</i></dt><dd><p>Refresh cached data for a specific package</p>

</dd><dt><code>--reinstall</code></dt><dd><p>Reinstall all packages, regardless of whether they&#8217;re already installed. Implies <code>--refresh</code></p>

</dd><dt><code>--reinstall-package</code> <i>reinstall-package</i></dt><dd><p>Reinstall a specific package, regardless of whether it&#8217;s already installed. Implies <code>--refresh-package</code></p>

</dd><dt><code>--resolution</code> <i>resolution</i></dt><dd><p>The strategy to use when selecting between the different compatible versions for a given package requirement.</p>

<p>By default, uv will use the latest compatible version of each package (<code>highest</code>).</p>

<p>Possible values:</p>

<ul>
<li><code>highest</code>:  Resolve the highest compatible version of each package</li>

<li><code>lowest</code>:  Resolve the lowest compatible version of each package</li>

<li><code>lowest-direct</code>:  Resolve the lowest compatible version of any direct dependencies, and the highest compatible version of any transitive dependencies</li>
</ul>
</dd><dt><code>--upgrade</code>, <code>-U</code></dt><dd><p>Allow package upgrades, ignoring pinned versions in any existing output file. Implies <code>--refresh</code></p>

</dd><dt><code>--upgrade-package</code>, <code>-P</code> <i>upgrade-package</i></dt><dd><p>Allow upgrades for a specific package, ignoring pinned versions in any existing output file. Implies <code>--refresh-package</code></p>

</dd><dt><code>--verbose</code>, <code>-v</code></dt><dd><p>Use verbose output.</p>

<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (&lt;https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives&gt;)</p>

</dd><dt><code>--version</code>, <code>-V</code></dt><dd><p>Display the uv version</p>

</dd><dt><code>--with</code> <i>with</i></dt><dd><p>Run with the given packages installed.</p>

<p>When used in a project, these dependencies will be layered on top of the project environment in a separate, ephemeral environment. These dependencies are allowed to conflict with those specified by the project.</p>

</dd><dt><code>--with-requirements</code> <i>with-requirements</i></dt><dd><p>Run with all packages listed in the given <code>requirements.txt</code> files.</p>

<p>The same environment semantics as <code>--with</code> apply.</p>

<p>Using <code>pyproject.toml</code>, <code>setup.py</code>, or <code>setup.cfg</code> files is not allowed.</p>

</dd></dl>

## uv init

Create a new project (experimental).

Follows the `pyproject.toml` specification.

If a `pyproject.toml` already exists at the target, uv will exit with an error.

If a `pyproject.toml` is found in any of the parent directories of the target path, the project will be added as a workspace member of the parent.

Some project state is not created until needed, e.g., the project virtual environment (`.venv`) and lockfile (`uv.lock`) are lazily created during the first sync.

<h3 class="cli-reference">Usage</h3>

```
uv init [OPTIONS] [PATH]
```

<h3 class="cli-reference">Arguments</h3>

<dl class="cli-reference"><dt><code>PATH</code></dt><dd><p>The path to use for the project.</p>

<p>Defaults to the current working directory. Accepts relative and absolute paths.</p>

<p>If a <code>pyproject.toml</code> is found in any of the parent directories of the target path, the project will be added as a workspace member of the parent, unless <code>--no-workspace</code> is provided.</p>

</dd></dl>

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt><code>--cache-dir</code> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>

<p>Defaults to <code>$HOME/Library/Caches/uv</code> on macOS, <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on Linux, and <code>{FOLDERID_LocalAppData}\uv\cache</code> on Windows.</p>

</dd><dt><code>--color</code> <i>color-choice</i></dt><dd><p>Control colors in output</p>

<p>[default: auto]</p>
<p>Possible values:</p>

<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>

<li><code>always</code>:  Enables colored output regardless of the detected environment</li>

<li><code>never</code>:  Disables colored output</li>
</ul>
</dd><dt><code>--config-file</code> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>

<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>

</dd><dt><code>--help</code>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>

</dd><dt><code>--name</code> <i>name</i></dt><dd><p>The name of the project.</p>

<p>Defaults to the name of the directory.</p>

</dd><dt><code>--native-tls</code></dt><dd><p>Whether to load TLS certificates from the platform&#8217;s native certificate store.</p>

<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>

<p>However, in some cases, you may want to use the platform&#8217;s native certificate store, especially if you&#8217;re relying on a corporate trust root (e.g., for a mandatory proxy) that&#8217;s included in your system&#8217;s certificate store.</p>

</dd><dt><code>--no-cache</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>

</dd><dt><code>--no-config</code></dt><dd><p>Avoid discovering configuration files (<code>pyproject.toml</code>, <code>uv.toml</code>).</p>

<p>Normally, configuration files are discovered in the current directory, parent directories, or user configuration directories.</p>

</dd><dt><code>--no-progress</code></dt><dd><p>Hide all progress outputs.</p>

<p>For example, spinners or progress bars.</p>

</dd><dt><code>--no-python-downloads</code></dt><dd><p>Disable automatic downloads of Python</p>

</dd><dt><code>--no-readme</code></dt><dd><p>Do not create a <code>README.md</code> file</p>

</dd><dt><code>--no-workspace</code></dt><dd><p>Avoid discovering a workspace.</p>

<p>Instead, create a standalone project.</p>

<p>By default, uv searches for workspaces in the current directory or any parent directory.</p>

</dd><dt><code>--offline</code></dt><dd><p>Disable network access.</p>

<p>When disabled, uv will only use locally cached data and locally available files.</p>

</dd><dt><code>--python</code>, <code>-p</code> <i>python</i></dt><dd><p>The Python interpreter to use to determine the minimum supported Python version.</p>

<p>See <a href="#uv-python">uv python</a> to view supported request formats.</p>

</dd><dt><code>--python-preference</code> <i>python-preference</i></dt><dd><p>Whether to prefer uv-managed or system Python installations.</p>

<p>By default, uv prefers using Python versions it manages. However, it will use system Python installations if a uv-managed Python is not installed. This option allows prioritizing or ignoring system Python installations.</p>

<p>Possible values:</p>

<ul>
<li><code>only-managed</code>:  Only use managed Python installations; never use system Python installations</li>

<li><code>managed</code>:  Prefer managed Python installations over system Python installations</li>

<li><code>system</code>:  Prefer system Python installations over managed Python installations</li>

<li><code>only-system</code>:  Only use system Python installations; never use managed Python installations</li>
</ul>
</dd><dt><code>--quiet</code>, <code>-q</code></dt><dd><p>Do not print any output</p>

</dd><dt><code>--verbose</code>, <code>-v</code></dt><dd><p>Use verbose output.</p>

<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (&lt;https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives&gt;)</p>

</dd><dt><code>--version</code>, <code>-V</code></dt><dd><p>Display the uv version</p>

</dd><dt><code>--virtual</code></dt><dd><p>Create a virtual workspace instead of a project.</p>

<p>A virtual workspace does not define project dependencies and cannot be published. Instead, workspace members declare project dependencies. Development dependencies may still be declared.</p>

</dd></dl>

## uv add

Add dependencies to the project (experimental).

Dependencies are added to the project's `pyproject.toml` file.

If no constraint or URL is provided for a dependency, a lower bound is added equal to the latest compatible version of the package, e.g., `>=1.2.3`, unless `--frozen` is provided, in which case no resolution is performed.

The lockfile and project environment will be updated to reflect the added dependencies. To skip updating the lockfile, use `--frozen`. To skip updating the environment, use `--no-sync`.

If any of the requested dependencies cannot be found, uv will exit with an error, unless the `--frozen` flag is provided, in which case uv will add the dependencies verbatim without checking that they exist or are compatible with the project.

uv will search for a project in the current directory or any parent directory. If a project cannot be found, uv will exit with an error.

<h3 class="cli-reference">Usage</h3>

```
uv add [OPTIONS] <REQUIREMENTS>...
```

<h3 class="cli-reference">Arguments</h3>

<dl class="cli-reference"><dt><code>REQUIREMENTS</code></dt><dd><p>The packages to add, as PEP 508 requirements (e.g., <code>ruff==0.5.0</code>)</p>

</dd></dl>

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt><code>--branch</code> <i>branch</i></dt><dd><p>Branch to use when adding a dependency from Git</p>

</dd><dt><code>--cache-dir</code> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>

<p>Defaults to <code>$HOME/Library/Caches/uv</code> on macOS, <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on Linux, and <code>{FOLDERID_LocalAppData}\uv\cache</code> on Windows.</p>

</dd><dt><code>--color</code> <i>color-choice</i></dt><dd><p>Control colors in output</p>

<p>[default: auto]</p>
<p>Possible values:</p>

<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>

<li><code>always</code>:  Enables colored output regardless of the detected environment</li>

<li><code>never</code>:  Disables colored output</li>
</ul>
</dd><dt><code>--compile-bytecode</code></dt><dd><p>Compile Python files to bytecode after installation.</p>

<p>By default, uv does not compile Python (<code>.py</code>) files to bytecode (<code>__pycache__/*.pyc</code>); instead, compilation is performed lazily the first time a module is imported. For use-cases in which start time is critical, such as CLI applications and Docker containers, this option can be enabled to trade longer installation times for faster start times.</p>

<p>When enabled, uv will process the entire site-packages directory (including packages that are not being modified by the current operation) for consistency. Like pip, it will also ignore errors.</p>

</dd><dt><code>--config-file</code> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>

<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>

</dd><dt><code>--config-setting</code>, <code>-C</code> <i>config-setting</i></dt><dd><p>Settings to pass to the PEP 517 build backend, specified as <code>KEY=VALUE</code> pairs</p>

</dd><dt><code>--dev</code></dt><dd><p>Add the requirements as development dependencies</p>

</dd><dt><code>--exclude-newer</code> <i>exclude-newer</i></dt><dd><p>Limit candidate packages to those that were uploaded prior to the given date.</p>

<p>Accepts both RFC 3339 timestamps (e.g., <code>2006-12-02T02:07:43Z</code>) and UTC dates in the same format (e.g., <code>2006-12-02</code>).</p>

</dd><dt><code>--extra</code> <i>extra</i></dt><dd><p>Extras to enable for the dependency.</p>

<p>May be provided more than once.</p>

<p>To add this dependency to an optional group in the current project instead, see <code>--optional</code>.</p>

</dd><dt><code>--extra-index-url</code> <i>extra-index-url</i></dt><dd><p>Extra URLs of package indexes to use, in addition to <code>--index-url</code>.</p>

<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>

<p>All indexes provided via this flag take priority over the index specified by <code>--index-url</code> (which defaults to PyPI). When multiple <code>--extra-index-url</code> flags are provided, earlier values take priority.</p>

</dd><dt><code>--find-links</code>, <code>-f</code> <i>find-links</i></dt><dd><p>Locations to search for candidate distributions, in addition to those found in the registry indexes.</p>

<p>If a path, the target must be a directory that contains packages as wheel files (<code>.whl</code>) or source distributions (<code>.tar.gz</code> or <code>.zip</code>) at the top level.</p>

<p>If a URL, the page must contain a flat list of links to package files adhering to the formats described above.</p>

</dd><dt><code>--frozen</code></dt><dd><p>Add dependencies without re-locking the project.</p>

<p>The project environment will not be synced.</p>

</dd><dt><code>--help</code>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>

</dd><dt><code>--index-strategy</code> <i>index-strategy</i></dt><dd><p>The strategy to use when resolving against multiple index URLs.</p>

<p>By default, uv will stop at the first index on which a given package is available, and limit resolutions to those present on that first index (<code>first-match</code>). This prevents &quot;dependency confusion&quot; attacks, whereby an attack can upload a malicious package under the same name to a secondary.</p>

<p>Possible values:</p>

<ul>
<li><code>first-index</code>:  Only use results from the first index that returns a match for a given package name</li>

<li><code>unsafe-first-match</code>:  Search for every package name across all indexes, exhausting the versions from the first index before moving on to the next</li>

<li><code>unsafe-best-match</code>:  Search for every package name across all indexes, preferring the &quot;best&quot; version found. If a package version is in multiple indexes, only look at the entry for the first index</li>
</ul>
</dd><dt><code>--index-url</code>, <code>-i</code> <i>index-url</i></dt><dd><p>The URL of the Python package index (by default: &lt;https://pypi.org/simple&gt;).</p>

<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>

<p>The index given by this flag is given lower priority than all other indexes specified via the <code>--extra-index-url</code> flag.</p>

</dd><dt><code>--keyring-provider</code> <i>keyring-provider</i></dt><dd><p>Attempt to use <code>keyring</code> for authentication for index URLs.</p>

<p>At present, only <code>--keyring-provider subprocess</code> is supported, which configures uv to use the <code>keyring</code> CLI to handle authentication.</p>

<p>Defaults to <code>disabled</code>.</p>

<p>Possible values:</p>

<ul>
<li><code>disabled</code>:  Do not use keyring for credential lookup</li>

<li><code>subprocess</code>:  Use the <code>keyring</code> command for credential lookup</li>
</ul>
</dd><dt><code>--link-mode</code> <i>link-mode</i></dt><dd><p>The method to use when installing packages from the global cache.</p>

<p>Defaults to <code>clone</code> (also known as Copy-on-Write) on macOS, and <code>hardlink</code> on Linux and Windows.</p>

<p>Possible values:</p>

<ul>
<li><code>clone</code>:  Clone (i.e., copy-on-write) packages from the wheel into the <code>site-packages</code> directory</li>

<li><code>copy</code>:  Copy packages from the wheel into the <code>site-packages</code> directory</li>

<li><code>hardlink</code>:  Hard link packages from the wheel into the <code>site-packages</code> directory</li>

<li><code>symlink</code>:  Symbolically link packages from the wheel into the <code>site-packages</code> directory</li>
</ul>
</dd><dt><code>--locked</code></dt><dd><p>Assert that the <code>uv.lock</code> will remain unchanged.</p>

<p>Requires that the lockfile is up-to-date. If the lockfile is missing or needs to be updated, uv will exit with an error.</p>

</dd><dt><code>--native-tls</code></dt><dd><p>Whether to load TLS certificates from the platform&#8217;s native certificate store.</p>

<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>

<p>However, in some cases, you may want to use the platform&#8217;s native certificate store, especially if you&#8217;re relying on a corporate trust root (e.g., for a mandatory proxy) that&#8217;s included in your system&#8217;s certificate store.</p>

</dd><dt><code>--no-binary</code></dt><dd><p>Don&#8217;t install pre-built wheels.</p>

<p>The given packages will be built and installed from source. The resolver will still use pre-built wheels to extract package metadata, if available.</p>

</dd><dt><code>--no-binary-package</code> <i>no-binary-package</i></dt><dd><p>Don&#8217;t install pre-built wheels for a specific package</p>

</dd><dt><code>--no-build</code></dt><dd><p>Don&#8217;t build source distributions.</p>

<p>When enabled, resolving will not run arbitrary Python code. The cached wheels of already-built source distributions will be reused, but operations that require building distributions will exit with an error.</p>

</dd><dt><code>--no-build-isolation</code></dt><dd><p>Disable isolation when building source distributions.</p>

<p>Assumes that build dependencies specified by PEP 518 are already installed.</p>

</dd><dt><code>--no-build-isolation-package</code> <i>no-build-isolation-package</i></dt><dd><p>Disable isolation when building source distributions for a specific package.</p>

<p>Assumes that the packages&#8217; build dependencies specified by PEP 518  are already installed.</p>

</dd><dt><code>--no-build-package</code> <i>no-build-package</i></dt><dd><p>Don&#8217;t build source distributions for a specific package</p>

</dd><dt><code>--no-cache</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>

</dd><dt><code>--no-config</code></dt><dd><p>Avoid discovering configuration files (<code>pyproject.toml</code>, <code>uv.toml</code>).</p>

<p>Normally, configuration files are discovered in the current directory, parent directories, or user configuration directories.</p>

</dd><dt><code>--no-editable</code></dt><dd><p>Don&#8217;t add the requirements as editables</p>

</dd><dt><code>--no-index</code></dt><dd><p>Ignore the registry index (e.g., PyPI), instead relying on direct URL dependencies and those provided via <code>--find-links</code></p>

</dd><dt><code>--no-progress</code></dt><dd><p>Hide all progress outputs.</p>

<p>For example, spinners or progress bars.</p>

</dd><dt><code>--no-python-downloads</code></dt><dd><p>Disable automatic downloads of Python</p>

</dd><dt><code>--no-sources</code></dt><dd><p>Ignore the <code>tool.uv.sources</code> table when resolving dependencies. Used to lock against the standards-compliant, publishable package metadata, as opposed to using any local or Git sources</p>

</dd><dt><code>--no-sync</code></dt><dd><p>Avoid syncing the virtual environment after re-locking the project</p>

</dd><dt><code>--offline</code></dt><dd><p>Disable network access.</p>

<p>When disabled, uv will only use locally cached data and locally available files.</p>

</dd><dt><code>--optional</code> <i>optional</i></dt><dd><p>Add the requirements to the specified optional dependency group.</p>

<p>The group may then be activated when installing the project with the <code>--extra</code> flag.</p>

<p>To enable an optional dependency group for this requirement instead, see <code>--extra</code>.</p>

</dd><dt><code>--package</code> <i>package</i></dt><dd><p>Add the dependency to a specific package in the workspace</p>

</dd><dt><code>--prerelease</code> <i>prerelease</i></dt><dd><p>The strategy to use when considering pre-release versions.</p>

<p>By default, uv will accept pre-releases for packages that <em>only</em> publish pre-releases, along with first-party requirements that contain an explicit pre-release marker in the declared specifiers (<code>if-necessary-or-explicit</code>).</p>

<p>Possible values:</p>

<ul>
<li><code>disallow</code>:  Disallow all pre-release versions</li>

<li><code>allow</code>:  Allow all pre-release versions</li>

<li><code>if-necessary</code>:  Allow pre-release versions if all versions of a package are pre-release</li>

<li><code>explicit</code>:  Allow pre-release versions for first-party packages with explicit pre-release markers in their version requirements</li>

<li><code>if-necessary-or-explicit</code>:  Allow pre-release versions if all versions of a package are pre-release, or if the package has an explicit pre-release marker in its version requirements</li>
</ul>
</dd><dt><code>--python</code>, <code>-p</code> <i>python</i></dt><dd><p>The Python interpreter to use for resolving and syncing.</p>

<p>See <a href="#uv-python">uv python</a> for details on Python discovery and supported request formats.</p>

</dd><dt><code>--python-preference</code> <i>python-preference</i></dt><dd><p>Whether to prefer uv-managed or system Python installations.</p>

<p>By default, uv prefers using Python versions it manages. However, it will use system Python installations if a uv-managed Python is not installed. This option allows prioritizing or ignoring system Python installations.</p>

<p>Possible values:</p>

<ul>
<li><code>only-managed</code>:  Only use managed Python installations; never use system Python installations</li>

<li><code>managed</code>:  Prefer managed Python installations over system Python installations</li>

<li><code>system</code>:  Prefer system Python installations over managed Python installations</li>

<li><code>only-system</code>:  Only use system Python installations; never use managed Python installations</li>
</ul>
</dd><dt><code>--quiet</code>, <code>-q</code></dt><dd><p>Do not print any output</p>

</dd><dt><code>--raw-sources</code></dt><dd><p>Add source requirements to <code>project.dependencies</code>, rather than <code>tool.uv.sources</code>.</p>

<p>By default, uv will use the <code>tool.uv.sources</code> section to record source information for Git, local, editable, and direct URL requirements.</p>

</dd><dt><code>--refresh</code></dt><dd><p>Refresh all cached data</p>

</dd><dt><code>--refresh-package</code> <i>refresh-package</i></dt><dd><p>Refresh cached data for a specific package</p>

</dd><dt><code>--reinstall</code></dt><dd><p>Reinstall all packages, regardless of whether they&#8217;re already installed. Implies <code>--refresh</code></p>

</dd><dt><code>--reinstall-package</code> <i>reinstall-package</i></dt><dd><p>Reinstall a specific package, regardless of whether it&#8217;s already installed. Implies <code>--refresh-package</code></p>

</dd><dt><code>--resolution</code> <i>resolution</i></dt><dd><p>The strategy to use when selecting between the different compatible versions for a given package requirement.</p>

<p>By default, uv will use the latest compatible version of each package (<code>highest</code>).</p>

<p>Possible values:</p>

<ul>
<li><code>highest</code>:  Resolve the highest compatible version of each package</li>

<li><code>lowest</code>:  Resolve the lowest compatible version of each package</li>

<li><code>lowest-direct</code>:  Resolve the lowest compatible version of any direct dependencies, and the highest compatible version of any transitive dependencies</li>
</ul>
</dd><dt><code>--rev</code> <i>rev</i></dt><dd><p>Commit to use when adding a dependency from Git</p>

</dd><dt><code>--script</code> <i>script</i></dt><dd><p>Add the dependency to the specified Python script, rather than to a project.</p>

<p>If provided, uv will add the dependency to the script&#8217;s inline metadata table, in adhere with PEP 723. If no such inline metadata table is present, a new one will be created and added to the script. When executed via <code>uv run</code>, uv will create a temporary environment for the script with all inline dependencies installed.</p>

</dd><dt><code>--tag</code> <i>tag</i></dt><dd><p>Tag to use when adding a dependency from Git</p>

</dd><dt><code>--upgrade</code>, <code>-U</code></dt><dd><p>Allow package upgrades, ignoring pinned versions in any existing output file. Implies <code>--refresh</code></p>

</dd><dt><code>--upgrade-package</code>, <code>-P</code> <i>upgrade-package</i></dt><dd><p>Allow upgrades for a specific package, ignoring pinned versions in any existing output file. Implies <code>--refresh-package</code></p>

</dd><dt><code>--verbose</code>, <code>-v</code></dt><dd><p>Use verbose output.</p>

<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (&lt;https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives&gt;)</p>

</dd><dt><code>--version</code>, <code>-V</code></dt><dd><p>Display the uv version</p>

</dd></dl>

## uv remove

Remove dependencies from the project (experimental).

Dependencies are removed from the project's `pyproject.toml` file.

The lockfile and project environment will be updated to reflect the removed dependencies. To skip updating the lockfile, use `--frozen`. To skip updating the environment, use `--no-sync`.

If any of the requested dependencies are not present in the project, uv will exit with an error.

If a package has been manually installed in the environment, i.e., with `uv pip install`, it will not be removed by `uv remove`.

uv will search for a project in the current directory or any parent directory. If a project cannot be found, uv will exit with an error.

<h3 class="cli-reference">Usage</h3>

```
uv remove [OPTIONS] <PACKAGES>...
```

<h3 class="cli-reference">Arguments</h3>

<dl class="cli-reference"><dt><code>PACKAGES</code></dt><dd><p>The names of the dependencies to remove (e.g., <code>ruff</code>)</p>

</dd></dl>

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt><code>--cache-dir</code> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>

<p>Defaults to <code>$HOME/Library/Caches/uv</code> on macOS, <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on Linux, and <code>{FOLDERID_LocalAppData}\uv\cache</code> on Windows.</p>

</dd><dt><code>--color</code> <i>color-choice</i></dt><dd><p>Control colors in output</p>

<p>[default: auto]</p>
<p>Possible values:</p>

<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>

<li><code>always</code>:  Enables colored output regardless of the detected environment</li>

<li><code>never</code>:  Disables colored output</li>
</ul>
</dd><dt><code>--compile-bytecode</code></dt><dd><p>Compile Python files to bytecode after installation.</p>

<p>By default, uv does not compile Python (<code>.py</code>) files to bytecode (<code>__pycache__/*.pyc</code>); instead, compilation is performed lazily the first time a module is imported. For use-cases in which start time is critical, such as CLI applications and Docker containers, this option can be enabled to trade longer installation times for faster start times.</p>

<p>When enabled, uv will process the entire site-packages directory (including packages that are not being modified by the current operation) for consistency. Like pip, it will also ignore errors.</p>

</dd><dt><code>--config-file</code> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>

<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>

</dd><dt><code>--config-setting</code>, <code>-C</code> <i>config-setting</i></dt><dd><p>Settings to pass to the PEP 517 build backend, specified as <code>KEY=VALUE</code> pairs</p>

</dd><dt><code>--dev</code></dt><dd><p>Remove the packages from the development dependencies</p>

</dd><dt><code>--exclude-newer</code> <i>exclude-newer</i></dt><dd><p>Limit candidate packages to those that were uploaded prior to the given date.</p>

<p>Accepts both RFC 3339 timestamps (e.g., <code>2006-12-02T02:07:43Z</code>) and UTC dates in the same format (e.g., <code>2006-12-02</code>).</p>

</dd><dt><code>--extra-index-url</code> <i>extra-index-url</i></dt><dd><p>Extra URLs of package indexes to use, in addition to <code>--index-url</code>.</p>

<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>

<p>All indexes provided via this flag take priority over the index specified by <code>--index-url</code> (which defaults to PyPI). When multiple <code>--extra-index-url</code> flags are provided, earlier values take priority.</p>

</dd><dt><code>--find-links</code>, <code>-f</code> <i>find-links</i></dt><dd><p>Locations to search for candidate distributions, in addition to those found in the registry indexes.</p>

<p>If a path, the target must be a directory that contains packages as wheel files (<code>.whl</code>) or source distributions (<code>.tar.gz</code> or <code>.zip</code>) at the top level.</p>

<p>If a URL, the page must contain a flat list of links to package files adhering to the formats described above.</p>

</dd><dt><code>--frozen</code></dt><dd><p>Remove dependencies without re-locking the project.</p>

<p>The project environment will not be synced.</p>

</dd><dt><code>--help</code>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>

</dd><dt><code>--index-strategy</code> <i>index-strategy</i></dt><dd><p>The strategy to use when resolving against multiple index URLs.</p>

<p>By default, uv will stop at the first index on which a given package is available, and limit resolutions to those present on that first index (<code>first-match</code>). This prevents &quot;dependency confusion&quot; attacks, whereby an attack can upload a malicious package under the same name to a secondary.</p>

<p>Possible values:</p>

<ul>
<li><code>first-index</code>:  Only use results from the first index that returns a match for a given package name</li>

<li><code>unsafe-first-match</code>:  Search for every package name across all indexes, exhausting the versions from the first index before moving on to the next</li>

<li><code>unsafe-best-match</code>:  Search for every package name across all indexes, preferring the &quot;best&quot; version found. If a package version is in multiple indexes, only look at the entry for the first index</li>
</ul>
</dd><dt><code>--index-url</code>, <code>-i</code> <i>index-url</i></dt><dd><p>The URL of the Python package index (by default: &lt;https://pypi.org/simple&gt;).</p>

<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>

<p>The index given by this flag is given lower priority than all other indexes specified via the <code>--extra-index-url</code> flag.</p>

</dd><dt><code>--keyring-provider</code> <i>keyring-provider</i></dt><dd><p>Attempt to use <code>keyring</code> for authentication for index URLs.</p>

<p>At present, only <code>--keyring-provider subprocess</code> is supported, which configures uv to use the <code>keyring</code> CLI to handle authentication.</p>

<p>Defaults to <code>disabled</code>.</p>

<p>Possible values:</p>

<ul>
<li><code>disabled</code>:  Do not use keyring for credential lookup</li>

<li><code>subprocess</code>:  Use the <code>keyring</code> command for credential lookup</li>
</ul>
</dd><dt><code>--link-mode</code> <i>link-mode</i></dt><dd><p>The method to use when installing packages from the global cache.</p>

<p>Defaults to <code>clone</code> (also known as Copy-on-Write) on macOS, and <code>hardlink</code> on Linux and Windows.</p>

<p>Possible values:</p>

<ul>
<li><code>clone</code>:  Clone (i.e., copy-on-write) packages from the wheel into the <code>site-packages</code> directory</li>

<li><code>copy</code>:  Copy packages from the wheel into the <code>site-packages</code> directory</li>

<li><code>hardlink</code>:  Hard link packages from the wheel into the <code>site-packages</code> directory</li>

<li><code>symlink</code>:  Symbolically link packages from the wheel into the <code>site-packages</code> directory</li>
</ul>
</dd><dt><code>--locked</code></dt><dd><p>Assert that the <code>uv.lock</code> will remain unchanged.</p>

<p>Requires that the lockfile is up-to-date. If the lockfile is missing or needs to be updated, uv will exit with an error.</p>

</dd><dt><code>--native-tls</code></dt><dd><p>Whether to load TLS certificates from the platform&#8217;s native certificate store.</p>

<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>

<p>However, in some cases, you may want to use the platform&#8217;s native certificate store, especially if you&#8217;re relying on a corporate trust root (e.g., for a mandatory proxy) that&#8217;s included in your system&#8217;s certificate store.</p>

</dd><dt><code>--no-binary</code></dt><dd><p>Don&#8217;t install pre-built wheels.</p>

<p>The given packages will be built and installed from source. The resolver will still use pre-built wheels to extract package metadata, if available.</p>

</dd><dt><code>--no-binary-package</code> <i>no-binary-package</i></dt><dd><p>Don&#8217;t install pre-built wheels for a specific package</p>

</dd><dt><code>--no-build</code></dt><dd><p>Don&#8217;t build source distributions.</p>

<p>When enabled, resolving will not run arbitrary Python code. The cached wheels of already-built source distributions will be reused, but operations that require building distributions will exit with an error.</p>

</dd><dt><code>--no-build-isolation</code></dt><dd><p>Disable isolation when building source distributions.</p>

<p>Assumes that build dependencies specified by PEP 518 are already installed.</p>

</dd><dt><code>--no-build-isolation-package</code> <i>no-build-isolation-package</i></dt><dd><p>Disable isolation when building source distributions for a specific package.</p>

<p>Assumes that the packages&#8217; build dependencies specified by PEP 518  are already installed.</p>

</dd><dt><code>--no-build-package</code> <i>no-build-package</i></dt><dd><p>Don&#8217;t build source distributions for a specific package</p>

</dd><dt><code>--no-cache</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>

</dd><dt><code>--no-config</code></dt><dd><p>Avoid discovering configuration files (<code>pyproject.toml</code>, <code>uv.toml</code>).</p>

<p>Normally, configuration files are discovered in the current directory, parent directories, or user configuration directories.</p>

</dd><dt><code>--no-index</code></dt><dd><p>Ignore the registry index (e.g., PyPI), instead relying on direct URL dependencies and those provided via <code>--find-links</code></p>

</dd><dt><code>--no-progress</code></dt><dd><p>Hide all progress outputs.</p>

<p>For example, spinners or progress bars.</p>

</dd><dt><code>--no-python-downloads</code></dt><dd><p>Disable automatic downloads of Python</p>

</dd><dt><code>--no-sources</code></dt><dd><p>Ignore the <code>tool.uv.sources</code> table when resolving dependencies. Used to lock against the standards-compliant, publishable package metadata, as opposed to using any local or Git sources</p>

</dd><dt><code>--no-sync</code></dt><dd><p>Avoid syncing the virtual environment after re-locking the project</p>

</dd><dt><code>--offline</code></dt><dd><p>Disable network access.</p>

<p>When disabled, uv will only use locally cached data and locally available files.</p>

</dd><dt><code>--optional</code> <i>optional</i></dt><dd><p>Remove the packages from the specified optional dependency group</p>

</dd><dt><code>--package</code> <i>package</i></dt><dd><p>Remove the dependencies from a specific package in the workspace</p>

</dd><dt><code>--prerelease</code> <i>prerelease</i></dt><dd><p>The strategy to use when considering pre-release versions.</p>

<p>By default, uv will accept pre-releases for packages that <em>only</em> publish pre-releases, along with first-party requirements that contain an explicit pre-release marker in the declared specifiers (<code>if-necessary-or-explicit</code>).</p>

<p>Possible values:</p>

<ul>
<li><code>disallow</code>:  Disallow all pre-release versions</li>

<li><code>allow</code>:  Allow all pre-release versions</li>

<li><code>if-necessary</code>:  Allow pre-release versions if all versions of a package are pre-release</li>

<li><code>explicit</code>:  Allow pre-release versions for first-party packages with explicit pre-release markers in their version requirements</li>

<li><code>if-necessary-or-explicit</code>:  Allow pre-release versions if all versions of a package are pre-release, or if the package has an explicit pre-release marker in its version requirements</li>
</ul>
</dd><dt><code>--python</code>, <code>-p</code> <i>python</i></dt><dd><p>The Python interpreter to use for resolving and syncing.</p>

<p>See <a href="#uv-python">uv python</a> for details on Python discovery and supported request formats.</p>

</dd><dt><code>--python-preference</code> <i>python-preference</i></dt><dd><p>Whether to prefer uv-managed or system Python installations.</p>

<p>By default, uv prefers using Python versions it manages. However, it will use system Python installations if a uv-managed Python is not installed. This option allows prioritizing or ignoring system Python installations.</p>

<p>Possible values:</p>

<ul>
<li><code>only-managed</code>:  Only use managed Python installations; never use system Python installations</li>

<li><code>managed</code>:  Prefer managed Python installations over system Python installations</li>

<li><code>system</code>:  Prefer system Python installations over managed Python installations</li>

<li><code>only-system</code>:  Only use system Python installations; never use managed Python installations</li>
</ul>
</dd><dt><code>--quiet</code>, <code>-q</code></dt><dd><p>Do not print any output</p>

</dd><dt><code>--refresh</code></dt><dd><p>Refresh all cached data</p>

</dd><dt><code>--refresh-package</code> <i>refresh-package</i></dt><dd><p>Refresh cached data for a specific package</p>

</dd><dt><code>--reinstall</code></dt><dd><p>Reinstall all packages, regardless of whether they&#8217;re already installed. Implies <code>--refresh</code></p>

</dd><dt><code>--reinstall-package</code> <i>reinstall-package</i></dt><dd><p>Reinstall a specific package, regardless of whether it&#8217;s already installed. Implies <code>--refresh-package</code></p>

</dd><dt><code>--resolution</code> <i>resolution</i></dt><dd><p>The strategy to use when selecting between the different compatible versions for a given package requirement.</p>

<p>By default, uv will use the latest compatible version of each package (<code>highest</code>).</p>

<p>Possible values:</p>

<ul>
<li><code>highest</code>:  Resolve the highest compatible version of each package</li>

<li><code>lowest</code>:  Resolve the lowest compatible version of each package</li>

<li><code>lowest-direct</code>:  Resolve the lowest compatible version of any direct dependencies, and the highest compatible version of any transitive dependencies</li>
</ul>
</dd><dt><code>--script</code> <i>script</i></dt><dd><p>Remove the dependency from the specified Python script, rather than from a project.</p>

<p>If provided, uv will remove the dependency from the script&#8217;s inline metadata table, in adhere with PEP 723.</p>

</dd><dt><code>--upgrade</code>, <code>-U</code></dt><dd><p>Allow package upgrades, ignoring pinned versions in any existing output file. Implies <code>--refresh</code></p>

</dd><dt><code>--upgrade-package</code>, <code>-P</code> <i>upgrade-package</i></dt><dd><p>Allow upgrades for a specific package, ignoring pinned versions in any existing output file. Implies <code>--refresh-package</code></p>

</dd><dt><code>--verbose</code>, <code>-v</code></dt><dd><p>Use verbose output.</p>

<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (&lt;https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives&gt;)</p>

</dd><dt><code>--version</code>, <code>-V</code></dt><dd><p>Display the uv version</p>

</dd></dl>

## uv sync

Update the project's environment (experimental).

Syncing ensures that all project dependencies are installed and up-to-date with the lockfile. Syncing also removes packages that are not declared as dependencies of the project.

If the project virtual environment (`.venv`) does not exist, it will be created.

The project is re-locked before syncing unless the `--locked` or `--frozen` flag is provided.

uv will search for a project in the current directory or any parent directory. If a project cannot be found, uv will exit with an error.

<h3 class="cli-reference">Usage</h3>

```
uv sync [OPTIONS]
```

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt><code>--all-extras</code></dt><dd><p>Include all optional dependencies</p>

</dd><dt><code>--cache-dir</code> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>

<p>Defaults to <code>$HOME/Library/Caches/uv</code> on macOS, <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on Linux, and <code>{FOLDERID_LocalAppData}\uv\cache</code> on Windows.</p>

</dd><dt><code>--color</code> <i>color-choice</i></dt><dd><p>Control colors in output</p>

<p>[default: auto]</p>
<p>Possible values:</p>

<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>

<li><code>always</code>:  Enables colored output regardless of the detected environment</li>

<li><code>never</code>:  Disables colored output</li>
</ul>
</dd><dt><code>--compile-bytecode</code></dt><dd><p>Compile Python files to bytecode after installation.</p>

<p>By default, uv does not compile Python (<code>.py</code>) files to bytecode (<code>__pycache__/*.pyc</code>); instead, compilation is performed lazily the first time a module is imported. For use-cases in which start time is critical, such as CLI applications and Docker containers, this option can be enabled to trade longer installation times for faster start times.</p>

<p>When enabled, uv will process the entire site-packages directory (including packages that are not being modified by the current operation) for consistency. Like pip, it will also ignore errors.</p>

</dd><dt><code>--config-file</code> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>

<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>

</dd><dt><code>--config-setting</code>, <code>-C</code> <i>config-setting</i></dt><dd><p>Settings to pass to the PEP 517 build backend, specified as <code>KEY=VALUE</code> pairs</p>

</dd><dt><code>--exclude-newer</code> <i>exclude-newer</i></dt><dd><p>Limit candidate packages to those that were uploaded prior to the given date.</p>

<p>Accepts both RFC 3339 timestamps (e.g., <code>2006-12-02T02:07:43Z</code>) and UTC dates in the same format (e.g., <code>2006-12-02</code>).</p>

</dd><dt><code>--extra</code> <i>extra</i></dt><dd><p>Include optional dependencies from the extra group name.</p>

<p>May be provided more than once.</p>

</dd><dt><code>--extra-index-url</code> <i>extra-index-url</i></dt><dd><p>Extra URLs of package indexes to use, in addition to <code>--index-url</code>.</p>

<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>

<p>All indexes provided via this flag take priority over the index specified by <code>--index-url</code> (which defaults to PyPI). When multiple <code>--extra-index-url</code> flags are provided, earlier values take priority.</p>

</dd><dt><code>--find-links</code>, <code>-f</code> <i>find-links</i></dt><dd><p>Locations to search for candidate distributions, in addition to those found in the registry indexes.</p>

<p>If a path, the target must be a directory that contains packages as wheel files (<code>.whl</code>) or source distributions (<code>.tar.gz</code> or <code>.zip</code>) at the top level.</p>

<p>If a URL, the page must contain a flat list of links to package files adhering to the formats described above.</p>

</dd><dt><code>--frozen</code></dt><dd><p>Sync without updating the <code>uv.lock</code> file.</p>

<p>Instead of checking if the lockfile is up-to-date, uses the versions in the lockfile as the source of truth. If the lockfile is missing, uv will exit with an error. If the <code>pyproject.toml</code> includes changes to dependencies that have not been included in the lockfile yet, they will not be present in the environment.</p>

</dd><dt><code>--help</code>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>

</dd><dt><code>--index-strategy</code> <i>index-strategy</i></dt><dd><p>The strategy to use when resolving against multiple index URLs.</p>

<p>By default, uv will stop at the first index on which a given package is available, and limit resolutions to those present on that first index (<code>first-match</code>). This prevents &quot;dependency confusion&quot; attacks, whereby an attack can upload a malicious package under the same name to a secondary.</p>

<p>Possible values:</p>

<ul>
<li><code>first-index</code>:  Only use results from the first index that returns a match for a given package name</li>

<li><code>unsafe-first-match</code>:  Search for every package name across all indexes, exhausting the versions from the first index before moving on to the next</li>

<li><code>unsafe-best-match</code>:  Search for every package name across all indexes, preferring the &quot;best&quot; version found. If a package version is in multiple indexes, only look at the entry for the first index</li>
</ul>
</dd><dt><code>--index-url</code>, <code>-i</code> <i>index-url</i></dt><dd><p>The URL of the Python package index (by default: &lt;https://pypi.org/simple&gt;).</p>

<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>

<p>The index given by this flag is given lower priority than all other indexes specified via the <code>--extra-index-url</code> flag.</p>

</dd><dt><code>--keyring-provider</code> <i>keyring-provider</i></dt><dd><p>Attempt to use <code>keyring</code> for authentication for index URLs.</p>

<p>At present, only <code>--keyring-provider subprocess</code> is supported, which configures uv to use the <code>keyring</code> CLI to handle authentication.</p>

<p>Defaults to <code>disabled</code>.</p>

<p>Possible values:</p>

<ul>
<li><code>disabled</code>:  Do not use keyring for credential lookup</li>

<li><code>subprocess</code>:  Use the <code>keyring</code> command for credential lookup</li>
</ul>
</dd><dt><code>--link-mode</code> <i>link-mode</i></dt><dd><p>The method to use when installing packages from the global cache.</p>

<p>Defaults to <code>clone</code> (also known as Copy-on-Write) on macOS, and <code>hardlink</code> on Linux and Windows.</p>

<p>Possible values:</p>

<ul>
<li><code>clone</code>:  Clone (i.e., copy-on-write) packages from the wheel into the <code>site-packages</code> directory</li>

<li><code>copy</code>:  Copy packages from the wheel into the <code>site-packages</code> directory</li>

<li><code>hardlink</code>:  Hard link packages from the wheel into the <code>site-packages</code> directory</li>

<li><code>symlink</code>:  Symbolically link packages from the wheel into the <code>site-packages</code> directory</li>
</ul>
</dd><dt><code>--locked</code></dt><dd><p>Assert that the <code>uv.lock</code> will remain unchanged.</p>

<p>Requires that the lockfile is up-to-date. If the lockfile is missing or needs to be updated, uv will exit with an error.</p>

</dd><dt><code>--native-tls</code></dt><dd><p>Whether to load TLS certificates from the platform&#8217;s native certificate store.</p>

<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>

<p>However, in some cases, you may want to use the platform&#8217;s native certificate store, especially if you&#8217;re relying on a corporate trust root (e.g., for a mandatory proxy) that&#8217;s included in your system&#8217;s certificate store.</p>

</dd><dt><code>--no-binary</code></dt><dd><p>Don&#8217;t install pre-built wheels.</p>

<p>The given packages will be built and installed from source. The resolver will still use pre-built wheels to extract package metadata, if available.</p>

</dd><dt><code>--no-binary-package</code> <i>no-binary-package</i></dt><dd><p>Don&#8217;t install pre-built wheels for a specific package</p>

</dd><dt><code>--no-build</code></dt><dd><p>Don&#8217;t build source distributions.</p>

<p>When enabled, resolving will not run arbitrary Python code. The cached wheels of already-built source distributions will be reused, but operations that require building distributions will exit with an error.</p>

</dd><dt><code>--no-build-isolation</code></dt><dd><p>Disable isolation when building source distributions.</p>

<p>Assumes that build dependencies specified by PEP 518 are already installed.</p>

</dd><dt><code>--no-build-isolation-package</code> <i>no-build-isolation-package</i></dt><dd><p>Disable isolation when building source distributions for a specific package.</p>

<p>Assumes that the packages&#8217; build dependencies specified by PEP 518  are already installed.</p>

</dd><dt><code>--no-build-package</code> <i>no-build-package</i></dt><dd><p>Don&#8217;t build source distributions for a specific package</p>

</dd><dt><code>--no-cache</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>

</dd><dt><code>--no-clean</code></dt><dd><p>Do not remove extraneous packages.</p>

<p>When enabled, uv will make the minimum necessary changes to satisfy the requirements.</p>

<p>By default, syncing will remove any extraneous packages from the environment, unless <code>--no-build-isolation</code> is enabled, in which case extra packages are considered necessary for builds.</p>

</dd><dt><code>--no-config</code></dt><dd><p>Avoid discovering configuration files (<code>pyproject.toml</code>, <code>uv.toml</code>).</p>

<p>Normally, configuration files are discovered in the current directory, parent directories, or user configuration directories.</p>

</dd><dt><code>--no-dev</code></dt><dd><p>Omit development dependencies</p>

</dd><dt><code>--no-index</code></dt><dd><p>Ignore the registry index (e.g., PyPI), instead relying on direct URL dependencies and those provided via <code>--find-links</code></p>

</dd><dt><code>--no-progress</code></dt><dd><p>Hide all progress outputs.</p>

<p>For example, spinners or progress bars.</p>

</dd><dt><code>--no-python-downloads</code></dt><dd><p>Disable automatic downloads of Python</p>

</dd><dt><code>--no-sources</code></dt><dd><p>Ignore the <code>tool.uv.sources</code> table when resolving dependencies. Used to lock against the standards-compliant, publishable package metadata, as opposed to using any local or Git sources</p>

</dd><dt><code>--offline</code></dt><dd><p>Disable network access.</p>

<p>When disabled, uv will only use locally cached data and locally available files.</p>

</dd><dt><code>--package</code> <i>package</i></dt><dd><p>Sync for a specific package in the workspace.</p>

<p>The workspace&#8217;s environment (<code>.venv</code>) is updated to reflect the subset of dependencies declared by the specified workspace member package.</p>

<p>If not in a workspace, or if the workspace member does not exist, uv will exit with an error.</p>

</dd><dt><code>--prerelease</code> <i>prerelease</i></dt><dd><p>The strategy to use when considering pre-release versions.</p>

<p>By default, uv will accept pre-releases for packages that <em>only</em> publish pre-releases, along with first-party requirements that contain an explicit pre-release marker in the declared specifiers (<code>if-necessary-or-explicit</code>).</p>

<p>Possible values:</p>

<ul>
<li><code>disallow</code>:  Disallow all pre-release versions</li>

<li><code>allow</code>:  Allow all pre-release versions</li>

<li><code>if-necessary</code>:  Allow pre-release versions if all versions of a package are pre-release</li>

<li><code>explicit</code>:  Allow pre-release versions for first-party packages with explicit pre-release markers in their version requirements</li>

<li><code>if-necessary-or-explicit</code>:  Allow pre-release versions if all versions of a package are pre-release, or if the package has an explicit pre-release marker in its version requirements</li>
</ul>
</dd><dt><code>--python</code>, <code>-p</code> <i>python</i></dt><dd><p>The Python interpreter to use for the project environment.</p>

<p>By default, the first interpreter that meets the project&#8217;s <code>requires-python</code> constraint is used.</p>

<p>If a Python interpreter in a virtual environment is provided, the packages will not be synced to the given environment. The interpreter will be used to create a virtual environment in the project.</p>

<p>See <a href="#uv-python">uv python</a> for details on Python discovery and supported request formats.</p>

</dd><dt><code>--python-preference</code> <i>python-preference</i></dt><dd><p>Whether to prefer uv-managed or system Python installations.</p>

<p>By default, uv prefers using Python versions it manages. However, it will use system Python installations if a uv-managed Python is not installed. This option allows prioritizing or ignoring system Python installations.</p>

<p>Possible values:</p>

<ul>
<li><code>only-managed</code>:  Only use managed Python installations; never use system Python installations</li>

<li><code>managed</code>:  Prefer managed Python installations over system Python installations</li>

<li><code>system</code>:  Prefer system Python installations over managed Python installations</li>

<li><code>only-system</code>:  Only use system Python installations; never use managed Python installations</li>
</ul>
</dd><dt><code>--quiet</code>, <code>-q</code></dt><dd><p>Do not print any output</p>

</dd><dt><code>--refresh</code></dt><dd><p>Refresh all cached data</p>

</dd><dt><code>--refresh-package</code> <i>refresh-package</i></dt><dd><p>Refresh cached data for a specific package</p>

</dd><dt><code>--reinstall</code></dt><dd><p>Reinstall all packages, regardless of whether they&#8217;re already installed. Implies <code>--refresh</code></p>

</dd><dt><code>--reinstall-package</code> <i>reinstall-package</i></dt><dd><p>Reinstall a specific package, regardless of whether it&#8217;s already installed. Implies <code>--refresh-package</code></p>

</dd><dt><code>--resolution</code> <i>resolution</i></dt><dd><p>The strategy to use when selecting between the different compatible versions for a given package requirement.</p>

<p>By default, uv will use the latest compatible version of each package (<code>highest</code>).</p>

<p>Possible values:</p>

<ul>
<li><code>highest</code>:  Resolve the highest compatible version of each package</li>

<li><code>lowest</code>:  Resolve the lowest compatible version of each package</li>

<li><code>lowest-direct</code>:  Resolve the lowest compatible version of any direct dependencies, and the highest compatible version of any transitive dependencies</li>
</ul>
</dd><dt><code>--upgrade</code>, <code>-U</code></dt><dd><p>Allow package upgrades, ignoring pinned versions in any existing output file. Implies <code>--refresh</code></p>

</dd><dt><code>--upgrade-package</code>, <code>-P</code> <i>upgrade-package</i></dt><dd><p>Allow upgrades for a specific package, ignoring pinned versions in any existing output file. Implies <code>--refresh-package</code></p>

</dd><dt><code>--verbose</code>, <code>-v</code></dt><dd><p>Use verbose output.</p>

<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (&lt;https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives&gt;)</p>

</dd><dt><code>--version</code>, <code>-V</code></dt><dd><p>Display the uv version</p>

</dd></dl>

## uv lock

Update the project's lockfile (experimental).

If the project lockfile (`uv.lock`) does not exist, it will be created. If a lockfile is present, its contents will be used as preferences for the resolution.

If there are no changes to the project's dependencies, locking will have no effect unless the `--upgrade` flag is provided.

<h3 class="cli-reference">Usage</h3>

```
uv lock [OPTIONS]
```

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt><code>--cache-dir</code> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>

<p>Defaults to <code>$HOME/Library/Caches/uv</code> on macOS, <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on Linux, and <code>{FOLDERID_LocalAppData}\uv\cache</code> on Windows.</p>

</dd><dt><code>--color</code> <i>color-choice</i></dt><dd><p>Control colors in output</p>

<p>[default: auto]</p>
<p>Possible values:</p>

<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>

<li><code>always</code>:  Enables colored output regardless of the detected environment</li>

<li><code>never</code>:  Disables colored output</li>
</ul>
</dd><dt><code>--config-file</code> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>

<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>

</dd><dt><code>--config-setting</code>, <code>-C</code> <i>config-setting</i></dt><dd><p>Settings to pass to the PEP 517 build backend, specified as <code>KEY=VALUE</code> pairs</p>

</dd><dt><code>--exclude-newer</code> <i>exclude-newer</i></dt><dd><p>Limit candidate packages to those that were uploaded prior to the given date.</p>

<p>Accepts both RFC 3339 timestamps (e.g., <code>2006-12-02T02:07:43Z</code>) and UTC dates in the same format (e.g., <code>2006-12-02</code>).</p>

</dd><dt><code>--extra-index-url</code> <i>extra-index-url</i></dt><dd><p>Extra URLs of package indexes to use, in addition to <code>--index-url</code>.</p>

<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>

<p>All indexes provided via this flag take priority over the index specified by <code>--index-url</code> (which defaults to PyPI). When multiple <code>--extra-index-url</code> flags are provided, earlier values take priority.</p>

</dd><dt><code>--find-links</code>, <code>-f</code> <i>find-links</i></dt><dd><p>Locations to search for candidate distributions, in addition to those found in the registry indexes.</p>

<p>If a path, the target must be a directory that contains packages as wheel files (<code>.whl</code>) or source distributions (<code>.tar.gz</code> or <code>.zip</code>) at the top level.</p>

<p>If a URL, the page must contain a flat list of links to package files adhering to the formats described above.</p>

</dd><dt><code>--frozen</code></dt><dd><p>Assert that a <code>uv.lock</code> exists, without updating it</p>

</dd><dt><code>--help</code>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>

</dd><dt><code>--index-strategy</code> <i>index-strategy</i></dt><dd><p>The strategy to use when resolving against multiple index URLs.</p>

<p>By default, uv will stop at the first index on which a given package is available, and limit resolutions to those present on that first index (<code>first-match</code>). This prevents &quot;dependency confusion&quot; attacks, whereby an attack can upload a malicious package under the same name to a secondary.</p>

<p>Possible values:</p>

<ul>
<li><code>first-index</code>:  Only use results from the first index that returns a match for a given package name</li>

<li><code>unsafe-first-match</code>:  Search for every package name across all indexes, exhausting the versions from the first index before moving on to the next</li>

<li><code>unsafe-best-match</code>:  Search for every package name across all indexes, preferring the &quot;best&quot; version found. If a package version is in multiple indexes, only look at the entry for the first index</li>
</ul>
</dd><dt><code>--index-url</code>, <code>-i</code> <i>index-url</i></dt><dd><p>The URL of the Python package index (by default: &lt;https://pypi.org/simple&gt;).</p>

<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>

<p>The index given by this flag is given lower priority than all other indexes specified via the <code>--extra-index-url</code> flag.</p>

</dd><dt><code>--keyring-provider</code> <i>keyring-provider</i></dt><dd><p>Attempt to use <code>keyring</code> for authentication for index URLs.</p>

<p>At present, only <code>--keyring-provider subprocess</code> is supported, which configures uv to use the <code>keyring</code> CLI to handle authentication.</p>

<p>Defaults to <code>disabled</code>.</p>

<p>Possible values:</p>

<ul>
<li><code>disabled</code>:  Do not use keyring for credential lookup</li>

<li><code>subprocess</code>:  Use the <code>keyring</code> command for credential lookup</li>
</ul>
</dd><dt><code>--link-mode</code> <i>link-mode</i></dt><dd><p>The method to use when installing packages from the global cache.</p>

<p>This option is only used when building source distributions.</p>

<p>Defaults to <code>clone</code> (also known as Copy-on-Write) on macOS, and <code>hardlink</code> on Linux and Windows.</p>

<p>Possible values:</p>

<ul>
<li><code>clone</code>:  Clone (i.e., copy-on-write) packages from the wheel into the <code>site-packages</code> directory</li>

<li><code>copy</code>:  Copy packages from the wheel into the <code>site-packages</code> directory</li>

<li><code>hardlink</code>:  Hard link packages from the wheel into the <code>site-packages</code> directory</li>

<li><code>symlink</code>:  Symbolically link packages from the wheel into the <code>site-packages</code> directory</li>
</ul>
</dd><dt><code>--locked</code></dt><dd><p>Assert that the <code>uv.lock</code> will remain unchanged.</p>

<p>Requires that the lockfile is up-to-date. If the lockfile is missing or needs to be updated, uv will exit with an error.</p>

</dd><dt><code>--native-tls</code></dt><dd><p>Whether to load TLS certificates from the platform&#8217;s native certificate store.</p>

<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>

<p>However, in some cases, you may want to use the platform&#8217;s native certificate store, especially if you&#8217;re relying on a corporate trust root (e.g., for a mandatory proxy) that&#8217;s included in your system&#8217;s certificate store.</p>

</dd><dt><code>--no-binary</code></dt><dd><p>Don&#8217;t install pre-built wheels.</p>

<p>The given packages will be built and installed from source. The resolver will still use pre-built wheels to extract package metadata, if available.</p>

</dd><dt><code>--no-binary-package</code> <i>no-binary-package</i></dt><dd><p>Don&#8217;t install pre-built wheels for a specific package</p>

</dd><dt><code>--no-build</code></dt><dd><p>Don&#8217;t build source distributions.</p>

<p>When enabled, resolving will not run arbitrary Python code. The cached wheels of already-built source distributions will be reused, but operations that require building distributions will exit with an error.</p>

</dd><dt><code>--no-build-isolation</code></dt><dd><p>Disable isolation when building source distributions.</p>

<p>Assumes that build dependencies specified by PEP 518 are already installed.</p>

</dd><dt><code>--no-build-isolation-package</code> <i>no-build-isolation-package</i></dt><dd><p>Disable isolation when building source distributions for a specific package.</p>

<p>Assumes that the packages&#8217; build dependencies specified by PEP 518  are already installed.</p>

</dd><dt><code>--no-build-package</code> <i>no-build-package</i></dt><dd><p>Don&#8217;t build source distributions for a specific package</p>

</dd><dt><code>--no-cache</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>

</dd><dt><code>--no-config</code></dt><dd><p>Avoid discovering configuration files (<code>pyproject.toml</code>, <code>uv.toml</code>).</p>

<p>Normally, configuration files are discovered in the current directory, parent directories, or user configuration directories.</p>

</dd><dt><code>--no-index</code></dt><dd><p>Ignore the registry index (e.g., PyPI), instead relying on direct URL dependencies and those provided via <code>--find-links</code></p>

</dd><dt><code>--no-progress</code></dt><dd><p>Hide all progress outputs.</p>

<p>For example, spinners or progress bars.</p>

</dd><dt><code>--no-python-downloads</code></dt><dd><p>Disable automatic downloads of Python</p>

</dd><dt><code>--no-sources</code></dt><dd><p>Ignore the <code>tool.uv.sources</code> table when resolving dependencies. Used to lock against the standards-compliant, publishable package metadata, as opposed to using any local or Git sources</p>

</dd><dt><code>--offline</code></dt><dd><p>Disable network access.</p>

<p>When disabled, uv will only use locally cached data and locally available files.</p>

</dd><dt><code>--prerelease</code> <i>prerelease</i></dt><dd><p>The strategy to use when considering pre-release versions.</p>

<p>By default, uv will accept pre-releases for packages that <em>only</em> publish pre-releases, along with first-party requirements that contain an explicit pre-release marker in the declared specifiers (<code>if-necessary-or-explicit</code>).</p>

<p>Possible values:</p>

<ul>
<li><code>disallow</code>:  Disallow all pre-release versions</li>

<li><code>allow</code>:  Allow all pre-release versions</li>

<li><code>if-necessary</code>:  Allow pre-release versions if all versions of a package are pre-release</li>

<li><code>explicit</code>:  Allow pre-release versions for first-party packages with explicit pre-release markers in their version requirements</li>

<li><code>if-necessary-or-explicit</code>:  Allow pre-release versions if all versions of a package are pre-release, or if the package has an explicit pre-release marker in its version requirements</li>
</ul>
</dd><dt><code>--python</code>, <code>-p</code> <i>python</i></dt><dd><p>The Python interpreter to use during resolution.</p>

<p>A Python interpreter is required for building source distributions to determine package metadata when there are not wheels.</p>

<p>The interpreter is also used as the fallback value for the minimum Python version if <code>requires-python</code> is not set.</p>

<p>See <a href="#uv-python">uv python</a> for details on Python discovery and supported request formats.</p>

</dd><dt><code>--python-preference</code> <i>python-preference</i></dt><dd><p>Whether to prefer uv-managed or system Python installations.</p>

<p>By default, uv prefers using Python versions it manages. However, it will use system Python installations if a uv-managed Python is not installed. This option allows prioritizing or ignoring system Python installations.</p>

<p>Possible values:</p>

<ul>
<li><code>only-managed</code>:  Only use managed Python installations; never use system Python installations</li>

<li><code>managed</code>:  Prefer managed Python installations over system Python installations</li>

<li><code>system</code>:  Prefer system Python installations over managed Python installations</li>

<li><code>only-system</code>:  Only use system Python installations; never use managed Python installations</li>
</ul>
</dd><dt><code>--quiet</code>, <code>-q</code></dt><dd><p>Do not print any output</p>

</dd><dt><code>--refresh</code></dt><dd><p>Refresh all cached data</p>

</dd><dt><code>--refresh-package</code> <i>refresh-package</i></dt><dd><p>Refresh cached data for a specific package</p>

</dd><dt><code>--resolution</code> <i>resolution</i></dt><dd><p>The strategy to use when selecting between the different compatible versions for a given package requirement.</p>

<p>By default, uv will use the latest compatible version of each package (<code>highest</code>).</p>

<p>Possible values:</p>

<ul>
<li><code>highest</code>:  Resolve the highest compatible version of each package</li>

<li><code>lowest</code>:  Resolve the lowest compatible version of each package</li>

<li><code>lowest-direct</code>:  Resolve the lowest compatible version of any direct dependencies, and the highest compatible version of any transitive dependencies</li>
</ul>
</dd><dt><code>--upgrade</code>, <code>-U</code></dt><dd><p>Allow package upgrades, ignoring pinned versions in any existing output file. Implies <code>--refresh</code></p>

</dd><dt><code>--upgrade-package</code>, <code>-P</code> <i>upgrade-package</i></dt><dd><p>Allow upgrades for a specific package, ignoring pinned versions in any existing output file. Implies <code>--refresh-package</code></p>

</dd><dt><code>--verbose</code>, <code>-v</code></dt><dd><p>Use verbose output.</p>

<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (&lt;https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives&gt;)</p>

</dd><dt><code>--version</code>, <code>-V</code></dt><dd><p>Display the uv version</p>

</dd></dl>

## uv tree

Display the project's dependency tree (experimental)

<h3 class="cli-reference">Usage</h3>

```
uv tree [OPTIONS]
```

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt><code>--cache-dir</code> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>

<p>Defaults to <code>$HOME/Library/Caches/uv</code> on macOS, <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on Linux, and <code>{FOLDERID_LocalAppData}\uv\cache</code> on Windows.</p>

</dd><dt><code>--color</code> <i>color-choice</i></dt><dd><p>Control colors in output</p>

<p>[default: auto]</p>
<p>Possible values:</p>

<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>

<li><code>always</code>:  Enables colored output regardless of the detected environment</li>

<li><code>never</code>:  Disables colored output</li>
</ul>
</dd><dt><code>--config-file</code> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>

<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>

</dd><dt><code>--config-setting</code>, <code>-C</code> <i>config-setting</i></dt><dd><p>Settings to pass to the PEP 517 build backend, specified as <code>KEY=VALUE</code> pairs</p>

</dd><dt><code>--depth</code>, <code>-d</code> <i>depth</i></dt><dd><p>Maximum display depth of the dependency tree</p>

<p>[default: 255]</p>
</dd><dt><code>--exclude-newer</code> <i>exclude-newer</i></dt><dd><p>Limit candidate packages to those that were uploaded prior to the given date.</p>

<p>Accepts both RFC 3339 timestamps (e.g., <code>2006-12-02T02:07:43Z</code>) and UTC dates in the same format (e.g., <code>2006-12-02</code>).</p>

</dd><dt><code>--extra-index-url</code> <i>extra-index-url</i></dt><dd><p>Extra URLs of package indexes to use, in addition to <code>--index-url</code>.</p>

<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>

<p>All indexes provided via this flag take priority over the index specified by <code>--index-url</code> (which defaults to PyPI). When multiple <code>--extra-index-url</code> flags are provided, earlier values take priority.</p>

</dd><dt><code>--find-links</code>, <code>-f</code> <i>find-links</i></dt><dd><p>Locations to search for candidate distributions, in addition to those found in the registry indexes.</p>

<p>If a path, the target must be a directory that contains packages as wheel files (<code>.whl</code>) or source distributions (<code>.tar.gz</code> or <code>.zip</code>) at the top level.</p>

<p>If a URL, the page must contain a flat list of links to package files adhering to the formats described above.</p>

</dd><dt><code>--frozen</code></dt><dd><p>Display the requirements without locking the project.</p>

<p>If the lockfile is missing, uv will exit with an error.</p>

</dd><dt><code>--help</code>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>

</dd><dt><code>--index-strategy</code> <i>index-strategy</i></dt><dd><p>The strategy to use when resolving against multiple index URLs.</p>

<p>By default, uv will stop at the first index on which a given package is available, and limit resolutions to those present on that first index (<code>first-match</code>). This prevents &quot;dependency confusion&quot; attacks, whereby an attack can upload a malicious package under the same name to a secondary.</p>

<p>Possible values:</p>

<ul>
<li><code>first-index</code>:  Only use results from the first index that returns a match for a given package name</li>

<li><code>unsafe-first-match</code>:  Search for every package name across all indexes, exhausting the versions from the first index before moving on to the next</li>

<li><code>unsafe-best-match</code>:  Search for every package name across all indexes, preferring the &quot;best&quot; version found. If a package version is in multiple indexes, only look at the entry for the first index</li>
</ul>
</dd><dt><code>--index-url</code>, <code>-i</code> <i>index-url</i></dt><dd><p>The URL of the Python package index (by default: &lt;https://pypi.org/simple&gt;).</p>

<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>

<p>The index given by this flag is given lower priority than all other indexes specified via the <code>--extra-index-url</code> flag.</p>

</dd><dt><code>--invert</code></dt><dd><p>Show the reverse dependencies for the given package. This flag will invert the tree and display the packages that depend on the given package</p>

</dd><dt><code>--keyring-provider</code> <i>keyring-provider</i></dt><dd><p>Attempt to use <code>keyring</code> for authentication for index URLs.</p>

<p>At present, only <code>--keyring-provider subprocess</code> is supported, which configures uv to use the <code>keyring</code> CLI to handle authentication.</p>

<p>Defaults to <code>disabled</code>.</p>

<p>Possible values:</p>

<ul>
<li><code>disabled</code>:  Do not use keyring for credential lookup</li>

<li><code>subprocess</code>:  Use the <code>keyring</code> command for credential lookup</li>
</ul>
</dd><dt><code>--link-mode</code> <i>link-mode</i></dt><dd><p>The method to use when installing packages from the global cache.</p>

<p>This option is only used when building source distributions.</p>

<p>Defaults to <code>clone</code> (also known as Copy-on-Write) on macOS, and <code>hardlink</code> on Linux and Windows.</p>

<p>Possible values:</p>

<ul>
<li><code>clone</code>:  Clone (i.e., copy-on-write) packages from the wheel into the <code>site-packages</code> directory</li>

<li><code>copy</code>:  Copy packages from the wheel into the <code>site-packages</code> directory</li>

<li><code>hardlink</code>:  Hard link packages from the wheel into the <code>site-packages</code> directory</li>

<li><code>symlink</code>:  Symbolically link packages from the wheel into the <code>site-packages</code> directory</li>
</ul>
</dd><dt><code>--locked</code></dt><dd><p>Assert that the <code>uv.lock</code> will remain unchanged.</p>

<p>Requires that the lockfile is up-to-date. If the lockfile is missing or needs to be updated, uv will exit with an error.</p>

</dd><dt><code>--native-tls</code></dt><dd><p>Whether to load TLS certificates from the platform&#8217;s native certificate store.</p>

<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>

<p>However, in some cases, you may want to use the platform&#8217;s native certificate store, especially if you&#8217;re relying on a corporate trust root (e.g., for a mandatory proxy) that&#8217;s included in your system&#8217;s certificate store.</p>

</dd><dt><code>--no-binary</code></dt><dd><p>Don&#8217;t install pre-built wheels.</p>

<p>The given packages will be built and installed from source. The resolver will still use pre-built wheels to extract package metadata, if available.</p>

</dd><dt><code>--no-binary-package</code> <i>no-binary-package</i></dt><dd><p>Don&#8217;t install pre-built wheels for a specific package</p>

</dd><dt><code>--no-build</code></dt><dd><p>Don&#8217;t build source distributions.</p>

<p>When enabled, resolving will not run arbitrary Python code. The cached wheels of already-built source distributions will be reused, but operations that require building distributions will exit with an error.</p>

</dd><dt><code>--no-build-isolation</code></dt><dd><p>Disable isolation when building source distributions.</p>

<p>Assumes that build dependencies specified by PEP 518 are already installed.</p>

</dd><dt><code>--no-build-isolation-package</code> <i>no-build-isolation-package</i></dt><dd><p>Disable isolation when building source distributions for a specific package.</p>

<p>Assumes that the packages&#8217; build dependencies specified by PEP 518  are already installed.</p>

</dd><dt><code>--no-build-package</code> <i>no-build-package</i></dt><dd><p>Don&#8217;t build source distributions for a specific package</p>

</dd><dt><code>--no-cache</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>

</dd><dt><code>--no-config</code></dt><dd><p>Avoid discovering configuration files (<code>pyproject.toml</code>, <code>uv.toml</code>).</p>

<p>Normally, configuration files are discovered in the current directory, parent directories, or user configuration directories.</p>

</dd><dt><code>--no-dedupe</code></dt><dd><p>Do not de-duplicate repeated dependencies. Usually, when a package has already displayed its dependencies, further occurrences will not re-display its dependencies, and will include a (*) to indicate it has already been shown. This flag will cause those duplicates to be repeated</p>

</dd><dt><code>--no-index</code></dt><dd><p>Ignore the registry index (e.g., PyPI), instead relying on direct URL dependencies and those provided via <code>--find-links</code></p>

</dd><dt><code>--no-progress</code></dt><dd><p>Hide all progress outputs.</p>

<p>For example, spinners or progress bars.</p>

</dd><dt><code>--no-python-downloads</code></dt><dd><p>Disable automatic downloads of Python</p>

</dd><dt><code>--no-sources</code></dt><dd><p>Ignore the <code>tool.uv.sources</code> table when resolving dependencies. Used to lock against the standards-compliant, publishable package metadata, as opposed to using any local or Git sources</p>

</dd><dt><code>--offline</code></dt><dd><p>Disable network access.</p>

<p>When disabled, uv will only use locally cached data and locally available files.</p>

</dd><dt><code>--package</code> <i>package</i></dt><dd><p>Display only the specified packages</p>

</dd><dt><code>--prerelease</code> <i>prerelease</i></dt><dd><p>The strategy to use when considering pre-release versions.</p>

<p>By default, uv will accept pre-releases for packages that <em>only</em> publish pre-releases, along with first-party requirements that contain an explicit pre-release marker in the declared specifiers (<code>if-necessary-or-explicit</code>).</p>

<p>Possible values:</p>

<ul>
<li><code>disallow</code>:  Disallow all pre-release versions</li>

<li><code>allow</code>:  Allow all pre-release versions</li>

<li><code>if-necessary</code>:  Allow pre-release versions if all versions of a package are pre-release</li>

<li><code>explicit</code>:  Allow pre-release versions for first-party packages with explicit pre-release markers in their version requirements</li>

<li><code>if-necessary-or-explicit</code>:  Allow pre-release versions if all versions of a package are pre-release, or if the package has an explicit pre-release marker in its version requirements</li>
</ul>
</dd><dt><code>--prune</code> <i>prune</i></dt><dd><p>Prune the given package from the display of the dependency tree</p>

</dd><dt><code>--python</code>, <code>-p</code> <i>python</i></dt><dd><p>The Python interpreter to use for locking and filtering.</p>

<p>By default, the tree is filtered to match the platform as reported by the Python interpreter. Use <code>--universal</code> to display the tree for all platforms, or use <code>--python-version</code> or <code>--python-platform</code> to override a subset of markers.</p>

<p>See <a href="#uv-python">uv python</a> for details on Python discovery and supported request formats.</p>

</dd><dt><code>--python-platform</code> <i>python-platform</i></dt><dd><p>The platform to use when filtering the tree.</p>

<p>For example, pass <code>--platform windows</code> to display the dependencies that would be included when installing on Windows.</p>

<p>Represented as a &quot;target triple&quot;, a string that describes the target platform in terms of its CPU, vendor, and operating system name, like <code>x86_64-unknown-linux-gnu</code> or <code>aaarch64-apple-darwin</code>.</p>

<p>Possible values:</p>

<ul>
<li><code>windows</code>:  An alias for <code>x86_64-pc-windows-msvc</code>, the default target for Windows</li>

<li><code>linux</code>:  An alias for <code>x86_64-unknown-linux-gnu</code>, the default target for Linux</li>

<li><code>macos</code>:  An alias for <code>aarch64-apple-darwin</code>, the default target for macOS</li>

<li><code>x86_64-pc-windows-msvc</code>:  An x86 Windows target</li>

<li><code>x86_64-unknown-linux-gnu</code>:  An x86 Linux target. Equivalent to <code>x86_64-manylinux_2_17</code></li>

<li><code>aarch64-apple-darwin</code>:  An ARM-based macOS target, as seen on Apple Silicon devices</li>

<li><code>x86_64-apple-darwin</code>:  An x86 macOS target</li>

<li><code>aarch64-unknown-linux-gnu</code>:  An ARM64 Linux target. Equivalent to <code>aarch64-manylinux_2_17</code></li>

<li><code>aarch64-unknown-linux-musl</code>:  An ARM64 Linux target</li>

<li><code>x86_64-unknown-linux-musl</code>:  An <code>x86_64</code> Linux target</li>

<li><code>x86_64-manylinux_2_17</code>:  An <code>x86_64</code> target for the <code>manylinux_2_17</code> platform</li>

<li><code>x86_64-manylinux_2_28</code>:  An <code>x86_64</code> target for the <code>manylinux_2_28</code> platform</li>

<li><code>x86_64-manylinux_2_31</code>:  An <code>x86_64</code> target for the <code>manylinux_2_31</code> platform</li>

<li><code>aarch64-manylinux_2_17</code>:  An ARM64 target for the <code>manylinux_2_17</code> platform</li>

<li><code>aarch64-manylinux_2_28</code>:  An ARM64 target for the <code>manylinux_2_28</code> platform</li>

<li><code>aarch64-manylinux_2_31</code>:  An ARM64 target for the <code>manylinux_2_31</code> platform</li>
</ul>
</dd><dt><code>--python-preference</code> <i>python-preference</i></dt><dd><p>Whether to prefer uv-managed or system Python installations.</p>

<p>By default, uv prefers using Python versions it manages. However, it will use system Python installations if a uv-managed Python is not installed. This option allows prioritizing or ignoring system Python installations.</p>

<p>Possible values:</p>

<ul>
<li><code>only-managed</code>:  Only use managed Python installations; never use system Python installations</li>

<li><code>managed</code>:  Prefer managed Python installations over system Python installations</li>

<li><code>system</code>:  Prefer system Python installations over managed Python installations</li>

<li><code>only-system</code>:  Only use system Python installations; never use managed Python installations</li>
</ul>
</dd><dt><code>--python-version</code> <i>python-version</i></dt><dd><p>The Python version to use when filtering the tree.</p>

<p>For example, pass <code>--python-version 3.10</code> to display the dependencies that would be included when installing on Python 3.10.</p>

<p>Defaults to the version of the discovered Python interpreter.</p>

</dd><dt><code>--quiet</code>, <code>-q</code></dt><dd><p>Do not print any output</p>

</dd><dt><code>--resolution</code> <i>resolution</i></dt><dd><p>The strategy to use when selecting between the different compatible versions for a given package requirement.</p>

<p>By default, uv will use the latest compatible version of each package (<code>highest</code>).</p>

<p>Possible values:</p>

<ul>
<li><code>highest</code>:  Resolve the highest compatible version of each package</li>

<li><code>lowest</code>:  Resolve the lowest compatible version of each package</li>

<li><code>lowest-direct</code>:  Resolve the lowest compatible version of any direct dependencies, and the highest compatible version of any transitive dependencies</li>
</ul>
</dd><dt><code>--universal</code></dt><dd><p>Show a platform-independent dependency tree.</p>

<p>Shows resolved package versions for all Python versions and platforms, rather than filtering to those that are relevant for the current environment.</p>

<p>Multiple versions may be shown for a each package.</p>

</dd><dt><code>--upgrade</code>, <code>-U</code></dt><dd><p>Allow package upgrades, ignoring pinned versions in any existing output file. Implies <code>--refresh</code></p>

</dd><dt><code>--upgrade-package</code>, <code>-P</code> <i>upgrade-package</i></dt><dd><p>Allow upgrades for a specific package, ignoring pinned versions in any existing output file. Implies <code>--refresh-package</code></p>

</dd><dt><code>--verbose</code>, <code>-v</code></dt><dd><p>Use verbose output.</p>

<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (&lt;https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives&gt;)</p>

</dd><dt><code>--version</code>, <code>-V</code></dt><dd><p>Display the uv version</p>

</dd></dl>

## uv tool

Run and install commands provided by Python packages (experimental)

<h3 class="cli-reference">Usage</h3>

```
uv tool [OPTIONS] <COMMAND>
```

<h3 class="cli-reference">Commands</h3>

<dl class="cli-reference"><dt><a href="#uv-tool-run"><code>uv tool run</code></a></dt><dd><p>Run a command provided by a Python package</p>
</dd>
<dt><a href="#uv-tool-install"><code>uv tool install</code></a></dt><dd><p>Install commands provided by a Python package</p>
</dd>
<dt><a href="#uv-tool-upgrade"><code>uv tool upgrade</code></a></dt><dd><p>Upgrade installed tools</p>
</dd>
<dt><a href="#uv-tool-list"><code>uv tool list</code></a></dt><dd><p>List installed tools</p>
</dd>
<dt><a href="#uv-tool-uninstall"><code>uv tool uninstall</code></a></dt><dd><p>Uninstall a tool</p>
</dd>
<dt><a href="#uv-tool-update-shell"><code>uv tool update-shell</code></a></dt><dd><p>Ensure that the tool executable directory is on the <code>PATH</code></p>
</dd>
<dt><a href="#uv-tool-dir"><code>uv tool dir</code></a></dt><dd><p>Show the path to the uv tools directory</p>
</dd>
</dl>

### uv tool run

Run a command provided by a Python package.

By default, the package to install is assumed to match the command name.

The name of the command can include an exact version in the format `<package>@<version>`, e.g., `uv run ruff@0.3.0`. If more complex version specification is desired or if the command is provided by a different package, use `--from`.

If the tool was previously installed, i.e., via `uv tool install`, the installed version will be used unless a version is requested or the `--isolated` flag is used.

`uvx` is provided as a convenient alias for `uv tool run`, their behavior is identical.

If no command is provided, the installed tools are displayed.

Packages are installed into an ephemeral virtual environment in the uv cache directory.

<h3 class="cli-reference">Usage</h3>

```
uv tool run [OPTIONS] [COMMAND]
```

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt><code>--cache-dir</code> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>

<p>Defaults to <code>$HOME/Library/Caches/uv</code> on macOS, <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on Linux, and <code>{FOLDERID_LocalAppData}\uv\cache</code> on Windows.</p>

</dd><dt><code>--color</code> <i>color-choice</i></dt><dd><p>Control colors in output</p>

<p>[default: auto]</p>
<p>Possible values:</p>

<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>

<li><code>always</code>:  Enables colored output regardless of the detected environment</li>

<li><code>never</code>:  Disables colored output</li>
</ul>
</dd><dt><code>--compile-bytecode</code></dt><dd><p>Compile Python files to bytecode after installation.</p>

<p>By default, uv does not compile Python (<code>.py</code>) files to bytecode (<code>__pycache__/*.pyc</code>); instead, compilation is performed lazily the first time a module is imported. For use-cases in which start time is critical, such as CLI applications and Docker containers, this option can be enabled to trade longer installation times for faster start times.</p>

<p>When enabled, uv will process the entire site-packages directory (including packages that are not being modified by the current operation) for consistency. Like pip, it will also ignore errors.</p>

</dd><dt><code>--config-file</code> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>

<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>

</dd><dt><code>--config-setting</code>, <code>-C</code> <i>config-setting</i></dt><dd><p>Settings to pass to the PEP 517 build backend, specified as <code>KEY=VALUE</code> pairs</p>

</dd><dt><code>--exclude-newer</code> <i>exclude-newer</i></dt><dd><p>Limit candidate packages to those that were uploaded prior to the given date.</p>

<p>Accepts both RFC 3339 timestamps (e.g., <code>2006-12-02T02:07:43Z</code>) and UTC dates in the same format (e.g., <code>2006-12-02</code>).</p>

</dd><dt><code>--extra-index-url</code> <i>extra-index-url</i></dt><dd><p>Extra URLs of package indexes to use, in addition to <code>--index-url</code>.</p>

<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>

<p>All indexes provided via this flag take priority over the index specified by <code>--index-url</code> (which defaults to PyPI). When multiple <code>--extra-index-url</code> flags are provided, earlier values take priority.</p>

</dd><dt><code>--find-links</code>, <code>-f</code> <i>find-links</i></dt><dd><p>Locations to search for candidate distributions, in addition to those found in the registry indexes.</p>

<p>If a path, the target must be a directory that contains packages as wheel files (<code>.whl</code>) or source distributions (<code>.tar.gz</code> or <code>.zip</code>) at the top level.</p>

<p>If a URL, the page must contain a flat list of links to package files adhering to the formats described above.</p>

</dd><dt><code>--from</code> <i>from</i></dt><dd><p>Use the given package to provide the command.</p>

<p>By default, the package name is assumed to match the command name.</p>

</dd><dt><code>--help</code>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>

</dd><dt><code>--index-strategy</code> <i>index-strategy</i></dt><dd><p>The strategy to use when resolving against multiple index URLs.</p>

<p>By default, uv will stop at the first index on which a given package is available, and limit resolutions to those present on that first index (<code>first-match</code>). This prevents &quot;dependency confusion&quot; attacks, whereby an attack can upload a malicious package under the same name to a secondary.</p>

<p>Possible values:</p>

<ul>
<li><code>first-index</code>:  Only use results from the first index that returns a match for a given package name</li>

<li><code>unsafe-first-match</code>:  Search for every package name across all indexes, exhausting the versions from the first index before moving on to the next</li>

<li><code>unsafe-best-match</code>:  Search for every package name across all indexes, preferring the &quot;best&quot; version found. If a package version is in multiple indexes, only look at the entry for the first index</li>
</ul>
</dd><dt><code>--index-url</code>, <code>-i</code> <i>index-url</i></dt><dd><p>The URL of the Python package index (by default: &lt;https://pypi.org/simple&gt;).</p>

<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>

<p>The index given by this flag is given lower priority than all other indexes specified via the <code>--extra-index-url</code> flag.</p>

</dd><dt><code>--isolated</code></dt><dd><p>Run the tool in an isolated virtual environment, ignoring any already-installed tools</p>

</dd><dt><code>--keyring-provider</code> <i>keyring-provider</i></dt><dd><p>Attempt to use <code>keyring</code> for authentication for index URLs.</p>

<p>At present, only <code>--keyring-provider subprocess</code> is supported, which configures uv to use the <code>keyring</code> CLI to handle authentication.</p>

<p>Defaults to <code>disabled</code>.</p>

<p>Possible values:</p>

<ul>
<li><code>disabled</code>:  Do not use keyring for credential lookup</li>

<li><code>subprocess</code>:  Use the <code>keyring</code> command for credential lookup</li>
</ul>
</dd><dt><code>--link-mode</code> <i>link-mode</i></dt><dd><p>The method to use when installing packages from the global cache.</p>

<p>Defaults to <code>clone</code> (also known as Copy-on-Write) on macOS, and <code>hardlink</code> on Linux and Windows.</p>

<p>Possible values:</p>

<ul>
<li><code>clone</code>:  Clone (i.e., copy-on-write) packages from the wheel into the <code>site-packages</code> directory</li>

<li><code>copy</code>:  Copy packages from the wheel into the <code>site-packages</code> directory</li>

<li><code>hardlink</code>:  Hard link packages from the wheel into the <code>site-packages</code> directory</li>

<li><code>symlink</code>:  Symbolically link packages from the wheel into the <code>site-packages</code> directory</li>
</ul>
</dd><dt><code>--native-tls</code></dt><dd><p>Whether to load TLS certificates from the platform&#8217;s native certificate store.</p>

<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>

<p>However, in some cases, you may want to use the platform&#8217;s native certificate store, especially if you&#8217;re relying on a corporate trust root (e.g., for a mandatory proxy) that&#8217;s included in your system&#8217;s certificate store.</p>

</dd><dt><code>--no-binary</code></dt><dd><p>Don&#8217;t install pre-built wheels.</p>

<p>The given packages will be built and installed from source. The resolver will still use pre-built wheels to extract package metadata, if available.</p>

</dd><dt><code>--no-binary-package</code> <i>no-binary-package</i></dt><dd><p>Don&#8217;t install pre-built wheels for a specific package</p>

</dd><dt><code>--no-build</code></dt><dd><p>Don&#8217;t build source distributions.</p>

<p>When enabled, resolving will not run arbitrary Python code. The cached wheels of already-built source distributions will be reused, but operations that require building distributions will exit with an error.</p>

</dd><dt><code>--no-build-isolation</code></dt><dd><p>Disable isolation when building source distributions.</p>

<p>Assumes that build dependencies specified by PEP 518 are already installed.</p>

</dd><dt><code>--no-build-isolation-package</code> <i>no-build-isolation-package</i></dt><dd><p>Disable isolation when building source distributions for a specific package.</p>

<p>Assumes that the packages&#8217; build dependencies specified by PEP 518  are already installed.</p>

</dd><dt><code>--no-build-package</code> <i>no-build-package</i></dt><dd><p>Don&#8217;t build source distributions for a specific package</p>

</dd><dt><code>--no-cache</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>

</dd><dt><code>--no-config</code></dt><dd><p>Avoid discovering configuration files (<code>pyproject.toml</code>, <code>uv.toml</code>).</p>

<p>Normally, configuration files are discovered in the current directory, parent directories, or user configuration directories.</p>

</dd><dt><code>--no-index</code></dt><dd><p>Ignore the registry index (e.g., PyPI), instead relying on direct URL dependencies and those provided via <code>--find-links</code></p>

</dd><dt><code>--no-progress</code></dt><dd><p>Hide all progress outputs.</p>

<p>For example, spinners or progress bars.</p>

</dd><dt><code>--no-python-downloads</code></dt><dd><p>Disable automatic downloads of Python</p>

</dd><dt><code>--no-sources</code></dt><dd><p>Ignore the <code>tool.uv.sources</code> table when resolving dependencies. Used to lock against the standards-compliant, publishable package metadata, as opposed to using any local or Git sources</p>

</dd><dt><code>--offline</code></dt><dd><p>Disable network access.</p>

<p>When disabled, uv will only use locally cached data and locally available files.</p>

</dd><dt><code>--prerelease</code> <i>prerelease</i></dt><dd><p>The strategy to use when considering pre-release versions.</p>

<p>By default, uv will accept pre-releases for packages that <em>only</em> publish pre-releases, along with first-party requirements that contain an explicit pre-release marker in the declared specifiers (<code>if-necessary-or-explicit</code>).</p>

<p>Possible values:</p>

<ul>
<li><code>disallow</code>:  Disallow all pre-release versions</li>

<li><code>allow</code>:  Allow all pre-release versions</li>

<li><code>if-necessary</code>:  Allow pre-release versions if all versions of a package are pre-release</li>

<li><code>explicit</code>:  Allow pre-release versions for first-party packages with explicit pre-release markers in their version requirements</li>

<li><code>if-necessary-or-explicit</code>:  Allow pre-release versions if all versions of a package are pre-release, or if the package has an explicit pre-release marker in its version requirements</li>
</ul>
</dd><dt><code>--python</code>, <code>-p</code> <i>python</i></dt><dd><p>The Python interpreter to use to build the run environment.</p>

<p>See <a href="#uv-python">uv python</a> for details on Python discovery and supported request formats.</p>

</dd><dt><code>--python-preference</code> <i>python-preference</i></dt><dd><p>Whether to prefer uv-managed or system Python installations.</p>

<p>By default, uv prefers using Python versions it manages. However, it will use system Python installations if a uv-managed Python is not installed. This option allows prioritizing or ignoring system Python installations.</p>

<p>Possible values:</p>

<ul>
<li><code>only-managed</code>:  Only use managed Python installations; never use system Python installations</li>

<li><code>managed</code>:  Prefer managed Python installations over system Python installations</li>

<li><code>system</code>:  Prefer system Python installations over managed Python installations</li>

<li><code>only-system</code>:  Only use system Python installations; never use managed Python installations</li>
</ul>
</dd><dt><code>--quiet</code>, <code>-q</code></dt><dd><p>Do not print any output</p>

</dd><dt><code>--refresh</code></dt><dd><p>Refresh all cached data</p>

</dd><dt><code>--refresh-package</code> <i>refresh-package</i></dt><dd><p>Refresh cached data for a specific package</p>

</dd><dt><code>--reinstall</code></dt><dd><p>Reinstall all packages, regardless of whether they&#8217;re already installed. Implies <code>--refresh</code></p>

</dd><dt><code>--reinstall-package</code> <i>reinstall-package</i></dt><dd><p>Reinstall a specific package, regardless of whether it&#8217;s already installed. Implies <code>--refresh-package</code></p>

</dd><dt><code>--resolution</code> <i>resolution</i></dt><dd><p>The strategy to use when selecting between the different compatible versions for a given package requirement.</p>

<p>By default, uv will use the latest compatible version of each package (<code>highest</code>).</p>

<p>Possible values:</p>

<ul>
<li><code>highest</code>:  Resolve the highest compatible version of each package</li>

<li><code>lowest</code>:  Resolve the lowest compatible version of each package</li>

<li><code>lowest-direct</code>:  Resolve the lowest compatible version of any direct dependencies, and the highest compatible version of any transitive dependencies</li>
</ul>
</dd><dt><code>--upgrade</code>, <code>-U</code></dt><dd><p>Allow package upgrades, ignoring pinned versions in any existing output file. Implies <code>--refresh</code></p>

</dd><dt><code>--upgrade-package</code>, <code>-P</code> <i>upgrade-package</i></dt><dd><p>Allow upgrades for a specific package, ignoring pinned versions in any existing output file. Implies <code>--refresh-package</code></p>

</dd><dt><code>--verbose</code>, <code>-v</code></dt><dd><p>Use verbose output.</p>

<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (&lt;https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives&gt;)</p>

</dd><dt><code>--version</code>, <code>-V</code></dt><dd><p>Display the uv version</p>

</dd><dt><code>--with</code> <i>with</i></dt><dd><p>Run with the given packages installed</p>

</dd><dt><code>--with-requirements</code> <i>with-requirements</i></dt><dd><p>Run with all packages listed in the given <code>requirements.txt</code> files</p>

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

<dl class="cli-reference"><dt><code>PACKAGE</code></dt><dd><p>The package to install commands from</p>

</dd></dl>

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt><code>--cache-dir</code> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>

<p>Defaults to <code>$HOME/Library/Caches/uv</code> on macOS, <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on Linux, and <code>{FOLDERID_LocalAppData}\uv\cache</code> on Windows.</p>

</dd><dt><code>--color</code> <i>color-choice</i></dt><dd><p>Control colors in output</p>

<p>[default: auto]</p>
<p>Possible values:</p>

<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>

<li><code>always</code>:  Enables colored output regardless of the detected environment</li>

<li><code>never</code>:  Disables colored output</li>
</ul>
</dd><dt><code>--compile-bytecode</code></dt><dd><p>Compile Python files to bytecode after installation.</p>

<p>By default, uv does not compile Python (<code>.py</code>) files to bytecode (<code>__pycache__/*.pyc</code>); instead, compilation is performed lazily the first time a module is imported. For use-cases in which start time is critical, such as CLI applications and Docker containers, this option can be enabled to trade longer installation times for faster start times.</p>

<p>When enabled, uv will process the entire site-packages directory (including packages that are not being modified by the current operation) for consistency. Like pip, it will also ignore errors.</p>

</dd><dt><code>--config-file</code> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>

<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>

</dd><dt><code>--config-setting</code>, <code>-C</code> <i>config-setting</i></dt><dd><p>Settings to pass to the PEP 517 build backend, specified as <code>KEY=VALUE</code> pairs</p>

</dd><dt><code>--editable</code>, <code>-e</code></dt><dt><code>--exclude-newer</code> <i>exclude-newer</i></dt><dd><p>Limit candidate packages to those that were uploaded prior to the given date.</p>

<p>Accepts both RFC 3339 timestamps (e.g., <code>2006-12-02T02:07:43Z</code>) and UTC dates in the same format (e.g., <code>2006-12-02</code>).</p>

</dd><dt><code>--extra-index-url</code> <i>extra-index-url</i></dt><dd><p>Extra URLs of package indexes to use, in addition to <code>--index-url</code>.</p>

<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>

<p>All indexes provided via this flag take priority over the index specified by <code>--index-url</code> (which defaults to PyPI). When multiple <code>--extra-index-url</code> flags are provided, earlier values take priority.</p>

</dd><dt><code>--find-links</code>, <code>-f</code> <i>find-links</i></dt><dd><p>Locations to search for candidate distributions, in addition to those found in the registry indexes.</p>

<p>If a path, the target must be a directory that contains packages as wheel files (<code>.whl</code>) or source distributions (<code>.tar.gz</code> or <code>.zip</code>) at the top level.</p>

<p>If a URL, the page must contain a flat list of links to package files adhering to the formats described above.</p>

</dd><dt><code>--force</code></dt><dd><p>Force installation of the tool.</p>

<p>Will replace any existing entry points with the same name in the executable directory.</p>

</dd><dt><code>--help</code>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>

</dd><dt><code>--index-strategy</code> <i>index-strategy</i></dt><dd><p>The strategy to use when resolving against multiple index URLs.</p>

<p>By default, uv will stop at the first index on which a given package is available, and limit resolutions to those present on that first index (<code>first-match</code>). This prevents &quot;dependency confusion&quot; attacks, whereby an attack can upload a malicious package under the same name to a secondary.</p>

<p>Possible values:</p>

<ul>
<li><code>first-index</code>:  Only use results from the first index that returns a match for a given package name</li>

<li><code>unsafe-first-match</code>:  Search for every package name across all indexes, exhausting the versions from the first index before moving on to the next</li>

<li><code>unsafe-best-match</code>:  Search for every package name across all indexes, preferring the &quot;best&quot; version found. If a package version is in multiple indexes, only look at the entry for the first index</li>
</ul>
</dd><dt><code>--index-url</code>, <code>-i</code> <i>index-url</i></dt><dd><p>The URL of the Python package index (by default: &lt;https://pypi.org/simple&gt;).</p>

<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>

<p>The index given by this flag is given lower priority than all other indexes specified via the <code>--extra-index-url</code> flag.</p>

</dd><dt><code>--keyring-provider</code> <i>keyring-provider</i></dt><dd><p>Attempt to use <code>keyring</code> for authentication for index URLs.</p>

<p>At present, only <code>--keyring-provider subprocess</code> is supported, which configures uv to use the <code>keyring</code> CLI to handle authentication.</p>

<p>Defaults to <code>disabled</code>.</p>

<p>Possible values:</p>

<ul>
<li><code>disabled</code>:  Do not use keyring for credential lookup</li>

<li><code>subprocess</code>:  Use the <code>keyring</code> command for credential lookup</li>
</ul>
</dd><dt><code>--link-mode</code> <i>link-mode</i></dt><dd><p>The method to use when installing packages from the global cache.</p>

<p>Defaults to <code>clone</code> (also known as Copy-on-Write) on macOS, and <code>hardlink</code> on Linux and Windows.</p>

<p>Possible values:</p>

<ul>
<li><code>clone</code>:  Clone (i.e., copy-on-write) packages from the wheel into the <code>site-packages</code> directory</li>

<li><code>copy</code>:  Copy packages from the wheel into the <code>site-packages</code> directory</li>

<li><code>hardlink</code>:  Hard link packages from the wheel into the <code>site-packages</code> directory</li>

<li><code>symlink</code>:  Symbolically link packages from the wheel into the <code>site-packages</code> directory</li>
</ul>
</dd><dt><code>--native-tls</code></dt><dd><p>Whether to load TLS certificates from the platform&#8217;s native certificate store.</p>

<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>

<p>However, in some cases, you may want to use the platform&#8217;s native certificate store, especially if you&#8217;re relying on a corporate trust root (e.g., for a mandatory proxy) that&#8217;s included in your system&#8217;s certificate store.</p>

</dd><dt><code>--no-binary</code></dt><dd><p>Don&#8217;t install pre-built wheels.</p>

<p>The given packages will be built and installed from source. The resolver will still use pre-built wheels to extract package metadata, if available.</p>

</dd><dt><code>--no-binary-package</code> <i>no-binary-package</i></dt><dd><p>Don&#8217;t install pre-built wheels for a specific package</p>

</dd><dt><code>--no-build</code></dt><dd><p>Don&#8217;t build source distributions.</p>

<p>When enabled, resolving will not run arbitrary Python code. The cached wheels of already-built source distributions will be reused, but operations that require building distributions will exit with an error.</p>

</dd><dt><code>--no-build-isolation</code></dt><dd><p>Disable isolation when building source distributions.</p>

<p>Assumes that build dependencies specified by PEP 518 are already installed.</p>

</dd><dt><code>--no-build-isolation-package</code> <i>no-build-isolation-package</i></dt><dd><p>Disable isolation when building source distributions for a specific package.</p>

<p>Assumes that the packages&#8217; build dependencies specified by PEP 518  are already installed.</p>

</dd><dt><code>--no-build-package</code> <i>no-build-package</i></dt><dd><p>Don&#8217;t build source distributions for a specific package</p>

</dd><dt><code>--no-cache</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>

</dd><dt><code>--no-config</code></dt><dd><p>Avoid discovering configuration files (<code>pyproject.toml</code>, <code>uv.toml</code>).</p>

<p>Normally, configuration files are discovered in the current directory, parent directories, or user configuration directories.</p>

</dd><dt><code>--no-index</code></dt><dd><p>Ignore the registry index (e.g., PyPI), instead relying on direct URL dependencies and those provided via <code>--find-links</code></p>

</dd><dt><code>--no-progress</code></dt><dd><p>Hide all progress outputs.</p>

<p>For example, spinners or progress bars.</p>

</dd><dt><code>--no-python-downloads</code></dt><dd><p>Disable automatic downloads of Python</p>

</dd><dt><code>--no-sources</code></dt><dd><p>Ignore the <code>tool.uv.sources</code> table when resolving dependencies. Used to lock against the standards-compliant, publishable package metadata, as opposed to using any local or Git sources</p>

</dd><dt><code>--offline</code></dt><dd><p>Disable network access.</p>

<p>When disabled, uv will only use locally cached data and locally available files.</p>

</dd><dt><code>--prerelease</code> <i>prerelease</i></dt><dd><p>The strategy to use when considering pre-release versions.</p>

<p>By default, uv will accept pre-releases for packages that <em>only</em> publish pre-releases, along with first-party requirements that contain an explicit pre-release marker in the declared specifiers (<code>if-necessary-or-explicit</code>).</p>

<p>Possible values:</p>

<ul>
<li><code>disallow</code>:  Disallow all pre-release versions</li>

<li><code>allow</code>:  Allow all pre-release versions</li>

<li><code>if-necessary</code>:  Allow pre-release versions if all versions of a package are pre-release</li>

<li><code>explicit</code>:  Allow pre-release versions for first-party packages with explicit pre-release markers in their version requirements</li>

<li><code>if-necessary-or-explicit</code>:  Allow pre-release versions if all versions of a package are pre-release, or if the package has an explicit pre-release marker in its version requirements</li>
</ul>
</dd><dt><code>--python</code>, <code>-p</code> <i>python</i></dt><dd><p>The Python interpreter to use to build the tool environment.</p>

<p>See <a href="#uv-python">uv python</a> for details on Python discovery and supported request formats.</p>

</dd><dt><code>--python-preference</code> <i>python-preference</i></dt><dd><p>Whether to prefer uv-managed or system Python installations.</p>

<p>By default, uv prefers using Python versions it manages. However, it will use system Python installations if a uv-managed Python is not installed. This option allows prioritizing or ignoring system Python installations.</p>

<p>Possible values:</p>

<ul>
<li><code>only-managed</code>:  Only use managed Python installations; never use system Python installations</li>

<li><code>managed</code>:  Prefer managed Python installations over system Python installations</li>

<li><code>system</code>:  Prefer system Python installations over managed Python installations</li>

<li><code>only-system</code>:  Only use system Python installations; never use managed Python installations</li>
</ul>
</dd><dt><code>--quiet</code>, <code>-q</code></dt><dd><p>Do not print any output</p>

</dd><dt><code>--refresh</code></dt><dd><p>Refresh all cached data</p>

</dd><dt><code>--refresh-package</code> <i>refresh-package</i></dt><dd><p>Refresh cached data for a specific package</p>

</dd><dt><code>--reinstall</code></dt><dd><p>Reinstall all packages, regardless of whether they&#8217;re already installed. Implies <code>--refresh</code></p>

</dd><dt><code>--reinstall-package</code> <i>reinstall-package</i></dt><dd><p>Reinstall a specific package, regardless of whether it&#8217;s already installed. Implies <code>--refresh-package</code></p>

</dd><dt><code>--resolution</code> <i>resolution</i></dt><dd><p>The strategy to use when selecting between the different compatible versions for a given package requirement.</p>

<p>By default, uv will use the latest compatible version of each package (<code>highest</code>).</p>

<p>Possible values:</p>

<ul>
<li><code>highest</code>:  Resolve the highest compatible version of each package</li>

<li><code>lowest</code>:  Resolve the lowest compatible version of each package</li>

<li><code>lowest-direct</code>:  Resolve the lowest compatible version of any direct dependencies, and the highest compatible version of any transitive dependencies</li>
</ul>
</dd><dt><code>--upgrade</code>, <code>-U</code></dt><dd><p>Allow package upgrades, ignoring pinned versions in any existing output file. Implies <code>--refresh</code></p>

</dd><dt><code>--upgrade-package</code>, <code>-P</code> <i>upgrade-package</i></dt><dd><p>Allow upgrades for a specific package, ignoring pinned versions in any existing output file. Implies <code>--refresh-package</code></p>

</dd><dt><code>--verbose</code>, <code>-v</code></dt><dd><p>Use verbose output.</p>

<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (&lt;https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives&gt;)</p>

</dd><dt><code>--version</code>, <code>-V</code></dt><dd><p>Display the uv version</p>

</dd><dt><code>--with</code> <i>with</i></dt><dd><p>Include the following extra requirements</p>

</dd><dt><code>--with-requirements</code> <i>with-requirements</i></dt><dd><p>Run all requirements listed in the given <code>requirements.txt</code> files</p>

</dd></dl>

### uv tool upgrade

Upgrade installed tools.

If a tool was installed with version constraints, they will be respected on upgrade — to upgrade a tool beyond the originally provided constraints, use `uv tool install` again.

If a tool was installed with specific settings, they will be respected on upgraded. For example, if `--prereleases allow` was provided during installation, it will continue to be respected in upgrades.

<h3 class="cli-reference">Usage</h3>

```
uv tool upgrade [OPTIONS] <NAME>
```

<h3 class="cli-reference">Arguments</h3>

<dl class="cli-reference"><dt><code>NAME</code></dt><dd><p>The name of the tool to upgrade</p>

</dd></dl>

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt><code>--all</code></dt><dd><p>Upgrade all tools</p>

</dd><dt><code>--cache-dir</code> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>

<p>Defaults to <code>$HOME/Library/Caches/uv</code> on macOS, <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on Linux, and <code>{FOLDERID_LocalAppData}\uv\cache</code> on Windows.</p>

</dd><dt><code>--color</code> <i>color-choice</i></dt><dd><p>Control colors in output</p>

<p>[default: auto]</p>
<p>Possible values:</p>

<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>

<li><code>always</code>:  Enables colored output regardless of the detected environment</li>

<li><code>never</code>:  Disables colored output</li>
</ul>
</dd><dt><code>--compile-bytecode</code></dt><dd><p>Compile Python files to bytecode after installation.</p>

<p>By default, uv does not compile Python (<code>.py</code>) files to bytecode (<code>__pycache__/*.pyc</code>); instead, compilation is performed lazily the first time a module is imported. For use-cases in which start time is critical, such as CLI applications and Docker containers, this option can be enabled to trade longer installation times for faster start times.</p>

<p>When enabled, uv will process the entire site-packages directory (including packages that are not being modified by the current operation) for consistency. Like pip, it will also ignore errors.</p>

</dd><dt><code>--config-file</code> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>

<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>

</dd><dt><code>--config-setting</code>, <code>-C</code> <i>config-setting</i></dt><dd><p>Settings to pass to the PEP 517 build backend, specified as <code>KEY=VALUE</code> pairs</p>

</dd><dt><code>--exclude-newer</code> <i>exclude-newer</i></dt><dd><p>Limit candidate packages to those that were uploaded prior to the given date.</p>

<p>Accepts both RFC 3339 timestamps (e.g., <code>2006-12-02T02:07:43Z</code>) and UTC dates in the same format (e.g., <code>2006-12-02</code>).</p>

</dd><dt><code>--extra-index-url</code> <i>extra-index-url</i></dt><dd><p>Extra URLs of package indexes to use, in addition to <code>--index-url</code>.</p>

<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>

<p>All indexes provided via this flag take priority over the index specified by <code>--index-url</code> (which defaults to PyPI). When multiple <code>--extra-index-url</code> flags are provided, earlier values take priority.</p>

</dd><dt><code>--find-links</code>, <code>-f</code> <i>find-links</i></dt><dd><p>Locations to search for candidate distributions, in addition to those found in the registry indexes.</p>

<p>If a path, the target must be a directory that contains packages as wheel files (<code>.whl</code>) or source distributions (<code>.tar.gz</code> or <code>.zip</code>) at the top level.</p>

<p>If a URL, the page must contain a flat list of links to package files adhering to the formats described above.</p>

</dd><dt><code>--help</code>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>

</dd><dt><code>--index-strategy</code> <i>index-strategy</i></dt><dd><p>The strategy to use when resolving against multiple index URLs.</p>

<p>By default, uv will stop at the first index on which a given package is available, and limit resolutions to those present on that first index (<code>first-match</code>). This prevents &quot;dependency confusion&quot; attacks, whereby an attack can upload a malicious package under the same name to a secondary.</p>

<p>Possible values:</p>

<ul>
<li><code>first-index</code>:  Only use results from the first index that returns a match for a given package name</li>

<li><code>unsafe-first-match</code>:  Search for every package name across all indexes, exhausting the versions from the first index before moving on to the next</li>

<li><code>unsafe-best-match</code>:  Search for every package name across all indexes, preferring the &quot;best&quot; version found. If a package version is in multiple indexes, only look at the entry for the first index</li>
</ul>
</dd><dt><code>--index-url</code>, <code>-i</code> <i>index-url</i></dt><dd><p>The URL of the Python package index (by default: &lt;https://pypi.org/simple&gt;).</p>

<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>

<p>The index given by this flag is given lower priority than all other indexes specified via the <code>--extra-index-url</code> flag.</p>

</dd><dt><code>--keyring-provider</code> <i>keyring-provider</i></dt><dd><p>Attempt to use <code>keyring</code> for authentication for index URLs.</p>

<p>At present, only <code>--keyring-provider subprocess</code> is supported, which configures uv to use the <code>keyring</code> CLI to handle authentication.</p>

<p>Defaults to <code>disabled</code>.</p>

<p>Possible values:</p>

<ul>
<li><code>disabled</code>:  Do not use keyring for credential lookup</li>

<li><code>subprocess</code>:  Use the <code>keyring</code> command for credential lookup</li>
</ul>
</dd><dt><code>--link-mode</code> <i>link-mode</i></dt><dd><p>The method to use when installing packages from the global cache.</p>

<p>Defaults to <code>clone</code> (also known as Copy-on-Write) on macOS, and <code>hardlink</code> on Linux and Windows.</p>

<p>Possible values:</p>

<ul>
<li><code>clone</code>:  Clone (i.e., copy-on-write) packages from the wheel into the <code>site-packages</code> directory</li>

<li><code>copy</code>:  Copy packages from the wheel into the <code>site-packages</code> directory</li>

<li><code>hardlink</code>:  Hard link packages from the wheel into the <code>site-packages</code> directory</li>

<li><code>symlink</code>:  Symbolically link packages from the wheel into the <code>site-packages</code> directory</li>
</ul>
</dd><dt><code>--native-tls</code></dt><dd><p>Whether to load TLS certificates from the platform&#8217;s native certificate store.</p>

<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>

<p>However, in some cases, you may want to use the platform&#8217;s native certificate store, especially if you&#8217;re relying on a corporate trust root (e.g., for a mandatory proxy) that&#8217;s included in your system&#8217;s certificate store.</p>

</dd><dt><code>--no-binary</code></dt><dd><p>Don&#8217;t install pre-built wheels.</p>

<p>The given packages will be built and installed from source. The resolver will still use pre-built wheels to extract package metadata, if available.</p>

</dd><dt><code>--no-binary-package</code> <i>no-binary-package</i></dt><dd><p>Don&#8217;t install pre-built wheels for a specific package</p>

</dd><dt><code>--no-build</code></dt><dd><p>Don&#8217;t build source distributions.</p>

<p>When enabled, resolving will not run arbitrary Python code. The cached wheels of already-built source distributions will be reused, but operations that require building distributions will exit with an error.</p>

</dd><dt><code>--no-build-isolation</code></dt><dd><p>Disable isolation when building source distributions.</p>

<p>Assumes that build dependencies specified by PEP 518 are already installed.</p>

</dd><dt><code>--no-build-isolation-package</code> <i>no-build-isolation-package</i></dt><dd><p>Disable isolation when building source distributions for a specific package.</p>

<p>Assumes that the packages&#8217; build dependencies specified by PEP 518  are already installed.</p>

</dd><dt><code>--no-build-package</code> <i>no-build-package</i></dt><dd><p>Don&#8217;t build source distributions for a specific package</p>

</dd><dt><code>--no-cache</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>

</dd><dt><code>--no-config</code></dt><dd><p>Avoid discovering configuration files (<code>pyproject.toml</code>, <code>uv.toml</code>).</p>

<p>Normally, configuration files are discovered in the current directory, parent directories, or user configuration directories.</p>

</dd><dt><code>--no-index</code></dt><dd><p>Ignore the registry index (e.g., PyPI), instead relying on direct URL dependencies and those provided via <code>--find-links</code></p>

</dd><dt><code>--no-progress</code></dt><dd><p>Hide all progress outputs.</p>

<p>For example, spinners or progress bars.</p>

</dd><dt><code>--no-python-downloads</code></dt><dd><p>Disable automatic downloads of Python</p>

</dd><dt><code>--no-sources</code></dt><dd><p>Ignore the <code>tool.uv.sources</code> table when resolving dependencies. Used to lock against the standards-compliant, publishable package metadata, as opposed to using any local or Git sources</p>

</dd><dt><code>--offline</code></dt><dd><p>Disable network access.</p>

<p>When disabled, uv will only use locally cached data and locally available files.</p>

</dd><dt><code>--prerelease</code> <i>prerelease</i></dt><dd><p>The strategy to use when considering pre-release versions.</p>

<p>By default, uv will accept pre-releases for packages that <em>only</em> publish pre-releases, along with first-party requirements that contain an explicit pre-release marker in the declared specifiers (<code>if-necessary-or-explicit</code>).</p>

<p>Possible values:</p>

<ul>
<li><code>disallow</code>:  Disallow all pre-release versions</li>

<li><code>allow</code>:  Allow all pre-release versions</li>

<li><code>if-necessary</code>:  Allow pre-release versions if all versions of a package are pre-release</li>

<li><code>explicit</code>:  Allow pre-release versions for first-party packages with explicit pre-release markers in their version requirements</li>

<li><code>if-necessary-or-explicit</code>:  Allow pre-release versions if all versions of a package are pre-release, or if the package has an explicit pre-release marker in its version requirements</li>
</ul>
</dd><dt><code>--python-preference</code> <i>python-preference</i></dt><dd><p>Whether to prefer uv-managed or system Python installations.</p>

<p>By default, uv prefers using Python versions it manages. However, it will use system Python installations if a uv-managed Python is not installed. This option allows prioritizing or ignoring system Python installations.</p>

<p>Possible values:</p>

<ul>
<li><code>only-managed</code>:  Only use managed Python installations; never use system Python installations</li>

<li><code>managed</code>:  Prefer managed Python installations over system Python installations</li>

<li><code>system</code>:  Prefer system Python installations over managed Python installations</li>

<li><code>only-system</code>:  Only use system Python installations; never use managed Python installations</li>
</ul>
</dd><dt><code>--quiet</code>, <code>-q</code></dt><dd><p>Do not print any output</p>

</dd><dt><code>--reinstall</code></dt><dd><p>Reinstall all packages, regardless of whether they&#8217;re already installed. Implies <code>--refresh</code></p>

</dd><dt><code>--reinstall-package</code> <i>reinstall-package</i></dt><dd><p>Reinstall a specific package, regardless of whether it&#8217;s already installed. Implies <code>--refresh-package</code></p>

</dd><dt><code>--resolution</code> <i>resolution</i></dt><dd><p>The strategy to use when selecting between the different compatible versions for a given package requirement.</p>

<p>By default, uv will use the latest compatible version of each package (<code>highest</code>).</p>

<p>Possible values:</p>

<ul>
<li><code>highest</code>:  Resolve the highest compatible version of each package</li>

<li><code>lowest</code>:  Resolve the lowest compatible version of each package</li>

<li><code>lowest-direct</code>:  Resolve the lowest compatible version of any direct dependencies, and the highest compatible version of any transitive dependencies</li>
</ul>
</dd><dt><code>--upgrade</code>, <code>-U</code></dt><dd><p>Allow package upgrades, ignoring pinned versions in any existing output file. Implies <code>--refresh</code></p>

</dd><dt><code>--upgrade-package</code>, <code>-P</code> <i>upgrade-package</i></dt><dd><p>Allow upgrades for a specific package, ignoring pinned versions in any existing output file. Implies <code>--refresh-package</code></p>

</dd><dt><code>--verbose</code>, <code>-v</code></dt><dd><p>Use verbose output.</p>

<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (&lt;https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives&gt;)</p>

</dd><dt><code>--version</code>, <code>-V</code></dt><dd><p>Display the uv version</p>

</dd></dl>

### uv tool list

List installed tools

<h3 class="cli-reference">Usage</h3>

```
uv tool list [OPTIONS]
```

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt><code>--cache-dir</code> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>

<p>Defaults to <code>$HOME/Library/Caches/uv</code> on macOS, <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on Linux, and <code>{FOLDERID_LocalAppData}\uv\cache</code> on Windows.</p>

</dd><dt><code>--color</code> <i>color-choice</i></dt><dd><p>Control colors in output</p>

<p>[default: auto]</p>
<p>Possible values:</p>

<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>

<li><code>always</code>:  Enables colored output regardless of the detected environment</li>

<li><code>never</code>:  Disables colored output</li>
</ul>
</dd><dt><code>--config-file</code> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>

<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>

</dd><dt><code>--help</code>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>

</dd><dt><code>--native-tls</code></dt><dd><p>Whether to load TLS certificates from the platform&#8217;s native certificate store.</p>

<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>

<p>However, in some cases, you may want to use the platform&#8217;s native certificate store, especially if you&#8217;re relying on a corporate trust root (e.g., for a mandatory proxy) that&#8217;s included in your system&#8217;s certificate store.</p>

</dd><dt><code>--no-cache</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>

</dd><dt><code>--no-config</code></dt><dd><p>Avoid discovering configuration files (<code>pyproject.toml</code>, <code>uv.toml</code>).</p>

<p>Normally, configuration files are discovered in the current directory, parent directories, or user configuration directories.</p>

</dd><dt><code>--no-progress</code></dt><dd><p>Hide all progress outputs.</p>

<p>For example, spinners or progress bars.</p>

</dd><dt><code>--offline</code></dt><dd><p>Disable network access.</p>

<p>When disabled, uv will only use locally cached data and locally available files.</p>

</dd><dt><code>--quiet</code>, <code>-q</code></dt><dd><p>Do not print any output</p>

</dd><dt><code>--show-paths</code></dt><dd><p>Whether to display the path to each tool environment and installed executable</p>

</dd><dt><code>--verbose</code>, <code>-v</code></dt><dd><p>Use verbose output.</p>

<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (&lt;https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives&gt;)</p>

</dd><dt><code>--version</code>, <code>-V</code></dt><dd><p>Display the uv version</p>

</dd></dl>

### uv tool uninstall

Uninstall a tool

<h3 class="cli-reference">Usage</h3>

```
uv tool uninstall [OPTIONS] <NAME>
```

<h3 class="cli-reference">Arguments</h3>

<dl class="cli-reference"><dt><code>NAME</code></dt><dd><p>The name of the tool to uninstall</p>

</dd></dl>

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt><code>--all</code></dt><dd><p>Uninstall all tools</p>

</dd><dt><code>--cache-dir</code> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>

<p>Defaults to <code>$HOME/Library/Caches/uv</code> on macOS, <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on Linux, and <code>{FOLDERID_LocalAppData}\uv\cache</code> on Windows.</p>

</dd><dt><code>--color</code> <i>color-choice</i></dt><dd><p>Control colors in output</p>

<p>[default: auto]</p>
<p>Possible values:</p>

<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>

<li><code>always</code>:  Enables colored output regardless of the detected environment</li>

<li><code>never</code>:  Disables colored output</li>
</ul>
</dd><dt><code>--config-file</code> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>

<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>

</dd><dt><code>--help</code>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>

</dd><dt><code>--native-tls</code></dt><dd><p>Whether to load TLS certificates from the platform&#8217;s native certificate store.</p>

<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>

<p>However, in some cases, you may want to use the platform&#8217;s native certificate store, especially if you&#8217;re relying on a corporate trust root (e.g., for a mandatory proxy) that&#8217;s included in your system&#8217;s certificate store.</p>

</dd><dt><code>--no-cache</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>

</dd><dt><code>--no-config</code></dt><dd><p>Avoid discovering configuration files (<code>pyproject.toml</code>, <code>uv.toml</code>).</p>

<p>Normally, configuration files are discovered in the current directory, parent directories, or user configuration directories.</p>

</dd><dt><code>--no-progress</code></dt><dd><p>Hide all progress outputs.</p>

<p>For example, spinners or progress bars.</p>

</dd><dt><code>--no-python-downloads</code></dt><dd><p>Disable automatic downloads of Python</p>

</dd><dt><code>--offline</code></dt><dd><p>Disable network access.</p>

<p>When disabled, uv will only use locally cached data and locally available files.</p>

</dd><dt><code>--python-preference</code> <i>python-preference</i></dt><dd><p>Whether to prefer uv-managed or system Python installations.</p>

<p>By default, uv prefers using Python versions it manages. However, it will use system Python installations if a uv-managed Python is not installed. This option allows prioritizing or ignoring system Python installations.</p>

<p>Possible values:</p>

<ul>
<li><code>only-managed</code>:  Only use managed Python installations; never use system Python installations</li>

<li><code>managed</code>:  Prefer managed Python installations over system Python installations</li>

<li><code>system</code>:  Prefer system Python installations over managed Python installations</li>

<li><code>only-system</code>:  Only use system Python installations; never use managed Python installations</li>
</ul>
</dd><dt><code>--quiet</code>, <code>-q</code></dt><dd><p>Do not print any output</p>

</dd><dt><code>--verbose</code>, <code>-v</code></dt><dd><p>Use verbose output.</p>

<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (&lt;https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives&gt;)</p>

</dd><dt><code>--version</code>, <code>-V</code></dt><dd><p>Display the uv version</p>

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

<dl class="cli-reference"><dt><code>--cache-dir</code> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>

<p>Defaults to <code>$HOME/Library/Caches/uv</code> on macOS, <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on Linux, and <code>{FOLDERID_LocalAppData}\uv\cache</code> on Windows.</p>

</dd><dt><code>--color</code> <i>color-choice</i></dt><dd><p>Control colors in output</p>

<p>[default: auto]</p>
<p>Possible values:</p>

<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>

<li><code>always</code>:  Enables colored output regardless of the detected environment</li>

<li><code>never</code>:  Disables colored output</li>
</ul>
</dd><dt><code>--config-file</code> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>

<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>

</dd><dt><code>--help</code>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>

</dd><dt><code>--native-tls</code></dt><dd><p>Whether to load TLS certificates from the platform&#8217;s native certificate store.</p>

<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>

<p>However, in some cases, you may want to use the platform&#8217;s native certificate store, especially if you&#8217;re relying on a corporate trust root (e.g., for a mandatory proxy) that&#8217;s included in your system&#8217;s certificate store.</p>

</dd><dt><code>--no-cache</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>

</dd><dt><code>--no-config</code></dt><dd><p>Avoid discovering configuration files (<code>pyproject.toml</code>, <code>uv.toml</code>).</p>

<p>Normally, configuration files are discovered in the current directory, parent directories, or user configuration directories.</p>

</dd><dt><code>--no-progress</code></dt><dd><p>Hide all progress outputs.</p>

<p>For example, spinners or progress bars.</p>

</dd><dt><code>--no-python-downloads</code></dt><dd><p>Disable automatic downloads of Python</p>

</dd><dt><code>--offline</code></dt><dd><p>Disable network access.</p>

<p>When disabled, uv will only use locally cached data and locally available files.</p>

</dd><dt><code>--python-preference</code> <i>python-preference</i></dt><dd><p>Whether to prefer uv-managed or system Python installations.</p>

<p>By default, uv prefers using Python versions it manages. However, it will use system Python installations if a uv-managed Python is not installed. This option allows prioritizing or ignoring system Python installations.</p>

<p>Possible values:</p>

<ul>
<li><code>only-managed</code>:  Only use managed Python installations; never use system Python installations</li>

<li><code>managed</code>:  Prefer managed Python installations over system Python installations</li>

<li><code>system</code>:  Prefer system Python installations over managed Python installations</li>

<li><code>only-system</code>:  Only use system Python installations; never use managed Python installations</li>
</ul>
</dd><dt><code>--quiet</code>, <code>-q</code></dt><dd><p>Do not print any output</p>

</dd><dt><code>--verbose</code>, <code>-v</code></dt><dd><p>Use verbose output.</p>

<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (&lt;https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives&gt;)</p>

</dd><dt><code>--version</code>, <code>-V</code></dt><dd><p>Display the uv version</p>

</dd></dl>

### uv tool dir

Show the path to the uv tools directory.

The tools directory is used to store environments and metadata for installed tools.

To instead view the directory uv installs executables into, use the `--bin` flag.

<h3 class="cli-reference">Usage</h3>

```
uv tool dir [OPTIONS]
```

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt><code>--bin</code></dt><dd><p>Show the directory into which <code>uv tool</code> will install executables.</p>

<p>By default, <code>uv tool dir</code> shows the directory into which the tool Python environments themselves are installed, rather than the directory containing the linked executables.</p>

</dd><dt><code>--cache-dir</code> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>

<p>Defaults to <code>$HOME/Library/Caches/uv</code> on macOS, <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on Linux, and <code>{FOLDERID_LocalAppData}\uv\cache</code> on Windows.</p>

</dd><dt><code>--color</code> <i>color-choice</i></dt><dd><p>Control colors in output</p>

<p>[default: auto]</p>
<p>Possible values:</p>

<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>

<li><code>always</code>:  Enables colored output regardless of the detected environment</li>

<li><code>never</code>:  Disables colored output</li>
</ul>
</dd><dt><code>--config-file</code> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>

<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>

</dd><dt><code>--help</code>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>

</dd><dt><code>--native-tls</code></dt><dd><p>Whether to load TLS certificates from the platform&#8217;s native certificate store.</p>

<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>

<p>However, in some cases, you may want to use the platform&#8217;s native certificate store, especially if you&#8217;re relying on a corporate trust root (e.g., for a mandatory proxy) that&#8217;s included in your system&#8217;s certificate store.</p>

</dd><dt><code>--no-cache</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>

</dd><dt><code>--no-config</code></dt><dd><p>Avoid discovering configuration files (<code>pyproject.toml</code>, <code>uv.toml</code>).</p>

<p>Normally, configuration files are discovered in the current directory, parent directories, or user configuration directories.</p>

</dd><dt><code>--no-progress</code></dt><dd><p>Hide all progress outputs.</p>

<p>For example, spinners or progress bars.</p>

</dd><dt><code>--no-python-downloads</code></dt><dd><p>Disable automatic downloads of Python</p>

</dd><dt><code>--offline</code></dt><dd><p>Disable network access.</p>

<p>When disabled, uv will only use locally cached data and locally available files.</p>

</dd><dt><code>--python-preference</code> <i>python-preference</i></dt><dd><p>Whether to prefer uv-managed or system Python installations.</p>

<p>By default, uv prefers using Python versions it manages. However, it will use system Python installations if a uv-managed Python is not installed. This option allows prioritizing or ignoring system Python installations.</p>

<p>Possible values:</p>

<ul>
<li><code>only-managed</code>:  Only use managed Python installations; never use system Python installations</li>

<li><code>managed</code>:  Prefer managed Python installations over system Python installations</li>

<li><code>system</code>:  Prefer system Python installations over managed Python installations</li>

<li><code>only-system</code>:  Only use system Python installations; never use managed Python installations</li>
</ul>
</dd><dt><code>--quiet</code>, <code>-q</code></dt><dd><p>Do not print any output</p>

</dd><dt><code>--verbose</code>, <code>-v</code></dt><dd><p>Use verbose output.</p>

<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (&lt;https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives&gt;)</p>

</dd><dt><code>--version</code>, <code>-V</code></dt><dd><p>Display the uv version</p>

</dd></dl>

## uv python

Manage Python versions and installations (experimental)

Generally, uv first searches for Python in a virtual environment, either
active or in a `.venv` directory  in the current working directory or
any parent directory. If a virtual environment is not required, uv will
then search for a Python interpreter. Python interpreters are found by
searching for Python executables in the `PATH` environment variable.

On Windows, the `py` launcher is also invoked to find Python
executables.

When preview is enabled, i.e., via `--preview` or by using a preview
command, uv will download Python if a version cannot be found. This
behavior can be disabled with the `--python-downloads` option.

The `--python` option allows requesting a different interpreter.

The following Python version request formats are supported:

- `<version>` e.g. `3`, `3.12`, `3.12.3`
- `<version-specifier>` e.g. `>=3.12,<3.13`
- `<implementation>` e.g. `cpython` or `cp`
- `<implementation>@<version>` e.g. `cpython@3.12`
- `<implementation><version>` e.g. `cpython3.12` or `cp312`
- `<implementation><version-specifier>` e.g. `cpython>=3.12,<3.13`
- `<implementation>-<version>-<os>-<arch>-<libc>` e.g.
  `cpython-3.12.3-macos-aarch64-none`

Additionally, a specific system Python interpreter can often be
requested with:

- `<executable-path>` e.g. `/opt/homebrew/bin/python3`
- `<executable-name>` e.g. `mypython3`
- `<install-dir>` e.g. `/some/environment/`

When the `--python` option is used, normal discovery rules apply but
discovered interpreters are checked for compatibility with the request,
e.g., if `pypy` is requested, uv will first check if the virtual
environment contains a PyPy interpreter then check if each executable in
the path is a PyPy interpreter.

uv supports discovering CPython, PyPy, and GraalPy interpreters.
Unsupported interpreters will be skipped during discovery. If an
unsupported interpreter implementation is requested, uv will exit with
an error.

<h3 class="cli-reference">Usage</h3>

```
uv python [OPTIONS] <COMMAND>
```

<h3 class="cli-reference">Commands</h3>

<dl class="cli-reference"><dt><a href="#uv-python-list"><code>uv python list</code></a></dt><dd><p>List the available Python installations</p>
</dd>
<dt><a href="#uv-python-install"><code>uv python install</code></a></dt><dd><p>Download and install Python versions</p>
</dd>
<dt><a href="#uv-python-find"><code>uv python find</code></a></dt><dd><p>Search for a Python installation</p>
</dd>
<dt><a href="#uv-python-pin"><code>uv python pin</code></a></dt><dd><p>Pin to a specific Python version</p>
</dd>
<dt><a href="#uv-python-dir"><code>uv python dir</code></a></dt><dd><p>Show the uv Python installation directory</p>
</dd>
<dt><a href="#uv-python-uninstall"><code>uv python uninstall</code></a></dt><dd><p>Uninstall Python versions</p>
</dd>
</dl>

### uv python list

List the available Python installations.

By default, installed Python versions and the downloads for latest available patch version of each supported Python major version are shown.

The displayed versions are filtered by the `--python-preference` option, i.e., if using `only-system`, no managed Python versions will be shown.

Use `--all-versions` to view all available patch versions.

Use `--only-installed` to omit available downloads.

<h3 class="cli-reference">Usage</h3>

```
uv python list [OPTIONS]
```

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt><code>--all-platforms</code></dt><dd><p>List Python downloads for all platforms.</p>

<p>By default, only downloads for the current platform are shown.</p>

</dd><dt><code>--all-versions</code></dt><dd><p>List all Python versions, including old patch versions.</p>

<p>By default, only the latest patch version is shown for each minor version.</p>

</dd><dt><code>--cache-dir</code> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>

<p>Defaults to <code>$HOME/Library/Caches/uv</code> on macOS, <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on Linux, and <code>{FOLDERID_LocalAppData}\uv\cache</code> on Windows.</p>

</dd><dt><code>--color</code> <i>color-choice</i></dt><dd><p>Control colors in output</p>

<p>[default: auto]</p>
<p>Possible values:</p>

<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>

<li><code>always</code>:  Enables colored output regardless of the detected environment</li>

<li><code>never</code>:  Disables colored output</li>
</ul>
</dd><dt><code>--config-file</code> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>

<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>

</dd><dt><code>--help</code>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>

</dd><dt><code>--native-tls</code></dt><dd><p>Whether to load TLS certificates from the platform&#8217;s native certificate store.</p>

<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>

<p>However, in some cases, you may want to use the platform&#8217;s native certificate store, especially if you&#8217;re relying on a corporate trust root (e.g., for a mandatory proxy) that&#8217;s included in your system&#8217;s certificate store.</p>

</dd><dt><code>--no-cache</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>

</dd><dt><code>--no-config</code></dt><dd><p>Avoid discovering configuration files (<code>pyproject.toml</code>, <code>uv.toml</code>).</p>

<p>Normally, configuration files are discovered in the current directory, parent directories, or user configuration directories.</p>

</dd><dt><code>--no-progress</code></dt><dd><p>Hide all progress outputs.</p>

<p>For example, spinners or progress bars.</p>

</dd><dt><code>--no-python-downloads</code></dt><dd><p>Disable automatic downloads of Python</p>

</dd><dt><code>--offline</code></dt><dd><p>Disable network access.</p>

<p>When disabled, uv will only use locally cached data and locally available files.</p>

</dd><dt><code>--only-installed</code></dt><dd><p>Only show installed Python versions, exclude available downloads.</p>

<p>By default, available downloads for the current platform are shown.</p>

</dd><dt><code>--python-preference</code> <i>python-preference</i></dt><dd><p>Whether to prefer uv-managed or system Python installations.</p>

<p>By default, uv prefers using Python versions it manages. However, it will use system Python installations if a uv-managed Python is not installed. This option allows prioritizing or ignoring system Python installations.</p>

<p>Possible values:</p>

<ul>
<li><code>only-managed</code>:  Only use managed Python installations; never use system Python installations</li>

<li><code>managed</code>:  Prefer managed Python installations over system Python installations</li>

<li><code>system</code>:  Prefer system Python installations over managed Python installations</li>

<li><code>only-system</code>:  Only use system Python installations; never use managed Python installations</li>
</ul>
</dd><dt><code>--quiet</code>, <code>-q</code></dt><dd><p>Do not print any output</p>

</dd><dt><code>--verbose</code>, <code>-v</code></dt><dd><p>Use verbose output.</p>

<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (&lt;https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives&gt;)</p>

</dd><dt><code>--version</code>, <code>-V</code></dt><dd><p>Display the uv version</p>

</dd></dl>

### uv python install

Download and install Python versions.

Multiple Python versions may be requested.

Supports CPython and PyPy.

CPython distributions are downloaded from the `python-build-standalone` project.

Python versions are installed into the uv Python directory, which can be retrieved with `uv python dir`. A `python` executable is not made globally available, managed Python versions are only used in uv commands or in active virtual environments.

See `uv help python` to view supported request formats.

<h3 class="cli-reference">Usage</h3>

```
uv python install [OPTIONS] [TARGETS]...
```

<h3 class="cli-reference">Arguments</h3>

<dl class="cli-reference"><dt><code>TARGETS</code></dt><dd><p>The Python version(s) to install.</p>

<p>If not provided, the requested Python version(s) will be read from the <code>.python-versions</code> or <code>.python-version</code> files. If neither file is present, uv will check if it has installed any Python versions. If not, it will install the latest stable version of Python.</p>

<p>See <a href="#uv-python">uv python</a> to view supported request formats.</p>

</dd></dl>

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt><code>--cache-dir</code> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>

<p>Defaults to <code>$HOME/Library/Caches/uv</code> on macOS, <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on Linux, and <code>{FOLDERID_LocalAppData}\uv\cache</code> on Windows.</p>

</dd><dt><code>--color</code> <i>color-choice</i></dt><dd><p>Control colors in output</p>

<p>[default: auto]</p>
<p>Possible values:</p>

<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>

<li><code>always</code>:  Enables colored output regardless of the detected environment</li>

<li><code>never</code>:  Disables colored output</li>
</ul>
</dd><dt><code>--config-file</code> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>

<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>

</dd><dt><code>--help</code>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>

</dd><dt><code>--native-tls</code></dt><dd><p>Whether to load TLS certificates from the platform&#8217;s native certificate store.</p>

<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>

<p>However, in some cases, you may want to use the platform&#8217;s native certificate store, especially if you&#8217;re relying on a corporate trust root (e.g., for a mandatory proxy) that&#8217;s included in your system&#8217;s certificate store.</p>

</dd><dt><code>--no-cache</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>

</dd><dt><code>--no-config</code></dt><dd><p>Avoid discovering configuration files (<code>pyproject.toml</code>, <code>uv.toml</code>).</p>

<p>Normally, configuration files are discovered in the current directory, parent directories, or user configuration directories.</p>

</dd><dt><code>--no-progress</code></dt><dd><p>Hide all progress outputs.</p>

<p>For example, spinners or progress bars.</p>

</dd><dt><code>--no-python-downloads</code></dt><dd><p>Disable automatic downloads of Python</p>

</dd><dt><code>--offline</code></dt><dd><p>Disable network access.</p>

<p>When disabled, uv will only use locally cached data and locally available files.</p>

</dd><dt><code>--python-preference</code> <i>python-preference</i></dt><dd><p>Whether to prefer uv-managed or system Python installations.</p>

<p>By default, uv prefers using Python versions it manages. However, it will use system Python installations if a uv-managed Python is not installed. This option allows prioritizing or ignoring system Python installations.</p>

<p>Possible values:</p>

<ul>
<li><code>only-managed</code>:  Only use managed Python installations; never use system Python installations</li>

<li><code>managed</code>:  Prefer managed Python installations over system Python installations</li>

<li><code>system</code>:  Prefer system Python installations over managed Python installations</li>

<li><code>only-system</code>:  Only use system Python installations; never use managed Python installations</li>
</ul>
</dd><dt><code>--quiet</code>, <code>-q</code></dt><dd><p>Do not print any output</p>

</dd><dt><code>--reinstall</code>, <code>-r</code></dt><dd><p>Reinstall the requested Python version, if it&#8217;s already installed.</p>

<p>By default, uv will exit successfully if the version is already installed.</p>

</dd><dt><code>--verbose</code>, <code>-v</code></dt><dd><p>Use verbose output.</p>

<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (&lt;https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives&gt;)</p>

</dd><dt><code>--version</code>, <code>-V</code></dt><dd><p>Display the uv version</p>

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

<dl class="cli-reference"><dt><code>REQUEST</code></dt><dd><p>The Python request.</p>

<p>See <a href="#uv-python">uv python</a> to view supported request formats.</p>

</dd></dl>

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt><code>--cache-dir</code> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>

<p>Defaults to <code>$HOME/Library/Caches/uv</code> on macOS, <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on Linux, and <code>{FOLDERID_LocalAppData}\uv\cache</code> on Windows.</p>

</dd><dt><code>--color</code> <i>color-choice</i></dt><dd><p>Control colors in output</p>

<p>[default: auto]</p>
<p>Possible values:</p>

<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>

<li><code>always</code>:  Enables colored output regardless of the detected environment</li>

<li><code>never</code>:  Disables colored output</li>
</ul>
</dd><dt><code>--config-file</code> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>

<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>

</dd><dt><code>--help</code>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>

</dd><dt><code>--native-tls</code></dt><dd><p>Whether to load TLS certificates from the platform&#8217;s native certificate store.</p>

<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>

<p>However, in some cases, you may want to use the platform&#8217;s native certificate store, especially if you&#8217;re relying on a corporate trust root (e.g., for a mandatory proxy) that&#8217;s included in your system&#8217;s certificate store.</p>

</dd><dt><code>--no-cache</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>

</dd><dt><code>--no-config</code></dt><dd><p>Avoid discovering configuration files (<code>pyproject.toml</code>, <code>uv.toml</code>).</p>

<p>Normally, configuration files are discovered in the current directory, parent directories, or user configuration directories.</p>

</dd><dt><code>--no-progress</code></dt><dd><p>Hide all progress outputs.</p>

<p>For example, spinners or progress bars.</p>

</dd><dt><code>--no-python-downloads</code></dt><dd><p>Disable automatic downloads of Python</p>

</dd><dt><code>--offline</code></dt><dd><p>Disable network access.</p>

<p>When disabled, uv will only use locally cached data and locally available files.</p>

</dd><dt><code>--python-preference</code> <i>python-preference</i></dt><dd><p>Whether to prefer uv-managed or system Python installations.</p>

<p>By default, uv prefers using Python versions it manages. However, it will use system Python installations if a uv-managed Python is not installed. This option allows prioritizing or ignoring system Python installations.</p>

<p>Possible values:</p>

<ul>
<li><code>only-managed</code>:  Only use managed Python installations; never use system Python installations</li>

<li><code>managed</code>:  Prefer managed Python installations over system Python installations</li>

<li><code>system</code>:  Prefer system Python installations over managed Python installations</li>

<li><code>only-system</code>:  Only use system Python installations; never use managed Python installations</li>
</ul>
</dd><dt><code>--quiet</code>, <code>-q</code></dt><dd><p>Do not print any output</p>

</dd><dt><code>--verbose</code>, <code>-v</code></dt><dd><p>Use verbose output.</p>

<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (&lt;https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives&gt;)</p>

</dd><dt><code>--version</code>, <code>-V</code></dt><dd><p>Display the uv version</p>

</dd></dl>

### uv python pin

Pin to a specific Python version.

Writes the pinned version to a `.python-version` file, which is then read by other uv commands when determining the required Python version.

See `uv help python` to view supported request formats.

<h3 class="cli-reference">Usage</h3>

```
uv python pin [OPTIONS] [REQUEST]
```

<h3 class="cli-reference">Arguments</h3>

<dl class="cli-reference"><dt><code>REQUEST</code></dt><dd><p>The Python version request.</p>

<p>uv supports more formats than other tools that read <code>.python-version</code> files, i.e., <code>pyenv</code>. If compatibility with those tools is needed, only use version numbers instead of complex requests such as <code>cpython@3.10</code>.</p>

<p>See <a href="#uv-python">uv python</a> to view supported request formats.</p>

</dd></dl>

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt><code>--cache-dir</code> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>

<p>Defaults to <code>$HOME/Library/Caches/uv</code> on macOS, <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on Linux, and <code>{FOLDERID_LocalAppData}\uv\cache</code> on Windows.</p>

</dd><dt><code>--color</code> <i>color-choice</i></dt><dd><p>Control colors in output</p>

<p>[default: auto]</p>
<p>Possible values:</p>

<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>

<li><code>always</code>:  Enables colored output regardless of the detected environment</li>

<li><code>never</code>:  Disables colored output</li>
</ul>
</dd><dt><code>--config-file</code> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>

<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>

</dd><dt><code>--help</code>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>

</dd><dt><code>--native-tls</code></dt><dd><p>Whether to load TLS certificates from the platform&#8217;s native certificate store.</p>

<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>

<p>However, in some cases, you may want to use the platform&#8217;s native certificate store, especially if you&#8217;re relying on a corporate trust root (e.g., for a mandatory proxy) that&#8217;s included in your system&#8217;s certificate store.</p>

</dd><dt><code>--no-cache</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>

</dd><dt><code>--no-config</code></dt><dd><p>Avoid discovering configuration files (<code>pyproject.toml</code>, <code>uv.toml</code>).</p>

<p>Normally, configuration files are discovered in the current directory, parent directories, or user configuration directories.</p>

</dd><dt><code>--no-progress</code></dt><dd><p>Hide all progress outputs.</p>

<p>For example, spinners or progress bars.</p>

</dd><dt><code>--no-python-downloads</code></dt><dd><p>Disable automatic downloads of Python</p>

</dd><dt><code>--no-workspace</code></dt><dd><p>Avoid validating the Python pin is compatible with the workspace.</p>

<p>By default, a workspace is discovered in the current directory or any parent directory. If a workspace is found, the Python pin is validated against the workspace&#8217;s <code>requires-python</code> constraint.</p>

</dd><dt><code>--offline</code></dt><dd><p>Disable network access.</p>

<p>When disabled, uv will only use locally cached data and locally available files.</p>

</dd><dt><code>--python-preference</code> <i>python-preference</i></dt><dd><p>Whether to prefer uv-managed or system Python installations.</p>

<p>By default, uv prefers using Python versions it manages. However, it will use system Python installations if a uv-managed Python is not installed. This option allows prioritizing or ignoring system Python installations.</p>

<p>Possible values:</p>

<ul>
<li><code>only-managed</code>:  Only use managed Python installations; never use system Python installations</li>

<li><code>managed</code>:  Prefer managed Python installations over system Python installations</li>

<li><code>system</code>:  Prefer system Python installations over managed Python installations</li>

<li><code>only-system</code>:  Only use system Python installations; never use managed Python installations</li>
</ul>
</dd><dt><code>--quiet</code>, <code>-q</code></dt><dd><p>Do not print any output</p>

</dd><dt><code>--resolved</code></dt><dd><p>Write the resolved Python interpreter path instead of the request.</p>

<p>Ensures that the exact same interpreter is used.</p>

<p>This option is usually not safe to use when committing the <code>.python-version</code> file to version control.</p>

</dd><dt><code>--verbose</code>, <code>-v</code></dt><dd><p>Use verbose output.</p>

<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (&lt;https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives&gt;)</p>

</dd><dt><code>--version</code>, <code>-V</code></dt><dd><p>Display the uv version</p>

</dd></dl>

### uv python dir

Show the uv Python installation directory

<h3 class="cli-reference">Usage</h3>

```
uv python dir [OPTIONS]
```

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt><code>--cache-dir</code> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>

<p>Defaults to <code>$HOME/Library/Caches/uv</code> on macOS, <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on Linux, and <code>{FOLDERID_LocalAppData}\uv\cache</code> on Windows.</p>

</dd><dt><code>--color</code> <i>color-choice</i></dt><dd><p>Control colors in output</p>

<p>[default: auto]</p>
<p>Possible values:</p>

<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>

<li><code>always</code>:  Enables colored output regardless of the detected environment</li>

<li><code>never</code>:  Disables colored output</li>
</ul>
</dd><dt><code>--config-file</code> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>

<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>

</dd><dt><code>--help</code>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>

</dd><dt><code>--native-tls</code></dt><dd><p>Whether to load TLS certificates from the platform&#8217;s native certificate store.</p>

<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>

<p>However, in some cases, you may want to use the platform&#8217;s native certificate store, especially if you&#8217;re relying on a corporate trust root (e.g., for a mandatory proxy) that&#8217;s included in your system&#8217;s certificate store.</p>

</dd><dt><code>--no-cache</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>

</dd><dt><code>--no-config</code></dt><dd><p>Avoid discovering configuration files (<code>pyproject.toml</code>, <code>uv.toml</code>).</p>

<p>Normally, configuration files are discovered in the current directory, parent directories, or user configuration directories.</p>

</dd><dt><code>--no-progress</code></dt><dd><p>Hide all progress outputs.</p>

<p>For example, spinners or progress bars.</p>

</dd><dt><code>--no-python-downloads</code></dt><dd><p>Disable automatic downloads of Python</p>

</dd><dt><code>--offline</code></dt><dd><p>Disable network access.</p>

<p>When disabled, uv will only use locally cached data and locally available files.</p>

</dd><dt><code>--python-preference</code> <i>python-preference</i></dt><dd><p>Whether to prefer uv-managed or system Python installations.</p>

<p>By default, uv prefers using Python versions it manages. However, it will use system Python installations if a uv-managed Python is not installed. This option allows prioritizing or ignoring system Python installations.</p>

<p>Possible values:</p>

<ul>
<li><code>only-managed</code>:  Only use managed Python installations; never use system Python installations</li>

<li><code>managed</code>:  Prefer managed Python installations over system Python installations</li>

<li><code>system</code>:  Prefer system Python installations over managed Python installations</li>

<li><code>only-system</code>:  Only use system Python installations; never use managed Python installations</li>
</ul>
</dd><dt><code>--quiet</code>, <code>-q</code></dt><dd><p>Do not print any output</p>

</dd><dt><code>--verbose</code>, <code>-v</code></dt><dd><p>Use verbose output.</p>

<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (&lt;https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives&gt;)</p>

</dd><dt><code>--version</code>, <code>-V</code></dt><dd><p>Display the uv version</p>

</dd></dl>

### uv python uninstall

Uninstall Python versions

<h3 class="cli-reference">Usage</h3>

```
uv python uninstall [OPTIONS] <TARGETS>...
```

<h3 class="cli-reference">Arguments</h3>

<dl class="cli-reference"><dt><code>TARGETS</code></dt><dd><p>The Python version(s) to uninstall.</p>

<p>See <a href="#uv-python">uv python</a> to view supported request formats.</p>

</dd></dl>

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt><code>--all</code></dt><dd><p>Uninstall all managed Python versions</p>

</dd><dt><code>--cache-dir</code> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>

<p>Defaults to <code>$HOME/Library/Caches/uv</code> on macOS, <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on Linux, and <code>{FOLDERID_LocalAppData}\uv\cache</code> on Windows.</p>

</dd><dt><code>--color</code> <i>color-choice</i></dt><dd><p>Control colors in output</p>

<p>[default: auto]</p>
<p>Possible values:</p>

<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>

<li><code>always</code>:  Enables colored output regardless of the detected environment</li>

<li><code>never</code>:  Disables colored output</li>
</ul>
</dd><dt><code>--config-file</code> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>

<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>

</dd><dt><code>--help</code>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>

</dd><dt><code>--native-tls</code></dt><dd><p>Whether to load TLS certificates from the platform&#8217;s native certificate store.</p>

<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>

<p>However, in some cases, you may want to use the platform&#8217;s native certificate store, especially if you&#8217;re relying on a corporate trust root (e.g., for a mandatory proxy) that&#8217;s included in your system&#8217;s certificate store.</p>

</dd><dt><code>--no-cache</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>

</dd><dt><code>--no-config</code></dt><dd><p>Avoid discovering configuration files (<code>pyproject.toml</code>, <code>uv.toml</code>).</p>

<p>Normally, configuration files are discovered in the current directory, parent directories, or user configuration directories.</p>

</dd><dt><code>--no-progress</code></dt><dd><p>Hide all progress outputs.</p>

<p>For example, spinners or progress bars.</p>

</dd><dt><code>--no-python-downloads</code></dt><dd><p>Disable automatic downloads of Python</p>

</dd><dt><code>--offline</code></dt><dd><p>Disable network access.</p>

<p>When disabled, uv will only use locally cached data and locally available files.</p>

</dd><dt><code>--python-preference</code> <i>python-preference</i></dt><dd><p>Whether to prefer uv-managed or system Python installations.</p>

<p>By default, uv prefers using Python versions it manages. However, it will use system Python installations if a uv-managed Python is not installed. This option allows prioritizing or ignoring system Python installations.</p>

<p>Possible values:</p>

<ul>
<li><code>only-managed</code>:  Only use managed Python installations; never use system Python installations</li>

<li><code>managed</code>:  Prefer managed Python installations over system Python installations</li>

<li><code>system</code>:  Prefer system Python installations over managed Python installations</li>

<li><code>only-system</code>:  Only use system Python installations; never use managed Python installations</li>
</ul>
</dd><dt><code>--quiet</code>, <code>-q</code></dt><dd><p>Do not print any output</p>

</dd><dt><code>--verbose</code>, <code>-v</code></dt><dd><p>Use verbose output.</p>

<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (&lt;https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives&gt;)</p>

</dd><dt><code>--version</code>, <code>-V</code></dt><dd><p>Display the uv version</p>

</dd></dl>

## uv pip

Manage Python packages with a pip-compatible interface

<h3 class="cli-reference">Usage</h3>

```
uv pip [OPTIONS] <COMMAND>
```

<h3 class="cli-reference">Commands</h3>

<dl class="cli-reference"><dt><a href="#uv-pip-compile"><code>uv pip compile</code></a></dt><dd><p>Compile a <code>requirements.in</code> file to a <code>requirements.txt</code> file</p>
</dd>
<dt><a href="#uv-pip-sync"><code>uv pip sync</code></a></dt><dd><p>Sync an environment with a <code>requirements.txt</code> file</p>
</dd>
<dt><a href="#uv-pip-install"><code>uv pip install</code></a></dt><dd><p>Install packages into an environment</p>
</dd>
<dt><a href="#uv-pip-uninstall"><code>uv pip uninstall</code></a></dt><dd><p>Uninstall packages from an environment</p>
</dd>
<dt><a href="#uv-pip-freeze"><code>uv pip freeze</code></a></dt><dd><p>List, in requirements format, packages installed in an environment</p>
</dd>
<dt><a href="#uv-pip-list"><code>uv pip list</code></a></dt><dd><p>List, in tabular format, packages installed in an environment</p>
</dd>
<dt><a href="#uv-pip-show"><code>uv pip show</code></a></dt><dd><p>Show information about one or more installed packages</p>
</dd>
<dt><a href="#uv-pip-tree"><code>uv pip tree</code></a></dt><dd><p>Display the dependency tree for an environment</p>
</dd>
<dt><a href="#uv-pip-check"><code>uv pip check</code></a></dt><dd><p>Verify installed packages have compatible dependencies</p>
</dd>
</dl>

### uv pip compile

Compile a `requirements.in` file to a `requirements.txt` file

<h3 class="cli-reference">Usage</h3>

```
uv pip compile [OPTIONS] <SRC_FILE>...
```

<h3 class="cli-reference">Arguments</h3>

<dl class="cli-reference"><dt><code>SRC_FILE</code></dt><dd><p>Include all packages listed in the given <code>requirements.in</code> files.</p>

<p>If a <code>pyproject.toml</code>, <code>setup.py</code>, or <code>setup.cfg</code> file is provided, uv will extract the requirements for the relevant project.</p>

<p>If <code>-</code> is provided, then requirements will be read from stdin.</p>

</dd></dl>

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt><code>--all-extras</code></dt><dd><p>Include all optional dependencies.</p>

<p>Only applies to <code>pyproject.toml</code>, <code>setup.py</code>, and <code>setup.cfg</code> sources.</p>

</dd><dt><code>--annotation-style</code> <i>annotation-style</i></dt><dd><p>The style of the annotation comments included in the output file, used to indicate the source of each package.</p>

<p>Defaults to <code>split</code>.</p>

<p>Possible values:</p>

<ul>
<li><code>line</code>:  Render the annotations on a single, comma-separated line</li>

<li><code>split</code>:  Render each annotation on its own line</li>
</ul>
</dd><dt><code>--build-constraint</code>, <code>-b</code> <i>build-constraint</i></dt><dd><p>Constrain build dependencies using the given requirements files when building source distributions.</p>

<p>Constraints files are <code>requirements.txt</code>-like files that only control the <em>version</em> of a requirement that&#8217;s installed. However, including a package in a constraints file will <em>not</em> trigger the installation of that package.</p>

</dd><dt><code>--cache-dir</code> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>

<p>Defaults to <code>$HOME/Library/Caches/uv</code> on macOS, <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on Linux, and <code>{FOLDERID_LocalAppData}\uv\cache</code> on Windows.</p>

</dd><dt><code>--color</code> <i>color-choice</i></dt><dd><p>Control colors in output</p>

<p>[default: auto]</p>
<p>Possible values:</p>

<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>

<li><code>always</code>:  Enables colored output regardless of the detected environment</li>

<li><code>never</code>:  Disables colored output</li>
</ul>
</dd><dt><code>--config-file</code> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>

<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>

</dd><dt><code>--config-setting</code>, <code>-C</code> <i>config-setting</i></dt><dd><p>Settings to pass to the PEP 517 build backend, specified as <code>KEY=VALUE</code> pairs</p>

</dd><dt><code>--constraint</code>, <code>-c</code> <i>constraint</i></dt><dd><p>Constrain versions using the given requirements files.</p>

<p>Constraints files are <code>requirements.txt</code>-like files that only control the <em>version</em> of a requirement that&#8217;s installed. However, including a package in a constraints file will <em>not</em> trigger the installation of that package.</p>

<p>This is equivalent to pip&#8217;s <code>--constraint</code> option.</p>

</dd><dt><code>--custom-compile-command</code> <i>custom-compile-command</i></dt><dd><p>The header comment to include at the top of the output file generated by <code>uv pip compile</code>.</p>

<p>Used to reflect custom build scripts and commands that wrap <code>uv pip compile</code>.</p>

</dd><dt><code>--emit-build-options</code></dt><dd><p>Include <code>--no-binary</code> and <code>--only-binary</code> entries in the generated output file</p>

</dd><dt><code>--emit-find-links</code></dt><dd><p>Include <code>--find-links</code> entries in the generated output file</p>

</dd><dt><code>--emit-index-annotation</code></dt><dd><p>Include comment annotations indicating the index used to resolve each package (e.g., <code># from https://pypi.org/simple</code>)</p>

</dd><dt><code>--emit-index-url</code></dt><dd><p>Include <code>--index-url</code> and <code>--extra-index-url</code> entries in the generated output file</p>

</dd><dt><code>--exclude-newer</code> <i>exclude-newer</i></dt><dd><p>Limit candidate packages to those that were uploaded prior to the given date.</p>

<p>Accepts both RFC 3339 timestamps (e.g., <code>2006-12-02T02:07:43Z</code>) and UTC dates in the same format (e.g., <code>2006-12-02</code>).</p>

</dd><dt><code>--extra</code> <i>extra</i></dt><dd><p>Include optional dependencies from the extra group name; may be provided more than once.</p>

<p>Only applies to <code>pyproject.toml</code>, <code>setup.py</code>, and <code>setup.cfg</code> sources.</p>

</dd><dt><code>--extra-index-url</code> <i>extra-index-url</i></dt><dd><p>Extra URLs of package indexes to use, in addition to <code>--index-url</code>.</p>

<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>

<p>All indexes provided via this flag take priority over the index specified by <code>--index-url</code> (which defaults to PyPI). When multiple <code>--extra-index-url</code> flags are provided, earlier values take priority.</p>

</dd><dt><code>--find-links</code>, <code>-f</code> <i>find-links</i></dt><dd><p>Locations to search for candidate distributions, in addition to those found in the registry indexes.</p>

<p>If a path, the target must be a directory that contains packages as wheel files (<code>.whl</code>) or source distributions (<code>.tar.gz</code> or <code>.zip</code>) at the top level.</p>

<p>If a URL, the page must contain a flat list of links to package files adhering to the formats described above.</p>

</dd><dt><code>--generate-hashes</code></dt><dd><p>Include distribution hashes in the output file</p>

</dd><dt><code>--help</code>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>

</dd><dt><code>--index-strategy</code> <i>index-strategy</i></dt><dd><p>The strategy to use when resolving against multiple index URLs.</p>

<p>By default, uv will stop at the first index on which a given package is available, and limit resolutions to those present on that first index (<code>first-match</code>). This prevents &quot;dependency confusion&quot; attacks, whereby an attack can upload a malicious package under the same name to a secondary.</p>

<p>Possible values:</p>

<ul>
<li><code>first-index</code>:  Only use results from the first index that returns a match for a given package name</li>

<li><code>unsafe-first-match</code>:  Search for every package name across all indexes, exhausting the versions from the first index before moving on to the next</li>

<li><code>unsafe-best-match</code>:  Search for every package name across all indexes, preferring the &quot;best&quot; version found. If a package version is in multiple indexes, only look at the entry for the first index</li>
</ul>
</dd><dt><code>--index-url</code>, <code>-i</code> <i>index-url</i></dt><dd><p>The URL of the Python package index (by default: &lt;https://pypi.org/simple&gt;).</p>

<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>

<p>The index given by this flag is given lower priority than all other indexes specified via the <code>--extra-index-url</code> flag.</p>

</dd><dt><code>--keyring-provider</code> <i>keyring-provider</i></dt><dd><p>Attempt to use <code>keyring</code> for authentication for index URLs.</p>

<p>At present, only <code>--keyring-provider subprocess</code> is supported, which configures uv to use the <code>keyring</code> CLI to handle authentication.</p>

<p>Defaults to <code>disabled</code>.</p>

<p>Possible values:</p>

<ul>
<li><code>disabled</code>:  Do not use keyring for credential lookup</li>

<li><code>subprocess</code>:  Use the <code>keyring</code> command for credential lookup</li>
</ul>
</dd><dt><code>--legacy-setup-py</code></dt><dd><p>Use legacy <code>setuptools</code> behavior when building source distributions without a <code>pyproject.toml</code></p>

</dd><dt><code>--link-mode</code> <i>link-mode</i></dt><dd><p>The method to use when installing packages from the global cache.</p>

<p>This option is only used when building source distributions.</p>

<p>Defaults to <code>clone</code> (also known as Copy-on-Write) on macOS, and <code>hardlink</code> on Linux and Windows.</p>

<p>Possible values:</p>

<ul>
<li><code>clone</code>:  Clone (i.e., copy-on-write) packages from the wheel into the <code>site-packages</code> directory</li>

<li><code>copy</code>:  Copy packages from the wheel into the <code>site-packages</code> directory</li>

<li><code>hardlink</code>:  Hard link packages from the wheel into the <code>site-packages</code> directory</li>

<li><code>symlink</code>:  Symbolically link packages from the wheel into the <code>site-packages</code> directory</li>
</ul>
</dd><dt><code>--native-tls</code></dt><dd><p>Whether to load TLS certificates from the platform&#8217;s native certificate store.</p>

<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>

<p>However, in some cases, you may want to use the platform&#8217;s native certificate store, especially if you&#8217;re relying on a corporate trust root (e.g., for a mandatory proxy) that&#8217;s included in your system&#8217;s certificate store.</p>

</dd><dt><code>--no-annotate</code></dt><dd><p>Exclude comment annotations indicating the source of each package</p>

</dd><dt><code>--no-binary</code> <i>no-binary</i></dt><dd><p>Don&#8217;t install pre-built wheels.</p>

<p>The given packages will be built and installed from source. The resolver will still use pre-built wheels to extract package metadata, if available.</p>

<p>Multiple packages may be provided. Disable binaries for all packages with <code>:all:</code>. Clear previously specified packages with <code>:none:</code>.</p>

</dd><dt><code>--no-build</code></dt><dd><p>Don&#8217;t build source distributions.</p>

<p>When enabled, resolving will not run arbitrary Python code. The cached wheels of already-built source distributions will be reused, but operations that require building distributions will exit with an error.</p>

<p>Alias for <code>--only-binary :all:</code>.</p>

</dd><dt><code>--no-build-isolation</code></dt><dd><p>Disable isolation when building source distributions.</p>

<p>Assumes that build dependencies specified by PEP 518 are already installed.</p>

</dd><dt><code>--no-build-isolation-package</code> <i>no-build-isolation-package</i></dt><dd><p>Disable isolation when building source distributions for a specific package.</p>

<p>Assumes that the packages&#8217; build dependencies specified by PEP 518  are already installed.</p>

</dd><dt><code>--no-cache</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>

</dd><dt><code>--no-deps</code></dt><dd><p>Ignore package dependencies, instead only add those packages explicitly listed on the command line to the resulting the requirements file</p>

</dd><dt><code>--no-emit-package</code> <i>no-emit-package</i></dt><dd><p>Specify a package to omit from the output resolution. Its dependencies will still be included in the resolution. Equivalent to pip-compile&#8217;s <code>--unsafe-package</code> option</p>

</dd><dt><code>--no-header</code></dt><dd><p>Exclude the comment header at the top of the generated output file</p>

</dd><dt><code>--no-index</code></dt><dd><p>Ignore the registry index (e.g., PyPI), instead relying on direct URL dependencies and those provided via <code>--find-links</code></p>

</dd><dt><code>--no-progress</code></dt><dd><p>Hide all progress outputs.</p>

<p>For example, spinners or progress bars.</p>

</dd><dt><code>--no-python-downloads</code></dt><dd><p>Disable automatic downloads of Python</p>

</dd><dt><code>--no-sources</code></dt><dd><p>Ignore the <code>tool.uv.sources</code> table when resolving dependencies. Used to lock against the standards-compliant, publishable package metadata, as opposed to using any local or Git sources</p>

</dd><dt><code>--no-strip-extras</code></dt><dd><p>Include extras in the output file.</p>

<p>By default, uv strips extras, as any packages pulled in by the extras are already included as dependencies in the output file directly. Further, output files generated with <code>--no-strip-extras</code> cannot be used as constraints files in <code>install</code> and <code>sync</code> invocations.</p>

</dd><dt><code>--no-strip-markers</code></dt><dd><p>Include environment markers in the output file.</p>

<p>By default, uv strips environment markers, as the resolution generated by <code>compile</code> is only guaranteed to be correct for the target environment.</p>

</dd><dt><code>--offline</code></dt><dd><p>Disable network access.</p>

<p>When disabled, uv will only use locally cached data and locally available files.</p>

</dd><dt><code>--only-binary</code> <i>only-binary</i></dt><dd><p>Only use pre-built wheels; don&#8217;t build source distributions.</p>

<p>When enabled, resolving will not run code from the given packages. The cached wheels of already-built source distributions will be reused, but operations that require building distributions will exit with an error.</p>

<p>Multiple packages may be provided. Disable binaries for all packages with <code>:all:</code>. Clear previously specified packages with <code>:none:</code>.</p>

</dd><dt><code>--output-file</code>, <code>-o</code> <i>output-file</i></dt><dd><p>Write the compiled requirements to the given <code>requirements.txt</code> file.</p>

<p>If the file already exists, the existing versions will be preferred when resolving dependencies, unless <code>--upgrade</code> is also specified.</p>

</dd><dt><code>--override</code> <i>override</i></dt><dd><p>Override versions using the given requirements files.</p>

<p>Overrides files are <code>requirements.txt</code>-like files that force a specific version of a requirement to be installed, regardless of the requirements declared by any constituent package, and regardless of whether this would be considered an invalid resolution.</p>

<p>While constraints are <em>additive</em>, in that they&#8217;re combined with the requirements of the constituent packages, overrides are <em>absolute</em>, in that they completely replace the requirements of the constituent packages.</p>

</dd><dt><code>--prerelease</code> <i>prerelease</i></dt><dd><p>The strategy to use when considering pre-release versions.</p>

<p>By default, uv will accept pre-releases for packages that <em>only</em> publish pre-releases, along with first-party requirements that contain an explicit pre-release marker in the declared specifiers (<code>if-necessary-or-explicit</code>).</p>

<p>Possible values:</p>

<ul>
<li><code>disallow</code>:  Disallow all pre-release versions</li>

<li><code>allow</code>:  Allow all pre-release versions</li>

<li><code>if-necessary</code>:  Allow pre-release versions if all versions of a package are pre-release</li>

<li><code>explicit</code>:  Allow pre-release versions for first-party packages with explicit pre-release markers in their version requirements</li>

<li><code>if-necessary-or-explicit</code>:  Allow pre-release versions if all versions of a package are pre-release, or if the package has an explicit pre-release marker in its version requirements</li>
</ul>
</dd><dt><code>--python</code> <i>python</i></dt><dd><p>The Python interpreter to use during resolution.</p>

<p>A Python interpreter is required for building source distributions to determine package metadata when there are not wheels.</p>

<p>The interpreter is also used to determine the default minimum Python version, unless <code>--python-version</code> is provided.</p>

<p>See <a href="#uv-python">uv python</a> for details on Python discovery and supported request formats.</p>

</dd><dt><code>--python-platform</code> <i>python-platform</i></dt><dd><p>The platform for which requirements should be resolved.</p>

<p>Represented as a &quot;target triple&quot;, a string that describes the target platform in terms of its CPU, vendor, and operating system name, like <code>x86_64-unknown-linux-gnu</code> or <code>aaarch64-apple-darwin</code>.</p>

<p>Possible values:</p>

<ul>
<li><code>windows</code>:  An alias for <code>x86_64-pc-windows-msvc</code>, the default target for Windows</li>

<li><code>linux</code>:  An alias for <code>x86_64-unknown-linux-gnu</code>, the default target for Linux</li>

<li><code>macos</code>:  An alias for <code>aarch64-apple-darwin</code>, the default target for macOS</li>

<li><code>x86_64-pc-windows-msvc</code>:  An x86 Windows target</li>

<li><code>x86_64-unknown-linux-gnu</code>:  An x86 Linux target. Equivalent to <code>x86_64-manylinux_2_17</code></li>

<li><code>aarch64-apple-darwin</code>:  An ARM-based macOS target, as seen on Apple Silicon devices</li>

<li><code>x86_64-apple-darwin</code>:  An x86 macOS target</li>

<li><code>aarch64-unknown-linux-gnu</code>:  An ARM64 Linux target. Equivalent to <code>aarch64-manylinux_2_17</code></li>

<li><code>aarch64-unknown-linux-musl</code>:  An ARM64 Linux target</li>

<li><code>x86_64-unknown-linux-musl</code>:  An <code>x86_64</code> Linux target</li>

<li><code>x86_64-manylinux_2_17</code>:  An <code>x86_64</code> target for the <code>manylinux_2_17</code> platform</li>

<li><code>x86_64-manylinux_2_28</code>:  An <code>x86_64</code> target for the <code>manylinux_2_28</code> platform</li>

<li><code>x86_64-manylinux_2_31</code>:  An <code>x86_64</code> target for the <code>manylinux_2_31</code> platform</li>

<li><code>aarch64-manylinux_2_17</code>:  An ARM64 target for the <code>manylinux_2_17</code> platform</li>

<li><code>aarch64-manylinux_2_28</code>:  An ARM64 target for the <code>manylinux_2_28</code> platform</li>

<li><code>aarch64-manylinux_2_31</code>:  An ARM64 target for the <code>manylinux_2_31</code> platform</li>
</ul>
</dd><dt><code>--python-preference</code> <i>python-preference</i></dt><dd><p>Whether to prefer uv-managed or system Python installations.</p>

<p>By default, uv prefers using Python versions it manages. However, it will use system Python installations if a uv-managed Python is not installed. This option allows prioritizing or ignoring system Python installations.</p>

<p>Possible values:</p>

<ul>
<li><code>only-managed</code>:  Only use managed Python installations; never use system Python installations</li>

<li><code>managed</code>:  Prefer managed Python installations over system Python installations</li>

<li><code>system</code>:  Prefer system Python installations over managed Python installations</li>

<li><code>only-system</code>:  Only use system Python installations; never use managed Python installations</li>
</ul>
</dd><dt><code>--python-version</code>, <code>-p</code> <i>python-version</i></dt><dd><p>The Python version to use for resolution.</p>

<p>For example, <code>3.8</code> or <code>3.8.17</code>.</p>

<p>Defaults to the version of the Python interpreter used for resolution.</p>

<p>Defines the minimum Python version that must be supported by the resolved requirements.</p>

<p>If a patch version is omitted, the minimum patch version is assumed. For example, <code>3.8</code> is mapped to <code>3.8.0</code>.</p>

</dd><dt><code>--quiet</code>, <code>-q</code></dt><dd><p>Do not print any output</p>

</dd><dt><code>--refresh</code></dt><dd><p>Refresh all cached data</p>

</dd><dt><code>--refresh-package</code> <i>refresh-package</i></dt><dd><p>Refresh cached data for a specific package</p>

</dd><dt><code>--resolution</code> <i>resolution</i></dt><dd><p>The strategy to use when selecting between the different compatible versions for a given package requirement.</p>

<p>By default, uv will use the latest compatible version of each package (<code>highest</code>).</p>

<p>Possible values:</p>

<ul>
<li><code>highest</code>:  Resolve the highest compatible version of each package</li>

<li><code>lowest</code>:  Resolve the lowest compatible version of each package</li>

<li><code>lowest-direct</code>:  Resolve the lowest compatible version of any direct dependencies, and the highest compatible version of any transitive dependencies</li>
</ul>
</dd><dt><code>--system</code></dt><dd><p>Install packages into the system Python environment.</p>

<p>By default, uv uses the virtual environment in the current working directory or any parent directory, falling back to searching for a Python executable in <code>PATH</code>. The <code>--system</code> option instructs uv to avoid using a virtual environment Python and restrict its search to the system path.</p>

</dd><dt><code>--universal</code></dt><dd><p>Perform a universal resolution, attempting to generate a single <code>requirements.txt</code> output file that is compatible with all operating systems, architectures, and Python implementations.</p>

<p>In universal mode, the current Python version (or user-provided <code>--python-version</code>) will be treated as a lower bound. For example, <code>--universal --python-version 3.7</code> would produce a universal resolution for Python 3.7 and later.</p>

<p>Implies <code>--no-strip-markers</code>.</p>

</dd><dt><code>--upgrade</code>, <code>-U</code></dt><dd><p>Allow package upgrades, ignoring pinned versions in any existing output file. Implies <code>--refresh</code></p>

</dd><dt><code>--upgrade-package</code>, <code>-P</code> <i>upgrade-package</i></dt><dd><p>Allow upgrades for a specific package, ignoring pinned versions in any existing output file. Implies <code>--refresh-package</code></p>

</dd><dt><code>--verbose</code>, <code>-v</code></dt><dd><p>Use verbose output.</p>

<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (&lt;https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives&gt;)</p>

</dd><dt><code>--version</code>, <code>-V</code></dt><dd><p>Display the uv version</p>

</dd></dl>

### uv pip sync

Sync an environment with a `requirements.txt` file

<h3 class="cli-reference">Usage</h3>

```
uv pip sync [OPTIONS] <SRC_FILE>...
```

<h3 class="cli-reference">Arguments</h3>

<dl class="cli-reference"><dt><code>SRC_FILE</code></dt><dd><p>Include all packages listed in the given <code>requirements.txt</code> files.</p>

<p>If a <code>pyproject.toml</code>, <code>setup.py</code>, or <code>setup.cfg</code> file is provided, uv will extract the requirements for the relevant project.</p>

<p>If <code>-</code> is provided, then requirements will be read from stdin.</p>

</dd></dl>

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt><code>--allow-empty-requirements</code></dt><dd><p>Allow sync of empty requirements, which will clear the environment of all packages</p>

</dd><dt><code>--break-system-packages</code></dt><dd><p>Allow uv to modify an <code>EXTERNALLY-MANAGED</code> Python installation.</p>

<p>WARNING: <code>--break-system-packages</code> is intended for use in continuous integration (CI) environments, when installing into Python installations that are managed by an external package manager, like <code>apt</code>. It should be used with caution, as such Python installations explicitly recommend against modifications by other package managers (like uv or <code>pip</code>).</p>

</dd><dt><code>--build-constraint</code>, <code>-b</code> <i>build-constraint</i></dt><dd><p>Constrain build dependencies using the given requirements files when building source distributions.</p>

<p>Constraints files are <code>requirements.txt</code>-like files that only control the <em>version</em> of a requirement that&#8217;s installed. However, including a package in a constraints file will <em>not</em> trigger the installation of that package.</p>

</dd><dt><code>--cache-dir</code> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>

<p>Defaults to <code>$HOME/Library/Caches/uv</code> on macOS, <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on Linux, and <code>{FOLDERID_LocalAppData}\uv\cache</code> on Windows.</p>

</dd><dt><code>--color</code> <i>color-choice</i></dt><dd><p>Control colors in output</p>

<p>[default: auto]</p>
<p>Possible values:</p>

<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>

<li><code>always</code>:  Enables colored output regardless of the detected environment</li>

<li><code>never</code>:  Disables colored output</li>
</ul>
</dd><dt><code>--compile-bytecode</code></dt><dd><p>Compile Python files to bytecode after installation.</p>

<p>By default, uv does not compile Python (<code>.py</code>) files to bytecode (<code>__pycache__/*.pyc</code>); instead, compilation is performed lazily the first time a module is imported. For use-cases in which start time is critical, such as CLI applications and Docker containers, this option can be enabled to trade longer installation times for faster start times.</p>

<p>When enabled, uv will process the entire site-packages directory (including packages that are not being modified by the current operation) for consistency. Like pip, it will also ignore errors.</p>

</dd><dt><code>--config-file</code> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>

<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>

</dd><dt><code>--config-setting</code>, <code>-C</code> <i>config-setting</i></dt><dd><p>Settings to pass to the PEP 517 build backend, specified as <code>KEY=VALUE</code> pairs</p>

</dd><dt><code>--constraint</code>, <code>-c</code> <i>constraint</i></dt><dd><p>Constrain versions using the given requirements files.</p>

<p>Constraints files are <code>requirements.txt</code>-like files that only control the <em>version</em> of a requirement that&#8217;s installed. However, including a package in a constraints file will <em>not</em> trigger the installation of that package.</p>

<p>This is equivalent to pip&#8217;s <code>--constraint</code> option.</p>

</dd><dt><code>--dry-run</code></dt><dd><p>Perform a dry run, i.e., don&#8217;t actually install anything but resolve the dependencies and print the resulting plan</p>

</dd><dt><code>--exclude-newer</code> <i>exclude-newer</i></dt><dd><p>Limit candidate packages to those that were uploaded prior to the given date.</p>

<p>Accepts both RFC 3339 timestamps (e.g., <code>2006-12-02T02:07:43Z</code>) and UTC dates in the same format (e.g., <code>2006-12-02</code>).</p>

</dd><dt><code>--extra-index-url</code> <i>extra-index-url</i></dt><dd><p>Extra URLs of package indexes to use, in addition to <code>--index-url</code>.</p>

<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>

<p>All indexes provided via this flag take priority over the index specified by <code>--index-url</code> (which defaults to PyPI). When multiple <code>--extra-index-url</code> flags are provided, earlier values take priority.</p>

</dd><dt><code>--find-links</code>, <code>-f</code> <i>find-links</i></dt><dd><p>Locations to search for candidate distributions, in addition to those found in the registry indexes.</p>

<p>If a path, the target must be a directory that contains packages as wheel files (<code>.whl</code>) or source distributions (<code>.tar.gz</code> or <code>.zip</code>) at the top level.</p>

<p>If a URL, the page must contain a flat list of links to package files adhering to the formats described above.</p>

</dd><dt><code>--help</code>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>

</dd><dt><code>--index-strategy</code> <i>index-strategy</i></dt><dd><p>The strategy to use when resolving against multiple index URLs.</p>

<p>By default, uv will stop at the first index on which a given package is available, and limit resolutions to those present on that first index (<code>first-match</code>). This prevents &quot;dependency confusion&quot; attacks, whereby an attack can upload a malicious package under the same name to a secondary.</p>

<p>Possible values:</p>

<ul>
<li><code>first-index</code>:  Only use results from the first index that returns a match for a given package name</li>

<li><code>unsafe-first-match</code>:  Search for every package name across all indexes, exhausting the versions from the first index before moving on to the next</li>

<li><code>unsafe-best-match</code>:  Search for every package name across all indexes, preferring the &quot;best&quot; version found. If a package version is in multiple indexes, only look at the entry for the first index</li>
</ul>
</dd><dt><code>--index-url</code>, <code>-i</code> <i>index-url</i></dt><dd><p>The URL of the Python package index (by default: &lt;https://pypi.org/simple&gt;).</p>

<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>

<p>The index given by this flag is given lower priority than all other indexes specified via the <code>--extra-index-url</code> flag.</p>

</dd><dt><code>--keyring-provider</code> <i>keyring-provider</i></dt><dd><p>Attempt to use <code>keyring</code> for authentication for index URLs.</p>

<p>At present, only <code>--keyring-provider subprocess</code> is supported, which configures uv to use the <code>keyring</code> CLI to handle authentication.</p>

<p>Defaults to <code>disabled</code>.</p>

<p>Possible values:</p>

<ul>
<li><code>disabled</code>:  Do not use keyring for credential lookup</li>

<li><code>subprocess</code>:  Use the <code>keyring</code> command for credential lookup</li>
</ul>
</dd><dt><code>--legacy-setup-py</code></dt><dd><p>Use legacy <code>setuptools</code> behavior when building source distributions without a <code>pyproject.toml</code></p>

</dd><dt><code>--link-mode</code> <i>link-mode</i></dt><dd><p>The method to use when installing packages from the global cache.</p>

<p>Defaults to <code>clone</code> (also known as Copy-on-Write) on macOS, and <code>hardlink</code> on Linux and Windows.</p>

<p>Possible values:</p>

<ul>
<li><code>clone</code>:  Clone (i.e., copy-on-write) packages from the wheel into the <code>site-packages</code> directory</li>

<li><code>copy</code>:  Copy packages from the wheel into the <code>site-packages</code> directory</li>

<li><code>hardlink</code>:  Hard link packages from the wheel into the <code>site-packages</code> directory</li>

<li><code>symlink</code>:  Symbolically link packages from the wheel into the <code>site-packages</code> directory</li>
</ul>
</dd><dt><code>--native-tls</code></dt><dd><p>Whether to load TLS certificates from the platform&#8217;s native certificate store.</p>

<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>

<p>However, in some cases, you may want to use the platform&#8217;s native certificate store, especially if you&#8217;re relying on a corporate trust root (e.g., for a mandatory proxy) that&#8217;s included in your system&#8217;s certificate store.</p>

</dd><dt><code>--no-allow-empty-requirements</code></dt><dt><code>--no-binary</code> <i>no-binary</i></dt><dd><p>Don&#8217;t install pre-built wheels.</p>

<p>The given packages will be built and installed from source. The resolver will still use pre-built wheels to extract package metadata, if available.</p>

<p>Multiple packages may be provided. Disable binaries for all packages with <code>:all:</code>. Clear previously specified packages with <code>:none:</code>.</p>

</dd><dt><code>--no-break-system-packages</code></dt><dt><code>--no-build</code></dt><dd><p>Don&#8217;t build source distributions.</p>

<p>When enabled, resolving will not run arbitrary Python code. The cached wheels of already-built source distributions will be reused, but operations that require building distributions will exit with an error.</p>

<p>Alias for <code>--only-binary :all:</code>.</p>

</dd><dt><code>--no-build-isolation</code></dt><dd><p>Disable isolation when building source distributions.</p>

<p>Assumes that build dependencies specified by PEP 518 are already installed.</p>

</dd><dt><code>--no-cache</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>

</dd><dt><code>--no-index</code></dt><dd><p>Ignore the registry index (e.g., PyPI), instead relying on direct URL dependencies and those provided via <code>--find-links</code></p>

</dd><dt><code>--no-progress</code></dt><dd><p>Hide all progress outputs.</p>

<p>For example, spinners or progress bars.</p>

</dd><dt><code>--no-python-downloads</code></dt><dd><p>Disable automatic downloads of Python</p>

</dd><dt><code>--no-sources</code></dt><dd><p>Ignore the <code>tool.uv.sources</code> table when resolving dependencies. Used to lock against the standards-compliant, publishable package metadata, as opposed to using any local or Git sources</p>

</dd><dt><code>--offline</code></dt><dd><p>Disable network access.</p>

<p>When disabled, uv will only use locally cached data and locally available files.</p>

</dd><dt><code>--only-binary</code> <i>only-binary</i></dt><dd><p>Only use pre-built wheels; don&#8217;t build source distributions.</p>

<p>When enabled, resolving will not run code from the given packages. The cached wheels of already-built source distributions will be reused, but operations that require building distributions will exit with an error.</p>

<p>Multiple packages may be provided. Disable binaries for all packages with <code>:all:</code>. Clear previously specified packages with <code>:none:</code>.</p>

</dd><dt><code>--prefix</code> <i>prefix</i></dt><dd><p>Install packages into <code>lib</code>, <code>bin</code>, and other top-level folders under the specified directory, as if a virtual environment were present at that location.</p>

<p>In general, prefer the use of <code>--python</code> to install into an alternate environment, as scripts and other artifacts installed via <code>--prefix</code> will reference the installing interpreter, rather than any interpreter added to the <code>--prefix</code> directory, rendering them non-portable.</p>

</dd><dt><code>--python</code>, <code>-p</code> <i>python</i></dt><dd><p>The Python interpreter into which packages should be installed.</p>

<p>By default, syncing requires a virtual environment. An path to an alternative Python can be provided, but it is only recommended in continuous integration (CI) environments and should be used with caution, as it can modify the system Python installation.</p>

<p>See <a href="#uv-python">uv python</a> for details on Python discovery and supported request formats.</p>

</dd><dt><code>--python-platform</code> <i>python-platform</i></dt><dd><p>The platform for which requirements should be installed.</p>

<p>Represented as a &quot;target triple&quot;, a string that describes the target platform in terms of its CPU, vendor, and operating system name, like <code>x86_64-unknown-linux-gnu</code> or <code>aaarch64-apple-darwin</code>.</p>

<p>WARNING: When specified, uv will select wheels that are compatible with the <em>target</em> platform; as a result, the installed distributions may not be compatible with the <em>current</em> platform. Conversely, any distributions that are built from source may be incompatible with the <em>target</em> platform, as they will be built for the <em>current</em> platform. The <code>--python-platform</code> option is intended for advanced use cases.</p>

<p>Possible values:</p>

<ul>
<li><code>windows</code>:  An alias for <code>x86_64-pc-windows-msvc</code>, the default target for Windows</li>

<li><code>linux</code>:  An alias for <code>x86_64-unknown-linux-gnu</code>, the default target for Linux</li>

<li><code>macos</code>:  An alias for <code>aarch64-apple-darwin</code>, the default target for macOS</li>

<li><code>x86_64-pc-windows-msvc</code>:  An x86 Windows target</li>

<li><code>x86_64-unknown-linux-gnu</code>:  An x86 Linux target. Equivalent to <code>x86_64-manylinux_2_17</code></li>

<li><code>aarch64-apple-darwin</code>:  An ARM-based macOS target, as seen on Apple Silicon devices</li>

<li><code>x86_64-apple-darwin</code>:  An x86 macOS target</li>

<li><code>aarch64-unknown-linux-gnu</code>:  An ARM64 Linux target. Equivalent to <code>aarch64-manylinux_2_17</code></li>

<li><code>aarch64-unknown-linux-musl</code>:  An ARM64 Linux target</li>

<li><code>x86_64-unknown-linux-musl</code>:  An <code>x86_64</code> Linux target</li>

<li><code>x86_64-manylinux_2_17</code>:  An <code>x86_64</code> target for the <code>manylinux_2_17</code> platform</li>

<li><code>x86_64-manylinux_2_28</code>:  An <code>x86_64</code> target for the <code>manylinux_2_28</code> platform</li>

<li><code>x86_64-manylinux_2_31</code>:  An <code>x86_64</code> target for the <code>manylinux_2_31</code> platform</li>

<li><code>aarch64-manylinux_2_17</code>:  An ARM64 target for the <code>manylinux_2_17</code> platform</li>

<li><code>aarch64-manylinux_2_28</code>:  An ARM64 target for the <code>manylinux_2_28</code> platform</li>

<li><code>aarch64-manylinux_2_31</code>:  An ARM64 target for the <code>manylinux_2_31</code> platform</li>
</ul>
</dd><dt><code>--python-preference</code> <i>python-preference</i></dt><dd><p>Whether to prefer uv-managed or system Python installations.</p>

<p>By default, uv prefers using Python versions it manages. However, it will use system Python installations if a uv-managed Python is not installed. This option allows prioritizing or ignoring system Python installations.</p>

<p>Possible values:</p>

<ul>
<li><code>only-managed</code>:  Only use managed Python installations; never use system Python installations</li>

<li><code>managed</code>:  Prefer managed Python installations over system Python installations</li>

<li><code>system</code>:  Prefer system Python installations over managed Python installations</li>

<li><code>only-system</code>:  Only use system Python installations; never use managed Python installations</li>
</ul>
</dd><dt><code>--python-version</code> <i>python-version</i></dt><dd><p>The minimum Python version that should be supported by the requirements (e.g., <code>3.7</code> or <code>3.7.9</code>).</p>

<p>If a patch version is omitted, the minimum patch version is assumed. For example, <code>3.7</code> is mapped to <code>3.7.0</code>.</p>

</dd><dt><code>--quiet</code>, <code>-q</code></dt><dd><p>Do not print any output</p>

</dd><dt><code>--refresh</code></dt><dd><p>Refresh all cached data</p>

</dd><dt><code>--refresh-package</code> <i>refresh-package</i></dt><dd><p>Refresh cached data for a specific package</p>

</dd><dt><code>--reinstall</code></dt><dd><p>Reinstall all packages, regardless of whether they&#8217;re already installed. Implies <code>--refresh</code></p>

</dd><dt><code>--reinstall-package</code> <i>reinstall-package</i></dt><dd><p>Reinstall a specific package, regardless of whether it&#8217;s already installed. Implies <code>--refresh-package</code></p>

</dd><dt><code>--require-hashes</code></dt><dd><p>Require a matching hash for each requirement.</p>

<p>Hash-checking mode is all or nothing. If enabled, <em>all</em> requirements must be provided with a corresponding hash or set of hashes. Additionally, if enabled, <em>all</em> requirements must either be pinned to exact versions (e.g., <code>==1.0.0</code>), or be specified via direct URL.</p>

<p>Hash-checking mode introduces a number of additional constraints:</p>

<ul>
<li>Git dependencies are not supported. - Editable installs are not supported. - Local dependencies are not supported, unless they point to a specific wheel (<code>.whl</code>) or source archive (<code>.zip</code>, <code>.tar.gz</code>), as opposed to a directory.</li>
</ul>

</dd><dt><code>--strict</code></dt><dd><p>Validate the Python environment after completing the installation, to detect and with missing dependencies or other issues</p>

</dd><dt><code>--system</code></dt><dd><p>Install packages into the system Python environment.</p>

<p>By default, uv installs into the virtual environment in the current working directory or any parent directory. The <code>--system</code> option instructs uv to instead use the first Python found in the system <code>PATH</code>.</p>

<p>WARNING: <code>--system</code> is intended for use in continuous integration (CI) environments and should be used with caution, as it can modify the system Python installation.</p>

</dd><dt><code>--target</code> <i>target</i></dt><dd><p>Install packages into the specified directory, rather than into the virtual or system Python environment. The packages will be installed at the top-level of the directory</p>

</dd><dt><code>--verbose</code>, <code>-v</code></dt><dd><p>Use verbose output.</p>

<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (&lt;https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives&gt;)</p>

</dd><dt><code>--verify-hashes</code></dt><dd><p>Validate any hashes provided in the requirements file.</p>

<p>Unlike <code>--require-hashes</code>, <code>--verify-hashes</code> does not require that all requirements have hashes; instead, it will limit itself to verifying the hashes of those requirements that do include them.</p>

</dd><dt><code>--version</code>, <code>-V</code></dt><dd><p>Display the uv version</p>

</dd></dl>

### uv pip install

Install packages into an environment

<h3 class="cli-reference">Usage</h3>

```
uv pip install [OPTIONS] <PACKAGE|--requirement <REQUIREMENT>|--editable <EDITABLE>>
```

<h3 class="cli-reference">Arguments</h3>

<dl class="cli-reference"><dt><code>PACKAGE</code></dt><dd><p>Install all listed packages</p>

</dd></dl>

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt><code>--all-extras</code></dt><dd><p>Include all optional dependencies.</p>

<p>Only applies to <code>pyproject.toml</code>, <code>setup.py</code>, and <code>setup.cfg</code> sources.</p>

</dd><dt><code>--break-system-packages</code></dt><dd><p>Allow uv to modify an <code>EXTERNALLY-MANAGED</code> Python installation.</p>

<p>WARNING: <code>--break-system-packages</code> is intended for use in continuous integration (CI) environments, when installing into Python installations that are managed by an external package manager, like <code>apt</code>. It should be used with caution, as such Python installations explicitly recommend against modifications by other package managers (like uv or <code>pip</code>).</p>

</dd><dt><code>--build-constraint</code>, <code>-b</code> <i>build-constraint</i></dt><dd><p>Constrain build dependencies using the given requirements files when building source distributions.</p>

<p>Constraints files are <code>requirements.txt</code>-like files that only control the <em>version</em> of a requirement that&#8217;s installed. However, including a package in a constraints file will <em>not</em> trigger the installation of that package.</p>

</dd><dt><code>--cache-dir</code> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>

<p>Defaults to <code>$HOME/Library/Caches/uv</code> on macOS, <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on Linux, and <code>{FOLDERID_LocalAppData}\uv\cache</code> on Windows.</p>

</dd><dt><code>--color</code> <i>color-choice</i></dt><dd><p>Control colors in output</p>

<p>[default: auto]</p>
<p>Possible values:</p>

<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>

<li><code>always</code>:  Enables colored output regardless of the detected environment</li>

<li><code>never</code>:  Disables colored output</li>
</ul>
</dd><dt><code>--compile-bytecode</code></dt><dd><p>Compile Python files to bytecode after installation.</p>

<p>By default, uv does not compile Python (<code>.py</code>) files to bytecode (<code>__pycache__/*.pyc</code>); instead, compilation is performed lazily the first time a module is imported. For use-cases in which start time is critical, such as CLI applications and Docker containers, this option can be enabled to trade longer installation times for faster start times.</p>

<p>When enabled, uv will process the entire site-packages directory (including packages that are not being modified by the current operation) for consistency. Like pip, it will also ignore errors.</p>

</dd><dt><code>--config-file</code> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>

<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>

</dd><dt><code>--config-setting</code>, <code>-C</code> <i>config-setting</i></dt><dd><p>Settings to pass to the PEP 517 build backend, specified as <code>KEY=VALUE</code> pairs</p>

</dd><dt><code>--constraint</code>, <code>-c</code> <i>constraint</i></dt><dd><p>Constrain versions using the given requirements files.</p>

<p>Constraints files are <code>requirements.txt</code>-like files that only control the <em>version</em> of a requirement that&#8217;s installed. However, including a package in a constraints file will <em>not</em> trigger the installation of that package.</p>

<p>This is equivalent to pip&#8217;s <code>--constraint</code> option.</p>

</dd><dt><code>--dry-run</code></dt><dd><p>Perform a dry run, i.e., don&#8217;t actually install anything but resolve the dependencies and print the resulting plan</p>

</dd><dt><code>--editable</code>, <code>-e</code> <i>editable</i></dt><dd><p>Install the editable package based on the provided local file path</p>

</dd><dt><code>--exclude-newer</code> <i>exclude-newer</i></dt><dd><p>Limit candidate packages to those that were uploaded prior to the given date.</p>

<p>Accepts both RFC 3339 timestamps (e.g., <code>2006-12-02T02:07:43Z</code>) and UTC dates in the same format (e.g., <code>2006-12-02</code>).</p>

</dd><dt><code>--extra</code> <i>extra</i></dt><dd><p>Include optional dependencies from the extra group name; may be provided more than once.</p>

<p>Only applies to <code>pyproject.toml</code>, <code>setup.py</code>, and <code>setup.cfg</code> sources.</p>

</dd><dt><code>--extra-index-url</code> <i>extra-index-url</i></dt><dd><p>Extra URLs of package indexes to use, in addition to <code>--index-url</code>.</p>

<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>

<p>All indexes provided via this flag take priority over the index specified by <code>--index-url</code> (which defaults to PyPI). When multiple <code>--extra-index-url</code> flags are provided, earlier values take priority.</p>

</dd><dt><code>--find-links</code>, <code>-f</code> <i>find-links</i></dt><dd><p>Locations to search for candidate distributions, in addition to those found in the registry indexes.</p>

<p>If a path, the target must be a directory that contains packages as wheel files (<code>.whl</code>) or source distributions (<code>.tar.gz</code> or <code>.zip</code>) at the top level.</p>

<p>If a URL, the page must contain a flat list of links to package files adhering to the formats described above.</p>

</dd><dt><code>--help</code>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>

</dd><dt><code>--index-strategy</code> <i>index-strategy</i></dt><dd><p>The strategy to use when resolving against multiple index URLs.</p>

<p>By default, uv will stop at the first index on which a given package is available, and limit resolutions to those present on that first index (<code>first-match</code>). This prevents &quot;dependency confusion&quot; attacks, whereby an attack can upload a malicious package under the same name to a secondary.</p>

<p>Possible values:</p>

<ul>
<li><code>first-index</code>:  Only use results from the first index that returns a match for a given package name</li>

<li><code>unsafe-first-match</code>:  Search for every package name across all indexes, exhausting the versions from the first index before moving on to the next</li>

<li><code>unsafe-best-match</code>:  Search for every package name across all indexes, preferring the &quot;best&quot; version found. If a package version is in multiple indexes, only look at the entry for the first index</li>
</ul>
</dd><dt><code>--index-url</code>, <code>-i</code> <i>index-url</i></dt><dd><p>The URL of the Python package index (by default: &lt;https://pypi.org/simple&gt;).</p>

<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>

<p>The index given by this flag is given lower priority than all other indexes specified via the <code>--extra-index-url</code> flag.</p>

</dd><dt><code>--keyring-provider</code> <i>keyring-provider</i></dt><dd><p>Attempt to use <code>keyring</code> for authentication for index URLs.</p>

<p>At present, only <code>--keyring-provider subprocess</code> is supported, which configures uv to use the <code>keyring</code> CLI to handle authentication.</p>

<p>Defaults to <code>disabled</code>.</p>

<p>Possible values:</p>

<ul>
<li><code>disabled</code>:  Do not use keyring for credential lookup</li>

<li><code>subprocess</code>:  Use the <code>keyring</code> command for credential lookup</li>
</ul>
</dd><dt><code>--legacy-setup-py</code></dt><dd><p>Use legacy <code>setuptools</code> behavior when building source distributions without a <code>pyproject.toml</code></p>

</dd><dt><code>--link-mode</code> <i>link-mode</i></dt><dd><p>The method to use when installing packages from the global cache.</p>

<p>Defaults to <code>clone</code> (also known as Copy-on-Write) on macOS, and <code>hardlink</code> on Linux and Windows.</p>

<p>Possible values:</p>

<ul>
<li><code>clone</code>:  Clone (i.e., copy-on-write) packages from the wheel into the <code>site-packages</code> directory</li>

<li><code>copy</code>:  Copy packages from the wheel into the <code>site-packages</code> directory</li>

<li><code>hardlink</code>:  Hard link packages from the wheel into the <code>site-packages</code> directory</li>

<li><code>symlink</code>:  Symbolically link packages from the wheel into the <code>site-packages</code> directory</li>
</ul>
</dd><dt><code>--native-tls</code></dt><dd><p>Whether to load TLS certificates from the platform&#8217;s native certificate store.</p>

<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>

<p>However, in some cases, you may want to use the platform&#8217;s native certificate store, especially if you&#8217;re relying on a corporate trust root (e.g., for a mandatory proxy) that&#8217;s included in your system&#8217;s certificate store.</p>

</dd><dt><code>--no-binary</code> <i>no-binary</i></dt><dd><p>Don&#8217;t install pre-built wheels.</p>

<p>The given packages will be built and installed from source. The resolver will still use pre-built wheels to extract package metadata, if available.</p>

<p>Multiple packages may be provided. Disable binaries for all packages with <code>:all:</code>. Clear previously specified packages with <code>:none:</code>.</p>

</dd><dt><code>--no-break-system-packages</code></dt><dt><code>--no-build</code></dt><dd><p>Don&#8217;t build source distributions.</p>

<p>When enabled, resolving will not run arbitrary Python code. The cached wheels of already-built source distributions will be reused, but operations that require building distributions will exit with an error.</p>

<p>Alias for <code>--only-binary :all:</code>.</p>

</dd><dt><code>--no-build-isolation</code></dt><dd><p>Disable isolation when building source distributions.</p>

<p>Assumes that build dependencies specified by PEP 518 are already installed.</p>

</dd><dt><code>--no-build-isolation-package</code> <i>no-build-isolation-package</i></dt><dd><p>Disable isolation when building source distributions for a specific package.</p>

<p>Assumes that the packages&#8217; build dependencies specified by PEP 518  are already installed.</p>

</dd><dt><code>--no-cache</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>

</dd><dt><code>--no-config</code></dt><dd><p>Avoid discovering configuration files (<code>pyproject.toml</code>, <code>uv.toml</code>).</p>

<p>Normally, configuration files are discovered in the current directory, parent directories, or user configuration directories.</p>

</dd><dt><code>--no-deps</code></dt><dd><p>Ignore package dependencies, instead only installing those packages explicitly listed on the command line or in the requirements files</p>

</dd><dt><code>--no-index</code></dt><dd><p>Ignore the registry index (e.g., PyPI), instead relying on direct URL dependencies and those provided via <code>--find-links</code></p>

</dd><dt><code>--no-progress</code></dt><dd><p>Hide all progress outputs.</p>

<p>For example, spinners or progress bars.</p>

</dd><dt><code>--no-python-downloads</code></dt><dd><p>Disable automatic downloads of Python</p>

</dd><dt><code>--no-sources</code></dt><dd><p>Ignore the <code>tool.uv.sources</code> table when resolving dependencies. Used to lock against the standards-compliant, publishable package metadata, as opposed to using any local or Git sources</p>

</dd><dt><code>--offline</code></dt><dd><p>Disable network access.</p>

<p>When disabled, uv will only use locally cached data and locally available files.</p>

</dd><dt><code>--only-binary</code> <i>only-binary</i></dt><dd><p>Only use pre-built wheels; don&#8217;t build source distributions.</p>

<p>When enabled, resolving will not run code from the given packages. The cached wheels of already-built source distributions will be reused, but operations that require building distributions will exit with an error.</p>

<p>Multiple packages may be provided. Disable binaries for all packages with <code>:all:</code>. Clear previously specified packages with <code>:none:</code>.</p>

</dd><dt><code>--override</code> <i>override</i></dt><dd><p>Override versions using the given requirements files.</p>

<p>Overrides files are <code>requirements.txt</code>-like files that force a specific version of a requirement to be installed, regardless of the requirements declared by any constituent package, and regardless of whether this would be considered an invalid resolution.</p>

<p>While constraints are <em>additive</em>, in that they&#8217;re combined with the requirements of the constituent packages, overrides are <em>absolute</em>, in that they completely replace the requirements of the constituent packages.</p>

</dd><dt><code>--prefix</code> <i>prefix</i></dt><dd><p>Install packages into <code>lib</code>, <code>bin</code>, and other top-level folders under the specified directory, as if a virtual environment were present at that location.</p>

<p>In general, prefer the use of <code>--python</code> to install into an alternate environment, as scripts and other artifacts installed via <code>--prefix</code> will reference the installing interpreter, rather than any interpreter added to the <code>--prefix</code> directory, rendering them non-portable.</p>

</dd><dt><code>--prerelease</code> <i>prerelease</i></dt><dd><p>The strategy to use when considering pre-release versions.</p>

<p>By default, uv will accept pre-releases for packages that <em>only</em> publish pre-releases, along with first-party requirements that contain an explicit pre-release marker in the declared specifiers (<code>if-necessary-or-explicit</code>).</p>

<p>Possible values:</p>

<ul>
<li><code>disallow</code>:  Disallow all pre-release versions</li>

<li><code>allow</code>:  Allow all pre-release versions</li>

<li><code>if-necessary</code>:  Allow pre-release versions if all versions of a package are pre-release</li>

<li><code>explicit</code>:  Allow pre-release versions for first-party packages with explicit pre-release markers in their version requirements</li>

<li><code>if-necessary-or-explicit</code>:  Allow pre-release versions if all versions of a package are pre-release, or if the package has an explicit pre-release marker in its version requirements</li>
</ul>
</dd><dt><code>--python</code>, <code>-p</code> <i>python</i></dt><dd><p>The Python interpreter into which packages should be installed.</p>

<p>By default, installation requires a virtual environment. An path to an alternative Python can be provided, but it is only recommended in continuous integration (CI) environments and should be used with caution, as it can modify the system Python installation.</p>

<p>See <a href="#uv-python">uv python</a> for details on Python discovery and supported request formats.</p>

</dd><dt><code>--python-platform</code> <i>python-platform</i></dt><dd><p>The platform for which requirements should be installed.</p>

<p>Represented as a &quot;target triple&quot;, a string that describes the target platform in terms of its CPU, vendor, and operating system name, like <code>x86_64-unknown-linux-gnu</code> or <code>aaarch64-apple-darwin</code>.</p>

<p>WARNING: When specified, uv will select wheels that are compatible with the <em>target</em> platform; as a result, the installed distributions may not be compatible with the <em>current</em> platform. Conversely, any distributions that are built from source may be incompatible with the <em>target</em> platform, as they will be built for the <em>current</em> platform. The <code>--python-platform</code> option is intended for advanced use cases.</p>

<p>Possible values:</p>

<ul>
<li><code>windows</code>:  An alias for <code>x86_64-pc-windows-msvc</code>, the default target for Windows</li>

<li><code>linux</code>:  An alias for <code>x86_64-unknown-linux-gnu</code>, the default target for Linux</li>

<li><code>macos</code>:  An alias for <code>aarch64-apple-darwin</code>, the default target for macOS</li>

<li><code>x86_64-pc-windows-msvc</code>:  An x86 Windows target</li>

<li><code>x86_64-unknown-linux-gnu</code>:  An x86 Linux target. Equivalent to <code>x86_64-manylinux_2_17</code></li>

<li><code>aarch64-apple-darwin</code>:  An ARM-based macOS target, as seen on Apple Silicon devices</li>

<li><code>x86_64-apple-darwin</code>:  An x86 macOS target</li>

<li><code>aarch64-unknown-linux-gnu</code>:  An ARM64 Linux target. Equivalent to <code>aarch64-manylinux_2_17</code></li>

<li><code>aarch64-unknown-linux-musl</code>:  An ARM64 Linux target</li>

<li><code>x86_64-unknown-linux-musl</code>:  An <code>x86_64</code> Linux target</li>

<li><code>x86_64-manylinux_2_17</code>:  An <code>x86_64</code> target for the <code>manylinux_2_17</code> platform</li>

<li><code>x86_64-manylinux_2_28</code>:  An <code>x86_64</code> target for the <code>manylinux_2_28</code> platform</li>

<li><code>x86_64-manylinux_2_31</code>:  An <code>x86_64</code> target for the <code>manylinux_2_31</code> platform</li>

<li><code>aarch64-manylinux_2_17</code>:  An ARM64 target for the <code>manylinux_2_17</code> platform</li>

<li><code>aarch64-manylinux_2_28</code>:  An ARM64 target for the <code>manylinux_2_28</code> platform</li>

<li><code>aarch64-manylinux_2_31</code>:  An ARM64 target for the <code>manylinux_2_31</code> platform</li>
</ul>
</dd><dt><code>--python-preference</code> <i>python-preference</i></dt><dd><p>Whether to prefer uv-managed or system Python installations.</p>

<p>By default, uv prefers using Python versions it manages. However, it will use system Python installations if a uv-managed Python is not installed. This option allows prioritizing or ignoring system Python installations.</p>

<p>Possible values:</p>

<ul>
<li><code>only-managed</code>:  Only use managed Python installations; never use system Python installations</li>

<li><code>managed</code>:  Prefer managed Python installations over system Python installations</li>

<li><code>system</code>:  Prefer system Python installations over managed Python installations</li>

<li><code>only-system</code>:  Only use system Python installations; never use managed Python installations</li>
</ul>
</dd><dt><code>--python-version</code> <i>python-version</i></dt><dd><p>The minimum Python version that should be supported by the requirements (e.g., <code>3.7</code> or <code>3.7.9</code>).</p>

<p>If a patch version is omitted, the minimum patch version is assumed. For example, <code>3.7</code> is mapped to <code>3.7.0</code>.</p>

</dd><dt><code>--quiet</code>, <code>-q</code></dt><dd><p>Do not print any output</p>

</dd><dt><code>--refresh</code></dt><dd><p>Refresh all cached data</p>

</dd><dt><code>--refresh-package</code> <i>refresh-package</i></dt><dd><p>Refresh cached data for a specific package</p>

</dd><dt><code>--reinstall</code></dt><dd><p>Reinstall all packages, regardless of whether they&#8217;re already installed. Implies <code>--refresh</code></p>

</dd><dt><code>--reinstall-package</code> <i>reinstall-package</i></dt><dd><p>Reinstall a specific package, regardless of whether it&#8217;s already installed. Implies <code>--refresh-package</code></p>

</dd><dt><code>--require-hashes</code></dt><dd><p>Require a matching hash for each requirement.</p>

<p>Hash-checking mode is all or nothing. If enabled, <em>all</em> requirements must be provided with a corresponding hash or set of hashes. Additionally, if enabled, <em>all</em> requirements must either be pinned to exact versions (e.g., <code>==1.0.0</code>), or be specified via direct URL.</p>

<p>Hash-checking mode introduces a number of additional constraints:</p>

<ul>
<li>Git dependencies are not supported. - Editable installs are not supported. - Local dependencies are not supported, unless they point to a specific wheel (<code>.whl</code>) or source archive (<code>.zip</code>, <code>.tar.gz</code>), as opposed to a directory.</li>
</ul>

</dd><dt><code>--requirement</code>, <code>-r</code> <i>requirement</i></dt><dd><p>Install all packages listed in the given <code>requirements.txt</code> files.</p>

<p>If a <code>pyproject.toml</code>, <code>setup.py</code>, or <code>setup.cfg</code> file is provided, uv will extract the requirements for the relevant project.</p>

<p>If <code>-</code> is provided, then requirements will be read from stdin.</p>

</dd><dt><code>--resolution</code> <i>resolution</i></dt><dd><p>The strategy to use when selecting between the different compatible versions for a given package requirement.</p>

<p>By default, uv will use the latest compatible version of each package (<code>highest</code>).</p>

<p>Possible values:</p>

<ul>
<li><code>highest</code>:  Resolve the highest compatible version of each package</li>

<li><code>lowest</code>:  Resolve the lowest compatible version of each package</li>

<li><code>lowest-direct</code>:  Resolve the lowest compatible version of any direct dependencies, and the highest compatible version of any transitive dependencies</li>
</ul>
</dd><dt><code>--strict</code></dt><dd><p>Validate the Python environment after completing the installation, to detect and with missing dependencies or other issues</p>

</dd><dt><code>--system</code></dt><dd><p>Install packages into the system Python environment.</p>

<p>By default, uv installs into the virtual environment in the current working directory or any parent directory. The <code>--system</code> option instructs uv to instead use the first Python found in the system <code>PATH</code>.</p>

<p>WARNING: <code>--system</code> is intended for use in continuous integration (CI) environments and should be used with caution, as it can modify the system Python installation.</p>

</dd><dt><code>--target</code> <i>target</i></dt><dd><p>Install packages into the specified directory, rather than into the virtual or system Python environment. The packages will be installed at the top-level of the directory</p>

</dd><dt><code>--upgrade</code>, <code>-U</code></dt><dd><p>Allow package upgrades, ignoring pinned versions in any existing output file. Implies <code>--refresh</code></p>

</dd><dt><code>--upgrade-package</code>, <code>-P</code> <i>upgrade-package</i></dt><dd><p>Allow upgrades for a specific package, ignoring pinned versions in any existing output file. Implies <code>--refresh-package</code></p>

</dd><dt><code>--user</code></dt><dt><code>--verbose</code>, <code>-v</code></dt><dd><p>Use verbose output.</p>

<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (&lt;https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives&gt;)</p>

</dd><dt><code>--verify-hashes</code></dt><dd><p>Validate any hashes provided in the requirements file.</p>

<p>Unlike <code>--require-hashes</code>, <code>--verify-hashes</code> does not require that all requirements have hashes; instead, it will limit itself to verifying the hashes of those requirements that do include them.</p>

</dd><dt><code>--version</code>, <code>-V</code></dt><dd><p>Display the uv version</p>

</dd></dl>

### uv pip uninstall

Uninstall packages from an environment

<h3 class="cli-reference">Usage</h3>

```
uv pip uninstall [OPTIONS] <PACKAGE|--requirement <REQUIREMENT>>
```

<h3 class="cli-reference">Arguments</h3>

<dl class="cli-reference"><dt><code>PACKAGE</code></dt><dd><p>Uninstall all listed packages</p>

</dd></dl>

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt><code>--break-system-packages</code></dt><dd><p>Allow uv to modify an <code>EXTERNALLY-MANAGED</code> Python installation.</p>

<p>WARNING: <code>--break-system-packages</code> is intended for use in continuous integration (CI) environments, when installing into Python installations that are managed by an external package manager, like <code>apt</code>. It should be used with caution, as such Python installations explicitly recommend against modifications by other package managers (like uv or <code>pip</code>).</p>

</dd><dt><code>--cache-dir</code> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>

<p>Defaults to <code>$HOME/Library/Caches/uv</code> on macOS, <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on Linux, and <code>{FOLDERID_LocalAppData}\uv\cache</code> on Windows.</p>

</dd><dt><code>--color</code> <i>color-choice</i></dt><dd><p>Control colors in output</p>

<p>[default: auto]</p>
<p>Possible values:</p>

<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>

<li><code>always</code>:  Enables colored output regardless of the detected environment</li>

<li><code>never</code>:  Disables colored output</li>
</ul>
</dd><dt><code>--config-file</code> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>

<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>

</dd><dt><code>--help</code>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>

</dd><dt><code>--keyring-provider</code> <i>keyring-provider</i></dt><dd><p>Attempt to use <code>keyring</code> for authentication for remote requirements files.</p>

<p>At present, only <code>--keyring-provider subprocess</code> is supported, which configures uv to use the <code>keyring</code> CLI to handle authentication.</p>

<p>Defaults to <code>disabled</code>.</p>

<p>Possible values:</p>

<ul>
<li><code>disabled</code>:  Do not use keyring for credential lookup</li>

<li><code>subprocess</code>:  Use the <code>keyring</code> command for credential lookup</li>
</ul>
</dd><dt><code>--native-tls</code></dt><dd><p>Whether to load TLS certificates from the platform&#8217;s native certificate store.</p>

<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>

<p>However, in some cases, you may want to use the platform&#8217;s native certificate store, especially if you&#8217;re relying on a corporate trust root (e.g., for a mandatory proxy) that&#8217;s included in your system&#8217;s certificate store.</p>

</dd><dt><code>--no-break-system-packages</code></dt><dt><code>--no-cache</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>

</dd><dt><code>--no-config</code></dt><dd><p>Avoid discovering configuration files (<code>pyproject.toml</code>, <code>uv.toml</code>).</p>

<p>Normally, configuration files are discovered in the current directory, parent directories, or user configuration directories.</p>

</dd><dt><code>--no-progress</code></dt><dd><p>Hide all progress outputs.</p>

<p>For example, spinners or progress bars.</p>

</dd><dt><code>--no-python-downloads</code></dt><dd><p>Disable automatic downloads of Python</p>

</dd><dt><code>--offline</code></dt><dd><p>Disable network access.</p>

<p>When disabled, uv will only use locally cached data and locally available files.</p>

</dd><dt><code>--prefix</code> <i>prefix</i></dt><dd><p>Uninstall packages from the specified <code>--prefix</code> directory</p>

</dd><dt><code>--python</code>, <code>-p</code> <i>python</i></dt><dd><p>The Python interpreter from which packages should be uninstalled.</p>

<p>By default, uninstallation requires a virtual environment. An path to an alternative Python can be provided, but it is only recommended in continuous integration (CI) environments and should be used with caution, as it can modify the system Python installation.</p>

<p>See <a href="#uv-python">uv python</a> for details on Python discovery and supported request formats.</p>

</dd><dt><code>--python-preference</code> <i>python-preference</i></dt><dd><p>Whether to prefer uv-managed or system Python installations.</p>

<p>By default, uv prefers using Python versions it manages. However, it will use system Python installations if a uv-managed Python is not installed. This option allows prioritizing or ignoring system Python installations.</p>

<p>Possible values:</p>

<ul>
<li><code>only-managed</code>:  Only use managed Python installations; never use system Python installations</li>

<li><code>managed</code>:  Prefer managed Python installations over system Python installations</li>

<li><code>system</code>:  Prefer system Python installations over managed Python installations</li>

<li><code>only-system</code>:  Only use system Python installations; never use managed Python installations</li>
</ul>
</dd><dt><code>--quiet</code>, <code>-q</code></dt><dd><p>Do not print any output</p>

</dd><dt><code>--requirement</code>, <code>-r</code> <i>requirement</i></dt><dd><p>Uninstall all packages listed in the given requirements files</p>

</dd><dt><code>--system</code></dt><dd><p>Use the system Python to uninstall packages.</p>

<p>By default, uv uninstalls from the virtual environment in the current working directory or any parent directory. The <code>--system</code> option instructs uv to instead use the first Python found in the system <code>PATH</code>.</p>

<p>WARNING: <code>--system</code> is intended for use in continuous integration (CI) environments and should be used with caution, as it can modify the system Python installation.</p>

</dd><dt><code>--target</code> <i>target</i></dt><dd><p>Uninstall packages from the specified <code>--target</code> directory</p>

</dd><dt><code>--verbose</code>, <code>-v</code></dt><dd><p>Use verbose output.</p>

<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (&lt;https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives&gt;)</p>

</dd><dt><code>--version</code>, <code>-V</code></dt><dd><p>Display the uv version</p>

</dd></dl>

### uv pip freeze

List, in requirements format, packages installed in an environment

<h3 class="cli-reference">Usage</h3>

```
uv pip freeze [OPTIONS]
```

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt><code>--cache-dir</code> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>

<p>Defaults to <code>$HOME/Library/Caches/uv</code> on macOS, <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on Linux, and <code>{FOLDERID_LocalAppData}\uv\cache</code> on Windows.</p>

</dd><dt><code>--color</code> <i>color-choice</i></dt><dd><p>Control colors in output</p>

<p>[default: auto]</p>
<p>Possible values:</p>

<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>

<li><code>always</code>:  Enables colored output regardless of the detected environment</li>

<li><code>never</code>:  Disables colored output</li>
</ul>
</dd><dt><code>--config-file</code> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>

<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>

</dd><dt><code>--exclude-editable</code></dt><dd><p>Exclude any editable packages from output</p>

</dd><dt><code>--help</code>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>

</dd><dt><code>--native-tls</code></dt><dd><p>Whether to load TLS certificates from the platform&#8217;s native certificate store.</p>

<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>

<p>However, in some cases, you may want to use the platform&#8217;s native certificate store, especially if you&#8217;re relying on a corporate trust root (e.g., for a mandatory proxy) that&#8217;s included in your system&#8217;s certificate store.</p>

</dd><dt><code>--no-cache</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>

</dd><dt><code>--no-config</code></dt><dd><p>Avoid discovering configuration files (<code>pyproject.toml</code>, <code>uv.toml</code>).</p>

<p>Normally, configuration files are discovered in the current directory, parent directories, or user configuration directories.</p>

</dd><dt><code>--no-progress</code></dt><dd><p>Hide all progress outputs.</p>

<p>For example, spinners or progress bars.</p>

</dd><dt><code>--no-python-downloads</code></dt><dd><p>Disable automatic downloads of Python</p>

</dd><dt><code>--offline</code></dt><dd><p>Disable network access.</p>

<p>When disabled, uv will only use locally cached data and locally available files.</p>

</dd><dt><code>--python</code>, <code>-p</code> <i>python</i></dt><dd><p>The Python interpreter for which packages should be listed.</p>

<p>By default, uv lists packages in a virtual environment but will show packages in a system Python environment if no virtual environment is found.</p>

<p>See <a href="#uv-python">uv python</a> for details on Python discovery and supported request formats.</p>

</dd><dt><code>--python-preference</code> <i>python-preference</i></dt><dd><p>Whether to prefer uv-managed or system Python installations.</p>

<p>By default, uv prefers using Python versions it manages. However, it will use system Python installations if a uv-managed Python is not installed. This option allows prioritizing or ignoring system Python installations.</p>

<p>Possible values:</p>

<ul>
<li><code>only-managed</code>:  Only use managed Python installations; never use system Python installations</li>

<li><code>managed</code>:  Prefer managed Python installations over system Python installations</li>

<li><code>system</code>:  Prefer system Python installations over managed Python installations</li>

<li><code>only-system</code>:  Only use system Python installations; never use managed Python installations</li>
</ul>
</dd><dt><code>--quiet</code>, <code>-q</code></dt><dd><p>Do not print any output</p>

</dd><dt><code>--strict</code></dt><dd><p>Validate the Python environment, to detect packages with missing dependencies and other issues</p>

</dd><dt><code>--system</code></dt><dd><p>List packages in the system Python environment.</p>

<p>Disables discovery of virtual environments.</p>

<p>See <a href="#uv-python">uv python</a> for details on Python discovery.</p>

</dd><dt><code>--verbose</code>, <code>-v</code></dt><dd><p>Use verbose output.</p>

<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (&lt;https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives&gt;)</p>

</dd><dt><code>--version</code>, <code>-V</code></dt><dd><p>Display the uv version</p>

</dd></dl>

### uv pip list

List, in tabular format, packages installed in an environment

<h3 class="cli-reference">Usage</h3>

```
uv pip list [OPTIONS]
```

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt><code>--cache-dir</code> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>

<p>Defaults to <code>$HOME/Library/Caches/uv</code> on macOS, <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on Linux, and <code>{FOLDERID_LocalAppData}\uv\cache</code> on Windows.</p>

</dd><dt><code>--color</code> <i>color-choice</i></dt><dd><p>Control colors in output</p>

<p>[default: auto]</p>
<p>Possible values:</p>

<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>

<li><code>always</code>:  Enables colored output regardless of the detected environment</li>

<li><code>never</code>:  Disables colored output</li>
</ul>
</dd><dt><code>--config-file</code> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>

<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>

</dd><dt><code>--editable</code>, <code>-e</code></dt><dd><p>Only include editable projects</p>

</dd><dt><code>--exclude</code> <i>exclude</i></dt><dd><p>Exclude the specified package(s) from the output</p>

</dd><dt><code>--exclude-editable</code></dt><dd><p>Exclude any editable packages from output</p>

</dd><dt><code>--format</code> <i>format</i></dt><dd><p>Select the output format between: <code>columns</code> (default), <code>freeze</code>, or <code>json</code></p>

<p>[default: columns]</p>
<p>Possible values:</p>

<ul>
<li><code>columns</code>:  Display the list of packages in a human-readable table</li>

<li><code>freeze</code>:  Display the list of packages in a <code>pip freeze</code>-like format, with one package per line alongside its version</li>

<li><code>json</code>:  Display the list of packages in a machine-readable JSON format</li>
</ul>
</dd><dt><code>--help</code>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>

</dd><dt><code>--native-tls</code></dt><dd><p>Whether to load TLS certificates from the platform&#8217;s native certificate store.</p>

<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>

<p>However, in some cases, you may want to use the platform&#8217;s native certificate store, especially if you&#8217;re relying on a corporate trust root (e.g., for a mandatory proxy) that&#8217;s included in your system&#8217;s certificate store.</p>

</dd><dt><code>--no-cache</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>

</dd><dt><code>--no-config</code></dt><dd><p>Avoid discovering configuration files (<code>pyproject.toml</code>, <code>uv.toml</code>).</p>

<p>Normally, configuration files are discovered in the current directory, parent directories, or user configuration directories.</p>

</dd><dt><code>--no-progress</code></dt><dd><p>Hide all progress outputs.</p>

<p>For example, spinners or progress bars.</p>

</dd><dt><code>--no-python-downloads</code></dt><dd><p>Disable automatic downloads of Python</p>

</dd><dt><code>--offline</code></dt><dd><p>Disable network access.</p>

<p>When disabled, uv will only use locally cached data and locally available files.</p>

</dd><dt><code>--python</code>, <code>-p</code> <i>python</i></dt><dd><p>The Python interpreter for which packages should be listed.</p>

<p>By default, uv lists packages in a virtual environment but will show packages in a system Python environment if no virtual environment is found.</p>

<p>See <a href="#uv-python">uv python</a> for details on Python discovery and supported request formats.</p>

</dd><dt><code>--python-preference</code> <i>python-preference</i></dt><dd><p>Whether to prefer uv-managed or system Python installations.</p>

<p>By default, uv prefers using Python versions it manages. However, it will use system Python installations if a uv-managed Python is not installed. This option allows prioritizing or ignoring system Python installations.</p>

<p>Possible values:</p>

<ul>
<li><code>only-managed</code>:  Only use managed Python installations; never use system Python installations</li>

<li><code>managed</code>:  Prefer managed Python installations over system Python installations</li>

<li><code>system</code>:  Prefer system Python installations over managed Python installations</li>

<li><code>only-system</code>:  Only use system Python installations; never use managed Python installations</li>
</ul>
</dd><dt><code>--quiet</code>, <code>-q</code></dt><dd><p>Do not print any output</p>

</dd><dt><code>--strict</code></dt><dd><p>Validate the Python environment, to detect packages with missing dependencies and other issues</p>

</dd><dt><code>--system</code></dt><dd><p>List packages in the system Python environment.</p>

<p>Disables discovery of virtual environments.</p>

<p>See <a href="#uv-python">uv python</a> for details on Python discovery.</p>

</dd><dt><code>--verbose</code>, <code>-v</code></dt><dd><p>Use verbose output.</p>

<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (&lt;https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives&gt;)</p>

</dd><dt><code>--version</code>, <code>-V</code></dt><dd><p>Display the uv version</p>

</dd></dl>

### uv pip show

Show information about one or more installed packages

<h3 class="cli-reference">Usage</h3>

```
uv pip show [OPTIONS] [PACKAGE]...
```

<h3 class="cli-reference">Arguments</h3>

<dl class="cli-reference"><dt><code>PACKAGE</code></dt><dd><p>The package(s) to display</p>

</dd></dl>

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt><code>--cache-dir</code> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>

<p>Defaults to <code>$HOME/Library/Caches/uv</code> on macOS, <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on Linux, and <code>{FOLDERID_LocalAppData}\uv\cache</code> on Windows.</p>

</dd><dt><code>--color</code> <i>color-choice</i></dt><dd><p>Control colors in output</p>

<p>[default: auto]</p>
<p>Possible values:</p>

<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>

<li><code>always</code>:  Enables colored output regardless of the detected environment</li>

<li><code>never</code>:  Disables colored output</li>
</ul>
</dd><dt><code>--config-file</code> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>

<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>

</dd><dt><code>--help</code>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>

</dd><dt><code>--native-tls</code></dt><dd><p>Whether to load TLS certificates from the platform&#8217;s native certificate store.</p>

<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>

<p>However, in some cases, you may want to use the platform&#8217;s native certificate store, especially if you&#8217;re relying on a corporate trust root (e.g., for a mandatory proxy) that&#8217;s included in your system&#8217;s certificate store.</p>

</dd><dt><code>--no-cache</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>

</dd><dt><code>--no-config</code></dt><dd><p>Avoid discovering configuration files (<code>pyproject.toml</code>, <code>uv.toml</code>).</p>

<p>Normally, configuration files are discovered in the current directory, parent directories, or user configuration directories.</p>

</dd><dt><code>--no-progress</code></dt><dd><p>Hide all progress outputs.</p>

<p>For example, spinners or progress bars.</p>

</dd><dt><code>--no-python-downloads</code></dt><dd><p>Disable automatic downloads of Python</p>

</dd><dt><code>--offline</code></dt><dd><p>Disable network access.</p>

<p>When disabled, uv will only use locally cached data and locally available files.</p>

</dd><dt><code>--python</code>, <code>-p</code> <i>python</i></dt><dd><p>The Python interpreter to find the package in.</p>

<p>By default, uv looks for packages in a virtual environment but will look for packages in a system Python environment if no virtual environment is found.</p>

<p>See <a href="#uv-python">uv python</a> for details on Python discovery and supported request formats.</p>

</dd><dt><code>--python-preference</code> <i>python-preference</i></dt><dd><p>Whether to prefer uv-managed or system Python installations.</p>

<p>By default, uv prefers using Python versions it manages. However, it will use system Python installations if a uv-managed Python is not installed. This option allows prioritizing or ignoring system Python installations.</p>

<p>Possible values:</p>

<ul>
<li><code>only-managed</code>:  Only use managed Python installations; never use system Python installations</li>

<li><code>managed</code>:  Prefer managed Python installations over system Python installations</li>

<li><code>system</code>:  Prefer system Python installations over managed Python installations</li>

<li><code>only-system</code>:  Only use system Python installations; never use managed Python installations</li>
</ul>
</dd><dt><code>--quiet</code>, <code>-q</code></dt><dd><p>Do not print any output</p>

</dd><dt><code>--strict</code></dt><dd><p>Validate the Python environment, to detect packages with missing dependencies and other issues</p>

</dd><dt><code>--system</code></dt><dd><p>Show a package in the system Python environment.</p>

<p>Disables discovery of virtual environments.</p>

<p>See <a href="#uv-python">uv python</a> for details on Python discovery.</p>

</dd><dt><code>--verbose</code>, <code>-v</code></dt><dd><p>Use verbose output.</p>

<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (&lt;https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives&gt;)</p>

</dd><dt><code>--version</code>, <code>-V</code></dt><dd><p>Display the uv version</p>

</dd></dl>

### uv pip tree

Display the dependency tree for an environment

<h3 class="cli-reference">Usage</h3>

```
uv pip tree [OPTIONS]
```

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt><code>--cache-dir</code> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>

<p>Defaults to <code>$HOME/Library/Caches/uv</code> on macOS, <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on Linux, and <code>{FOLDERID_LocalAppData}\uv\cache</code> on Windows.</p>

</dd><dt><code>--color</code> <i>color-choice</i></dt><dd><p>Control colors in output</p>

<p>[default: auto]</p>
<p>Possible values:</p>

<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>

<li><code>always</code>:  Enables colored output regardless of the detected environment</li>

<li><code>never</code>:  Disables colored output</li>
</ul>
</dd><dt><code>--config-file</code> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>

<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>

</dd><dt><code>--depth</code>, <code>-d</code> <i>depth</i></dt><dd><p>Maximum display depth of the dependency tree</p>

<p>[default: 255]</p>
</dd><dt><code>--help</code>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>

</dd><dt><code>--invert</code></dt><dd><p>Show the reverse dependencies for the given package. This flag will invert the tree and display the packages that depend on the given package</p>

</dd><dt><code>--native-tls</code></dt><dd><p>Whether to load TLS certificates from the platform&#8217;s native certificate store.</p>

<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>

<p>However, in some cases, you may want to use the platform&#8217;s native certificate store, especially if you&#8217;re relying on a corporate trust root (e.g., for a mandatory proxy) that&#8217;s included in your system&#8217;s certificate store.</p>

</dd><dt><code>--no-cache</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>

</dd><dt><code>--no-config</code></dt><dd><p>Avoid discovering configuration files (<code>pyproject.toml</code>, <code>uv.toml</code>).</p>

<p>Normally, configuration files are discovered in the current directory, parent directories, or user configuration directories.</p>

</dd><dt><code>--no-dedupe</code></dt><dd><p>Do not de-duplicate repeated dependencies. Usually, when a package has already displayed its dependencies, further occurrences will not re-display its dependencies, and will include a (*) to indicate it has already been shown. This flag will cause those duplicates to be repeated</p>

</dd><dt><code>--no-progress</code></dt><dd><p>Hide all progress outputs.</p>

<p>For example, spinners or progress bars.</p>

</dd><dt><code>--no-python-downloads</code></dt><dd><p>Disable automatic downloads of Python</p>

</dd><dt><code>--no-system</code></dt><dt><code>--offline</code></dt><dd><p>Disable network access.</p>

<p>When disabled, uv will only use locally cached data and locally available files.</p>

</dd><dt><code>--package</code> <i>package</i></dt><dd><p>Display only the specified packages</p>

</dd><dt><code>--prune</code> <i>prune</i></dt><dd><p>Prune the given package from the display of the dependency tree</p>

</dd><dt><code>--python</code>, <code>-p</code> <i>python</i></dt><dd><p>The Python interpreter for which packages should be listed.</p>

<p>By default, uv lists packages in a virtual environment but will show packages in a system Python environment if no virtual environment is found.</p>

<p>See <a href="#uv-python">uv python</a> for details on Python discovery and supported request formats.</p>

</dd><dt><code>--python-preference</code> <i>python-preference</i></dt><dd><p>Whether to prefer uv-managed or system Python installations.</p>

<p>By default, uv prefers using Python versions it manages. However, it will use system Python installations if a uv-managed Python is not installed. This option allows prioritizing or ignoring system Python installations.</p>

<p>Possible values:</p>

<ul>
<li><code>only-managed</code>:  Only use managed Python installations; never use system Python installations</li>

<li><code>managed</code>:  Prefer managed Python installations over system Python installations</li>

<li><code>system</code>:  Prefer system Python installations over managed Python installations</li>

<li><code>only-system</code>:  Only use system Python installations; never use managed Python installations</li>
</ul>
</dd><dt><code>--quiet</code>, <code>-q</code></dt><dd><p>Do not print any output</p>

</dd><dt><code>--show-version-specifiers</code></dt><dd><p>Show the version constraint(s) imposed on each package</p>

</dd><dt><code>--strict</code></dt><dd><p>Validate the Python environment, to detect packages with missing dependencies and other issues</p>

</dd><dt><code>--system</code></dt><dd><p>List packages in the system Python environment.</p>

<p>Disables discovery of virtual environments.</p>

<p>See <a href="#uv-python">uv python</a> for details on Python discovery.</p>

</dd><dt><code>--verbose</code>, <code>-v</code></dt><dd><p>Use verbose output.</p>

<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (&lt;https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives&gt;)</p>

</dd><dt><code>--version</code>, <code>-V</code></dt><dd><p>Display the uv version</p>

</dd></dl>

### uv pip check

Verify installed packages have compatible dependencies

<h3 class="cli-reference">Usage</h3>

```
uv pip check [OPTIONS]
```

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt><code>--cache-dir</code> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>

<p>Defaults to <code>$HOME/Library/Caches/uv</code> on macOS, <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on Linux, and <code>{FOLDERID_LocalAppData}\uv\cache</code> on Windows.</p>

</dd><dt><code>--color</code> <i>color-choice</i></dt><dd><p>Control colors in output</p>

<p>[default: auto]</p>
<p>Possible values:</p>

<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>

<li><code>always</code>:  Enables colored output regardless of the detected environment</li>

<li><code>never</code>:  Disables colored output</li>
</ul>
</dd><dt><code>--config-file</code> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>

<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>

</dd><dt><code>--help</code>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>

</dd><dt><code>--native-tls</code></dt><dd><p>Whether to load TLS certificates from the platform&#8217;s native certificate store.</p>

<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>

<p>However, in some cases, you may want to use the platform&#8217;s native certificate store, especially if you&#8217;re relying on a corporate trust root (e.g., for a mandatory proxy) that&#8217;s included in your system&#8217;s certificate store.</p>

</dd><dt><code>--no-cache</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>

</dd><dt><code>--no-config</code></dt><dd><p>Avoid discovering configuration files (<code>pyproject.toml</code>, <code>uv.toml</code>).</p>

<p>Normally, configuration files are discovered in the current directory, parent directories, or user configuration directories.</p>

</dd><dt><code>--no-progress</code></dt><dd><p>Hide all progress outputs.</p>

<p>For example, spinners or progress bars.</p>

</dd><dt><code>--no-python-downloads</code></dt><dd><p>Disable automatic downloads of Python</p>

</dd><dt><code>--offline</code></dt><dd><p>Disable network access.</p>

<p>When disabled, uv will only use locally cached data and locally available files.</p>

</dd><dt><code>--python</code>, <code>-p</code> <i>python</i></dt><dd><p>The Python interpreter for which packages should be checked.</p>

<p>By default, uv checks packages in a virtual environment but will check packages in a system Python environment if no virtual environment is found.</p>

<p>See <a href="#uv-python">uv python</a> for details on Python discovery and supported request formats.</p>

</dd><dt><code>--python-preference</code> <i>python-preference</i></dt><dd><p>Whether to prefer uv-managed or system Python installations.</p>

<p>By default, uv prefers using Python versions it manages. However, it will use system Python installations if a uv-managed Python is not installed. This option allows prioritizing or ignoring system Python installations.</p>

<p>Possible values:</p>

<ul>
<li><code>only-managed</code>:  Only use managed Python installations; never use system Python installations</li>

<li><code>managed</code>:  Prefer managed Python installations over system Python installations</li>

<li><code>system</code>:  Prefer system Python installations over managed Python installations</li>

<li><code>only-system</code>:  Only use system Python installations; never use managed Python installations</li>
</ul>
</dd><dt><code>--quiet</code>, <code>-q</code></dt><dd><p>Do not print any output</p>

</dd><dt><code>--system</code></dt><dd><p>Check packages in the system Python environment.</p>

<p>Disables discovery of virtual environments.</p>

<p>See <a href="#uv-python">uv python</a> for details on Python discovery.</p>

</dd><dt><code>--verbose</code>, <code>-v</code></dt><dd><p>Use verbose output.</p>

<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (&lt;https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives&gt;)</p>

</dd><dt><code>--version</code>, <code>-V</code></dt><dd><p>Display the uv version</p>

</dd></dl>

## uv venv

Create a virtual environment.

By default, creates a virtual environment named `.venv` in the working directory. An alternative path may be provided positionally.

If a virtual environment exists at the target path, it will be removed and a new, empty virtual environment will be created.

When using uv, the virtual environment does not need to be activated. uv will find a virtual environment (named `.venv`) in the working directory or any parent directories.

<h3 class="cli-reference">Usage</h3>

```
uv venv [OPTIONS] [NAME]
```

<h3 class="cli-reference">Arguments</h3>

<dl class="cli-reference"><dt><code>NAME</code></dt><dd><p>The path to the virtual environment to create</p>

</dd></dl>

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt><code>--allow-existing</code></dt><dd><p>Preserve any existing files or directories at the target path.</p>

<p>By default, <code>uv venv</code> will remove an existing virtual environment at the given path, and exit with an error if the path is non-empty but <em>not</em> a virtual environment. The <code>--allow-existing</code> option will instead write to the given path, regardless of its contents, and without clearing it beforehand.</p>

<p>WARNING: This option can lead to unexpected behavior if the existing virtual environment and the newly-created virtual environment are linked to different Python interpreters.</p>

</dd><dt><code>--cache-dir</code> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>

<p>Defaults to <code>$HOME/Library/Caches/uv</code> on macOS, <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on Linux, and <code>{FOLDERID_LocalAppData}\uv\cache</code> on Windows.</p>

</dd><dt><code>--color</code> <i>color-choice</i></dt><dd><p>Control colors in output</p>

<p>[default: auto]</p>
<p>Possible values:</p>

<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>

<li><code>always</code>:  Enables colored output regardless of the detected environment</li>

<li><code>never</code>:  Disables colored output</li>
</ul>
</dd><dt><code>--config-file</code> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>

<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>

</dd><dt><code>--exclude-newer</code> <i>exclude-newer</i></dt><dd><p>Limit candidate packages to those that were uploaded prior to the given date.</p>

<p>Accepts both RFC 3339 timestamps (e.g., <code>2006-12-02T02:07:43Z</code>) and UTC dates in the same format (e.g., <code>2006-12-02</code>).</p>

</dd><dt><code>--extra-index-url</code> <i>extra-index-url</i></dt><dd><p>Extra URLs of package indexes to use, in addition to <code>--index-url</code>.</p>

<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>

<p>All indexes provided via this flag take priority over the index specified by <code>--index-url</code> (which defaults to PyPI). When multiple <code>--extra-index-url</code> flags are provided, earlier values take priority.</p>

</dd><dt><code>--find-links</code>, <code>-f</code> <i>find-links</i></dt><dd><p>Locations to search for candidate distributions, in addition to those found in the registry indexes.</p>

<p>If a path, the target must be a directory that contains packages as wheel files (<code>.whl</code>) or source distributions (<code>.tar.gz</code> or <code>.zip</code>) at the top level.</p>

<p>If a URL, the page must contain a flat list of links to package files adhering to the formats described above.</p>

</dd><dt><code>--help</code>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>

</dd><dt><code>--index-strategy</code> <i>index-strategy</i></dt><dd><p>The strategy to use when resolving against multiple index URLs.</p>

<p>By default, uv will stop at the first index on which a given package is available, and limit resolutions to those present on that first index (<code>first-match</code>). This prevents &quot;dependency confusion&quot; attacks, whereby an attack can upload a malicious package under the same name to a secondary.</p>

<p>Possible values:</p>

<ul>
<li><code>first-index</code>:  Only use results from the first index that returns a match for a given package name</li>

<li><code>unsafe-first-match</code>:  Search for every package name across all indexes, exhausting the versions from the first index before moving on to the next</li>

<li><code>unsafe-best-match</code>:  Search for every package name across all indexes, preferring the &quot;best&quot; version found. If a package version is in multiple indexes, only look at the entry for the first index</li>
</ul>
</dd><dt><code>--index-url</code>, <code>-i</code> <i>index-url</i></dt><dd><p>The URL of the Python package index (by default: &lt;https://pypi.org/simple&gt;).</p>

<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>

<p>The index given by this flag is given lower priority than all other indexes specified via the <code>--extra-index-url</code> flag.</p>

</dd><dt><code>--keyring-provider</code> <i>keyring-provider</i></dt><dd><p>Attempt to use <code>keyring</code> for authentication for index URLs.</p>

<p>At present, only <code>--keyring-provider subprocess</code> is supported, which configures uv to use the <code>keyring</code> CLI to handle authentication.</p>

<p>Defaults to <code>disabled</code>.</p>

<p>Possible values:</p>

<ul>
<li><code>disabled</code>:  Do not use keyring for credential lookup</li>

<li><code>subprocess</code>:  Use the <code>keyring</code> command for credential lookup</li>
</ul>
</dd><dt><code>--link-mode</code> <i>link-mode</i></dt><dd><p>The method to use when installing packages from the global cache.</p>

<p>This option is only used for installing seed packages.</p>

<p>Defaults to <code>clone</code> (also known as Copy-on-Write) on macOS, and <code>hardlink</code> on Linux and Windows.</p>

<p>Possible values:</p>

<ul>
<li><code>clone</code>:  Clone (i.e., copy-on-write) packages from the wheel into the <code>site-packages</code> directory</li>

<li><code>copy</code>:  Copy packages from the wheel into the <code>site-packages</code> directory</li>

<li><code>hardlink</code>:  Hard link packages from the wheel into the <code>site-packages</code> directory</li>

<li><code>symlink</code>:  Symbolically link packages from the wheel into the <code>site-packages</code> directory</li>
</ul>
</dd><dt><code>--native-tls</code></dt><dd><p>Whether to load TLS certificates from the platform&#8217;s native certificate store.</p>

<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>

<p>However, in some cases, you may want to use the platform&#8217;s native certificate store, especially if you&#8217;re relying on a corporate trust root (e.g., for a mandatory proxy) that&#8217;s included in your system&#8217;s certificate store.</p>

</dd><dt><code>--no-cache</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>

</dd><dt><code>--no-config</code></dt><dd><p>Avoid discovering configuration files (<code>pyproject.toml</code>, <code>uv.toml</code>).</p>

<p>Normally, configuration files are discovered in the current directory, parent directories, or user configuration directories.</p>

</dd><dt><code>--no-index</code></dt><dd><p>Ignore the registry index (e.g., PyPI), instead relying on direct URL dependencies and those provided via <code>--find-links</code></p>

</dd><dt><code>--no-progress</code></dt><dd><p>Hide all progress outputs.</p>

<p>For example, spinners or progress bars.</p>

</dd><dt><code>--no-python-downloads</code></dt><dd><p>Disable automatic downloads of Python</p>

</dd><dt><code>--offline</code></dt><dd><p>Disable network access.</p>

<p>When disabled, uv will only use locally cached data and locally available files.</p>

</dd><dt><code>--prompt</code> <i>prompt</i></dt><dd><p>Provide an alternative prompt prefix for the virtual environment.</p>

<p>By default, the prompt is dependent on whether a path was provided to <code>uv venv</code>. If provided (e.g, <code>uv venv project</code>), the prompt is set to the directory name. If not provided (<code>uv venv</code>), the prompt is set to the current directory&#8217;s name.</p>

<p>If &quot;.&quot; is provided, the the current directory name will be used regardless of whether a path was provided to <code>uv venv</code>.</p>

</dd><dt><code>--python</code>, <code>-p</code> <i>python</i></dt><dd><p>The Python interpreter to use for the virtual environment.</p>

<p>During virtual environment creation, uv will not look for Python interpreters in virtual environments.</p>

<p>See <code>uv python help</code> for details on Python discovery and supported request formats.</p>

</dd><dt><code>--python-preference</code> <i>python-preference</i></dt><dd><p>Whether to prefer uv-managed or system Python installations.</p>

<p>By default, uv prefers using Python versions it manages. However, it will use system Python installations if a uv-managed Python is not installed. This option allows prioritizing or ignoring system Python installations.</p>

<p>Possible values:</p>

<ul>
<li><code>only-managed</code>:  Only use managed Python installations; never use system Python installations</li>

<li><code>managed</code>:  Prefer managed Python installations over system Python installations</li>

<li><code>system</code>:  Prefer system Python installations over managed Python installations</li>

<li><code>only-system</code>:  Only use system Python installations; never use managed Python installations</li>
</ul>
</dd><dt><code>--quiet</code>, <code>-q</code></dt><dd><p>Do not print any output</p>

</dd><dt><code>--relocatable</code></dt><dd><p>Make the virtual environment relocatable.</p>

<p>A relocatable virtual environment can be moved around and redistributed without invalidating its associated entrypoint and activation scripts.</p>

<p>Note that this can only be guaranteed for standard <code>console_scripts</code> and <code>gui_scripts</code>. Other scripts may be adjusted if they ship with a generic <code>#!python[w]</code> shebang, and binaries are left as-is.</p>

<p>As a result of making the environment relocatable (by way of writing relative, rather than absolute paths), the entrypoints and scripts themselves will <em>not</em> be relocatable. In other words, copying those entrypoints and scripts to a location outside the environment will not work, as they reference paths relative to the environment itself.</p>

</dd><dt><code>--seed</code></dt><dd><p>Install seed packages (one or more of: <code>pip</code>, <code>setuptools</code>, and <code>wheel</code>) into the virtual environment.</p>

<p>Note <code>setuptools</code> and <code>wheel</code> are not included in Python 3.12+ environments.</p>

</dd><dt><code>--system-site-packages</code></dt><dd><p>Give the virtual environment access to the system site packages directory.</p>

<p>Unlike <code>pip</code>, when a virtual environment is created with <code>--system-site-packages</code>, uv will <em>not</em> take system site packages into account when running commands like <code>uv pip list</code> or <code>uv pip install</code>. The <code>--system-site-packages</code> flag will provide the virtual environment with access to the system site packages directory at runtime, but will not affect the behavior of uv commands.</p>

</dd><dt><code>--verbose</code>, <code>-v</code></dt><dd><p>Use verbose output.</p>

<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (&lt;https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives&gt;)</p>

</dd><dt><code>--version</code>, <code>-V</code></dt><dd><p>Display the uv version</p>

</dd></dl>

## uv cache

Manage uv's cache

<h3 class="cli-reference">Usage</h3>

```
uv cache [OPTIONS] <COMMAND>
```

<h3 class="cli-reference">Commands</h3>

<dl class="cli-reference"><dt><a href="#uv-cache-clean"><code>uv cache clean</code></a></dt><dd><p>Clear the cache, removing all entries or those linked to specific packages</p>
</dd>
<dt><a href="#uv-cache-prune"><code>uv cache prune</code></a></dt><dd><p>Prune all unreachable objects from the cache</p>
</dd>
<dt><a href="#uv-cache-dir"><code>uv cache dir</code></a></dt><dd><p>Show the cache directory</p>
</dd>
</dl>

### uv cache clean

Clear the cache, removing all entries or those linked to specific packages

<h3 class="cli-reference">Usage</h3>

```
uv cache clean [OPTIONS] [PACKAGE]...
```

<h3 class="cli-reference">Arguments</h3>

<dl class="cli-reference"><dt><code>PACKAGE</code></dt><dd><p>The packages to remove from the cache</p>

</dd></dl>

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt><code>--cache-dir</code> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>

<p>Defaults to <code>$HOME/Library/Caches/uv</code> on macOS, <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on Linux, and <code>{FOLDERID_LocalAppData}\uv\cache</code> on Windows.</p>

</dd><dt><code>--color</code> <i>color-choice</i></dt><dd><p>Control colors in output</p>

<p>[default: auto]</p>
<p>Possible values:</p>

<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>

<li><code>always</code>:  Enables colored output regardless of the detected environment</li>

<li><code>never</code>:  Disables colored output</li>
</ul>
</dd><dt><code>--config-file</code> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>

<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>

</dd><dt><code>--help</code>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>

</dd><dt><code>--native-tls</code></dt><dd><p>Whether to load TLS certificates from the platform&#8217;s native certificate store.</p>

<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>

<p>However, in some cases, you may want to use the platform&#8217;s native certificate store, especially if you&#8217;re relying on a corporate trust root (e.g., for a mandatory proxy) that&#8217;s included in your system&#8217;s certificate store.</p>

</dd><dt><code>--no-cache</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>

</dd><dt><code>--no-config</code></dt><dd><p>Avoid discovering configuration files (<code>pyproject.toml</code>, <code>uv.toml</code>).</p>

<p>Normally, configuration files are discovered in the current directory, parent directories, or user configuration directories.</p>

</dd><dt><code>--no-progress</code></dt><dd><p>Hide all progress outputs.</p>

<p>For example, spinners or progress bars.</p>

</dd><dt><code>--no-python-downloads</code></dt><dd><p>Disable automatic downloads of Python</p>

</dd><dt><code>--offline</code></dt><dd><p>Disable network access.</p>

<p>When disabled, uv will only use locally cached data and locally available files.</p>

</dd><dt><code>--python-preference</code> <i>python-preference</i></dt><dd><p>Whether to prefer uv-managed or system Python installations.</p>

<p>By default, uv prefers using Python versions it manages. However, it will use system Python installations if a uv-managed Python is not installed. This option allows prioritizing or ignoring system Python installations.</p>

<p>Possible values:</p>

<ul>
<li><code>only-managed</code>:  Only use managed Python installations; never use system Python installations</li>

<li><code>managed</code>:  Prefer managed Python installations over system Python installations</li>

<li><code>system</code>:  Prefer system Python installations over managed Python installations</li>

<li><code>only-system</code>:  Only use system Python installations; never use managed Python installations</li>
</ul>
</dd><dt><code>--quiet</code>, <code>-q</code></dt><dd><p>Do not print any output</p>

</dd><dt><code>--verbose</code>, <code>-v</code></dt><dd><p>Use verbose output.</p>

<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (&lt;https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives&gt;)</p>

</dd><dt><code>--version</code>, <code>-V</code></dt><dd><p>Display the uv version</p>

</dd></dl>

### uv cache prune

Prune all unreachable objects from the cache

<h3 class="cli-reference">Usage</h3>

```
uv cache prune [OPTIONS]
```

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt><code>--cache-dir</code> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>

<p>Defaults to <code>$HOME/Library/Caches/uv</code> on macOS, <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on Linux, and <code>{FOLDERID_LocalAppData}\uv\cache</code> on Windows.</p>

</dd><dt><code>--ci</code></dt><dd><p>Optimize the cache for persistence in a continuous integration environment, like GitHub Actions.</p>

<p>By default, uv caches both the wheels that it builds from source and the pre-built wheels that it downloads directly, to enable high-performance package installation. In some scenarios, though, persisting pre-built wheels may be undesirable. For example, in GitHub Actions, it&#8217;s faster to omit pre-built wheels from the cache and instead have re-download them on each run. However, it typically <em>is</em> faster to cache wheels that are built from source, since the wheel building process can be expensive, especially for extension modules.</p>

<p>In <code>--ci</code> mode, uv will prune any pre-built wheels from the cache, but retain any wheels that were built from source.</p>

</dd><dt><code>--color</code> <i>color-choice</i></dt><dd><p>Control colors in output</p>

<p>[default: auto]</p>
<p>Possible values:</p>

<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>

<li><code>always</code>:  Enables colored output regardless of the detected environment</li>

<li><code>never</code>:  Disables colored output</li>
</ul>
</dd><dt><code>--config-file</code> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>

<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>

</dd><dt><code>--help</code>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>

</dd><dt><code>--native-tls</code></dt><dd><p>Whether to load TLS certificates from the platform&#8217;s native certificate store.</p>

<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>

<p>However, in some cases, you may want to use the platform&#8217;s native certificate store, especially if you&#8217;re relying on a corporate trust root (e.g., for a mandatory proxy) that&#8217;s included in your system&#8217;s certificate store.</p>

</dd><dt><code>--no-cache</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>

</dd><dt><code>--no-config</code></dt><dd><p>Avoid discovering configuration files (<code>pyproject.toml</code>, <code>uv.toml</code>).</p>

<p>Normally, configuration files are discovered in the current directory, parent directories, or user configuration directories.</p>

</dd><dt><code>--no-progress</code></dt><dd><p>Hide all progress outputs.</p>

<p>For example, spinners or progress bars.</p>

</dd><dt><code>--no-python-downloads</code></dt><dd><p>Disable automatic downloads of Python</p>

</dd><dt><code>--offline</code></dt><dd><p>Disable network access.</p>

<p>When disabled, uv will only use locally cached data and locally available files.</p>

</dd><dt><code>--python-preference</code> <i>python-preference</i></dt><dd><p>Whether to prefer uv-managed or system Python installations.</p>

<p>By default, uv prefers using Python versions it manages. However, it will use system Python installations if a uv-managed Python is not installed. This option allows prioritizing or ignoring system Python installations.</p>

<p>Possible values:</p>

<ul>
<li><code>only-managed</code>:  Only use managed Python installations; never use system Python installations</li>

<li><code>managed</code>:  Prefer managed Python installations over system Python installations</li>

<li><code>system</code>:  Prefer system Python installations over managed Python installations</li>

<li><code>only-system</code>:  Only use system Python installations; never use managed Python installations</li>
</ul>
</dd><dt><code>--quiet</code>, <code>-q</code></dt><dd><p>Do not print any output</p>

</dd><dt><code>--verbose</code>, <code>-v</code></dt><dd><p>Use verbose output.</p>

<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (&lt;https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives&gt;)</p>

</dd><dt><code>--version</code>, <code>-V</code></dt><dd><p>Display the uv version</p>

</dd></dl>

### uv cache dir

Show the cache directory

<h3 class="cli-reference">Usage</h3>

```
uv cache dir [OPTIONS]
```

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt><code>--cache-dir</code> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>

<p>Defaults to <code>$HOME/Library/Caches/uv</code> on macOS, <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on Linux, and <code>{FOLDERID_LocalAppData}\uv\cache</code> on Windows.</p>

</dd><dt><code>--color</code> <i>color-choice</i></dt><dd><p>Control colors in output</p>

<p>[default: auto]</p>
<p>Possible values:</p>

<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>

<li><code>always</code>:  Enables colored output regardless of the detected environment</li>

<li><code>never</code>:  Disables colored output</li>
</ul>
</dd><dt><code>--config-file</code> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>

<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>

</dd><dt><code>--help</code>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>

</dd><dt><code>--native-tls</code></dt><dd><p>Whether to load TLS certificates from the platform&#8217;s native certificate store.</p>

<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>

<p>However, in some cases, you may want to use the platform&#8217;s native certificate store, especially if you&#8217;re relying on a corporate trust root (e.g., for a mandatory proxy) that&#8217;s included in your system&#8217;s certificate store.</p>

</dd><dt><code>--no-cache</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>

</dd><dt><code>--no-config</code></dt><dd><p>Avoid discovering configuration files (<code>pyproject.toml</code>, <code>uv.toml</code>).</p>

<p>Normally, configuration files are discovered in the current directory, parent directories, or user configuration directories.</p>

</dd><dt><code>--no-progress</code></dt><dd><p>Hide all progress outputs.</p>

<p>For example, spinners or progress bars.</p>

</dd><dt><code>--no-python-downloads</code></dt><dd><p>Disable automatic downloads of Python</p>

</dd><dt><code>--offline</code></dt><dd><p>Disable network access.</p>

<p>When disabled, uv will only use locally cached data and locally available files.</p>

</dd><dt><code>--python-preference</code> <i>python-preference</i></dt><dd><p>Whether to prefer uv-managed or system Python installations.</p>

<p>By default, uv prefers using Python versions it manages. However, it will use system Python installations if a uv-managed Python is not installed. This option allows prioritizing or ignoring system Python installations.</p>

<p>Possible values:</p>

<ul>
<li><code>only-managed</code>:  Only use managed Python installations; never use system Python installations</li>

<li><code>managed</code>:  Prefer managed Python installations over system Python installations</li>

<li><code>system</code>:  Prefer system Python installations over managed Python installations</li>

<li><code>only-system</code>:  Only use system Python installations; never use managed Python installations</li>
</ul>
</dd><dt><code>--quiet</code>, <code>-q</code></dt><dd><p>Do not print any output</p>

</dd><dt><code>--verbose</code>, <code>-v</code></dt><dd><p>Use verbose output.</p>

<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (&lt;https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives&gt;)</p>

</dd><dt><code>--version</code>, <code>-V</code></dt><dd><p>Display the uv version</p>

</dd></dl>

## uv version

Display uv's version

<h3 class="cli-reference">Usage</h3>

```
uv version [OPTIONS]
```

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt><code>--cache-dir</code> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>

<p>Defaults to <code>$HOME/Library/Caches/uv</code> on macOS, <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on Linux, and <code>{FOLDERID_LocalAppData}\uv\cache</code> on Windows.</p>

</dd><dt><code>--color</code> <i>color-choice</i></dt><dd><p>Control colors in output</p>

<p>[default: auto]</p>
<p>Possible values:</p>

<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>

<li><code>always</code>:  Enables colored output regardless of the detected environment</li>

<li><code>never</code>:  Disables colored output</li>
</ul>
</dd><dt><code>--config-file</code> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>

<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>

</dd><dt><code>--help</code>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>

</dd><dt><code>--native-tls</code></dt><dd><p>Whether to load TLS certificates from the platform&#8217;s native certificate store.</p>

<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>

<p>However, in some cases, you may want to use the platform&#8217;s native certificate store, especially if you&#8217;re relying on a corporate trust root (e.g., for a mandatory proxy) that&#8217;s included in your system&#8217;s certificate store.</p>

</dd><dt><code>--no-cache</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>

</dd><dt><code>--no-config</code></dt><dd><p>Avoid discovering configuration files (<code>pyproject.toml</code>, <code>uv.toml</code>).</p>

<p>Normally, configuration files are discovered in the current directory, parent directories, or user configuration directories.</p>

</dd><dt><code>--no-progress</code></dt><dd><p>Hide all progress outputs.</p>

<p>For example, spinners or progress bars.</p>

</dd><dt><code>--no-python-downloads</code></dt><dd><p>Disable automatic downloads of Python</p>

</dd><dt><code>--offline</code></dt><dd><p>Disable network access.</p>

<p>When disabled, uv will only use locally cached data and locally available files.</p>

</dd><dt><code>--output-format</code> <i>output-format</i></dt><dt><code>--python-preference</code> <i>python-preference</i></dt><dd><p>Whether to prefer uv-managed or system Python installations.</p>

<p>By default, uv prefers using Python versions it manages. However, it will use system Python installations if a uv-managed Python is not installed. This option allows prioritizing or ignoring system Python installations.</p>

<p>Possible values:</p>

<ul>
<li><code>only-managed</code>:  Only use managed Python installations; never use system Python installations</li>

<li><code>managed</code>:  Prefer managed Python installations over system Python installations</li>

<li><code>system</code>:  Prefer system Python installations over managed Python installations</li>

<li><code>only-system</code>:  Only use system Python installations; never use managed Python installations</li>
</ul>
</dd><dt><code>--quiet</code>, <code>-q</code></dt><dd><p>Do not print any output</p>

</dd><dt><code>--verbose</code>, <code>-v</code></dt><dd><p>Use verbose output.</p>

<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (&lt;https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives&gt;)</p>

</dd><dt><code>--version</code>, <code>-V</code></dt><dd><p>Display the uv version</p>

</dd></dl>

## uv help

Display documentation for a command

<h3 class="cli-reference">Usage</h3>

```
uv help [OPTIONS] [COMMAND]...
```

<h3 class="cli-reference">Arguments</h3>

<dl class="cli-reference"><dt><code>COMMAND</code></dt></dl>

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt><code>--cache-dir</code> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>

<p>Defaults to <code>$HOME/Library/Caches/uv</code> on macOS, <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on Linux, and <code>{FOLDERID_LocalAppData}\uv\cache</code> on Windows.</p>

</dd><dt><code>--color</code> <i>color-choice</i></dt><dd><p>Control colors in output</p>

<p>[default: auto]</p>
<p>Possible values:</p>

<ul>
<li><code>auto</code>:  Enables colored output only when the output is going to a terminal or TTY with support</li>

<li><code>always</code>:  Enables colored output regardless of the detected environment</li>

<li><code>never</code>:  Disables colored output</li>
</ul>
</dd><dt><code>--config-file</code> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration.</p>

<p>While uv configuration can be included in a <code>pyproject.toml</code> file, it is not allowed in this context.</p>

</dd><dt><code>--help</code>, <code>-h</code></dt><dd><p>Display the concise help for this command</p>

</dd><dt><code>--native-tls</code></dt><dd><p>Whether to load TLS certificates from the platform&#8217;s native certificate store.</p>

<p>By default, uv loads certificates from the bundled <code>webpki-roots</code> crate. The <code>webpki-roots</code> are a reliable set of trust roots from Mozilla, and including them in uv improves portability and performance (especially on macOS).</p>

<p>However, in some cases, you may want to use the platform&#8217;s native certificate store, especially if you&#8217;re relying on a corporate trust root (e.g., for a mandatory proxy) that&#8217;s included in your system&#8217;s certificate store.</p>

</dd><dt><code>--no-cache</code>, <code>-n</code></dt><dd><p>Avoid reading from or writing to the cache, instead using a temporary directory for the duration of the operation</p>

</dd><dt><code>--no-config</code></dt><dd><p>Avoid discovering configuration files (<code>pyproject.toml</code>, <code>uv.toml</code>).</p>

<p>Normally, configuration files are discovered in the current directory, parent directories, or user configuration directories.</p>

</dd><dt><code>--no-pager</code></dt><dd><p>Disable pager when printing help</p>

</dd><dt><code>--no-progress</code></dt><dd><p>Hide all progress outputs.</p>

<p>For example, spinners or progress bars.</p>

</dd><dt><code>--no-python-downloads</code></dt><dd><p>Disable automatic downloads of Python</p>

</dd><dt><code>--offline</code></dt><dd><p>Disable network access.</p>

<p>When disabled, uv will only use locally cached data and locally available files.</p>

</dd><dt><code>--python-preference</code> <i>python-preference</i></dt><dd><p>Whether to prefer uv-managed or system Python installations.</p>

<p>By default, uv prefers using Python versions it manages. However, it will use system Python installations if a uv-managed Python is not installed. This option allows prioritizing or ignoring system Python installations.</p>

<p>Possible values:</p>

<ul>
<li><code>only-managed</code>:  Only use managed Python installations; never use system Python installations</li>

<li><code>managed</code>:  Prefer managed Python installations over system Python installations</li>

<li><code>system</code>:  Prefer system Python installations over managed Python installations</li>

<li><code>only-system</code>:  Only use system Python installations; never use managed Python installations</li>
</ul>
</dd><dt><code>--quiet</code>, <code>-q</code></dt><dd><p>Do not print any output</p>

</dd><dt><code>--verbose</code>, <code>-v</code></dt><dd><p>Use verbose output.</p>

<p>You can configure fine-grained logging using the <code>RUST_LOG</code> environment variable. (&lt;https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives&gt;)</p>

</dd><dt><code>--version</code>, <code>-V</code></dt><dd><p>Display the uv version</p>

</dd></dl>

