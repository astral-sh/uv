# bench

Benchmarking scripts for uv and other package management tools.

## Getting Started

From the `bench` directory:

```shell
uv run __main__.py \
    --uv-pip \
    --poetry \
    ../scripts/requirements/trio.in --benchmark resolve-cold --min-runs 20
```
