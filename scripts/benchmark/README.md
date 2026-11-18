# benchmark

Benchmarking scripts for uv and other package management tools.

## Getting Started

From the `scripts/benchmark` directory:

```shell
uv run resolver.py \
    --uv-pip \
    --poetry \
    --benchmark resolve-cold \
    ../../test/requirements/trio.in

uv run tools.py --uv --pipx
```

Both examples use the uv binary at `target/release/uv` in the repository root. Each script declares
its own Python requirement and dependencies using PEP 723. Both environments install the shared
`benchmark` helpers from this directory as an editable dependency using uv's bundled build backend.
Use uv 0.13.7 or later.

The `.python-version` file keeps benchmark runs on Python 3.12.3 by default. You can override the
interpreter per script with `uv run --python`. The resolver's locked dependencies have wheels for
Python 3.11 and 3.12.

## Type checking

From the same directory, use `uv check` to run ty in each script's own environment:

```shell
UV_PREVIEW_FEATURES=check-command uv check --script resolver.py --locked
UV_PREVIEW_FEATURES=check-command uv check --script tools.py --locked
```
