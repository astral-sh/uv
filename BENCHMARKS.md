# Benchmarks

All benchmarks were computed on macOS, and come with a few important caveats:

- Benchmark performance may vary dramatically across different operating systems and filesystems.
  In particular, Puffin uses different installation strategies based on the underlying filesystem's
  capabilities. (For example, Puffin uses reflinking on macOS, and hardlinking on Linux.)
- Benchmark performance may vary dramatically depending on the set of packages being installed.
  For example, a resolution that requires building a single intensive source distribution may appear
  very similar across tools, since the bottleneck is tool-agnostic. This document benchmarks against
  Trio's dependencies, as a representative example of a realistic real-world project.
- Unlike Poetry, Puffin and pip-tools do not generate multi-platform lockfiles. As such, Poetry is
  (by design) doing significantly more work than the other tools in the benchmarks. Poetry is
  included for completeness, as many projects may not _need_ a multi-platform lockfile. However,
  it's critical to understand that benchmarking resolution against Poetry is an unfair comparison.
  (Benchmarking installation, however, _is_ a fair comparison.)

In each case, a smaller bar (i.e., lower) is better.

## Warm Resolution

Benchmarking dependency resolution (e.g., `puffin pip compile`) with a warm cache, but no existing
lockfile. This is equivalent to blowing away an existing `requirements.txt` file to regenerate it
from a `requirements.in` file.

![](https://github.com/astral-sh/ruff/assets/1309177/aab99181-e54e-4bdb-9ce6-15b018ef8466)

## Cold Resolution

Benchmarking dependency resolution (e.g., `puffin pip compile`) with a cold cache. This is
equivalent to running `puffin pip compile` on a new machine or in CI (assuming that the package
manager cache is not shared across runs).

![](https://github.com/astral-sh/ruff/assets/1309177/aab99181-e54e-4bdb-9ce6-15b018ef8466)

## Warm Installation

Benchmarking package installation (e.g., `puffin pip sync`) with a warm cache. This is equivalent
to removing and recreating a virtual environment, and then populating it with dependencies that
you've installed previously on the same machine.

![](https://github.com/astral-sh/ruff/assets/1309177/aab99181-e54e-4bdb-9ce6-15b018ef8466)

## Cold Installation

Benchmarking package installation (e.g., `puffin pip sync`) with a cold cache. This is equivalent
to running `puffin pip sync` on a new machine or in CI (assuming that the package manager cache is
not shared across runs).

![](https://github.com/astral-sh/ruff/assets/1309177/aab99181-e54e-4bdb-9ce6-15b018ef8466)

## Reproduction

All benchmarks were generated using the `scripts/bench/__main__.py` script, which wraps
[`hyperfine`](https://github.com/sharkdp/hyperfine) to facilitate benchmarking Puffin
against a variety of other tools.

For example, to benchmark Puffin's warm resolution against pip-compile:

```shell
python -m scripts.bench \
    --puffin \
    --pip-compile \
    --benchmark resolve-warm \
    scripts/requirements/black.in
```

The benchmark script itself has a several requirements:

- A local Puffin release build (`cargo build --release`).
- A virtual environment with the script's own dependencies installed (see: `scripts/bench/requirements.txt`).
- The [`hyperfine`](https://github.com/sharkdp/hyperfine) command-line tool installed on your system.

After running the benchmark script, you can generate the corresponding graph via:

```shell
cargo run -p puffin-dev render-benchmarks results.json --title "Warm Resolution"
```

## Acknowledgements

The inclusion of this `BENCHMARKS.md` file was inspired by the excellent benchmarking documentation
in [Orogene](https://github.com/orogene/orogene/blob/472e481b4fc6e97c2b57e69240bf8fe995dfab83/BENCHMARKS.md).
