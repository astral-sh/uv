FROM --platform=$BUILDPLATFORM ubuntu AS build
ENV HOME="/root" \
  # Place tool-specific caches in the buildkit tool cache.
  CARGO_HOME=/buildkit-cache/cargo \
  CARGO_ZIGBUILD_CACHE_DIR=/buildkit-cache/cargo-zigbuild \
  PIP_CACHE_DIR=/buildkit-cache/pip \
  RUSTUP_HOME=/buildkit-cache/rustup \
  ZIG_GLOBAL_CACHE_DIR=/buildkit-cache/zig
WORKDIR $HOME

RUN \
  --mount=type=cache,target=/var/cache/apt,sharing=locked \
  --mount=type=cache,target=/var/lib/apt,sharing=locked \
  # remove the default docker-specific apt config that auto-deletes /var/apt/cache archives
  rm -f /etc/apt/apt.conf.d/docker-clean && \
  # and configure apt-get to keep downloaded archives in the cache
  echo 'Binary::apt::APT::Keep-Downloaded-Packages "true";' >/etc/apt/apt.conf.d/keep-cache && \
  apt update \
  && apt install -y --no-install-recommends \
  build-essential \
  curl \
  python3-venv \
  cmake

# Setup zig as cross compiling linker
RUN python3 -m venv $HOME/.venv
RUN \
  --mount=type=cache,target=/buildkit-cache,id="tool-caches" \
  .venv/bin/pip install cargo-zigbuild
ENV PATH="$HOME/.venv/bin:$PATH"

# Install rust
ARG TARGETPLATFORM
RUN case "$TARGETPLATFORM" in \
  "linux/arm64") echo "aarch64-unknown-linux-musl" > rust_target.txt ;; \
  "linux/amd64") echo "x86_64-unknown-linux-musl" > rust_target.txt ;; \
  *) exit 1 ;; \
  esac

# Update rustup whenever we bump the rust version
ENV PATH="$CARGO_HOME/bin:$PATH"
COPY rust-toolchain.toml rust-toolchain.toml
RUN \
  --mount=type=cache,target=/buildkit-cache,id="tool-caches" \
  ( \
    rustup self update \
    || curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --target $(cat rust_target.txt) --profile minimal --default-toolchain none \
  ) \
  # Installs the correct toolchain version from rust-toolchain.toml and then the musl target
  && rustup target add $(cat rust_target.txt)

# Build
RUN \
  # bind mounts to access Cargo config, lock, and sources, without having to
  # copy them into the build layer and so bloat the docker build cache
  --mount=type=bind,source=crates,target=crates \
  --mount=type=bind,source=Cargo.toml,target=Cargo.toml \
  --mount=type=bind,source=Cargo.lock,target=Cargo.lock \
  # Cache mounts to speed up builds
  --mount=type=cache,target=$HOME/target/ \
  --mount=type=cache,target=/buildkit-cache,id="tool-caches" \
  case "${TARGETPLATFORM}" in \
  "linux/arm64") export JEMALLOC_SYS_WITH_LG_PAGE=16;; \
  esac && \
  cargo zigbuild --bin uv --bin uvx --target $(cat rust_target.txt) --release \
  && cp target/$(cat rust_target.txt)/release/uv /uv \
  && cp target/$(cat rust_target.txt)/release/uvx /uvx
# TODO(konsti): Optimize binary size, with a version that also works when cross compiling
# RUN strip --strip-all /uv

FROM scratch
COPY --from=build /uv /uvx /
WORKDIR /io
ENTRYPOINT ["/uv"]
