# Sandboxing

Sandboxing restricts what a process spawned by `uv run` can access on the host system. Sandboxing
uses OS-level isolation — [Seatbelt](https://developer.apple.com/documentation/security/app-sandbox)
on macOS and namespaces on Linux — so restrictions are enforced by the kernel, not by the
application. uv itself is never sandboxed; restrictions apply only to the spawned command.

!!! important

    Sandboxing is a [preview feature](./preview.md). Enable it with `--preview-features sandbox`.

When sandboxing is active, the child process starts in a deny-all state: filesystem access, network
access, and environment variables are all blocked unless explicitly permitted.

## Enabling sandboxing

Sandboxing is configured with a `[tool.uv.sandbox]` section in `pyproject.toml`:

```toml title="pyproject.toml"
[tool.uv.sandbox]
allow-read = [{ preset = "project" }, { preset = "python" }, { preset = "system" }]
allow-write = [{ preset = "project" }, { preset = "tmp" }]
allow-execute = [{ preset = "python" }, { preset = "system" }]
allow-env = [{ preset = "standard" }]
```

Or equivalently in `uv.toml`:

```toml title="uv.toml"
[sandbox]
allow-read = [{ preset = "project" }, { preset = "python" }, { preset = "system" }]
allow-write = [{ preset = "project" }, { preset = "tmp" }]
allow-execute = [{ preset = "python" }, { preset = "system" }]
allow-env = [{ preset = "standard" }]
```

The sandbox is activated when the configuration section is present **and** the preview feature is
enabled:

```console
$ uv run --preview-features sandbox python main.py
```

Without the preview feature, the `[tool.uv.sandbox]` section is parsed but ignored.

An empty `[tool.uv.sandbox]` section activates sandboxing with no permissions — the spawned command
will not be able to read files, access the network, or see environment variables. In practice, this
prevents even Python from starting since it can't read its own standard library.

## Permissions

Sandbox configuration follows a deny-by-default model. There are five permission categories, each
with an allow and deny field:

| Permission | Allow field | Deny field | Controls |
|---|---|---|---|
| **Read** | `allow-read` | `deny-read` | Which filesystem paths the process can read |
| **Write** | `allow-write` | `deny-write` | Which filesystem paths the process can write |
| **Execute** | `allow-execute` | `deny-execute` | Which filesystem paths the process can execute binaries from |
| **Network** | `allow-net` | `deny-net` | Whether the process can make network connections |
| **Environment** | `allow-env` | `deny-env` | Which environment variables the process can see |

**Deny always wins.** If a path appears in both `allow-read` and `deny-read`, it will be denied.
This lets you grant broad access with a preset, then carve out exceptions for sensitive paths.

## Presets

Rather than listing individual paths and variable names, sandbox configuration uses **presets** —
named groups maintained by uv that expand to concrete values at runtime.

Presets are written as `{ preset = "..." }` objects in TOML:

```toml title="pyproject.toml"
[tool.uv.sandbox]
allow-read = [{ preset = "project" }, { preset = "python" }]
deny-read = [{ preset = "known-secrets" }]
```

Literal strings in the same list are treated as paths or variable names:

```toml title="pyproject.toml"
[tool.uv.sandbox]
allow-read = [{ preset = "project" }, "/opt/shared-data"]
deny-write = [{ preset = "shell-configs" }, ".env"]
```

### Filesystem presets

These presets expand to filesystem paths based on the current project and Python environment:

| Preset | Typical use | Expands to |
|---|---|---|
| `project` | allow | The project root directory |
| `python` | allow | The Python interpreter, its stdlib, installation prefix, and virtualenv |
| `virtualenv` | allow | The project virtual environment (`.venv`) |
| `system` | allow | System libraries: `/usr/lib`, `/usr/share`, `/etc`, plus platform-specific paths |
| `home` | allow | The user's home directory |
| `uv-cache` | allow | The uv cache directory |
| `tmp` | allow | Temporary directories (`/tmp`, `$TMPDIR`) |
| `known-secrets` | deny | Credential directories: `~/.ssh/`, `~/.aws/`, `~/.gnupg/`, `~/.docker/config.json`, etc. |
| `shell-configs` | deny | Shell startup files: `~/.bashrc`, `~/.zshrc`, `~/.profile`, etc. |
| `git-hooks` | deny | Git hooks directory: `.git/hooks/` in the project root |
| `ide-configs` | deny | IDE directories: `.vscode/`, `.idea/`, `.vim/`, `.nvim/` in the project root |

All presets can be used in any field, but some are designed primarily for allow fields and others for
deny fields.

### Environment variable presets

| Preset | Typical use | Includes |
|---|---|---|
| `standard` | allow | `PATH`, `HOME`, `USER`, `SHELL`, `LANG`, `TERM`, `TMPDIR`, `VIRTUAL_ENV`, `PYTHONPATH`, `XDG_*`, color settings, and other common safe variables |
| `known-secrets` | deny | `AWS_SECRET_ACCESS_KEY`, `GITHUB_TOKEN`, `OPENAI_API_KEY`, `DATABASE_URL`, `PGPASSWORD`, and other specific credential variable names from major cloud providers, payment processors, and databases |

## Configuration examples

### Project access only

```toml title="pyproject.toml"
[tool.uv.sandbox]
allow-read = [{ preset = "project" }, { preset = "python" }, { preset = "system" }]
allow-execute = [{ preset = "python" }, { preset = "system" }]
allow-env = [{ preset = "standard" }]
```

The process can read the project and run Python, but cannot write anywhere or access the network.

### External data with network access

```toml title="pyproject.toml"
[tool.uv.sandbox]
allow-read = [{ preset = "project" }, { preset = "python" }, { preset = "system" }, "/data/datasets"]
allow-write = [{ preset = "project" }, { preset = "tmp" }]
allow-execute = [{ preset = "python" }, { preset = "system" }]
allow-net = true
allow-env = [{ preset = "standard" }, "DATABASE_URL"]
```

### Broad access with credential carve-outs

```toml title="pyproject.toml"
[tool.uv.sandbox]
allow-read = [{ preset = "home" }, { preset = "python" }, { preset = "system" }]
deny-read = [{ preset = "known-secrets" }]
allow-write = [{ preset = "project" }]
deny-write = [{ preset = "shell-configs" }, { preset = "git-hooks" }, { preset = "ide-configs" }]
allow-execute = [{ preset = "python" }, { preset = "system" }]
allow-env = true
deny-env = [{ preset = "known-secrets" }]
```

The process can read most of the home directory, but credential stores (`~/.ssh/`, `~/.aws/`, etc.)
are carved out. Environment variables like `GITHUB_TOKEN` and `AWS_SECRET_ACCESS_KEY` are hidden.

## CLI flags

Sandbox permissions can be set or overridden on the command line. CLI flags replace the
corresponding configuration field entirely — they are not merged.

```console
$ uv run --preview-features sandbox \
    --allow-read @project,@python,@system \
    --allow-execute @python,@system \
    python main.py
```

On the command line, presets use the `@` prefix: `@project`, `@python`, `@system`, etc. Multiple
values are comma-separated.

```console
$ uv run --preview-features sandbox --allow-net python main.py
```

```console
$ uv run --preview-features sandbox \
    --allow-env true \
    --deny-env MY_SECRET \
    python main.py
```

The available flags are:

| Flag | Value |
|---|---|
| `--allow-read` | Comma-separated paths and `@presets` |
| `--deny-read` | Comma-separated paths and `@presets` |
| `--allow-write` | Comma-separated paths and `@presets` |
| `--deny-write` | Comma-separated paths and `@presets` |
| `--allow-execute` | Comma-separated paths and `@presets` |
| `--deny-execute` | Comma-separated paths and `@presets` |
| `--allow-net` | `true` or `false` |
| `--deny-net` | Comma-separated hosts |
| `--allow-env` | `true`, `false`, or comma-separated names and `@presets` |
| `--deny-env` | Comma-separated names and `@presets` |

## How it works

Sandboxing is applied in the child process after `fork()` but before `exec()`, using the
[`pre_exec`](https://doc.rust-lang.org/std/os/unix/process/trait.CommandExt.html#tymethod.pre_exec)
hook on `std::process::Command`. This means:

- **uv is never sandboxed.** The parent process runs with full permissions at all times.
- **No re-execution.** The sandbox is applied in a single fork — there's no trampoline or
  re-invocation of uv.
- **Environment variable filtering** is handled by the `Command` API (clearing and selectively
  setting variables), so no unsafe mutation of the parent's environment is needed.

### macOS (Seatbelt)

On macOS, uv generates a [Seatbelt](https://developer.apple.com/documentation/security/app-sandbox)
profile in the
[SBPL](https://reverse.put.as/wp-content/uploads/2011/09/Apple-Sandbox-Guide-v1.0.pdf) language and
applies it with `sandbox_init()`. The profile starts with `(deny default)` and adds explicit
`(allow ...)` and `(deny ...)` rules for each configured path.

### Linux (namespaces)

On Linux, uv uses user namespaces (`unshare()`) and bind mounts to create an isolated filesystem
view, combined with network namespaces for network restriction.

!!! note

    Linux sandboxing is not yet implemented and will return an error. macOS sandboxing is
    fully functional.

## Platform support

| Platform | Status | Mechanism |
|---|---|---|
| macOS | ✅ Functional | Seatbelt (`sandbox_init`) |
| Linux | 🚧 Planned | User namespaces + seccomp |
| Windows | ❌ Not supported | — |

## Limitations

- **macOS only** in the current preview. Linux support is planned.
- **Network filtering is binary.** `allow-net` is either `true` (all network access) or `false`
  (no network access). Per-host filtering is planned.
- **No inline script metadata support.** The `[tool.uv.sandbox]` section is not recognized in
  [PEP 723](https://peps.python.org/pep-0723/) inline script metadata. Use CLI flags for scripts.
- **Sandbox profiles are not hermetic.** The `system` preset grants read access to broad system
  paths. A determined attacker with code execution could potentially find information in those
  paths.
- **Deny rules operate on paths, not content.** The sandbox cannot inspect what data flows through
  allowed paths.
- **macOS: file metadata is globally readable.** The sandboxed process can `stat()` any path,
  including paths in `deny-read` (e.g., `~/.ssh`). File *contents* are still protected, but
  existence, size, permissions, and timestamps are visible. This is required for Python's
  `os.path.exists()` and `importlib` path scanning to function.
- **macOS: signal scope is broader than ideal.** The sandboxed process can send signals to any
  process owned by the same user, not just its children. This is required for Python's
  `subprocess` and `multiprocessing` modules, which need to signal child processes. macOS Seatbelt
  only supports `(target self)` and `(target others)` — there is no `(target children)` filter.
- **macOS: IPC is not restricted.** Mach IPC and POSIX IPC are broadly allowed. A sandboxed process
  with `pyobjc` or `ctypes` could potentially interact with system services (e.g., Keychain,
  pasteboard). Tightening this requires a version-aware allowlist of undocumented Apple Mach
  services.
- **Linux: `/proc` is read-only.** The bind-mounted `/proc` is marked read-only to prevent writing
  to paths like `/proc/self/oom_score_adj`. This may cause issues with `multiprocessing` or other
  libraries that write to `/proc/self/` paths.
