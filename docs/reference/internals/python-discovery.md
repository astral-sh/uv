# Python discovery internals

!!! tip

    This document focuses on uv's internal Python discovery model: how uv turns a
    `--python` value, version file, project requirement, or default request into a concrete
    interpreter. For user-facing behavior, see the [Python versions](../../concepts/python-versions.md),
    [installing Python](../../guides/install-python.md), and
    [project](../../concepts/projects/index.md) documentation.

## Python discovery

uv often needs a Python interpreter before it can resolve, install, or run anything. Discovery has
two related jobs:

- Reuse an existing environment when a command is meant to operate in one.
- Find, or if allowed install, a base interpreter that can create a new environment.

Most commands move through the same phases:

- Determine the effective Python request from command-line input, version files, or
  `requires-python`.
- Decide which categories of interpreters are acceptable for the command, such as virtual
  environments, system installations, or uv-managed installations.
- Enumerate candidate executables from the allowed sources in preference order.
- Query candidates to learn what they actually are.
- Select the first candidate that satisfies the request and command constraints.
- If no installed interpreter works, optionally download a uv-managed Python.

The important invariant is that discovery is request-aware from beginning to end. uv does not pick
the first executable named `python`, then check it after the fact. It carries the same effective
request through environment reuse, interpreter search, download fallback, and environment creation
so each phase makes decisions against the same input.

## Requests

A Python request can come from several places:

- `--python`, `-p`, or another command-specific Python option.
- `.python-version` or `.python-versions`.
- `requires-python` in project or script metadata.
- A tool name such as `uvx python3.12`.
- The absence of any explicit request, which means uv should find a suitable default.

Requests can name broad concepts like "any Python", concrete versions like `3.12.4`, ranges like
`>=3.11,<3.13`, implementations such as PyPy, variants such as free-threaded builds, executable
paths, directories, executable names on `PATH`, or managed Python download keys.

The same syntax is not used everywhere. `--python` is intentionally permissive: if a value cannot be
parsed as a version, implementation, path, or download key, uv treats it as an executable name.
Tool-name parsing is stricter because `uvx python311` might be a request for Python 3.11, while
other tool names should still be interpreted as package executables.

`requires-python` constraints are translated into discovery requests too. A single `==` specifier is
treated as a concrete version request, so `requires-python = "==3.12"` causes uv to look for
versioned executables like `python3.12` instead of only broad names like `python`. Broader ranges
remain ranges and are checked against each queried interpreter.

`default` and `any` are different. The default request avoids choices that are surprising without an
opt-in, such as pre-releases, debug builds, free-threaded builds, and alternative implementations.
The `any` request is explicitly broader.

## Request Sources

When project commands do not receive an explicit Python request, uv resolves one in this order:

- The explicit command-line request.
- The nearest local version file between the project directory and the workspace root, then the
  global uv version file.
- The workspace `requires-python` requirement.

Script commands use the same shape, but include the script's inline metadata:

- The explicit command-line request.
- A version file near the script.
- The script's inline `requires-python` requirement.
- The surrounding workspace `requires-python` requirement, when applicable.

Version-file discovery walks up parent directories only as far as the command's project boundary,
then falls back to the global uv config directory. Blank lines and comments are ignored. Version
files may contain version-style requests, but not arbitrary executable names; a version file is a
portable project preference, not a place to depend on a particular shell's `PATH`.

Local and global pins have different authority. A local `.python-version` is project input, so an
incompatible local pin should be reported to the user instead of silently replaced. A global version
file is a user preference, so uv can ignore it when it conflicts with stronger project or script
metadata. This keeps a user's old global default from blocking a project that deliberately requires
a newer Python.

Tool commands use a narrower version-file rule. `uv tool run` and `uv tool install` may use the
global version file when no explicit Python request or versioned tool name was provided, but they
skip local version files. Tool execution should not accidentally inherit the Python pin of whatever
project directory the user happened to be in.

## Preferences

`python-preference` controls how uv treats uv-managed and system interpreters:

- `only-managed`: use uv-managed Python installations only.
- `managed`: prefer uv-managed installations, then system installations. This is the default.
- `system`: prefer system installations, but allow managed installations if no system interpreter
  matches.
- `only-system`: use system installations only.

The preference is enforced after a candidate is queried, not just when sources are enumerated. This
matters because an executable on `PATH` can still be a uv-managed interpreter. Conversely, explicit
sources such as a provided executable path or an activated environment are honored even when they
would not be found by the broad search implied by the current preference.

Commands also decide whether virtual environments are acceptable. Some commands require a virtual
environment, some should ignore virtual environments and find a base interpreter, and some can use
either. For example, project environment creation searches for a base interpreter to create a new
`.venv`, while script execution can use either a base interpreter or an environment depending on the
request.

`python-downloads` controls whether missing interpreters may be installed automatically:

- `automatic`: download a managed Python when discovery cannot find a usable interpreter.
- `manual`: report that a managed download is available, but require `uv python install`.
- `never`: never use Python downloads.

Downloads are considered only when managed interpreters are allowed, downloads are automatic, and uv
is online.

## Candidate Sources

Discovery is lazy. uv builds a sequence of candidate executables, queries each candidate only when
needed, and stops once it finds a usable interpreter.

When virtual environments are allowed, candidates can come from:

- The active `VIRTUAL_ENV`.
- An active conda environment.
- A project `.venv` discovered from the working directory.

When base interpreters are allowed, candidates can come from:

- uv-managed Python installations.
- Executables found on `PATH`, or `UV_PYTHON_SEARCH_PATH` when it is set.
- Windows registry entries and Microsoft Store installations on Windows.
- A base conda environment.
- The interpreter that invoked uv, when uv was launched through Python.

Source ordering reflects the command's preferences. With the default managed preference, uv checks
installed uv-managed Pythons before generic `PATH` interpreters, but still prefers an existing
system interpreter over downloading a new managed one. With the system preference, `PATH` and
Windows registry sources are checked before uv-managed installations, and a managed interpreter is
kept only as a fallback if no system interpreter matches.

The source order is not just a performance detail. It encodes user intent. An activated environment
or explicit path is stronger than a broad search result; a local project environment is stronger
than a random interpreter later on `PATH`; and a configured system-only command must not quietly
target a uv-managed interpreter just because that interpreter is visible through a shell shim.

## Search Path Discovery

For versioned requests, uv generates several executable names. A request for Python 3.12 may check
names such as `python`, `python3`, and `python3.12`; implementation and variant requests add names
such as `pypy3.10` or free-threaded suffixes.

uv scans search-path directories in order. Within each directory, it checks the generated names
before moving to the next directory. This preserves the user's path ordering while still allowing a
more specific executable in the same directory to beat a broader one.

Broad requests need one extra step. If `python` and `python3` are both Python 3.10, a request like
`>=3.11` should still find `python3.12` later in the same directory. uv therefore scans for matching
`python3.x` executables when the request is broad enough that generated names alone could miss a
valid interpreter.

`UV_PYTHON_SEARCH_PATH` replaces `PATH` for Python executable discovery. It is useful for isolating
discovery from the user's shell without changing the rest of the process environment. It does not
disable Windows registry or Microsoft Store discovery; those are separate sources. On Windows,
`UV_PYTHON_NO_REGISTRY` disables registry and Microsoft Store discovery explicitly.

Explicit executable-name requests are narrower. If the user asks for a specific executable name, uv
searches for that exact name and then validates the interpreters it finds.

## Candidate Validation

uv never trusts a candidate because of its filename, directory, or source. Every candidate must be
queried so uv can learn its actual version, implementation, platform, virtual-environment state,
installation scheme, prefixes, tags, GIL state, debug state, and other details needed by later
commands.

The probe runs the candidate interpreter in isolated mode and collects the same kind of metadata
regardless of where the executable came from. Query results are cached because spawning Python is
expensive, but cached results are tied to the executable path and file metadata so stale results are
not reused after the interpreter changes.

Discovery tolerates many broken candidates. A bad response, unsupported interpreter, missing
executable, permission-denied path, or broken symlink can be skipped so uv can keep searching. A
broken activated virtual environment is treated more seriously, because the user explicitly selected
it by activating it.

This validation step is also where source claims are checked. For example, an executable discovered
on `PATH` might actually live under uv's managed Python root, so it must still respect
`python-preference`.

## Interpreter Caching

The interpreter metadata cache is an optimization around candidate validation. It does not remember
"the Python uv should use"; it remembers facts learned by probing a specific executable. Source
ordering, user requests, project requirements, and preferences are still applied fresh for each
command.

A probe is expensive enough to matter. It has the same basic cost profile as starting Python in
isolated mode, then importing enough standard-library machinery to report paths, tags, platform
details, and environment state. On a local macOS CPython 3.14 installation, a plain isolated Python
startup measured in the tens of milliseconds, and a small metadata-style probe was roughly twice
that. In a debug uv build, comparing an explicit-path `uv python find` with and without a warm
interpreter cache showed the cache saving a few tens of milliseconds for one candidate. Broad
searches can multiply that cost by every candidate executable that has to be queried.

Cache correctness is more important than cache hit rate. The cache key accounts for the requested
path and the resolved executable path, so relative paths and symlinked paths do not collide. Cache
entries are also separated by host operating system and architecture, because interpreter metadata
can change after an OS upgrade even if the executable path is unchanged.

Before reusing a cached probe result, uv checks that the underlying executable still has the same
file timestamp. If the interpreter changed, uv probes it again. uv also avoids caching shell shims
that execute a different Python than the file uv invoked, because caching the shim path would make a
future environment change look like the old interpreter.

## Selection

uv selects the first queried interpreter that satisfies all active constraints:

- The effective Python request.
- Any project or script `requires-python` requirement.
- The command's virtual-environment policy.
- `python-preference`.
- Platform or architecture requests, when present.

Some candidates are held as fallbacks instead of selected immediately. A pre-release interpreter,
debug build, free-threaded build, or alternative implementation is not a default choice unless the
request or source opted into it, but uv may use such a candidate if nothing better matches. With
`python-preference = "system"`, a managed interpreter can also be held as a fallback while uv
continues looking for a system interpreter.

The default request is source-sensitive. If the user gives uv a specific path, activates an
environment, or is already inside a discovered environment, uv can treat that explicit source as an
acceptable default. For broad sources such as managed installations, `PATH`, the Windows registry,
and the Microsoft Store, default discovery stays conservative.

When no candidate matches, uv tries to report the most useful failure. If discovery saw a
non-fatal-looking interpreter error while searching, that error can be more actionable than a
generic "Python not found" message.

## Existing Environments

Project and script commands do not create a new environment just because a matching base interpreter
exists. They first inspect the existing environment for the project or script cache.

An existing environment can be reused only when it satisfies the effective request, the relevant
`requires-python` requirement, the command's Python preference, and the expected relationship
between the environment and its base interpreter. If the environment is incompatible, uv recreates
or replaces it using a newly discovered base interpreter.

This keeps environment reuse predictable: a `.venv` is not preserved merely because it exists, and a
project-level Python request is not forgotten once the environment has already been created.
Patch-specific pins are especially important here. If `.python-version` says `3.12.7`, uv should not
transparently move the project to a newer `3.12` patch just because a managed minor-version link now
points somewhere else.

## Managed Downloads

Downloads are a fallback, not a competing first source. uv searches installed candidates first. It
only considers downloading when no installed interpreter satisfies the request and the request can
be represented as a managed Python. Directory paths, file paths, and executable-name requests cannot
be turned into downloads.

When a download is possible but disabled, uv tries to explain the reason. The user may have set
`python-downloads = "manual"`, `python-downloads = "never"`, `python-preference = "only-system"`, or
offline mode; alternatively, the uv binary may simply not know about a newly released Python yet.

Managed installations are protected by an installation lock. uv downloads and extracts into the
managed Python root, marks the installation as externally managed, applies the layout adjustments
needed for standalone Python builds, creates stable executable names, and then queries the result
like any other candidate.

For non-patch requests, managed installations can expose a stable minor-version link such as the
current best `3.12`. That is convenient for commands that ask for a minor series. Patch-specific
requests avoid depending on that mutable link when creating project environments, preserving the
meaning of exact pins.

## Platform Notes

Managed Python downloads carry platform metadata, so uv can reject incompatible managed candidates
before running them. System interpreters generally have to be queried before uv can compare their
platform and architecture to the request.

On Unix, managed CPython executable names include the major and minor version, plus variant suffixes
when needed. On Windows, managed CPython uses `python.exe` or `pythonw.exe`; search path discovery
also accounts for `.exe` suffixes, `python.bat`, Windows Store proxy shims, and registry-provided
installations.

Architecture matching distinguishes explicit architecture requests from environment-derived
requests. If the user explicitly asks for an architecture, uv requires an exact match. If the
architecture came from the current environment, uv can accept a compatible target when the host
platform supports it.
