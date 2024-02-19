# Contributing

## Setup

[Rust](https://rustup.rs/), a C compiler, and CMake are required to build uv.

### Linux

On Ubuntu and other Debian-based distributions, you can install the C compiler and CMake with

```shell
sudo apt install build-essential cmake
```

### macOS

CMake may be installed with Homebrew:

```shell
brew install cmake
```

The Python bootstrapping script requires `coreutils` and `zstd`; we recommend installing them with Homebrew:

```shell
brew install coreutils zstd
```

See the [Python](#python) section for instructions on installing the Python versions.

### Windows

You can install CMake from the [installers](https://cmake.org/download/) or with `pipx install cmake` 
(make sure that the pipx install path is in `PATH`, pipx complains if it isn't).

## Testing

For running tests, we recommend [nextest](https://nexte.st/).

### Python

Testing uv requires multiple specific Python versions. You can install them into
`<project root>/bin` via our bootstrapping script:

```shell
pipx run scripts/bootstrap/install.py
```

Alternatively, you can install `zstandard` from PyPI, then run:

```shell
python3.12 scripts/bootstrap/install.py
```

## Running inside a docker container

Source distributions can run arbitrary code on build and can make unwanted modifications to your system (["Someone's Been Messing With My Subnormals!" on Blogspot](https://moyix.blogspot.com/2022/09/someones-been-messing-with-my-subnormals.html), ["nvidia-pyindex" on PyPI](https://pypi.org/project/nvidia-pyindex/)), which can even occur when just resolving requirements. To prevent this, there's a Docker container you can run commands in:

```bash
docker buildx build -t uv-builder -f builder.dockerfile --load .
# Build for musl to avoid glibc errors, might not be required with your OS version
cargo build --target x86_64-unknown-linux-musl --profile profiling --features vendored-openssl
docker run --rm -it -v $(pwd):/app uv-builder /app/target/x86_64-unknown-linux-musl/profiling/uv-dev resolve-many --cache-dir /app/cache-docker /app/scripts/popular_packages/pypi_10k_most_dependents.txt
```

We recommend using this container if you don't trust the dependency tree of the package(s) you are trying to resolve or install.

## Profiling

Please refer to Ruff's [Profiling Guide](https://github.com/astral-sh/ruff/blob/main/CONTRIBUTING.md#profiling-projects), it applies to uv, too.

### Analysing concurrency

You can use [tracing-durations-export](https://github.com/konstin/tracing-durations-export) to visualize parallel requests and find any spots where uv is CPU-bound. Example usage, with `uv` and `uv-dev` respectively:

```shell
RUST_LOG=uv=info TRACING_DURATIONS_FILE=target/traces/jupyter.ndjson cargo run --features tracing-durations-export --profile profiling -- pip compile scripts/requirements/jupyter.in
```

```shell
RUST_LOG=uv=info TRACING_DURATIONS_FILE=target/traces/jupyter.ndjson cargo run --features tracing-durations-export --bin uv-dev --profile profiling -- resolve jupyter
```

### Trace-level logging

You can enable `trace` level logging using the `RUST_LOG` environment variable, i.e.

```shell
RUST_LOG=trace uv …
```
