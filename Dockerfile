FROM --platform=$BUILDPLATFORM ubuntu:24.04@sha256:d1e2e92c075e5ca139d51a140fff46f84315c0fdce203eab2807c7e495eff4f9 AS build

ARG UBUNTU_SNAPSHOT=20260301T000000Z

ENV HOME="/root"
WORKDIR $HOME

# Install dependencies using an Ubuntu snapshot for reproducibility.
# ca-certificates are required for using the snapshot.
RUN --mount=type=cache,target=/var/lib/apt/lists \
  apt install -y --update ca-certificates && \
  apt install -y --update --snapshot ${UBUNTU_SNAPSHOT} --no-install-recommends \
  build-essential \
  curl \
  python3-venv

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
# Install the toolchain then the musl target
RUN rustup toolchain install
RUN rustup target add $(cat rust_target.txt)

# Build
COPY crates crates
COPY ./Cargo.toml Cargo.toml
COPY ./Cargo.lock Cargo.lock
RUN case "${TARGETPLATFORM}" in \
  "linux/arm64") export JEMALLOC_SYS_WITH_LG_PAGE=16;; \
  esac && \
  cargo zigbuild --bin uv --bin uvx --target $(cat rust_target.txt) --release
RUN cp target/$(cat rust_target.txt)/release/uv /uv \
  && cp target/$(cat rust_target.txt)/release/uvx /uvx
# TODO(konsti): Optimize binary size, with a version that also works when cross compiling
# RUN strip --strip-all /uv

FROM scratch
COPY --from=build /uv /uvx /
WORKDIR /io
ENTRYPOINT ["/uv"]
