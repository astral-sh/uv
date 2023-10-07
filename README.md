# `puffin`

An experimental Python packaging tool.

## Motivation

Puffin is an extremely fast (experimental) Python package resolver and installer, intended to
replace `pip` and `pip-tools` (`pip-compile` and `pip-sync`).

Puffin itself is not a complete "package manager", but rather a tool for locking dependencies
(similar to `pip-compile`) and installing them (similar to `pip-sync`). Puffin can be used to
generate a set of locked dependencies from a `requirements.txt` file, and then install those
locked dependencies into a virtual environment.

Puffin represents an intermediary goal in our pursuit of building a "Cargo for Python": a Python
package manager that is extremely fast, reliable, and easy to use -- capable of replacing not only
`pip`, but also `pipx`, `pip-tools`, `virtualenv`, `tox`, `setuptools`, and even `pyenv`, by way of
managing the Python installation itself.

Puffin's limited scope allows us to solve many of the low-level problems that are required to
build such a package manager (like package installation) while shipping an immediately useful tool
with a minimal barrier to adoption. Try it today in lieu of `pip` and `pip-tools`.

## Limitations

Puffin does not yet support:

- Source distributions
- VCS dependencies
- URL dependencies
- Uninstalling packages
- ...

Like `pip-compile`, Puffin generates a platform-specific `requirements.txt` file (unlike, e.g.,
`poetry`, which generates a platform-agnostic `poetry.lock` file). As such, Puffin's
`requirements.txt` files are not portable across platforms and Python versions.

## Usage

To resolve a `requirements.in` file:

```shell
cargo run -p puffin-cli -- compile requirements.in
```

To install from a resolved `requirements.txt` file:

```shell
cargo run -p puffin-cli -- sync requirements.txt
```

For more, see `cargo run -p puffin-cli -- --help`:

```text
Usage: puffin-cli <COMMAND>

Commands:
  compile  Compile a `requirements.in` file to a `requirements.txt` file
  sync     Sync dependencies from a `requirements.txt` file
  clean    Clear the cache
  freeze   Enumerate the installed packages in the current environment
  help     Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version
```

## Benchmarks

### Resolution

To compare a warm run of `puffin` to `pip-compile`:

```shell
hyperfine --runs 10 --warmup 3 --prepare "rm -f /tmp/tmp.txt" \
    "./target/release/puffin-cli compile requirements.txt" \
    "pip-compile requirements.txt -o /tmp/tmp.txt"
```

To compare a cold run of `puffin` to `pip-compile`:

```shell
hyperfine --runs 10 --warmup 3 --prepare "rm -f /tmp/tmp.txt" \
    "./target/release/puffin-cli compile requirements.txt --no-cache" \
    "pip-compile requirements.txt --rebuild --pip-args '--no-cache-dir' -o /tmp/tmp.txt"
```

### Installation

To compare a warm run of `puffin` to `pip`:

```shell
hyperfine --runs 10 --warmup 3 \
    "./target/release/puffin-cli sync requirements.txt --ignore-installed" \
    "pip install -r requirements.txt --ignore-installed --no-deps"
```

To compare a cold run of `puffin` to `pip`:

```shell
hyperfine --runs 10 --warmup 3 \
    "./target/release/puffin-cli sync requirements.txt --ignore-installed --no-cache" \
    "pip install -r requirements.txt --ignore-installed --no-cache-dir --no-deps"
```

To compare a run in which all requirements are already installed:

```shell
hyperfine --runs 10 --warmup 3 \
    "./target/release/puffin-cli sync requirements.txt" \
    "pip install -r requirements.txt --no-deps"
```

## License

Puffin is licensed under either of

- Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or https://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or https://opensource.org/licenses/MIT)

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in Puffin by you, as defined in the Apache-2.0 license, shall be
dually licensed as above, without any additional terms or conditions.

<div align="center">
  <a target="_blank" href="https://astral.sh" style="background:none">
    <img src="https://raw.githubusercontent.com/astral-sh/ruff/main/assets/svg/Astral.svg">
  </a>
</div>
