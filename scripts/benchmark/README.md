# benchmark

Benchmarking scripts for uv and other package management tools.

## Getting Started

From the `scripts/benchmark` directory:

```shell
uv run resolver \
    --uv-pip \
    --poetry \
    --benchmark \
    resolve-cold \
    ../requirements/trio.in
```

## Python shim startup

Build the draft's optimized benchmark binaries from the repository root:

```shell
cargo build --locked --profile profiling --package uv --bin uv --bin uv-python
```

With Python 3.11+ and `hyperfine` installed, run the standard-library-only driver (no package
installation is needed):

```shell
python3 scripts/benchmark/src/benchmark/python_shim.py \
    --uv-path target/profiling/uv \
    --python 3.12 \
    --output-dir "$HOME/code/tmp/python-shim-benchmark"
```

The driver requires a POSIX host and a new output directory. It installs one managed CPython and the
actual shims in an isolated fixture, creates a virtual environment, and checks that every shim
selects the same interpreter and environment as direct Python. Setup can download Python; timed runs
are offline.

It measures `python -c pass` directly and through `python`, `python3`, and the minor-version shim,
without a timing shell. `uv python find cpython` is included as a lookup-only diagnostic, not as an
equivalent Python invocation. Scenarios cover no virtual environment, an automatically discovered
`.venv`, and an explicit `VIRTUAL_ENV` outside the project directory.

Warm runs retain uv's cache. Cold runs delete the isolated uv cache before each sample; this
preparation is excluded from timing, and OS filesystem caches are **not** evicted. The default is
three repetitions of 100 samples after 10 warmups, with command and scenario order reversed on
alternating repetitions.

Raw Hyperfine samples, binary hashes, interpreter-selection checks, and cold/warm discovery traces
are retained in the output directory. `summary.json` reports the median of each repetition's median,
the range of repetition medians, and the median overhead/ratio against direct Python in the same
repetition. The fixture is removed when the driver exits. Profiling builds are optimized but omit
the release LTO and distribution PGO, so record the build configuration with any shared results.
