# syntax=docker/dockerfile:1

FROM --platform=${BUILDPLATFORM} ubuntu:24.04 AS builder-base
# Configure the shell to exit early if any command fails, or when referencing unset variables.
# Additionally `-x` outputs each command run, this is helpful for troubleshooting failures.
SHELL ["/bin/bash", "-eux", "-o", "pipefail", "-c"]

RUN \
  --mount=target=/var/lib/apt/lists,type=cache,sharing=locked \
  --mount=target=/var/cache/apt,type=cache,sharing=locked \
  <<HEREDOC
    # https://github.com/moby/buildkit/blob/master/frontend/dockerfile/docs/reference.md#example-cache-apt-packages
    # https://stackoverflow.com/questions/66808788/docker-can-you-cache-apt-get-package-installs#comment135104889_72851168
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

# Setup zig as cross compiling linker:
RUN <<HEREDOC
  python3 -m venv $HOME/.venv
  .venv/bin/pip install --no-cache-dir cargo-zigbuild
HEREDOC

# Install rust:
ENV PATH="$HOME/.cargo/bin:$PATH"
COPY rust-toolchain.toml .
RUN <<HEREDOC
  # Install rustup, but skip installing a default toolchain as we only want the version from `rust-toolchain.toml`:
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain none

  # When rustup installs the toolchain ensure it actually uses the minimal profile, avoiding excess layer weight:
  # https://github.com/rust-lang/rustup/issues/3805#issuecomment-2094066914
  echo 'profile = "minimal"' >> rust-toolchain.toml
  echo 'targets = [ "aarch64-unknown-linux-musl", "x86_64-unknown-linux-musl" ]' >> rust-toolchain.toml
  # Add the relevant musl target triples (for a building binary with static linking):
  # Workaround until `ensure` arrives: https://github.com/rust-lang/rustup/issues/2686#issuecomment-788825744
  rustup show
HEREDOC

# Handle individual images differences for ARM64 / AMD64:
FROM builder-base AS builder-arm64
ENV CARGO_BUILD_TARGET=aarch64-unknown-linux-musl

FROM builder-base AS builder-amd64
ENV CARGO_BUILD_TARGET=x86_64-unknown-linux-musl

# Build app:
FROM builder-${TARGETARCH} AS builder-app
COPY crates/ crates/
COPY Cargo.toml Cargo.lock .
ARG APP_NAME=uv
ARG CARGO_HOME=/usr/local/cargo
ARG RUSTFLAGS="-C strip=symbols -C relocation-model=static -C target-feature=+crt-static -C opt-level=z"
ARG TARGETPLATFORM
RUN \
  --mount=type=cache,target="/root/.cache/zig",id="zig-cache" \
  # Cache mounts (dirs for crates cache + build target):
  # https://doc.rust-lang.org/cargo/guide/cargo-home.html#caching-the-cargo-home-in-ci
  # CAUTION: As cargo uses multiple lock files (eg: `${CARGO_HOME}/{.global-cache,.package-cache,.package-cache-mutate}`), do not mount subdirs individually.
  --mount=type=cache,target="${CARGO_HOME}",id="cargo-cache" \
  # This cache mount is specific enough that you may not have any concurrent builds needing to share it, communicate that expectation explicitly:
  --mount=type=cache,target="target/",id="cargo-target-${APP_NAME}-${TARGETPLATFORM}",sharing=locked \
  # These are redundant as they're easily reconstructed from cache above. Use TMPFS mounts to exclude from cache mounts:
  # TMPFS mount is a better choice than `rm -rf` command (which is risky on a cache mount that is shared across concurrent builds).
  --mount=type=tmpfs,target="${CARGO_HOME}/registry/src" \
  --mount=type=tmpfs,target="${CARGO_HOME}/git/checkouts" \
  <<HEREDOC
    cargo zigbuild --release --bin "${APP_NAME}" --target "${CARGO_BUILD_TARGET}"
    cp "target/${CARGO_BUILD_TARGET}/release/${APP_NAME}" "/${APP_NAME}"
HEREDOC

# Final stage - Image containing only uv + empty /io dir:
FROM scratch
COPY --from=builder-app /uv /uv
WORKDIR /io
ENTRYPOINT ["/uv"]
