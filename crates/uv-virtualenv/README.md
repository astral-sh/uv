# uv-virtualenv

`uv-virtualenv` is a rust library to create Python virtual environments. It also has a CLI.

## Syncing with upstream virtualenv activation scripts

This crate tries to stay in sync with pypa/virtualenv project's activation scripts. However, there
are some deviations that are specific to this crate's implementation.

### License disclaimers added

This crate includes license information at the top of each activation script. This is done in
accordance with the pypa/virtualenv project's MIT License. Do not remove the declarative license
comments from this crate's activation scripts.

### Placeholder names are slightly different

Note, these activation scripts are actually templates that are populated with certain values when a
virtual environment is created.

In upstream, the placeholder names are found in
[`virtualenv.activation.ViaTemplateActivator.replacements()`][upstream-placeholders].

In this crate, the placeholder names are found in
[`uv_virtualenv::virtualenv::create()`][crate-placeholders]

[upstream-placeholders]:
  https://github.com/pypa/virtualenv/blob/dad9369e97f5aef7e33777b18dcdb51b1fdac7bd/src/virtualenv/activation/via_template.py#L43
[crate-placeholders]:
  https://github.com/astral-sh/uv/blob/d8f3f03198308be53de51a3a297c85566eabb084/crates/uv-virtualenv/src/virtualenv.rs#L462

It is important that the placeholder names (as used in the activation scripts) conform to the
placeholders names used in [this crate's source][crate-placeholders].

### Relocatable virtual environments

This crate uses some additional tweaks in the activation scripts to ensure the virtual environment
is relocatable. Retain the following shell-specific behavior when updating upstream templates:

- The POSIX activator determines the script path separately for Bash, Zsh, and KornShell
  ([astral-sh/uv#5640]). It also restores any pre-existing `SCRIPT_PATH` after activation instead of
  leaking its temporary value into the caller ([astral-sh/uv#12672]).
- Fish determines the relocated environment from `status -f` ([astral-sh/uv#5515]) without changing
  the caller's working directory or relying on Bash-style command substitution
  ([astral-sh/uv#19856]).
- Windows batch determines the relocated environment from `%~dp0..` and resolves it to an absolute
  path ([astral-sh/uv#5515]).
- Nushell determines the relocated environment from `path self` ([astral-sh/uv#17036]). Because
  `path self` can only be evaluated at parse time, the activator must declare `virtual_env` with
  `const`, not `let` ([astral-sh/uv#20743]).
- C shell cannot determine the path of a sourced script. Do not generate `activate.csh` for
  relocatable environments ([astral-sh/uv#17759]).

Dash, BusyBox ash, and Yash also do not expose the path of a sourced POSIX script. Relocatable
activation is not currently supported for these shells ([astral-sh/uv#20743]).

[astral-sh/uv#5515]: https://github.com/astral-sh/uv/pull/5515
[astral-sh/uv#5640]: https://github.com/astral-sh/uv/pull/5640
[astral-sh/uv#12672]: https://github.com/astral-sh/uv/pull/12672
[astral-sh/uv#17036]: https://github.com/astral-sh/uv/pull/17036
[astral-sh/uv#17759]: https://github.com/astral-sh/uv/pull/17759
[astral-sh/uv#19856]: https://github.com/astral-sh/uv/pull/19856
[astral-sh/uv#20743]: https://github.com/astral-sh/uv/pull/20743

### POSIX shell compatibility

Preserve the quoted `eval` around Zsh- and KornShell-specific parameter expansions. Yash parses
these expansions even in branches it does not execute, so removing `eval` prevents it from sourcing
the POSIX activator ([astral-sh/uv#20743]).

Use `${OSTYPE-}` when checking for Cygwin or MSYS. `OSTYPE` is not defined by POSIX and can be unset
in Dash and BusyBox ash. Referencing `$OSTYPE` directly causes activation to fail under `set -u`
([astral-sh/uv#20743]).

Keep `VIRTUAL_ENV_PROMPT` as the unformatted environment name and add parentheses only when
constructing `PS1` ([astral-sh/uv#13501]).

[astral-sh/uv#13501]: https://github.com/astral-sh/uv/pull/13501

### Fish on Windows

Preserve the `cygpath -u` conversion of `VIRTUAL_ENV` when Fish is running under Cygwin, MSYS, or
MinGW. Otherwise, Windows and POSIX paths are mixed in `PATH` and activation breaks
([astral-sh/uv#19703]).

[astral-sh/uv#19703]: https://github.com/astral-sh/uv/pull/19703

### Windows batch encoding

Preserve the temporary switch to the UTF-8 code page in `activate.bat` and restore the previous code
page after activation. Without it, environment paths containing non-ASCII characters can be
corrupted ([astral-sh/uv#11831]).

[astral-sh/uv#11831]: https://github.com/astral-sh/uv/pull/11831

### Shell activation tests

After updating activators, run the
[shell activation workflow](../../.github/workflows/test-shell-activation.yml). It covers standard
and relocatable activation, deactivation, preservation of existing shell state, unset `OSTYPE`, and
the shells and platforms for which activation is supported.

### TCL/TK library locations

The patches in upstream virtualenv ([pypa/virtualenv#2928] and [pypa/virtualenv#2940]) implement
dynamically locating the TCL/TK libraries of a base Python distribution (see [upstream
approach][upstream-tcl/tk-approach]).

[pypa/virtualenv#2928]: https://github.com/pypa/virtualenv/pull/2928
[pypa/virtualenv#2940]: https://github.com/pypa/virtualenv/pull/2940
[upstream-tcl/tk-approach]:
  https://github.com/pypa/virtualenv/blob/dad9369e97f5aef7e33777b18dcdb51b1fdac7bd/src/virtualenv/discovery/py_info.py#L140

This upstream implementation is considered an undesirable complexity in this project. As such, the
upstream TCL/TK related patches shall be omitted when syncing activation scripts with upstream
sources.
