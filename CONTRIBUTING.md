# Contributing

## Finding ways to help

We label issues that would be good for a first time contributor as
[`good first issue`](https://github.com/astral-sh/uv/issues?q=is%3Aopen+is%3Aissue+label%3A%22good+first+issue%22).
These usually do not require significant experience with Rust or the uv code base.

We label issues that we think are a good opportunity for subsequent contributions as
[`help wanted`](https://github.com/astral-sh/uv/issues?q=is%3Aopen+is%3Aissue+label%3A%22help+wanted%22).
These require varying levels of experience with Rust and uv. Often, we want to accomplish these
tasks but do not have the resources to do so ourselves.

You don't need our permission to start on an issue we have labeled as appropriate for community
contribution as described above. However, it's a good idea to indicate that you are going to work on
an issue to avoid concurrent attempts to solve the same problem.

Please check in with us before starting work on an issue that has not been labeled as appropriate
for community contribution. We're happy to receive contributions for other issues, but it's
important to make sure we have consensus on the solution to the problem first.

Outside of issues with the labels above, issues labeled as
[`bug`](https://github.com/astral-sh/uv/issues?q=is%3Aopen+is%3Aissue+label%3A%22bug%22) are the
best candidates for contribution. In contrast, issues labeled with `needs-decision` or
`needs-design` are _not_ good candidates for contribution. Please do not open pull requests for
issues with these labels.

Please do not open pull requests for new features without prior discussion. While we appreciate
exploration of new features, we will almost always close these pull requests immediately. Adding a
new feature to uv creates a long-term maintenance burden and requires strong consensus from the uv
team before it is appropriate to begin work on an implementation.

Do not use LLMs such as Claude Code or ChatGPT for communication. LLMs are notoriously unreliable
and make up smart sounding but ultimately wrong claims. Instead, phrase issue and pull request
comments in your own words. It's not important to sound perfect, but that we can follow what problem
you're trying to solve and why the pull request uses a correct approach.

## Setup

[Rust](https://rustup.rs/) (and a C compiler) are required to build uv.

On Ubuntu and other Debian-based distributions, you can install a C compiler with:

```shell
sudo apt install build-essential
```

On Fedora-based distributions, you can install a C compiler with:

```shell
sudo dnf install gcc
```

## Testing

For running tests, we recommend [nextest](https://nexte.st/).

To run a specific test by name:

```shell
cargo nextest run -E 'test(test_name)'
```

To run all tests and accept snapshot changes:

```shell
cargo insta test --accept --test-runner nextest
```

To update snapshots for a specific test:

```shell
cargo insta test --accept --test-runner nextest -- <test_name>
```

### Python

Testing uv requires multiple specific Python versions; they can be installed with:

```shell
cargo run python install
```

The storage directory can be configured with `UV_PYTHON_INSTALL_DIR`. (It must be an absolute path.)

### Snapshot testing

uv uses [insta](https://insta.rs/) for snapshot testing. It's recommended (but not necessary) to use
`cargo-insta` for a better snapshot review experience. See the
[installation guide](https://insta.rs/docs/cli/) for more information.

In tests, you can use `uv_snapshot!` macro to simplify creating snapshots for uv commands. For
example:

```rust
#[test]
fn test_add() {
    let context = TestContext::new("3.12");
    uv_snapshot!(context.filters(), context.add().arg("requests"), @"");
}
```

To run and review a specific snapshot test:

```shell
cargo test --package <package> --test <test> -- <test_name> -- --exact
cargo insta review
```

### Git and Git LFS

A subset of uv tests require both [Git](https://git-scm.com) and [Git LFS](https://git-lfs.com/) to
execute properly.

These tests can be disabled by turning off either `git` or `git-lfs` uv features.

### Local testing

You can invoke your development version of uv with `cargo run -- <args>`. For example:

```shell
cargo run -- venv
cargo run -- pip install requests
```

## Formatting

```shell
# Rust
cargo fmt --all

# Python
uvx ruff format .

# Markdown, YAML, and other files (requires Node.js)
npx prettier --write .
# or in Docker
docker run --rm -v .:/src/ -w /src/ node:alpine npx prettier --write .
```

## Linting

Linting requires [shellcheck](https://github.com/koalaman/shellcheck) and
[cargo-shear](https://github.com/Boshen/cargo-shear) to be installed separately.

```shell
# Rust
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings

# Python
uvx ruff check .

# Python type checking
uvx ty check python/uv

# Shell scripts
shellcheck <script>

# Spell checking
uvx typos

# Unused Rust dependencies
cargo shear
```

### Compiling for Windows from Unix

To run clippy for a Windows target from Linux or macOS, you can use
[cargo-xwin](https://github.com/rust-cross/cargo-xwin):

```shell
# Install cargo-xwin
cargo install cargo-xwin --locked

# Add the Windows target
rustup target add x86_64-pc-windows-msvc

# Run clippy for Windows
cargo xwin clippy --workspace --all-targets --all-features --locked -- -D warnings
```

## Crate structure

Rust does not allow circular dependencies between crates. To visualize the crate hierarchy, install
[cargo-depgraph](https://github.com/jplatte/cargo-depgraph) and graphviz, then run:

```shell
cargo depgraph --dedup-transitive-deps --workspace-only | dot -Tpng > graph.png
```

## Running inside a Docker container

Source distributions can run arbitrary code on build and can make unwanted modifications to your
system
(["Someone's Been Messing With My Subnormals!" on Blogspot](https://moyix.blogspot.com/2022/09/someones-been-messing-with-my-subnormals.html),
["nvidia-pyindex" on PyPI](https://pypi.org/project/nvidia-pyindex/)), which can even occur when
just resolving requirements. To prevent this, there's a Docker container you can run commands in:

```console
$ docker build -t uv-builder -f crates/uv-dev/builder.dockerfile --load .
# Build for musl to avoid glibc errors, might not be required with your OS version
cargo build --target x86_64-unknown-linux-musl --profile profiling
docker run --rm -it -v $(pwd):/app uv-builder /app/target/x86_64-unknown-linux-musl/profiling/uv-dev resolve-many --cache-dir /app/cache-docker /app/scripts/popular_packages/pypi_10k_most_dependents.txt
```

We recommend using this container if you don't trust the dependency tree of the package(s) you are
trying to resolve or install.

## Profiling and Benchmarking

Please refer to Ruff's
[Profiling Guide](https://github.com/astral-sh/ruff/blob/main/CONTRIBUTING.md#profiling-projects),
it applies to uv, too.

We provide diverse sets of requirements for testing and benchmarking the resolver in
`test/requirements` and for the installer in `test/requirements/compiled`.

You can use `scripts/benchmark` to benchmark predefined workloads between uv versions and with other
tools, e.g., from the `scripts/benchmark` directory:

```shell
uv run resolver \
    --uv-pip \
    --poetry \
    --benchmark \
    resolve-cold \
    ../test/requirements/trio.in
```

### Analyzing concurrency

You can use [tracing-durations-export](https://github.com/konstin/tracing-durations-export) to
visualize parallel requests and find any spots where uv is CPU-bound. Example usage, with `uv` and
`uv-dev` respectively:

```shell
RUST_LOG=uv=info TRACING_DURATIONS_FILE=target/traces/jupyter.ndjson cargo run --features tracing-durations-export --profile profiling -- pip compile test/requirements/jupyter.in
```

```shell
RUST_LOG=uv=info TRACING_DURATIONS_FILE=target/traces/jupyter.ndjson cargo run --features tracing-durations-export --bin uv-dev --profile profiling -- resolve jupyter
```

### Trace-level logging

You can enable `trace` level logging using the `RUST_LOG` environment variable, i.e.

```shell
RUST_LOG=trace uv
```

## Documentation

To preview any changes to the documentation locally:

1. Install the [Rust toolchain](https://www.rust-lang.org/tools/install).

2. Run `cargo dev generate-all`, to update any auto-generated documentation.

3. Run the development server with:

   ```shell
   uvx --with-requirements docs/requirements.txt -- mkdocs serve -f mkdocs.yml
   ```

The documentation should then be available locally at
[http://127.0.0.1:8000/uv/](http://127.0.0.1:8000/uv/).

To update the documentation dependencies, edit `docs/requirements.in`, then run:

```shell
uv pip compile docs/requirements.in -o docs/requirements.txt --universal -p 3.12
```

Documentation is deployed automatically on release by publishing to the
[Astral documentation](https://github.com/astral-sh/docs) repository, which itself deploys via
Cloudflare Pages.

After making changes to the documentation, [format the markdown files](#formatting) using Prettier.

## Development code signing on macOS

Code signing can only be performed by Astral team members.

Code signing on macOS can improve developer experience when running tests, e.g., when running tests
that access the macOS keychain, a signed binary can be approved once but an unsigned binary will
need to be approved on each re-compile.

### Acquiring a development certificate

1. Generate a
   [request for the certificate](https://developer.apple.com/help/account/certificates/create-a-certificate-signing-request)
2. Create a certificate in the
   [Apple Developer portal](https://developer.apple.com/account/resources/certificates/list)
3. Download and install the certificate to your login keychain

   ```shell
   security import ~/Downloads/mac_development.cer -k ~/Library/Keychains/login.keychain-db
   ```

4. Identify your code signing identity

   ```shell
   security find-identity -v -p codesigning
   ```

5. If the above fails to find your identity, install the intermediate certificates

   ```shell
   curl -sLO "https://www.apple.com/certificateauthority/AppleWWDRCAG3.cer"
   security import AppleWWDRCAG3.cer -k ~/Library/Keychains/login.keychain-db
   rm AppleWWDRCAG3.cer
   ```

6. Set `UV_TEST_CODESIGN_IDENTITY`

   ```shell
   export UV_TEST_CODESIGN_IDENTITY="Mac Developer: Your Name (TEAM_ID)"
   ```

Note `UV_TEST_CODESIGN_IDENTITY` is only supported via `nextest`.

## Releases

Releases can only be performed by Astral team members.

Changelog entries and version bumps are automated. First, run:

```shell
./scripts/release.sh
```

Then, editorialize the `CHANGELOG.md` file to ensure entries are consistently styled.

Then, open a pull request, e.g., `Bump version to ...`.

Binary builds will automatically be tested for the release.

After merging the pull request, run the
[release workflow](https://github.com/astral-sh/uv/actions/workflows/release.yml) with the version
tag. **Do not include a leading `v`**. The release will automatically be created on GitHub after
everything else publishes.
