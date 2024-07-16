## Global
#### [`cache-dir`](#cache-dir) {: #cache-dir }

Path to the cache directory.

Defaults to `$HOME/Library/Caches/uv` on macOS, `$XDG_CACHE_HOME/uv` or `$HOME/.cache/uv` on
Linux, and `{FOLDERID_LocalAppData}\uv\cache` on Windows.

**Default value**: `None`

**Type**: `str`

**Example usage**:

=== "pyproject.toml"

    ```toml
    [tool.uv]
    cache-dir = "./.uv_cache"
    ```
=== "uv.toml"

    ```toml
    
    cache-dir = "./.uv_cache"
    ```

---

#### [`compile-bytecode`](#compile-bytecode) {: #compile-bytecode }

Compile Python files to bytecode after installation.

By default, does not compile Python (`.py`) files to bytecode (`__pycache__/*.pyc`), instead
Python lazily does the compilation the first time a module is imported. In cases where the
first start time matters, such as CLI applications and docker containers, this option can
trade longer install time for faster startup.

The compile option will process the entire site-packages directory for consistency and
(like pip) ignore all errors.

**Default value**: `false`

**Type**: `bool`

**Example usage**:

=== "pyproject.toml"

    ```toml
    [tool.uv]
    compile-bytecode = true
    ```
=== "uv.toml"

    ```toml
    
    compile-bytecode = true
    ```

---

#### [`config-settings`](#config-settings) {: #config-settings }

Settings to pass to the PEP 517 build backend, specified as `KEY=VALUE` pairs.

**Default value**: `{}`

**Type**: `dict`

**Example usage**:

=== "pyproject.toml"

    ```toml
    [tool.uv]
    config-settings = { "editable_mode": "compat" }
    ```
=== "uv.toml"

    ```toml
    
    config-settings = { "editable_mode": "compat" }
    ```

---

#### [`exclude-newer`](#exclude-newer) {: #exclude-newer }

Limit candidate packages to those that were uploaded prior to the given date.

Accepts both RFC 3339 timestamps (e.g., `2006-12-02T02:07:43Z`) and UTC dates in the same
format (e.g., `2006-12-02`).

**Default value**: `None`

**Type**: `str`

**Example usage**:

=== "pyproject.toml"

    ```toml
    [tool.uv]
    exclude-newer = "2006-12-02"
    ```
=== "uv.toml"

    ```toml
    
    exclude-newer = "2006-12-02"
    ```

---

#### [`extra-index-url`](#extra-index-url) {: #extra-index-url }

Extra URLs of package indexes to use, in addition to `--index-url`.

Accepts either a repository compliant with PEP 503 (the simple repository API), or a local
directory laid out in the same format.

All indexes provided via this flag take priority over the index specified by
[`index_url`](#index-url). When multiple indexes are provided, earlier values take priority.

To control uv's resolution strategy when multiple indexes are present, see
[`index_strategy`](#index-strategy).

**Default value**: `[]`

**Type**: `list[str]`

**Example usage**:

=== "pyproject.toml"

    ```toml
    [tool.uv]
    extra-index-url = ["https://download.pytorch.org/whl/cpu"]
    ```
=== "uv.toml"

    ```toml
    
    extra-index-url = ["https://download.pytorch.org/whl/cpu"]
    ```

---

#### [`find-links`](#find-links) {: #find-links }

Locations to search for candidate distributions, in addition to those found in the registry
indexes.

If a path, the target must be a directory that contains packages as wheel files (`.whl`) or
source distributions (`.tar.gz` or `.zip`) at the top level.

If a URL, the page must contain a flat list of links to package files adhering to the
formats described above.

**Default value**: `[]`

**Type**: `list[str]`

**Example usage**:

=== "pyproject.toml"

    ```toml
    [tool.uv]
    find-links = ["https://download.pytorch.org/whl/torch_stable.html"]
    ```
=== "uv.toml"

    ```toml
    
    find-links = ["https://download.pytorch.org/whl/torch_stable.html"]
    ```

---

#### [`index-strategy`](#index-strategy) {: #index-strategy }

The strategy to use when resolving against multiple index URLs.

By default, uv will stop at the first index on which a given package is available, and
limit resolutions to those present on that first index (`first-match`). This prevents
"dependency confusion" attacks, whereby an attack can upload a malicious package under the
same name to a secondary.

Possible values:

- `"first-index"`:        Only use results from the first index that returns a match for a given package name.
- `"unsafe-first-match"`: Search for every package name across all indexes, exhausting the versions from the first index before moving on to the next.
- `"unsafe-best-match"`:  Search for every package name across all indexes, preferring the "best" version found. If a package version is in multiple indexes, only look at the entry for the first index.

**Default value**: `"first-index"`

**Type**: `str`

**Example usage**:

=== "pyproject.toml"

    ```toml
    [tool.uv]
    index-strategy = "unsafe-best-match"
    ```
=== "uv.toml"

    ```toml
    
    index-strategy = "unsafe-best-match"
    ```

---

#### [`index-url`](#index-url) {: #index-url }

The URL of the Python package index (by default: <https://pypi.org/simple>).

Accepts either a repository compliant with PEP 503 (the simple repository API), or a local
directory laid out in the same format.

The index provided by this setting is given lower priority than any indexes specified via
[`extra_index_url`](#extra-index-url).

**Default value**: `"https://pypi.org/simple"`

**Type**: `str`

**Example usage**:

=== "pyproject.toml"

    ```toml
    [tool.uv]
    index-url = "https://pypi.org/simple"
    ```
=== "uv.toml"

    ```toml
    
    index-url = "https://pypi.org/simple"
    ```

---

#### [`keyring-provider`](#keyring-provider) {: #keyring-provider }

Attempt to use `keyring` for authentication for index URLs.

At present, only `--keyring-provider subprocess` is supported, which configures uv to
use the `keyring` CLI to handle authentication.

**Default value**: `"disabled"`

**Type**: `str`

**Example usage**:

=== "pyproject.toml"

    ```toml
    [tool.uv]
    keyring-provider = "subprocess"
    ```
=== "uv.toml"

    ```toml
    
    keyring-provider = "subprocess"
    ```

---

#### [`link-mode`](#link-mode) {: #link-mode }

The method to use when installing packages from the global cache.

Defaults to `clone` (also known as Copy-on-Write) on macOS, and `hardlink` on Linux and
Windows.

**Default value**: `"clone" (macOS) or "hardlink" (Linux, Windows)`

**Type**: `str`

**Example usage**:

=== "pyproject.toml"

    ```toml
    [tool.uv]
    link-mode = "copy"
    ```
=== "uv.toml"

    ```toml
    
    link-mode = "copy"
    ```

---

#### [`managed`](#managed) {: #managed }

Whether the project is managed by uv. If `false`, uv will ignore the project when
`uv run` is invoked.

**Default value**: `true`

**Type**: `bool`

**Example usage**:

=== "pyproject.toml"

    ```toml
    [tool.uv]
    managed = false
    ```
=== "uv.toml"

    ```toml
    
    managed = false
    ```

---

#### [`native-tls`](#native-tls) {: #native-tls }

Whether to load TLS certificates from the platform's native certificate store.

By default, uv loads certificates from the bundled `webpki-roots` crate. The
`webpki-roots` are a reliable set of trust roots from Mozilla, and including them in uv
improves portability and performance (especially on macOS).

However, in some cases, you may want to use the platform's native certificate store,
especially if you're relying on a corporate trust root (e.g., for a mandatory proxy) that's
included in your system's certificate store.

**Default value**: `false`

**Type**: `bool`

**Example usage**:

=== "pyproject.toml"

    ```toml
    [tool.uv]
    native-tls = true
    ```
=== "uv.toml"

    ```toml
    
    native-tls = true
    ```

---

#### [`no-binary`](#no-binary) {: #no-binary }

Don't install pre-built wheels.

The given packages will be built and installed from source. The resolver will still use
pre-built wheels to extract package metadata, if available.

**Default value**: `false`

**Type**: `bool`

**Example usage**:

=== "pyproject.toml"

    ```toml
    [tool.uv]
    no-binary = true
    ```
=== "uv.toml"

    ```toml
    
    no-binary = true
    ```

---

#### [`no-binary-package`](#no-binary-package) {: #no-binary-package }

Don't install pre-built wheels for a specific package.

**Default value**: `[]`

**Type**: `list[str]`

**Example usage**:

=== "pyproject.toml"

    ```toml
    [tool.uv]
    no-binary-package = ["ruff"]
    ```
=== "uv.toml"

    ```toml
    
    no-binary-package = ["ruff"]
    ```

---

#### [`no-build`](#no-build) {: #no-build }

Don't build source distributions.

When enabled, resolving will not run arbitrary Python code. The cached wheels of
already-built source distributions will be reused, but operations that require building
distributions will exit with an error.

**Default value**: `false`

**Type**: `bool`

**Example usage**:

=== "pyproject.toml"

    ```toml
    [tool.uv]
    no-build = true
    ```
=== "uv.toml"

    ```toml
    
    no-build = true
    ```

---

#### [`no-build-package`](#no-build-package) {: #no-build-package }

Don't build source distributions for a specific package.

**Default value**: `[]`

**Type**: `list[str]`

**Example usage**:

=== "pyproject.toml"

    ```toml
    [tool.uv]
    no-build-package = ["ruff"]
    ```
=== "uv.toml"

    ```toml
    
    no-build-package = ["ruff"]
    ```

---

#### [`no-cache`](#no-cache) {: #no-cache }

Avoid reading from or writing to the cache, instead using a temporary directory for the
duration of the operation.

**Default value**: `false`

**Type**: `bool`

**Example usage**:

=== "pyproject.toml"

    ```toml
    [tool.uv]
    no-cache = true
    ```
=== "uv.toml"

    ```toml
    
    no-cache = true
    ```

---

#### [`no-index`](#no-index) {: #no-index }

Ignore all registry indexes (e.g., PyPI), instead relying on direct URL dependencies and
those provided via `--find-links`.

**Default value**: `false`

**Type**: `bool`

**Example usage**:

=== "pyproject.toml"

    ```toml
    [tool.uv]
    no-index = true
    ```
=== "uv.toml"

    ```toml
    
    no-index = true
    ```

---

#### [`offline`](#offline) {: #offline }

Disable network access, relying only on locally cached data and locally available files.

**Default value**: `false`

**Type**: `bool`

**Example usage**:

=== "pyproject.toml"

    ```toml
    [tool.uv]
    offline = true
    ```
=== "uv.toml"

    ```toml
    
    offline = true
    ```

---

#### [`prerelease`](#prerelease) {: #prerelease }

The strategy to use when considering pre-release versions.

By default, uv will accept pre-releases for packages that _only_ publish pre-releases,
along with first-party requirements that contain an explicit pre-release marker in the
declared specifiers (`if-necessary-or-explicit`).

**Default value**: `"if-necessary-or-explicit"`

**Type**: `str`

**Example usage**:

=== "pyproject.toml"

    ```toml
    [tool.uv]
    prerelease = "allow"
    ```
=== "uv.toml"

    ```toml
    
    prerelease = "allow"
    ```

---

#### [`preview`](#preview) {: #preview }

Whether to enable experimental, preview features.

**Default value**: `false`

**Type**: `bool`

**Example usage**:

=== "pyproject.toml"

    ```toml
    [tool.uv]
    preview = true
    ```
=== "uv.toml"

    ```toml
    
    preview = true
    ```

---

#### [`python-fetch`](#python-fetch) {: #python-fetch }

Whether to automatically download Python when required.

**Default value**: `"automatic"`

**Type**: `str`

**Example usage**:

=== "pyproject.toml"

    ```toml
    [tool.uv]
    python-fetch = \"automatic\"
    ```
=== "uv.toml"

    ```toml
    
    python-fetch = \"automatic\"
    ```

---

#### [`python-preference`](#python-preference) {: #python-preference }

Whether to prefer using Python installations that are already present on the system, or
those that are downloaded and installed by uv.

**Default value**: `"installed"`

**Type**: `str`

**Example usage**:

=== "pyproject.toml"

    ```toml
    [tool.uv]
    python-preference = "managed"
    ```
=== "uv.toml"

    ```toml
    
    python-preference = "managed"
    ```

---

#### [`reinstall`](#reinstall) {: #reinstall }

Reinstall all packages, regardless of whether they're already installed.

**Default value**: `false`

**Type**: `bool`

**Example usage**:

=== "pyproject.toml"

    ```toml
    [tool.uv]
    reinstall = true
    ```
=== "uv.toml"

    ```toml
    
    reinstall = true
    ```

---

#### [`reinstall-package`](#reinstall-package) {: #reinstall-package }

Reinstall a specific package, regardless of whether it's already installed.

**Default value**: `[]`

**Type**: `list[str]`

**Example usage**:

=== "pyproject.toml"

    ```toml
    [tool.uv]
    reinstall-package = ["ruff"]
    ```
=== "uv.toml"

    ```toml
    
    reinstall-package = ["ruff"]
    ```

---

#### [`resolution`](#resolution) {: #resolution }

The strategy to use when selecting between the different compatible versions for a given
package requirement.

By default, uv will use the latest compatible version of each package (`highest`).

**Default value**: `"highest"`

**Type**: `str`

**Example usage**:

=== "pyproject.toml"

    ```toml
    [tool.uv]
    resolution = "lowest-direct"
    ```
=== "uv.toml"

    ```toml
    
    resolution = "lowest-direct"
    ```

---

#### [`upgrade`](#upgrade) {: #upgrade }

Allow package upgrades, ignoring pinned versions in any existing output file.

**Default value**: `false`

**Type**: `bool`

**Example usage**:

=== "pyproject.toml"

    ```toml
    [tool.uv]
    upgrade = true
    ```
=== "uv.toml"

    ```toml
    
    upgrade = true
    ```

---

#### [`upgrade-package`](#upgrade-package) {: #upgrade-package }

Allow upgrades for a specific package, ignoring pinned versions in any existing output
file.

Accepts both standalone package names (`ruff`) and version specifiers (`ruff<0.5.0`).

**Default value**: `[]`

**Type**: `list[str]`

**Example usage**:

=== "pyproject.toml"

    ```toml
    [tool.uv]
    upgrade-package = ["ruff"]
    ```
=== "uv.toml"

    ```toml
    
    upgrade-package = ["ruff"]
    ```

---

## `pip`

Settings that are specific to the `uv pip` command-line interface.

These values will be ignored when running commands outside the `uv pip` namespace (e.g.,
`uv lock`, `uvx`).

#### [`no-header`](#pip_no-header) {: #pip_no-header }
<span id="no-header"></span>

Exclude the comment header at the top of output file generated by `uv pip compile`.

**Default value**: `false`

**Type**: `bool`

**Example usage**:

=== "pyproject.toml"

    ```toml
    [tool.uv.pip]
    no-header = true
    ```
=== "uv.toml"

    ```toml
    [pip]
    no-header = true
    ```

---

## `workspace`

#### [`exclude`](#workspace_exclude) {: #workspace_exclude }
<span id="exclude"></span>

Packages to exclude as workspace members. If a package matches both `members` and
`exclude`, it will be excluded.

Supports both globs and explicit paths.

For more information on the glob syntax, refer to the [`glob` documentation](https://docs.rs/glob/latest/glob/struct.Pattern.html).

**Default value**: `[]`

**Type**: `list[str]`

**Example usage**:

=== "pyproject.toml"

    ```toml
    [tool.uv.workspace]
    exclude = ["member1", "path/to/member2", "libs/*"]
    ```
=== "uv.toml"

    ```toml
    [workspace]
    exclude = ["member1", "path/to/member2", "libs/*"]
    ```

---

#### [`members`](#workspace_members) {: #workspace_members }
<span id="members"></span>

Packages to include as workspace members.

Supports both globs and explicit paths.

For more information on the glob syntax, refer to the [`glob` documentation](https://docs.rs/glob/latest/glob/struct.Pattern.html).

**Default value**: `[]`

**Type**: `list[str]`

**Example usage**:

=== "pyproject.toml"

    ```toml
    [tool.uv.workspace]
    members = ["member1", "path/to/member2", "libs/*"]
    ```
=== "uv.toml"

    ```toml
    [workspace]
    members = ["member1", "path/to/member2", "libs/*"]
    ```

---

