# CLI Reference

## uv

An extremely fast Python package manager.

<h3 class="cli-reference">Usage</h3>

```
uv [OPTIONS] <COMMAND>
```

<h3 class="cli-reference">Commands</h3>

<dl class="cli-reference"><dt><a href="uv-pip"><code>uv pip</code></a></dt><dd><p>Resolve and install Python packages</p>
</dd>
<dt><a href="uv-tool"><code>uv tool</code></a></dt><dd><p>Run and manage executable Python packages</p>
</dd>
<dt><a href="uv-python"><code>uv python</code></a></dt><dd><p>Manage Python installations</p>
</dd>
<dt><a href="uv-venv"><code>uv venv</code></a></dt><dd><p>Create a virtual environment</p>
</dd>
<dt><a href="uv-cache"><code>uv cache</code></a></dt><dd><p>Manage the cache</p>
</dd>
<dt><a href="uv-version"><code>uv version</code></a></dt><dd><p>Display uv&#8217;s version</p>
</dd>
<dt><a href="uv-help"><code>uv help</code></a></dt><dd><p>Display documentation for a command</p>
</dd>
</dl>

## uv pip

Resolve and install Python packages

<h3 class="cli-reference">Usage</h3>

```
uv pip [OPTIONS] <COMMAND>
```

<h3 class="cli-reference">Commands</h3>

<dl class="cli-reference"><dt><a href="uv-pip-compile"><code>uv pip compile</code></a></dt><dd><p>Compile a <code>requirements.in</code> file to a <code>requirements.txt</code> file</p>
</dd>
<dt><a href="uv-pip-sync"><code>uv pip sync</code></a></dt><dd><p>Sync an environment with a <code>requirements.txt</code> file</p>
</dd>
<dt><a href="uv-pip-install"><code>uv pip install</code></a></dt><dd><p>Install packages into an environment</p>
</dd>
<dt><a href="uv-pip-uninstall"><code>uv pip uninstall</code></a></dt><dd><p>Uninstall packages from an environment</p>
</dd>
<dt><a href="uv-pip-freeze"><code>uv pip freeze</code></a></dt><dd><p>List, in requirements format, packages installed in an environment</p>
</dd>
<dt><a href="uv-pip-list"><code>uv pip list</code></a></dt><dd><p>List, in tabular format, packages installed in an environment</p>
</dd>
<dt><a href="uv-pip-show"><code>uv pip show</code></a></dt><dd><p>Show information about one or more installed packages</p>
</dd>
<dt><a href="uv-pip-tree"><code>uv pip tree</code></a></dt><dd><p>Display the dependency tree for an environment</p>
</dd>
<dt><a href="uv-pip-check"><code>uv pip check</code></a></dt><dd><p>Verify installed packages have compatible dependencies</p>
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

<dl class="cli-reference"><dt><code>--constraint</code>, <code>-c</code> <i>constraint</i></dt><dd><p>Constrain versions using the given requirements files.</p>

<p>Constraints files are <code>requirements.txt</code>-like files that only control the <em>version</em> of a requirement that&#8217;s installed. However, including a package in a constraints file will <em>not</em> trigger the installation of that package.</p>

<p>This is equivalent to pip&#8217;s <code>--constraint</code> option.</p>

</dd><dt><code>--override</code> <i>override</i></dt><dd><p>Override versions using the given requirements files.</p>

<p>Overrides files are <code>requirements.txt</code>-like files that force a specific version of a requirement to be installed, regardless of the requirements declared by any constituent package, and regardless of whether this would be considered an invalid resolution.</p>

<p>While constraints are <em>additive</em>, in that they&#8217;re combined with the requirements of the constituent packages, overrides are <em>absolute</em>, in that they completely replace the requirements of the constituent packages.</p>

</dd><dt><code>--extra</code> <i>extra</i></dt><dd><p>Include optional dependencies from the extra group name; may be provided more than once.</p>

<p>Only applies to <code>pyproject.toml</code>, <code>setup.py</code>, and <code>setup.cfg</code> sources.</p>

</dd><dt><code>--index-url</code>, <code>-i</code> <i>index-url</i></dt><dd><p>The URL of the Python package index (by default: &lt;https://pypi.org/simple&gt;).</p>

<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>

<p>The index given by this flag is given lower priority than all other indexes specified via the <code>--extra-index-url</code> flag.</p>

</dd><dt><code>--extra-index-url</code> <i>extra-index-url</i></dt><dd><p>Extra URLs of package indexes to use, in addition to <code>--index-url</code>.</p>

<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>

<p>All indexes provided via this flag take priority over the index specified by <code>--index-url</code> (which defaults to PyPI). When multiple <code>--extra-index-url</code> flags are provided, earlier values take priority.</p>

</dd><dt><code>--find-links</code>, <code>-f</code> <i>find-links</i></dt><dd><p>Locations to search for candidate distributions, in addition to those found in the registry indexes.</p>

<p>If a path, the target must be a directory that contains packages as wheel files (<code>.whl</code>) or source distributions (<code>.tar.gz</code> or <code>.zip</code>) at the top level.</p>

<p>If a URL, the page must contain a flat list of links to package files adhering to the formats described above.</p>

</dd><dt><code>--upgrade-package</code>, <code>-P</code> <i>upgrade-package</i></dt><dd><p>Allow upgrades for a specific package, ignoring pinned versions in any existing output file</p>

</dd><dt><code>--index-strategy</code> <i>index-strategy</i></dt><dd><p>The strategy to use when resolving against multiple index URLs.</p>

<p>By default, uv will stop at the first index on which a given package is available, and limit resolutions to those present on that first index (<code>first-match</code>). This prevents &quot;dependency confusion&quot; attacks, whereby an attack can upload a malicious package under the same name to a secondary.</p>

</dd><dt><code>--keyring-provider</code> <i>keyring-provider</i></dt><dd><p>Attempt to use <code>keyring</code> for authentication for index URLs.</p>

<p>At present, only <code>--keyring-provider subprocess</code> is supported, which configures uv to use the <code>keyring</code> CLI to handle authentication.</p>

<p>Defaults to <code>disabled</code>.</p>

</dd><dt><code>--resolution</code> <i>resolution</i></dt><dd><p>The strategy to use when selecting between the different compatible versions for a given package requirement.</p>

<p>By default, uv will use the latest compatible version of each package (<code>highest</code>).</p>

</dd><dt><code>--prerelease</code> <i>prerelease</i></dt><dd><p>The strategy to use when considering pre-release versions.</p>

<p>By default, uv will accept pre-releases for packages that <em>only</em> publish pre-releases, along with first-party requirements that contain an explicit pre-release marker in the declared specifiers (<code>if-necessary-or-explicit</code>).</p>

</dd><dt><code>--config-setting</code>, <code>-C</code> <i>config-setting</i></dt><dd><p>Settings to pass to the PEP 517 build backend, specified as <code>KEY=VALUE</code> pairs</p>

</dd><dt><code>--exclude-newer</code> <i>exclude-newer</i></dt><dd><p>Limit candidate packages to those that were uploaded prior to the given date.</p>

<p>Accepts both RFC 3339 timestamps (e.g., <code>2006-12-02T02:07:43Z</code>) and UTC dates in the same format (e.g., <code>2006-12-02</code>).</p>

</dd><dt><code>--link-mode</code> <i>link-mode</i></dt><dd><p>The method to use when installing packages from the global cache.</p>

<p>This option is only used when building source distributions.</p>

<p>Defaults to <code>clone</code> (also known as Copy-on-Write) on macOS, and <code>hardlink</code> on Linux and Windows.</p>

</dd><dt><code>--refresh-package</code> <i>refresh-package</i></dt><dd><p>Refresh cached data for a specific package</p>

</dd><dt><code>--output-file</code>, <code>-o</code> <i>output-file</i></dt><dd><p>Write the compiled requirements to the given <code>requirements.txt</code> file.</p>

<p>If the file already exists, the existing versions will be preferred when resolving dependencies, unless <code>--upgrade</code> is also specified.</p>

</dd><dt><code>--annotation-style</code> <i>annotation-style</i></dt><dd><p>The style of the annotation comments included in the output file, used to indicate the source of each package.</p>

<p>Defaults to <code>split</code>.</p>

</dd><dt><code>--custom-compile-command</code> <i>custom-compile-command</i></dt><dd><p>The header comment to include at the top of the output file generated by <code>uv pip compile</code>.</p>

<p>Used to reflect custom build scripts and commands that wrap <code>uv pip compile</code>.</p>

</dd><dt><code>--python</code> <i>python</i></dt><dd><p>The Python interpreter against which to compile the requirements.</p>

<p>By default, uv uses the virtual environment in the current working directory or any parent directory, falling back to searching for a Python executable in <code>PATH</code>. The <code>--python</code> option allows you to specify a different interpreter.</p>

<p>Supported formats:</p>

<ul>
<li><code>3.10</code> looks for an installed Python 3.10 using <code>py --list-paths</code> on Windows, or <code>python3.10</code> on Linux and macOS.</li>

<li><code>python3.10</code> or <code>python.exe</code> looks for a binary with the given name in <code>PATH</code>.</li>

<li><code>/home/ferris/.local/bin/python3.10</code> uses the exact Python at the given path.</li>
</ul>

</dd><dt><code>--no-binary</code> <i>no-binary</i></dt><dd><p>Don&#8217;t install pre-built wheels.</p>

<p>The given packages will be built and installed from source. The resolver will still use pre-built wheels to extract package metadata, if available.</p>

<p>Multiple packages may be provided. Disable binaries for all packages with <code>:all:</code>. Clear previously specified packages with <code>:none:</code>.</p>

</dd><dt><code>--only-binary</code> <i>only-binary</i></dt><dd><p>Only use pre-built wheels; don&#8217;t build source distributions.</p>

<p>When enabled, resolving will not run code from the given packages. The cached wheels of already-built source distributions will be reused, but operations that require building distributions will exit with an error.</p>

<p>Multiple packages may be provided. Disable binaries for all packages with <code>:all:</code>. Clear previously specified packages with <code>:none:</code>.</p>

</dd><dt><code>--python-version</code>, <code>-p</code> <i>python-version</i></dt><dd><p>The minimum Python version that should be supported by the resolved requirements (e.g., <code>3.8</code> or <code>3.8.17</code>).</p>

<p>If a patch version is omitted, the minimum patch version is assumed. For example, <code>3.8</code> is mapped to <code>3.8.0</code>.</p>

</dd><dt><code>--python-platform</code> <i>python-platform</i></dt><dd><p>The platform for which requirements should be resolved.</p>

<p>Represented as a &quot;target triple&quot;, a string that describes the target platform in terms of its CPU, vendor, and operating system name, like <code>x86_64-unknown-linux-gnu</code> or <code>aaarch64-apple-darwin</code>.</p>

</dd><dt><code>--no-emit-package</code> <i>no-emit-package</i></dt><dd><p>Specify a package to omit from the output resolution. Its dependencies will still be included in the resolution. Equivalent to pip-compile&#8217;s <code>--unsafe-package</code> option</p>

</dd><dt><code>--cache-dir</code> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>

<p>Defaults to <code>$HOME/Library/Caches/uv</code> on macOS, <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on Linux, and <code>{FOLDERID_LocalAppData}\uv\cache</code> on Windows.</p>

</dd><dt><code>--python-preference</code> <i>python-preference</i></dt><dd><p>Whether to prefer using Python installations that are already present on the system, or those that are downloaded and installed by uv</p>

</dd><dt><code>--python-fetch</code> <i>python-fetch</i></dt><dd><p>Whether to automatically download Python when required</p>

</dd><dt><code>--color</code> <i>color-choice</i></dt><dd><p>Control colors in output</p>

</dd><dt><code>--config-file</code> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration</p>

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

<dl class="cli-reference"><dt><code>--constraint</code>, <code>-c</code> <i>constraint</i></dt><dd><p>Constrain versions using the given requirements files.</p>

<p>Constraints files are <code>requirements.txt</code>-like files that only control the <em>version</em> of a requirement that&#8217;s installed. However, including a package in a constraints file will <em>not</em> trigger the installation of that package.</p>

<p>This is equivalent to pip&#8217;s <code>--constraint</code> option.</p>

</dd><dt><code>--index-url</code>, <code>-i</code> <i>index-url</i></dt><dd><p>The URL of the Python package index (by default: &lt;https://pypi.org/simple&gt;).</p>

<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>

<p>The index given by this flag is given lower priority than all other indexes specified via the <code>--extra-index-url</code> flag.</p>

</dd><dt><code>--extra-index-url</code> <i>extra-index-url</i></dt><dd><p>Extra URLs of package indexes to use, in addition to <code>--index-url</code>.</p>

<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>

<p>All indexes provided via this flag take priority over the index specified by <code>--index-url</code> (which defaults to PyPI). When multiple <code>--extra-index-url</code> flags are provided, earlier values take priority.</p>

</dd><dt><code>--find-links</code>, <code>-f</code> <i>find-links</i></dt><dd><p>Locations to search for candidate distributions, in addition to those found in the registry indexes.</p>

<p>If a path, the target must be a directory that contains packages as wheel files (<code>.whl</code>) or source distributions (<code>.tar.gz</code> or <code>.zip</code>) at the top level.</p>

<p>If a URL, the page must contain a flat list of links to package files adhering to the formats described above.</p>

</dd><dt><code>--reinstall-package</code> <i>reinstall-package</i></dt><dd><p>Reinstall a specific package, regardless of whether it&#8217;s already installed. Implies <code>--refresh-package</code></p>

</dd><dt><code>--index-strategy</code> <i>index-strategy</i></dt><dd><p>The strategy to use when resolving against multiple index URLs.</p>

<p>By default, uv will stop at the first index on which a given package is available, and limit resolutions to those present on that first index (<code>first-match</code>). This prevents &quot;dependency confusion&quot; attacks, whereby an attack can upload a malicious package under the same name to a secondary.</p>

</dd><dt><code>--keyring-provider</code> <i>keyring-provider</i></dt><dd><p>Attempt to use <code>keyring</code> for authentication for index URLs.</p>

<p>At present, only <code>--keyring-provider subprocess</code> is supported, which configures uv to use the <code>keyring</code> CLI to handle authentication.</p>

<p>Defaults to <code>disabled</code>.</p>

</dd><dt><code>--config-setting</code>, <code>-C</code> <i>config-setting</i></dt><dd><p>Settings to pass to the PEP 517 build backend, specified as <code>KEY=VALUE</code> pairs</p>

</dd><dt><code>--exclude-newer</code> <i>exclude-newer</i></dt><dd><p>Limit candidate packages to those that were uploaded prior to the given date.</p>

<p>Accepts both RFC 3339 timestamps (e.g., <code>2006-12-02T02:07:43Z</code>) and UTC dates in the same format (e.g., <code>2006-12-02</code>).</p>

</dd><dt><code>--link-mode</code> <i>link-mode</i></dt><dd><p>The method to use when installing packages from the global cache.</p>

<p>Defaults to <code>clone</code> (also known as Copy-on-Write) on macOS, and <code>hardlink</code> on Linux and Windows.</p>

</dd><dt><code>--refresh-package</code> <i>refresh-package</i></dt><dd><p>Refresh cached data for a specific package</p>

</dd><dt><code>--python</code>, <code>-p</code> <i>python</i></dt><dd><p>The Python interpreter into which packages should be installed.</p>

<p>By default, uv installs into the virtual environment in the current working directory or any parent directory. The <code>--python</code> option allows you to specify a different interpreter, which is intended for use in continuous integration (CI) environments or other automated workflows.</p>

<p>Supported formats:</p>

<ul>
<li><code>3.10</code> looks for an installed Python 3.10 using <code>py --list-paths</code> on Windows, or <code>python3.10</code> on Linux and macOS.</li>

<li><code>python3.10</code> or <code>python.exe</code> looks for a binary with the given name in <code>PATH</code>.</li>

<li><code>/home/ferris/.local/bin/python3.10</code> uses the exact Python at the given path.</li>
</ul>

</dd><dt><code>--target</code> <i>target</i></dt><dd><p>Install packages into the specified directory, rather than into the virtual or system Python environment. The packages will be installed at the top-level of the directory</p>

</dd><dt><code>--prefix</code> <i>prefix</i></dt><dd><p>Install packages into <code>lib</code>, <code>bin</code>, and other top-level folders under the specified directory, as if a virtual environment were present at that location.</p>

<p>In general, prefer the use of <code>--python</code> to install into an alternate environment, as scripts and other artifacts installed via <code>--prefix</code> will reference the installing interpreter, rather than any interpreter added to the <code>--prefix</code> directory, rendering them non-portable.</p>

</dd><dt><code>--no-binary</code> <i>no-binary</i></dt><dd><p>Don&#8217;t install pre-built wheels.</p>

<p>The given packages will be built and installed from source. The resolver will still use pre-built wheels to extract package metadata, if available.</p>

<p>Multiple packages may be provided. Disable binaries for all packages with <code>:all:</code>. Clear previously specified packages with <code>:none:</code>.</p>

</dd><dt><code>--only-binary</code> <i>only-binary</i></dt><dd><p>Only use pre-built wheels; don&#8217;t build source distributions.</p>

<p>When enabled, resolving will not run code from the given packages. The cached wheels of already-built source distributions will be reused, but operations that require building distributions will exit with an error.</p>

<p>Multiple packages may be provided. Disable binaries for all packages with <code>:all:</code>. Clear previously specified packages with <code>:none:</code>.</p>

</dd><dt><code>--python-version</code> <i>python-version</i></dt><dd><p>The minimum Python version that should be supported by the requirements (e.g., <code>3.7</code> or <code>3.7.9</code>).</p>

<p>If a patch version is omitted, the minimum patch version is assumed. For example, <code>3.7</code> is mapped to <code>3.7.0</code>.</p>

</dd><dt><code>--python-platform</code> <i>python-platform</i></dt><dd><p>The platform for which requirements should be installed.</p>

<p>Represented as a &quot;target triple&quot;, a string that describes the target platform in terms of its CPU, vendor, and operating system name, like <code>x86_64-unknown-linux-gnu</code> or <code>aaarch64-apple-darwin</code>.</p>

<p>WARNING: When specified, uv will select wheels that are compatible with the <em>target</em> platform; as a result, the installed distributions may not be compatible with the <em>current</em> platform. Conversely, any distributions that are built from source may be incompatible with the <em>target</em> platform, as they will be built for the <em>current</em> platform. The <code>--python-platform</code> option is intended for advanced use cases.</p>

</dd><dt><code>--cache-dir</code> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>

<p>Defaults to <code>$HOME/Library/Caches/uv</code> on macOS, <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on Linux, and <code>{FOLDERID_LocalAppData}\uv\cache</code> on Windows.</p>

</dd><dt><code>--python-preference</code> <i>python-preference</i></dt><dd><p>Whether to prefer using Python installations that are already present on the system, or those that are downloaded and installed by uv</p>

</dd><dt><code>--python-fetch</code> <i>python-fetch</i></dt><dd><p>Whether to automatically download Python when required</p>

</dd><dt><code>--color</code> <i>color-choice</i></dt><dd><p>Control colors in output</p>

</dd><dt><code>--config-file</code> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration</p>

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

<dl class="cli-reference"><dt><code>--requirement</code>, <code>-r</code> <i>requirement</i></dt><dd><p>Install all packages listed in the given <code>requirements.txt</code> files.</p>

<p>If a <code>pyproject.toml</code>, <code>setup.py</code>, or <code>setup.cfg</code> file is provided, uv will extract the requirements for the relevant project.</p>

<p>If <code>-</code> is provided, then requirements will be read from stdin.</p>

</dd><dt><code>--editable</code>, <code>-e</code> <i>editable</i></dt><dd><p>Install the editable package based on the provided local file path</p>

</dd><dt><code>--constraint</code>, <code>-c</code> <i>constraint</i></dt><dd><p>Constrain versions using the given requirements files.</p>

<p>Constraints files are <code>requirements.txt</code>-like files that only control the <em>version</em> of a requirement that&#8217;s installed. However, including a package in a constraints file will <em>not</em> trigger the installation of that package.</p>

<p>This is equivalent to pip&#8217;s <code>--constraint</code> option.</p>

</dd><dt><code>--override</code> <i>override</i></dt><dd><p>Override versions using the given requirements files.</p>

<p>Overrides files are <code>requirements.txt</code>-like files that force a specific version of a requirement to be installed, regardless of the requirements declared by any constituent package, and regardless of whether this would be considered an invalid resolution.</p>

<p>While constraints are <em>additive</em>, in that they&#8217;re combined with the requirements of the constituent packages, overrides are <em>absolute</em>, in that they completely replace the requirements of the constituent packages.</p>

</dd><dt><code>--extra</code> <i>extra</i></dt><dd><p>Include optional dependencies from the extra group name; may be provided more than once.</p>

<p>Only applies to <code>pyproject.toml</code>, <code>setup.py</code>, and <code>setup.cfg</code> sources.</p>

</dd><dt><code>--index-url</code>, <code>-i</code> <i>index-url</i></dt><dd><p>The URL of the Python package index (by default: &lt;https://pypi.org/simple&gt;).</p>

<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>

<p>The index given by this flag is given lower priority than all other indexes specified via the <code>--extra-index-url</code> flag.</p>

</dd><dt><code>--extra-index-url</code> <i>extra-index-url</i></dt><dd><p>Extra URLs of package indexes to use, in addition to <code>--index-url</code>.</p>

<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>

<p>All indexes provided via this flag take priority over the index specified by <code>--index-url</code> (which defaults to PyPI). When multiple <code>--extra-index-url</code> flags are provided, earlier values take priority.</p>

</dd><dt><code>--find-links</code>, <code>-f</code> <i>find-links</i></dt><dd><p>Locations to search for candidate distributions, in addition to those found in the registry indexes.</p>

<p>If a path, the target must be a directory that contains packages as wheel files (<code>.whl</code>) or source distributions (<code>.tar.gz</code> or <code>.zip</code>) at the top level.</p>

<p>If a URL, the page must contain a flat list of links to package files adhering to the formats described above.</p>

</dd><dt><code>--upgrade-package</code>, <code>-P</code> <i>upgrade-package</i></dt><dd><p>Allow upgrades for a specific package, ignoring pinned versions in any existing output file</p>

</dd><dt><code>--reinstall-package</code> <i>reinstall-package</i></dt><dd><p>Reinstall a specific package, regardless of whether it&#8217;s already installed. Implies <code>--refresh-package</code></p>

</dd><dt><code>--index-strategy</code> <i>index-strategy</i></dt><dd><p>The strategy to use when resolving against multiple index URLs.</p>

<p>By default, uv will stop at the first index on which a given package is available, and limit resolutions to those present on that first index (<code>first-match</code>). This prevents &quot;dependency confusion&quot; attacks, whereby an attack can upload a malicious package under the same name to a secondary.</p>

</dd><dt><code>--keyring-provider</code> <i>keyring-provider</i></dt><dd><p>Attempt to use <code>keyring</code> for authentication for index URLs.</p>

<p>At present, only <code>--keyring-provider subprocess</code> is supported, which configures uv to use the <code>keyring</code> CLI to handle authentication.</p>

<p>Defaults to <code>disabled</code>.</p>

</dd><dt><code>--resolution</code> <i>resolution</i></dt><dd><p>The strategy to use when selecting between the different compatible versions for a given package requirement.</p>

<p>By default, uv will use the latest compatible version of each package (<code>highest</code>).</p>

</dd><dt><code>--prerelease</code> <i>prerelease</i></dt><dd><p>The strategy to use when considering pre-release versions.</p>

<p>By default, uv will accept pre-releases for packages that <em>only</em> publish pre-releases, along with first-party requirements that contain an explicit pre-release marker in the declared specifiers (<code>if-necessary-or-explicit</code>).</p>

</dd><dt><code>--config-setting</code>, <code>-C</code> <i>config-setting</i></dt><dd><p>Settings to pass to the PEP 517 build backend, specified as <code>KEY=VALUE</code> pairs</p>

</dd><dt><code>--exclude-newer</code> <i>exclude-newer</i></dt><dd><p>Limit candidate packages to those that were uploaded prior to the given date.</p>

<p>Accepts both RFC 3339 timestamps (e.g., <code>2006-12-02T02:07:43Z</code>) and UTC dates in the same format (e.g., <code>2006-12-02</code>).</p>

</dd><dt><code>--link-mode</code> <i>link-mode</i></dt><dd><p>The method to use when installing packages from the global cache.</p>

<p>Defaults to <code>clone</code> (also known as Copy-on-Write) on macOS, and <code>hardlink</code> on Linux and Windows.</p>

</dd><dt><code>--refresh-package</code> <i>refresh-package</i></dt><dd><p>Refresh cached data for a specific package</p>

</dd><dt><code>--python</code>, <code>-p</code> <i>python</i></dt><dd><p>The Python interpreter into which packages should be installed.</p>

<p>By default, uv installs into the virtual environment in the current working directory or any parent directory. The <code>--python</code> option allows you to specify a different interpreter, which is intended for use in continuous integration (CI) environments or other automated workflows.</p>

<p>Supported formats:</p>

<ul>
<li><code>3.10</code> looks for an installed Python 3.10 using <code>py --list-paths</code> on Windows, or <code>python3.10</code> on Linux and macOS.</li>

<li><code>python3.10</code> or <code>python.exe</code> looks for a binary with the given name in <code>PATH</code>.</li>

<li><code>/home/ferris/.local/bin/python3.10</code> uses the exact Python at the given path.</li>
</ul>

</dd><dt><code>--target</code> <i>target</i></dt><dd><p>Install packages into the specified directory, rather than into the virtual or system Python environment. The packages will be installed at the top-level of the directory</p>

</dd><dt><code>--prefix</code> <i>prefix</i></dt><dd><p>Install packages into <code>lib</code>, <code>bin</code>, and other top-level folders under the specified directory, as if a virtual environment were present at that location.</p>

<p>In general, prefer the use of <code>--python</code> to install into an alternate environment, as scripts and other artifacts installed via <code>--prefix</code> will reference the installing interpreter, rather than any interpreter added to the <code>--prefix</code> directory, rendering them non-portable.</p>

</dd><dt><code>--no-binary</code> <i>no-binary</i></dt><dd><p>Don&#8217;t install pre-built wheels.</p>

<p>The given packages will be built and installed from source. The resolver will still use pre-built wheels to extract package metadata, if available.</p>

<p>Multiple packages may be provided. Disable binaries for all packages with <code>:all:</code>. Clear previously specified packages with <code>:none:</code>.</p>

</dd><dt><code>--only-binary</code> <i>only-binary</i></dt><dd><p>Only use pre-built wheels; don&#8217;t build source distributions.</p>

<p>When enabled, resolving will not run code from the given packages. The cached wheels of already-built source distributions will be reused, but operations that require building distributions will exit with an error.</p>

<p>Multiple packages may be provided. Disable binaries for all packages with <code>:all:</code>. Clear previously specified packages with <code>:none:</code>.</p>

</dd><dt><code>--python-version</code> <i>python-version</i></dt><dd><p>The minimum Python version that should be supported by the requirements (e.g., <code>3.7</code> or <code>3.7.9</code>).</p>

<p>If a patch version is omitted, the minimum patch version is assumed. For example, <code>3.7</code> is mapped to <code>3.7.0</code>.</p>

</dd><dt><code>--python-platform</code> <i>python-platform</i></dt><dd><p>The platform for which requirements should be installed.</p>

<p>Represented as a &quot;target triple&quot;, a string that describes the target platform in terms of its CPU, vendor, and operating system name, like <code>x86_64-unknown-linux-gnu</code> or <code>aaarch64-apple-darwin</code>.</p>

<p>WARNING: When specified, uv will select wheels that are compatible with the <em>target</em> platform; as a result, the installed distributions may not be compatible with the <em>current</em> platform. Conversely, any distributions that are built from source may be incompatible with the <em>target</em> platform, as they will be built for the <em>current</em> platform. The <code>--python-platform</code> option is intended for advanced use cases.</p>

</dd><dt><code>--cache-dir</code> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>

<p>Defaults to <code>$HOME/Library/Caches/uv</code> on macOS, <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on Linux, and <code>{FOLDERID_LocalAppData}\uv\cache</code> on Windows.</p>

</dd><dt><code>--python-preference</code> <i>python-preference</i></dt><dd><p>Whether to prefer using Python installations that are already present on the system, or those that are downloaded and installed by uv</p>

</dd><dt><code>--python-fetch</code> <i>python-fetch</i></dt><dd><p>Whether to automatically download Python when required</p>

</dd><dt><code>--color</code> <i>color-choice</i></dt><dd><p>Control colors in output</p>

</dd><dt><code>--config-file</code> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration</p>

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

<dl class="cli-reference"><dt><code>--requirement</code>, <code>-r</code> <i>requirement</i></dt><dd><p>Uninstall all packages listed in the given requirements files</p>

</dd><dt><code>--python</code>, <code>-p</code> <i>python</i></dt><dd><p>The Python interpreter from which packages should be uninstalled.</p>

<p>By default, uv uninstalls from the virtual environment in the current working directory or any parent directory. The <code>--python</code> option allows you to specify a different interpreter, which is intended for use in continuous integration (CI) environments or other automated workflows.</p>

<p>Supported formats:</p>

<ul>
<li><code>3.10</code> looks for an installed Python 3.10 using <code>py --list-paths</code> on Windows, or <code>python3.10</code> on Linux and macOS.</li>

<li><code>python3.10</code> or <code>python.exe</code> looks for a binary with the given name in <code>PATH</code>.</li>

<li><code>/home/ferris/.local/bin/python3.10</code> uses the exact Python at the given path.</li>
</ul>

</dd><dt><code>--keyring-provider</code> <i>keyring-provider</i></dt><dd><p>Attempt to use <code>keyring</code> for authentication for remote requirements files.</p>

<p>At present, only <code>--keyring-provider subprocess</code> is supported, which configures uv to use the <code>keyring</code> CLI to handle authentication.</p>

<p>Defaults to <code>disabled</code>.</p>

</dd><dt><code>--target</code> <i>target</i></dt><dd><p>Uninstall packages from the specified <code>--target</code> directory</p>

</dd><dt><code>--prefix</code> <i>prefix</i></dt><dd><p>Uninstall packages from the specified <code>--prefix</code> directory</p>

</dd><dt><code>--cache-dir</code> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>

<p>Defaults to <code>$HOME/Library/Caches/uv</code> on macOS, <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on Linux, and <code>{FOLDERID_LocalAppData}\uv\cache</code> on Windows.</p>

</dd><dt><code>--python-preference</code> <i>python-preference</i></dt><dd><p>Whether to prefer using Python installations that are already present on the system, or those that are downloaded and installed by uv</p>

</dd><dt><code>--python-fetch</code> <i>python-fetch</i></dt><dd><p>Whether to automatically download Python when required</p>

</dd><dt><code>--color</code> <i>color-choice</i></dt><dd><p>Control colors in output</p>

</dd><dt><code>--config-file</code> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration</p>

</dd></dl>

### uv pip freeze

List, in requirements format, packages installed in an environment

<h3 class="cli-reference">Usage</h3>

```
uv pip freeze [OPTIONS]
```

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt><code>--python</code>, <code>-p</code> <i>python</i></dt><dd><p>The Python interpreter for which packages should be listed.</p>

<p>By default, uv lists packages in the currently activated virtual environment, or a virtual environment (<code>.venv</code>) located in the current working directory or any parent directory, falling back to the system Python if no virtual environment is found.</p>

<p>Supported formats:</p>

<ul>
<li><code>3.10</code> looks for an installed Python 3.10 using <code>py --list-paths</code> on Windows, or <code>python3.10</code> on Linux and macOS.</li>

<li><code>python3.10</code> or <code>python.exe</code> looks for a binary with the given name in <code>PATH</code>.</li>

<li><code>/home/ferris/.local/bin/python3.10</code> uses the exact Python at the given path.</li>
</ul>

</dd><dt><code>--cache-dir</code> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>

<p>Defaults to <code>$HOME/Library/Caches/uv</code> on macOS, <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on Linux, and <code>{FOLDERID_LocalAppData}\uv\cache</code> on Windows.</p>

</dd><dt><code>--python-preference</code> <i>python-preference</i></dt><dd><p>Whether to prefer using Python installations that are already present on the system, or those that are downloaded and installed by uv</p>

</dd><dt><code>--python-fetch</code> <i>python-fetch</i></dt><dd><p>Whether to automatically download Python when required</p>

</dd><dt><code>--color</code> <i>color-choice</i></dt><dd><p>Control colors in output</p>

</dd><dt><code>--config-file</code> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration</p>

</dd></dl>

### uv pip list

List, in tabular format, packages installed in an environment

<h3 class="cli-reference">Usage</h3>

```
uv pip list [OPTIONS]
```

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt><code>--exclude</code> <i>exclude</i></dt><dd><p>Exclude the specified package(s) from the output</p>

</dd><dt><code>--format</code> <i>format</i></dt><dd><p>Select the output format between: <code>columns</code> (default), <code>freeze</code>, or <code>json</code></p>

</dd><dt><code>--python</code>, <code>-p</code> <i>python</i></dt><dd><p>The Python interpreter for which packages should be listed.</p>

<p>By default, uv lists packages in the currently activated virtual environment, or a virtual environment (<code>.venv</code>) located in the current working directory or any parent directory, falling back to the system Python if no virtual environment is found.</p>

<p>Supported formats:</p>

<ul>
<li><code>3.10</code> looks for an installed Python 3.10 using <code>py --list-paths</code> on Windows, or <code>python3.10</code> on Linux and macOS.</li>

<li><code>python3.10</code> or <code>python.exe</code> looks for a binary with the given name in <code>PATH</code>.</li>

<li><code>/home/ferris/.local/bin/python3.10</code> uses the exact Python at the given path.</li>
</ul>

</dd><dt><code>--cache-dir</code> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>

<p>Defaults to <code>$HOME/Library/Caches/uv</code> on macOS, <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on Linux, and <code>{FOLDERID_LocalAppData}\uv\cache</code> on Windows.</p>

</dd><dt><code>--python-preference</code> <i>python-preference</i></dt><dd><p>Whether to prefer using Python installations that are already present on the system, or those that are downloaded and installed by uv</p>

</dd><dt><code>--python-fetch</code> <i>python-fetch</i></dt><dd><p>Whether to automatically download Python when required</p>

</dd><dt><code>--color</code> <i>color-choice</i></dt><dd><p>Control colors in output</p>

</dd><dt><code>--config-file</code> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration</p>

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

<dl class="cli-reference"><dt><code>--python</code>, <code>-p</code> <i>python</i></dt><dd><p>The Python interpreter for which packages should be listed.</p>

<p>By default, uv lists packages in the currently activated virtual environment, or a virtual environment (<code>.venv</code>) located in the current working directory or any parent directory, falling back to the system Python if no virtual environment is found.</p>

<p>Supported formats:</p>

<ul>
<li><code>3.10</code> looks for an installed Python 3.10 using <code>py --list-paths</code> on Windows, or <code>python3.10</code> on Linux and macOS.</li>

<li><code>python3.10</code> or <code>python.exe</code> looks for a binary with the given name in <code>PATH</code>.</li>

<li><code>/home/ferris/.local/bin/python3.10</code> uses the exact Python at the given path.</li>
</ul>

</dd><dt><code>--cache-dir</code> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>

<p>Defaults to <code>$HOME/Library/Caches/uv</code> on macOS, <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on Linux, and <code>{FOLDERID_LocalAppData}\uv\cache</code> on Windows.</p>

</dd><dt><code>--python-preference</code> <i>python-preference</i></dt><dd><p>Whether to prefer using Python installations that are already present on the system, or those that are downloaded and installed by uv</p>

</dd><dt><code>--python-fetch</code> <i>python-fetch</i></dt><dd><p>Whether to automatically download Python when required</p>

</dd><dt><code>--color</code> <i>color-choice</i></dt><dd><p>Control colors in output</p>

</dd><dt><code>--config-file</code> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration</p>

</dd></dl>

### uv pip tree

Display the dependency tree for an environment

<h3 class="cli-reference">Usage</h3>

```
uv pip tree [OPTIONS]
```

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt><code>--depth</code>, <code>-d</code> <i>depth</i></dt><dd><p>Maximum display depth of the dependency tree</p>

</dd><dt><code>--prune</code> <i>prune</i></dt><dd><p>Prune the given package from the display of the dependency tree</p>

</dd><dt><code>--package</code> <i>package</i></dt><dd><p>Display only the specified packages</p>

</dd><dt><code>--python</code>, <code>-p</code> <i>python</i></dt><dd><p>The Python interpreter for which packages should be listed.</p>

<p>By default, uv lists packages in the currently activated virtual environment, or a virtual environment (<code>.venv</code>) located in the current working directory or any parent directory, falling back to the system Python if no virtual environment is found.</p>

<p>Supported formats:</p>

<ul>
<li><code>3.10</code> looks for an installed Python 3.10 using <code>py --list-paths</code> on Windows, or <code>python3.10</code> on Linux and macOS.</li>

<li><code>python3.10</code> or <code>python.exe</code> looks for a binary with the given name in <code>PATH</code>.</li>

<li><code>/home/ferris/.local/bin/python3.10</code> uses the exact Python at the given path.</li>
</ul>

</dd><dt><code>--cache-dir</code> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>

<p>Defaults to <code>$HOME/Library/Caches/uv</code> on macOS, <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on Linux, and <code>{FOLDERID_LocalAppData}\uv\cache</code> on Windows.</p>

</dd><dt><code>--python-preference</code> <i>python-preference</i></dt><dd><p>Whether to prefer using Python installations that are already present on the system, or those that are downloaded and installed by uv</p>

</dd><dt><code>--python-fetch</code> <i>python-fetch</i></dt><dd><p>Whether to automatically download Python when required</p>

</dd><dt><code>--color</code> <i>color-choice</i></dt><dd><p>Control colors in output</p>

</dd><dt><code>--config-file</code> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration</p>

</dd></dl>

### uv pip check

Verify installed packages have compatible dependencies

<h3 class="cli-reference">Usage</h3>

```
uv pip check [OPTIONS]
```

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt><code>--python</code>, <code>-p</code> <i>python</i></dt><dd><p>The Python interpreter for which packages should be listed.</p>

<p>By default, uv lists packages in the currently activated virtual environment, or a virtual environment (<code>.venv</code>) located in the current working directory or any parent directory, falling back to the system Python if no virtual environment is found.</p>

<p>Supported formats:</p>

<ul>
<li><code>3.10</code> looks for an installed Python 3.10 using <code>py --list-paths</code> on Windows, or <code>python3.10</code> on Linux and macOS.</li>

<li><code>python3.10</code> or <code>python.exe</code> looks for a binary with the given name in <code>PATH</code>.</li>

<li><code>/home/ferris/.local/bin/python3.10</code> uses the exact Python at the given path.</li>
</ul>

</dd><dt><code>--cache-dir</code> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>

<p>Defaults to <code>$HOME/Library/Caches/uv</code> on macOS, <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on Linux, and <code>{FOLDERID_LocalAppData}\uv\cache</code> on Windows.</p>

</dd><dt><code>--python-preference</code> <i>python-preference</i></dt><dd><p>Whether to prefer using Python installations that are already present on the system, or those that are downloaded and installed by uv</p>

</dd><dt><code>--python-fetch</code> <i>python-fetch</i></dt><dd><p>Whether to automatically download Python when required</p>

</dd><dt><code>--color</code> <i>color-choice</i></dt><dd><p>Control colors in output</p>

</dd><dt><code>--config-file</code> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration</p>

</dd></dl>

## uv tool

Run and manage executable Python packages

<h3 class="cli-reference">Usage</h3>

```
uv tool [OPTIONS] <COMMAND>
```

<h3 class="cli-reference">Commands</h3>

<dl class="cli-reference"><dt><a href="uv-tool-run"><code>uv tool run</code></a></dt><dd><p>Run a tool</p>
</dd>
<dt><a href="uv-tool-install"><code>uv tool install</code></a></dt><dd><p>Install a tool</p>
</dd>
<dt><a href="uv-tool-list"><code>uv tool list</code></a></dt><dd><p>List installed tools</p>
</dd>
<dt><a href="uv-tool-uninstall"><code>uv tool uninstall</code></a></dt><dd><p>Uninstall a tool</p>
</dd>
<dt><a href="uv-tool-update-shell"><code>uv tool update-shell</code></a></dt><dd><p>Ensure that the tool executable directory is on <code>PATH</code></p>
</dd>
<dt><a href="uv-tool-dir"><code>uv tool dir</code></a></dt><dd><p>Show the tools directory</p>
</dd>
</dl>

### uv tool run

Run a tool

<h3 class="cli-reference">Usage</h3>

```
uv tool run [OPTIONS] [COMMAND]
```

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt><code>--from</code> <i>from</i></dt><dd><p>Use the given package to provide the command.</p>

<p>By default, the package name is assumed to match the command name.</p>

</dd><dt><code>--with</code> <i>with</i></dt><dd><p>Run with the given packages installed</p>

</dd><dt><code>--with-requirements</code> <i>with-requirements</i></dt><dd><p>Run with all packages listed in the given <code>requirements.txt</code> files</p>

</dd><dt><code>--index-url</code>, <code>-i</code> <i>index-url</i></dt><dd><p>The URL of the Python package index (by default: &lt;https://pypi.org/simple&gt;).</p>

<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>

<p>The index given by this flag is given lower priority than all other indexes specified via the <code>--extra-index-url</code> flag.</p>

</dd><dt><code>--extra-index-url</code> <i>extra-index-url</i></dt><dd><p>Extra URLs of package indexes to use, in addition to <code>--index-url</code>.</p>

<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>

<p>All indexes provided via this flag take priority over the index specified by <code>--index-url</code> (which defaults to PyPI). When multiple <code>--extra-index-url</code> flags are provided, earlier values take priority.</p>

</dd><dt><code>--find-links</code>, <code>-f</code> <i>find-links</i></dt><dd><p>Locations to search for candidate distributions, in addition to those found in the registry indexes.</p>

<p>If a path, the target must be a directory that contains packages as wheel files (<code>.whl</code>) or source distributions (<code>.tar.gz</code> or <code>.zip</code>) at the top level.</p>

<p>If a URL, the page must contain a flat list of links to package files adhering to the formats described above.</p>

</dd><dt><code>--upgrade-package</code>, <code>-P</code> <i>upgrade-package</i></dt><dd><p>Allow upgrades for a specific package, ignoring pinned versions in any existing output file</p>

</dd><dt><code>--reinstall-package</code> <i>reinstall-package</i></dt><dd><p>Reinstall a specific package, regardless of whether it&#8217;s already installed. Implies <code>--refresh-package</code></p>

</dd><dt><code>--index-strategy</code> <i>index-strategy</i></dt><dd><p>The strategy to use when resolving against multiple index URLs.</p>

<p>By default, uv will stop at the first index on which a given package is available, and limit resolutions to those present on that first index (<code>first-match</code>). This prevents &quot;dependency confusion&quot; attacks, whereby an attack can upload a malicious package under the same name to a secondary.</p>

</dd><dt><code>--keyring-provider</code> <i>keyring-provider</i></dt><dd><p>Attempt to use <code>keyring</code> for authentication for index URLs.</p>

<p>At present, only <code>--keyring-provider subprocess</code> is supported, which configures uv to use the <code>keyring</code> CLI to handle authentication.</p>

<p>Defaults to <code>disabled</code>.</p>

</dd><dt><code>--resolution</code> <i>resolution</i></dt><dd><p>The strategy to use when selecting between the different compatible versions for a given package requirement.</p>

<p>By default, uv will use the latest compatible version of each package (<code>highest</code>).</p>

</dd><dt><code>--prerelease</code> <i>prerelease</i></dt><dd><p>The strategy to use when considering pre-release versions.</p>

<p>By default, uv will accept pre-releases for packages that <em>only</em> publish pre-releases, along with first-party requirements that contain an explicit pre-release marker in the declared specifiers (<code>if-necessary-or-explicit</code>).</p>

</dd><dt><code>--config-setting</code>, <code>-C</code> <i>config-setting</i></dt><dd><p>Settings to pass to the PEP 517 build backend, specified as <code>KEY=VALUE</code> pairs</p>

</dd><dt><code>--exclude-newer</code> <i>exclude-newer</i></dt><dd><p>Limit candidate packages to those that were uploaded prior to the given date.</p>

<p>Accepts both RFC 3339 timestamps (e.g., <code>2006-12-02T02:07:43Z</code>) and UTC dates in the same format (e.g., <code>2006-12-02</code>).</p>

</dd><dt><code>--link-mode</code> <i>link-mode</i></dt><dd><p>The method to use when installing packages from the global cache.</p>

<p>Defaults to <code>clone</code> (also known as Copy-on-Write) on macOS, and <code>hardlink</code> on Linux and Windows.</p>

</dd><dt><code>--no-build-package</code> <i>no-build-package</i></dt><dd><p>Don&#8217;t build source distributions for a specific package</p>

</dd><dt><code>--no-binary-package</code> <i>no-binary-package</i></dt><dd><p>Don&#8217;t install pre-built wheels for a specific package</p>

</dd><dt><code>--refresh-package</code> <i>refresh-package</i></dt><dd><p>Refresh cached data for a specific package</p>

</dd><dt><code>--python</code>, <code>-p</code> <i>python</i></dt><dd><p>The Python interpreter to use to build the run environment.</p>

<p>By default, uv uses the virtual environment in the current working directory or any parent directory, falling back to searching for a Python executable in <code>PATH</code>. The <code>--python</code> option allows you to specify a different interpreter.</p>

<p>Supported formats:</p>

<ul>
<li><code>3.10</code> looks for an installed Python 3.10 using <code>py --list-paths</code> on Windows, or <code>python3.10</code> on Linux and macOS.</li>

<li><code>python3.10</code> or <code>python.exe</code> looks for a binary with the given name in <code>PATH</code>.</li>

<li><code>/home/ferris/.local/bin/python3.10</code> uses the exact Python at the given path.</li>
</ul>

</dd><dt><code>--cache-dir</code> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>

<p>Defaults to <code>$HOME/Library/Caches/uv</code> on macOS, <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on Linux, and <code>{FOLDERID_LocalAppData}\uv\cache</code> on Windows.</p>

</dd><dt><code>--python-preference</code> <i>python-preference</i></dt><dd><p>Whether to prefer using Python installations that are already present on the system, or those that are downloaded and installed by uv</p>

</dd><dt><code>--python-fetch</code> <i>python-fetch</i></dt><dd><p>Whether to automatically download Python when required</p>

</dd><dt><code>--color</code> <i>color-choice</i></dt><dd><p>Control colors in output</p>

</dd><dt><code>--config-file</code> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration</p>

</dd></dl>

### uv tool install

Install a tool

<h3 class="cli-reference">Usage</h3>

```
uv tool install [OPTIONS] <PACKAGE>
```

<h3 class="cli-reference">Arguments</h3>

<dl class="cli-reference"><dt><code>PACKAGE</code></dt><dd><p>The package to install commands from</p>

</dd></dl>

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt><code>--with</code> <i>with</i></dt><dd><p>Include the following extra requirements</p>

</dd><dt><code>--with-requirements</code> <i>with-requirements</i></dt><dd><p>Run all requirements listed in the given <code>requirements.txt</code> files</p>

</dd><dt><code>--index-url</code>, <code>-i</code> <i>index-url</i></dt><dd><p>The URL of the Python package index (by default: &lt;https://pypi.org/simple&gt;).</p>

<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>

<p>The index given by this flag is given lower priority than all other indexes specified via the <code>--extra-index-url</code> flag.</p>

</dd><dt><code>--extra-index-url</code> <i>extra-index-url</i></dt><dd><p>Extra URLs of package indexes to use, in addition to <code>--index-url</code>.</p>

<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>

<p>All indexes provided via this flag take priority over the index specified by <code>--index-url</code> (which defaults to PyPI). When multiple <code>--extra-index-url</code> flags are provided, earlier values take priority.</p>

</dd><dt><code>--find-links</code>, <code>-f</code> <i>find-links</i></dt><dd><p>Locations to search for candidate distributions, in addition to those found in the registry indexes.</p>

<p>If a path, the target must be a directory that contains packages as wheel files (<code>.whl</code>) or source distributions (<code>.tar.gz</code> or <code>.zip</code>) at the top level.</p>

<p>If a URL, the page must contain a flat list of links to package files adhering to the formats described above.</p>

</dd><dt><code>--upgrade-package</code>, <code>-P</code> <i>upgrade-package</i></dt><dd><p>Allow upgrades for a specific package, ignoring pinned versions in any existing output file</p>

</dd><dt><code>--reinstall-package</code> <i>reinstall-package</i></dt><dd><p>Reinstall a specific package, regardless of whether it&#8217;s already installed. Implies <code>--refresh-package</code></p>

</dd><dt><code>--index-strategy</code> <i>index-strategy</i></dt><dd><p>The strategy to use when resolving against multiple index URLs.</p>

<p>By default, uv will stop at the first index on which a given package is available, and limit resolutions to those present on that first index (<code>first-match</code>). This prevents &quot;dependency confusion&quot; attacks, whereby an attack can upload a malicious package under the same name to a secondary.</p>

</dd><dt><code>--keyring-provider</code> <i>keyring-provider</i></dt><dd><p>Attempt to use <code>keyring</code> for authentication for index URLs.</p>

<p>At present, only <code>--keyring-provider subprocess</code> is supported, which configures uv to use the <code>keyring</code> CLI to handle authentication.</p>

<p>Defaults to <code>disabled</code>.</p>

</dd><dt><code>--resolution</code> <i>resolution</i></dt><dd><p>The strategy to use when selecting between the different compatible versions for a given package requirement.</p>

<p>By default, uv will use the latest compatible version of each package (<code>highest</code>).</p>

</dd><dt><code>--prerelease</code> <i>prerelease</i></dt><dd><p>The strategy to use when considering pre-release versions.</p>

<p>By default, uv will accept pre-releases for packages that <em>only</em> publish pre-releases, along with first-party requirements that contain an explicit pre-release marker in the declared specifiers (<code>if-necessary-or-explicit</code>).</p>

</dd><dt><code>--config-setting</code>, <code>-C</code> <i>config-setting</i></dt><dd><p>Settings to pass to the PEP 517 build backend, specified as <code>KEY=VALUE</code> pairs</p>

</dd><dt><code>--exclude-newer</code> <i>exclude-newer</i></dt><dd><p>Limit candidate packages to those that were uploaded prior to the given date.</p>

<p>Accepts both RFC 3339 timestamps (e.g., <code>2006-12-02T02:07:43Z</code>) and UTC dates in the same format (e.g., <code>2006-12-02</code>).</p>

</dd><dt><code>--link-mode</code> <i>link-mode</i></dt><dd><p>The method to use when installing packages from the global cache.</p>

<p>Defaults to <code>clone</code> (also known as Copy-on-Write) on macOS, and <code>hardlink</code> on Linux and Windows.</p>

</dd><dt><code>--no-build-package</code> <i>no-build-package</i></dt><dd><p>Don&#8217;t build source distributions for a specific package</p>

</dd><dt><code>--no-binary-package</code> <i>no-binary-package</i></dt><dd><p>Don&#8217;t install pre-built wheels for a specific package</p>

</dd><dt><code>--refresh-package</code> <i>refresh-package</i></dt><dd><p>Refresh cached data for a specific package</p>

</dd><dt><code>--python</code>, <code>-p</code> <i>python</i></dt><dd><p>The Python interpreter to use to build the tool environment.</p>

<p>By default, uv will search for a Python executable in the <code>PATH</code>. uv ignores virtual environments while looking for interpreter for tools. The <code>--python</code> option allows you to specify a different interpreter.</p>

<p>Supported formats:</p>

<ul>
<li><code>3.10</code> looks for an installed Python 3.10 using <code>py --list-paths</code> on Windows, or <code>python3.10</code> on Linux and macOS.</li>

<li><code>python3.10</code> or <code>python.exe</code> looks for a binary with the given name in <code>PATH</code>.</li>

<li><code>/home/ferris/.local/bin/python3.10</code> uses the exact Python at the given path.</li>
</ul>

</dd><dt><code>--cache-dir</code> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>

<p>Defaults to <code>$HOME/Library/Caches/uv</code> on macOS, <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on Linux, and <code>{FOLDERID_LocalAppData}\uv\cache</code> on Windows.</p>

</dd><dt><code>--python-preference</code> <i>python-preference</i></dt><dd><p>Whether to prefer using Python installations that are already present on the system, or those that are downloaded and installed by uv</p>

</dd><dt><code>--python-fetch</code> <i>python-fetch</i></dt><dd><p>Whether to automatically download Python when required</p>

</dd><dt><code>--color</code> <i>color-choice</i></dt><dd><p>Control colors in output</p>

</dd><dt><code>--config-file</code> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration</p>

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

</dd><dt><code>--python-preference</code> <i>python-preference</i></dt><dd><p>Whether to prefer using Python installations that are already present on the system, or those that are downloaded and installed by uv</p>

</dd><dt><code>--python-fetch</code> <i>python-fetch</i></dt><dd><p>Whether to automatically download Python when required</p>

</dd><dt><code>--color</code> <i>color-choice</i></dt><dd><p>Control colors in output</p>

</dd><dt><code>--config-file</code> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration</p>

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

<dl class="cli-reference"><dt><code>--cache-dir</code> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>

<p>Defaults to <code>$HOME/Library/Caches/uv</code> on macOS, <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on Linux, and <code>{FOLDERID_LocalAppData}\uv\cache</code> on Windows.</p>

</dd><dt><code>--python-preference</code> <i>python-preference</i></dt><dd><p>Whether to prefer using Python installations that are already present on the system, or those that are downloaded and installed by uv</p>

</dd><dt><code>--python-fetch</code> <i>python-fetch</i></dt><dd><p>Whether to automatically download Python when required</p>

</dd><dt><code>--color</code> <i>color-choice</i></dt><dd><p>Control colors in output</p>

</dd><dt><code>--config-file</code> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration</p>

</dd></dl>

### uv tool update-shell

Ensure that the tool executable directory is on `PATH`

<h3 class="cli-reference">Usage</h3>

```
uv tool update-shell [OPTIONS]
```

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt><code>--cache-dir</code> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>

<p>Defaults to <code>$HOME/Library/Caches/uv</code> on macOS, <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on Linux, and <code>{FOLDERID_LocalAppData}\uv\cache</code> on Windows.</p>

</dd><dt><code>--python-preference</code> <i>python-preference</i></dt><dd><p>Whether to prefer using Python installations that are already present on the system, or those that are downloaded and installed by uv</p>

</dd><dt><code>--python-fetch</code> <i>python-fetch</i></dt><dd><p>Whether to automatically download Python when required</p>

</dd><dt><code>--color</code> <i>color-choice</i></dt><dd><p>Control colors in output</p>

</dd><dt><code>--config-file</code> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration</p>

</dd></dl>

### uv tool dir

Show the tools directory

<h3 class="cli-reference">Usage</h3>

```
uv tool dir [OPTIONS]
```

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt><code>--cache-dir</code> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>

<p>Defaults to <code>$HOME/Library/Caches/uv</code> on macOS, <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on Linux, and <code>{FOLDERID_LocalAppData}\uv\cache</code> on Windows.</p>

</dd><dt><code>--python-preference</code> <i>python-preference</i></dt><dd><p>Whether to prefer using Python installations that are already present on the system, or those that are downloaded and installed by uv</p>

</dd><dt><code>--python-fetch</code> <i>python-fetch</i></dt><dd><p>Whether to automatically download Python when required</p>

</dd><dt><code>--color</code> <i>color-choice</i></dt><dd><p>Control colors in output</p>

</dd><dt><code>--config-file</code> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration</p>

</dd></dl>

## uv python

Manage Python installations

<h3 class="cli-reference">Usage</h3>

```
uv python [OPTIONS] <COMMAND>
```

<h3 class="cli-reference">Commands</h3>

<dl class="cli-reference"><dt><a href="uv-python-list"><code>uv python list</code></a></dt><dd><p>List the available Python installations</p>
</dd>
<dt><a href="uv-python-install"><code>uv python install</code></a></dt><dd><p>Download and install Python versions</p>
</dd>
<dt><a href="uv-python-find"><code>uv python find</code></a></dt><dd><p>Search for a Python installation</p>
</dd>
<dt><a href="uv-python-pin"><code>uv python pin</code></a></dt><dd><p>Pin to a specific Python version</p>
</dd>
<dt><a href="uv-python-dir"><code>uv python dir</code></a></dt><dd><p>Show the uv Python installation directory</p>
</dd>
<dt><a href="uv-python-uninstall"><code>uv python uninstall</code></a></dt><dd><p>Uninstall Python versions</p>
</dd>
</dl>

### uv python list

List the available Python installations

<h3 class="cli-reference">Usage</h3>

```
uv python list [OPTIONS]
```

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt><code>--cache-dir</code> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>

<p>Defaults to <code>$HOME/Library/Caches/uv</code> on macOS, <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on Linux, and <code>{FOLDERID_LocalAppData}\uv\cache</code> on Windows.</p>

</dd><dt><code>--python-preference</code> <i>python-preference</i></dt><dd><p>Whether to prefer using Python installations that are already present on the system, or those that are downloaded and installed by uv</p>

</dd><dt><code>--python-fetch</code> <i>python-fetch</i></dt><dd><p>Whether to automatically download Python when required</p>

</dd><dt><code>--color</code> <i>color-choice</i></dt><dd><p>Control colors in output</p>

</dd><dt><code>--config-file</code> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration</p>

</dd></dl>

### uv python install

Download and install Python versions

<h3 class="cli-reference">Usage</h3>

```
uv python install [OPTIONS] [TARGETS]...
```

<h3 class="cli-reference">Arguments</h3>

<dl class="cli-reference"><dt><code>TARGETS</code></dt><dd><p>The Python version(s) to install.</p>

<p>If not provided, the requested Python version(s) will be read from the <code>.python-versions</code> or <code>.python-version</code> files. If neither file is present, uv will check if it has installed any Python versions. If not, it will install the latest stable version of Python.</p>

</dd></dl>

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt><code>--cache-dir</code> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>

<p>Defaults to <code>$HOME/Library/Caches/uv</code> on macOS, <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on Linux, and <code>{FOLDERID_LocalAppData}\uv\cache</code> on Windows.</p>

</dd><dt><code>--python-preference</code> <i>python-preference</i></dt><dd><p>Whether to prefer using Python installations that are already present on the system, or those that are downloaded and installed by uv</p>

</dd><dt><code>--python-fetch</code> <i>python-fetch</i></dt><dd><p>Whether to automatically download Python when required</p>

</dd><dt><code>--color</code> <i>color-choice</i></dt><dd><p>Control colors in output</p>

</dd><dt><code>--config-file</code> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration</p>

</dd></dl>

### uv python find

Search for a Python installation

<h3 class="cli-reference">Usage</h3>

```
uv python find [OPTIONS] [REQUEST]
```

<h3 class="cli-reference">Arguments</h3>

<dl class="cli-reference"><dt><code>REQUEST</code></dt><dd><p>The Python request</p>

</dd></dl>

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt><code>--cache-dir</code> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>

<p>Defaults to <code>$HOME/Library/Caches/uv</code> on macOS, <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on Linux, and <code>{FOLDERID_LocalAppData}\uv\cache</code> on Windows.</p>

</dd><dt><code>--python-preference</code> <i>python-preference</i></dt><dd><p>Whether to prefer using Python installations that are already present on the system, or those that are downloaded and installed by uv</p>

</dd><dt><code>--python-fetch</code> <i>python-fetch</i></dt><dd><p>Whether to automatically download Python when required</p>

</dd><dt><code>--color</code> <i>color-choice</i></dt><dd><p>Control colors in output</p>

</dd><dt><code>--config-file</code> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration</p>

</dd></dl>

### uv python pin

Pin to a specific Python version

<h3 class="cli-reference">Usage</h3>

```
uv python pin [OPTIONS] [REQUEST]
```

<h3 class="cli-reference">Arguments</h3>

<dl class="cli-reference"><dt><code>REQUEST</code></dt><dd><p>The Python version</p>

</dd></dl>

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt><code>--cache-dir</code> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>

<p>Defaults to <code>$HOME/Library/Caches/uv</code> on macOS, <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on Linux, and <code>{FOLDERID_LocalAppData}\uv\cache</code> on Windows.</p>

</dd><dt><code>--python-preference</code> <i>python-preference</i></dt><dd><p>Whether to prefer using Python installations that are already present on the system, or those that are downloaded and installed by uv</p>

</dd><dt><code>--python-fetch</code> <i>python-fetch</i></dt><dd><p>Whether to automatically download Python when required</p>

</dd><dt><code>--color</code> <i>color-choice</i></dt><dd><p>Control colors in output</p>

</dd><dt><code>--config-file</code> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration</p>

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

</dd><dt><code>--python-preference</code> <i>python-preference</i></dt><dd><p>Whether to prefer using Python installations that are already present on the system, or those that are downloaded and installed by uv</p>

</dd><dt><code>--python-fetch</code> <i>python-fetch</i></dt><dd><p>Whether to automatically download Python when required</p>

</dd><dt><code>--color</code> <i>color-choice</i></dt><dd><p>Control colors in output</p>

</dd><dt><code>--config-file</code> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration</p>

</dd></dl>

### uv python uninstall

Uninstall Python versions

<h3 class="cli-reference">Usage</h3>

```
uv python uninstall [OPTIONS] <TARGETS>...
```

<h3 class="cli-reference">Arguments</h3>

<dl class="cli-reference"><dt><code>TARGETS</code></dt><dd><p>The Python version(s) to uninstall</p>

</dd></dl>

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt><code>--cache-dir</code> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>

<p>Defaults to <code>$HOME/Library/Caches/uv</code> on macOS, <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on Linux, and <code>{FOLDERID_LocalAppData}\uv\cache</code> on Windows.</p>

</dd><dt><code>--python-preference</code> <i>python-preference</i></dt><dd><p>Whether to prefer using Python installations that are already present on the system, or those that are downloaded and installed by uv</p>

</dd><dt><code>--python-fetch</code> <i>python-fetch</i></dt><dd><p>Whether to automatically download Python when required</p>

</dd><dt><code>--color</code> <i>color-choice</i></dt><dd><p>Control colors in output</p>

</dd><dt><code>--config-file</code> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration</p>

</dd></dl>

## uv venv

Create a virtual environment

<h3 class="cli-reference">Usage</h3>

```
uv venv [OPTIONS] [NAME]
```

<h3 class="cli-reference">Arguments</h3>

<dl class="cli-reference"><dt><code>NAME</code></dt><dd><p>The path to the virtual environment to create</p>

</dd></dl>

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt><code>--python</code>, <code>-p</code> <i>python</i></dt><dd><p>The Python interpreter to use for the virtual environment.</p>

<p>Supported formats:</p>

<ul>
<li><code>3.10</code> looks for an installed Python 3.10 using <code>py --list-paths</code> on Windows, or <code>python3.10</code> on Linux and macOS.</li>

<li><code>python3.10</code> or <code>python.exe</code> looks for a binary with the given name in <code>PATH</code>.</li>

<li><code>/home/ferris/.local/bin/python3.10</code> uses the exact Python at the given path.</li>
</ul>

<p>Note that this is different from <code>--python-version</code> in <code>pip compile</code>, which takes <code>3.10</code> or <code>3.10.13</code> and doesn&#8217;t look for a Python interpreter on disk.</p>

</dd><dt><code>--prompt</code> <i>prompt</i></dt><dd><p>Provide an alternative prompt prefix for the virtual environment.</p>

<p>The default behavior depends on whether the virtual environment path is provided:</p>

<ul>
<li>If provided (<code>uv venv project</code>), the prompt is set to the virtual environment&#8217;s directory name.</li>

<li>If not provided (<code>uv venv</code>), the prompt is set to the current directory&#8217;s name.</li>
</ul>

<p>Possible values:</p>

<ul>
<li><code>.</code>: Use the current directory name.</li>

<li>Any string: Use the given string.</li>
</ul>

</dd><dt><code>--index-url</code>, <code>-i</code> <i>index-url</i></dt><dd><p>The URL of the Python package index (by default: &lt;https://pypi.org/simple&gt;).</p>

<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>

<p>The index given by this flag is given lower priority than all other indexes specified via the <code>--extra-index-url</code> flag.</p>

</dd><dt><code>--extra-index-url</code> <i>extra-index-url</i></dt><dd><p>Extra URLs of package indexes to use, in addition to <code>--index-url</code>.</p>

<p>Accepts either a repository compliant with PEP 503 (the simple repository API), or a local directory laid out in the same format.</p>

<p>All indexes provided via this flag take priority over the index specified by <code>--index-url</code> (which defaults to PyPI). When multiple <code>--extra-index-url</code> flags are provided, earlier values take priority.</p>

</dd><dt><code>--find-links</code>, <code>-f</code> <i>find-links</i></dt><dd><p>Locations to search for candidate distributions, in addition to those found in the registry indexes.</p>

<p>If a path, the target must be a directory that contains packages as wheel files (<code>.whl</code>) or source distributions (<code>.tar.gz</code> or <code>.zip</code>) at the top level.</p>

<p>If a URL, the page must contain a flat list of links to package files adhering to the formats described above.</p>

</dd><dt><code>--index-strategy</code> <i>index-strategy</i></dt><dd><p>The strategy to use when resolving against multiple index URLs.</p>

<p>By default, uv will stop at the first index on which a given package is available, and limit resolutions to those present on that first index (<code>first-match</code>). This prevents &quot;dependency confusion&quot; attacks, whereby an attack can upload a malicious package under the same name to a secondary.</p>

</dd><dt><code>--keyring-provider</code> <i>keyring-provider</i></dt><dd><p>Attempt to use <code>keyring</code> for authentication for index URLs.</p>

<p>At present, only <code>--keyring-provider subprocess</code> is supported, which configures uv to use the <code>keyring</code> CLI to handle authentication.</p>

<p>Defaults to <code>disabled</code>.</p>

</dd><dt><code>--exclude-newer</code> <i>exclude-newer</i></dt><dd><p>Limit candidate packages to those that were uploaded prior to the given date.</p>

<p>Accepts both RFC 3339 timestamps (e.g., <code>2006-12-02T02:07:43Z</code>) and UTC dates in the same format (e.g., <code>2006-12-02</code>).</p>

</dd><dt><code>--link-mode</code> <i>link-mode</i></dt><dd><p>The method to use when installing packages from the global cache.</p>

<p>This option is only used for installing seed packages.</p>

<p>Defaults to <code>clone</code> (also known as Copy-on-Write) on macOS, and <code>hardlink</code> on Linux and Windows.</p>

</dd><dt><code>--cache-dir</code> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>

<p>Defaults to <code>$HOME/Library/Caches/uv</code> on macOS, <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on Linux, and <code>{FOLDERID_LocalAppData}\uv\cache</code> on Windows.</p>

</dd><dt><code>--python-preference</code> <i>python-preference</i></dt><dd><p>Whether to prefer using Python installations that are already present on the system, or those that are downloaded and installed by uv</p>

</dd><dt><code>--python-fetch</code> <i>python-fetch</i></dt><dd><p>Whether to automatically download Python when required</p>

</dd><dt><code>--color</code> <i>color-choice</i></dt><dd><p>Control colors in output</p>

</dd><dt><code>--config-file</code> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration</p>

</dd></dl>

## uv cache

Manage the cache

<h3 class="cli-reference">Usage</h3>

```
uv cache [OPTIONS] <COMMAND>
```

<h3 class="cli-reference">Commands</h3>

<dl class="cli-reference"><dt><a href="uv-cache-clean"><code>uv cache clean</code></a></dt><dd><p>Clear the cache, removing all entries or those linked to specific packages</p>
</dd>
<dt><a href="uv-cache-prune"><code>uv cache prune</code></a></dt><dd><p>Prune all unreachable objects from the cache</p>
</dd>
<dt><a href="uv-cache-dir"><code>uv cache dir</code></a></dt><dd><p>Show the cache directory</p>
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

</dd><dt><code>--python-preference</code> <i>python-preference</i></dt><dd><p>Whether to prefer using Python installations that are already present on the system, or those that are downloaded and installed by uv</p>

</dd><dt><code>--python-fetch</code> <i>python-fetch</i></dt><dd><p>Whether to automatically download Python when required</p>

</dd><dt><code>--color</code> <i>color-choice</i></dt><dd><p>Control colors in output</p>

</dd><dt><code>--config-file</code> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration</p>

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

</dd><dt><code>--python-preference</code> <i>python-preference</i></dt><dd><p>Whether to prefer using Python installations that are already present on the system, or those that are downloaded and installed by uv</p>

</dd><dt><code>--python-fetch</code> <i>python-fetch</i></dt><dd><p>Whether to automatically download Python when required</p>

</dd><dt><code>--color</code> <i>color-choice</i></dt><dd><p>Control colors in output</p>

</dd><dt><code>--config-file</code> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration</p>

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

</dd><dt><code>--python-preference</code> <i>python-preference</i></dt><dd><p>Whether to prefer using Python installations that are already present on the system, or those that are downloaded and installed by uv</p>

</dd><dt><code>--python-fetch</code> <i>python-fetch</i></dt><dd><p>Whether to automatically download Python when required</p>

</dd><dt><code>--color</code> <i>color-choice</i></dt><dd><p>Control colors in output</p>

</dd><dt><code>--config-file</code> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration</p>

</dd></dl>

## uv version

Display uv's version

<h3 class="cli-reference">Usage</h3>

```
uv version [OPTIONS]
```

<h3 class="cli-reference">Options</h3>

<dl class="cli-reference"><dt><code>--output-format</code> <i>output-format</i></dt><dt><code>--cache-dir</code> <i>cache-dir</i></dt><dd><p>Path to the cache directory.</p>

<p>Defaults to <code>$HOME/Library/Caches/uv</code> on macOS, <code>$XDG_CACHE_HOME/uv</code> or <code>$HOME/.cache/uv</code> on Linux, and <code>{FOLDERID_LocalAppData}\uv\cache</code> on Windows.</p>

</dd><dt><code>--python-preference</code> <i>python-preference</i></dt><dd><p>Whether to prefer using Python installations that are already present on the system, or those that are downloaded and installed by uv</p>

</dd><dt><code>--python-fetch</code> <i>python-fetch</i></dt><dd><p>Whether to automatically download Python when required</p>

</dd><dt><code>--color</code> <i>color-choice</i></dt><dd><p>Control colors in output</p>

</dd><dt><code>--config-file</code> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration</p>

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

</dd><dt><code>--python-preference</code> <i>python-preference</i></dt><dd><p>Whether to prefer using Python installations that are already present on the system, or those that are downloaded and installed by uv</p>

</dd><dt><code>--python-fetch</code> <i>python-fetch</i></dt><dd><p>Whether to automatically download Python when required</p>

</dd><dt><code>--color</code> <i>color-choice</i></dt><dd><p>Control colors in output</p>

</dd><dt><code>--config-file</code> <i>config-file</i></dt><dd><p>The path to a <code>uv.toml</code> file to use for configuration</p>

</dd></dl>

