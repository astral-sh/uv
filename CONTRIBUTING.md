# Contributing

## Setup

[Rust](https://rustup.rs/), a C compiler, and CMake are required to build Puffin.

### Linux


On Ubuntu and other Debian-based distributions, you can install the C compiler and CMake with

```shell
sudo apt install build-essential cmake
```

### macOS

CMake may be installed with Homebrew:

```
brew install cmake
```

The Python bootstrapping script requires `coreutils` and `zstd`; we recommend installing them with Homebrew:

```
brew install coreutils zstd
```

See the [Python](#python) section for instructions on installing the Python versions.

### Windows

You can install CMake from the [installers](https://cmake.org/download/) or with `pipx install cmake` (make sure that the pipx install path is in `PATH`, pipx complains if it isn't).

## Testing

Testing Puffin requires multiple specific Python versions. Bootstrap them to `<project root>/bin`:

```shell
pipx run scripts/bootstrap/install.py
```

Alternatively you can also install `zstandard` from pypi and run

```
python3.12 scripts/bootstrap/install.py
```

To run the tests we recommend [nextest](https://nexte.st/). Make sure to run the tests with `--all-features`, otherwise you'll miss most of our integration tests.

## Running inside a docker container

Source distributions can run arbitrary code on build and can make unwanted modifications to your system (https://moyix.blogspot.com/2022/09/someones-been-messing-with-my-subnormals.html, https://pypi.org/project/nvidia-pyindex/), which can even occur when just resolving requirements. To prevent this, there's a Docker container you can run commands in:

```bash
docker buildx build -t puffin-builder -f builder.dockerfile --load .
# Build for musl to avoid glibc errors, might not be required with your OS version
cargo build --target x86_64-unknown-linux-musl --profile profiling --features vendored-openssl
docker run --rm -it -v $(pwd):/app puffin-builder /app/target/x86_64-unknown-linux-musl/profiling/puffin-dev resolve-many --cache-dir /app/cache-docker /app/scripts/popular_packages/pypi_10k_most_dependents.txt
```

We recommend using this container if you don't trust the dependency tree of the package(s) you are trying to resolve or install. 


## Profiling

Please refer to Ruff's [Profiling Guide](https://github.com/astral-sh/ruff/blob/main/CONTRIBUTING.md#profiling-projects), it applies to Puffin, too.

### Analysing concurrency

You can use [tracing-durations-export](https://github.com/konstin/tracing-durations-export) to visualize parallel requests and find any spots where Puffin is CPU-bound. Example usage, with `puffin` and `puffin-dev` respectively:

```bash
RUST_LOG=puffin=info TRACING_DURATIONS_FILE=target/traces/jupyter.ndjson cargo run --features tracing-durations-export --profile profiling -- pip compile scripts/requirements/jupyter.in
```

```bash
RUST_LOG=puffin=info TRACING_DURATIONS_FILE=target/traces/jupyter.ndjson cargo run --features tracing-durations-export --bin puffin-dev --profile profiling -- resolve jupyter
```
