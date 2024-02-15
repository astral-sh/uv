# Benchmarks

All benchmarks were computed on macOS using Python 3.12.0 (for non-uv tools), and come with a few
important caveats:

- Benchmark performance may vary dramatically across different operating systems and filesystems.
  In particular, uv uses different installation strategies based on the underlying filesystem's
  capabilities. (For example, uv uses reflinking on macOS, and hardlinking on Linux.)
- Benchmark performance may vary dramatically depending on the set of packages being installed.
  For example, a resolution that requires building a single intensive source distribution may appear
  very similar across tools, since the bottleneck is tool-agnostic.
- Unlike Poetry, both uv and pip-tools do _not_ generate multi-platform lockfiles. As such,
  Poetry is (by design) doing significantly more work than other tools in the resolution benchmarks.
  Poetry is included for completeness, as many projects may not _need_ a multi-platform lockfile.
  However, it's critical to understand that benchmarking uv's resolution time against Poetry is
  an unfair comparison. (Benchmarking installation, however, _is_ a fair comparison.)

This document benchmarks against Trio's `docs-requirements.in`, as a representative example of a
real-world project.

In each case, a smaller bar (i.e., lower) is better.

## Warm Installation

Benchmarking package installation (e.g., `uv pip sync`) with a warm cache. This is equivalent
to removing and recreating a virtual environment, and then populating it with dependencies that
you've installed previously on the same machine.

![install-warm](./assets/png/resolve-cold.png)

## Cold Installation

Benchmarking package installation (e.g., `uv pip sync`) with a cold cache. This is equivalent
to running `uv pip sync` on a new machine or in CI (assuming that the package manager cache is
not shared across runs).

![install-cold](./assets/png/resolve-cold.png)

## Warm Resolution

Benchmarking dependency resolution (e.g., `uv pip compile`) with a warm cache, but no existing
lockfile. This is equivalent to blowing away an existing `requirements.txt` file to regenerate it
from a `requirements.in` file.

![resolve-warm](./assets/png/resolve-cold.png)

## Cold Resolution

Benchmarking dependency resolution (e.g., `uv pip compile`) with a cold cache. This is
equivalent to running `uv pip compile` on a new machine or in CI (assuming that the package
manager cache is not shared across runs).

![resolve-cold](./assets/png/resolve-cold.png)

## Reproduction

All benchmarks were generated using the `scripts/bench/__main__.py` script, which wraps
[`hyperfine`](https://github.com/sharkdp/hyperfine) to facilitate benchmarking uv
against a variety of other tools.

The benchmark script itself has a several requirements:

- A local uv release build (`cargo build --release`).
- A virtual environment with the script's own dependencies installed (`uv venv && uv pip sync scripts/bench/requirements.txt`).
- The [`hyperfine`](https://github.com/sharkdp/hyperfine) command-line tool installed on your system.

To benchmark uv's resolution against pip-compile and Poetry:

```shell
python -m scripts.bench \
    --uv \
    --poetry \
    --pip-compile \
    --benchmark resolve-warm \
    scripts/requirements/trio.in \
    --json
```

To benchmark uv's installation against pip-sync and Poetry:

```shell
python -m scripts.bench \
    --uv \
    --poetry \
    --pip-sync \
    --benchmark install-warm \
    scripts/requirements/compiled/trio.txt \
    --json
```

After running the benchmark script, you can generate the corresponding graph via:

```shell
cargo run -p uv-dev render-benchmarks resolve-warm.json --title "Warm Resolution"
cargo run -p uv-dev render-benchmarks resolve-cold.json --title "Cold Resolution"
cargo run -p uv-dev render-benchmarks install-warm.json --title "Warm Installation"
cargo run -p uv-dev render-benchmarks install-cold.json --title "Cold Installation"
```

You need to install the [Roboto Font](https://fonts.google.com/specimen/Roboto) if the labels are missing in the generated graph.

## Acknowledgements

The inclusion of this `BENCHMARKS.md` file was inspired by the excellent benchmarking documentation
in [Orogene](https://github.com/orogene/orogene/blob/472e481b4fc6e97c2b57e69240bf8fe995dfab83/BENCHMARKS.md).
