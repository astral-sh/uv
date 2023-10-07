# `puffin`

An experimental Python package manager.

## Usage

To resolve a `requirements.in` file:

```shell
cargo run -p puffin-cli -- compile requirements.in
```

To install from a resolved `requirements.txt` file:

```shell
cargo run -p puffin-cli -- sync requirements.txt
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
    "./target/release/puffin-cli sync requirements.txt" \
    "pip install -r requirements.txt"
```

To compare a cold run of `puffin` to `pip`:

```shell
hyperfine --runs 10 --warmup 3 \
    "./target/release/puffin-cli sync requirements.txt --no-cache" \
    "pip install -r requirements.txt --ignore-installed --no-cache-dir --no-deps"
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
