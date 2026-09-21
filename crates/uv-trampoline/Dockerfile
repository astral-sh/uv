# Reproducible cross-compilation environment for Windows trampolines.
#
# All toolchain versions are pinned for binary reproducibility.
# This image provides the build environment; the workspace is bind-mounted
# at runtime by scripts/build-trampolines.sh.

ARG UBUNTU_SNAPSHOT=20260127T200000Z

ARG RUST_NIGHTLY=nightly-2025-11-02

ARG CARGO_XWIN_VERSION=0.21.4
ARG XWIN_SDK_VERSION=10.0.22621
ARG XWIN_CRT_VERSION=14.44.17.14

FROM docker.io/ubuntu:26.04@sha256:4095ef613201918336b5d7d00be15d8b09c72ddb77c80bca249c255887a64d87 AS base

ARG RUST_NIGHTLY
ARG UBUNTU_SNAPSHOT

ENV HOME=/app
ENV CARGO_HOME=${HOME}/rust/cargo
ENV RUSTUP_HOME=${HOME}/rust/rustup
ENV XWIN_CACHE_DIR=${HOME}/xwin-cache

WORKDIR ${HOME}

# Install build dependencies
RUN apt install -y --update ca-certificates && \
    apt install -y --update --snapshot ${UBUNTU_SNAPSHOT} \
        curl \
        llvm \
        clang \
        lld \
    && ln -s /usr/bin/llvm-mt /usr/bin/mt.exe \
    && rm -rf /var/lib/apt/lists/*

# Install Rust nightly toolchain with Windows targets
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | \
    sh -s -- -y --default-toolchain=${RUST_NIGHTLY} --profile=minimal

ENV PATH="${CARGO_HOME}/bin:${PATH}"

RUN rustup component add rust-src --toolchain ${RUST_NIGHTLY}-x86_64-unknown-linux-gnu && \
    rustup target add --toolchain ${RUST_NIGHTLY} \
        i686-pc-windows-msvc \
        x86_64-pc-windows-msvc \
        aarch64-pc-windows-msvc

# Install cargo-xwin and prefetch the Windows SDK/CRT
FROM base AS xwin-cache

ARG CARGO_XWIN_VERSION
ARG XWIN_SDK_VERSION
ARG XWIN_CRT_VERSION

RUN cargo install cargo-xwin@${CARGO_XWIN_VERSION}

RUN cargo xwin cache xwin \
      --xwin-sdk-version ${XWIN_SDK_VERSION} \
      --xwin-crt-version ${XWIN_CRT_VERSION} \
      --xwin-arch x86,x86_64,aarch64

FROM base

COPY --from=xwin-cache ${HOME}/xwin-cache ${HOME}/xwin-cache
RUN chmod a+rwX ${HOME}/xwin-cache
COPY --from=xwin-cache ${CARGO_HOME}/bin/cargo-xwin ${CARGO_HOME}/bin/cargo-xwin
