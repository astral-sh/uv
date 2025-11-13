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
is relocatable. Thus, the patch in [astral-sh/uv#5640] shall be retained.

[astral-sh/uv#5640]: https://github.com/astral-sh/uv/pull/5640

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
