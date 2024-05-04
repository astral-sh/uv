# syntax=docker/dockerfile:1

FROM --platform=${BUILDPLATFORM} ubuntu:24.04 AS builder
# Configure the shell to exit early if any command fails, or when referencing unset variables.
# Additionally `-x` outputs each command run, this is helpful for troubleshooting failures.
SHELL ["/bin/bash", "-eux", "-o", "pipefail", "-c"]

RUN \
  --mount=target=/var/lib/apt/lists,type=cache,sharing=locked \
  --mount=target=/var/cache/apt,type=cache,sharing=locked \
  <<HEREDOC
    # https://github.com/moby/buildkit/blob/master/frontend/dockerfile/docs/reference.md#example-cache-apt-packages
    rm -f /etc/apt/apt.conf.d/docker-clean
    echo 'Binary::apt::APT::Keep-Downloaded-Packages "true";' > /etc/apt/apt.conf.d/keep-cache

    apt update && apt install -y --no-install-recommends \
      build-essential \
      curl \
      python3-venv \
      cmake
HEREDOC

ENV HOME="/root"
ENV PATH="$HOME/.venv/bin:$PATH"
WORKDIR $HOME

# Setup zig as cross compiling linker
RUN <<HEREDOC
  python3 -m venv $HOME/.venv
  .venv/bin/pip install --no-cache-dir cargo-zigbuild
HEREDOC

# Install rust
ENV PATH="$HOME/.cargo/bin:$PATH"
COPY rust-toolchain.toml .
ARG TARGETPLATFORM
RUN <<HEREDOC
  case "${TARGETPLATFORM}" in
    ( 'linux/arm64' )
      CARGO_BUILD_TARGET='aarch64-unknown-linux-musl'
      ;;
    ( 'linux/amd64' )
      CARGO_BUILD_TARGET='x86_64-unknown-linux-musl'
      ;;
    *) exit 1 ;;
  esac

  # Install `rustup` to match the toolchain version in `rust-toolchain.toml`:
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain none
  # Ensure toolchain to be installed upon rustup command uses the minimal profile to avoid excess layer weight:
  # https://github.com/rust-lang/rustup/issues/3805#issuecomment-2094066914
  echo 'profile = "minimal"' >> rust-toolchain.toml
  # Add the relevant musl target triple (to build uv as static binary):
  rustup target add "${CARGO_BUILD_TARGET}"

  # For the next RUN layer to reference:
  echo "${CARGO_BUILD_TARGET}" > rust_target.txt
HEREDOC

# Build app
ARG APP_NAME=uv
ARG CARGO_HOME=/usr/local/cargo
COPY crates/ crates/
COPY Cargo.toml Cargo.lock .
RUN \
  --mount=type=cache,target="/root/.cache/zig",id="zig-cache" \
  # Cache mounts (dirs for crates cache + build target):
  # https://doc.rust-lang.org/cargo/guide/cargo-home.html#caching-the-cargo-home-in-ci
  # CAUTION: As cargo uses multiple lock files (eg: `${CARGO_HOME}/{.global-cache,.package-cache,.package-cache-mutate}`), do not mount subdirs individually.
  --mount=type=cache,target="${CARGO_HOME}",id="cargo-cache" \
  # This cache mount is specific enough that you may not have any concurrent builds needing to share it, communicate that expectation explicitly:
  --mount=type=cache,target="target/",id="cargo-target-${APP_NAME}",sharing=locked \
  # These are redundant as they're easily reconstructed from cache above. Use TMPFS mounts to exclude from cache mounts:
  # TMPFS mount is a better choice than `rm -rf` command (which is risky on a cache mount that is shared across concurrent builds).
  --mount=type=tmpfs,target="${CARGO_HOME}/registry/src" \
  --mount=type=tmpfs,target="${CARGO_HOME}/git/checkouts" \
  <<HEREDOC
    cargo zigbuild --release --target "$(cat rust_target.txt)" --bin "${APP_NAME}"
    cp "target/$(cat rust_target.txt)/release/${APP_NAME}" "/${APP_NAME}"

    # TODO(konsti): Optimize binary size, with a version that also works when cross compiling
    # strip --strip-all /uv
HEREDOC

FROM scratch AS output
COPY --from=builder /uv /uv
WORKDIR /io
ENTRYPOINT ["/uv"]
