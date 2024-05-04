# syntax=docker/dockerfile:1

FROM --platform=${BUILDPLATFORM} ubuntu AS builder
# Configure the shell to exit early if any command fails, or when referencing unset variables.
# Additionally `-x` outputs each command run, this is helpful for troubleshooting failures.
SHELL ["/bin/bash", "-eux", "-o", "pipefail", "-c"]

RUN <<HEREDOC
  apt update && apt install -y --no-install-recommends \
    build-essential \
    curl \
    python3-venv \
    cmake

  apt clean
  rm -rf /var/lib/apt/lists/*
HEREDOC

ENV HOME="/root"
ENV PATH="$HOME/.venv/bin:$PATH"
WORKDIR $HOME

# Setup zig as cross compiling linker
RUN <<HEREDOC
  python3 -m venv $HOME/.venv
  .venv/bin/pip install cargo-zigbuild
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
  # Add the relevant musl target triple (to build uv as static binary):
  rustup target add "${CARGO_BUILD_TARGET}"

  # For the next RUN layer to reference:
  echo "${CARGO_BUILD_TARGET}" > rust_target.txt
HEREDOC

# Build uv
COPY crates/ crates/
COPY Cargo.toml Cargo.lock .
RUN <<HEREDOC
  cargo zigbuild --target "$(cat rust_target.txt)" --bin uv --release
  cp "target/$(cat rust_target.txt)/release/uv" /uv

  # TODO(konsti): Optimize binary size, with a version that also works when cross compiling
  # strip --strip-all /uv
HEREDOC

FROM scratch AS output
COPY --from=builder /uv /uv
WORKDIR /io
ENTRYPOINT ["/uv"]
