# Contributing

## Setup

[Rust](https://rustup.rs/), a C compiler, and CMake are required to build Puffin.

Testing Puffin requires multiple specific Python versions. We provide a script to bootstrap development by downloading the required versions.

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

### Python

Install required Python versions with the bootstrapping script:

```
scripts/bootstrap/install
```

Then add the Python binaries to your path:

```
export PATH=$PWD/bin:$PATH
```

We also strongly recommend setting the `PUFFIN_PYTHON_PATH` variable to prevent your system Python versions from
being found during tests:

```
export PUFFIN_PYTHON_PATH=$PWD/bin
```

If you use [direnv](https://direnv.net/), these variables will be exported automatically after you run `direnv allow`.

## Testing

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

