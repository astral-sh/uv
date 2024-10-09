FROM --platform=$BUILDPLATFORM ubuntu AS build
ENV HOME="/root"
WORKDIR $HOME

RUN apt update \
  && apt install -y --no-install-recommends \
  build-essential \
  curl \
  python3-venv \
  cmake \
  && apt clean \
  && rm -rf /var/lib/apt/lists/*

# Setup zig as cross compiling linker
RUN python3 -m venv $HOME/.venv
RUN .venv/bin/pip install cargo-zigbuild
ENV PATH="$HOME/.venv/bin:$PATH"

# Install rust
ARG TARGETPLATFORM
RUN case "$TARGETPLATFORM" in \
  "linux/arm64") echo "aarch64-unknown-linux-musl" > rust_target.txt ;; \
  "linux/amd64") echo "x86_64-unknown-linux-musl" > rust_target.txt ;; \
  *) exit 1 ;; \
  esac
# Update rustup whenever we bump the rust version
COPY rust-toolchain.toml rust-toolchain.toml
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --target $(cat rust_target.txt) --profile minimal --default-toolchain none
ENV PATH="$HOME/.cargo/bin:$PATH"
# Installs the correct toolchain version from rust-toolchain.toml and then the musl target
RUN rustup target add $(cat rust_target.txt)

# Build
COPY crates crates
COPY ./Cargo.toml Cargo.toml
COPY ./Cargo.lock Cargo.lock
RUN cargo zigbuild --bin uv --bin uvx --target $(cat rust_target.txt) --release
RUN cp target/$(cat rust_target.txt)/release/uv /uv \
  && cp target/$(cat rust_target.txt)/release/uvx /uvx
# TODO(konsti): Optimize binary size, with a version that also works when cross compiling
# RUN strip --strip-all /uv

FROM scratch
COPY --from=build /uv /uvx /
WORKDIR /io
ENTRYPOINT ["/uv"]
