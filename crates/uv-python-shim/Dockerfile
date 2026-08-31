# Reproducible build environment for the non-macOS Python shims.
# Source and output are mounted separately, as for the Windows trampolines.
ARG UBUNTU_SNAPSHOT=20260127T200000Z
ARG RUST_VERSION=1.97.1
ARG CARGO_XWIN_VERSION=0.22.0

FROM docker.io/ubuntu:26.04@sha256:4095ef613201918336b5d7d00be15d8b09c72ddb77c80bca249c255887a64d87

ARG UBUNTU_SNAPSHOT
ARG RUST_VERSION
ARG CARGO_XWIN_VERSION

ENV HOME=/app \
    CARGO_HOME=/app/rust/cargo \
    RUSTUP_HOME=/app/rust/rustup \
    XWIN_CACHE_DIR=/app/xwin-cache \
    TMPDIR=/app/tmp \
    PATH=/app/rust/cargo/bin:/opt/zig:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin

RUN mkdir -p /app/tmp && apt install -y --update ca-certificates && \
    apt install -y --update --snapshot ${UBUNTU_SNAPSHOT} \
        ca-certificates curl python3 xz-utils llvm clang lld gcc libc6-dev && \
    ln -s /usr/bin/llvm-mt /usr/bin/mt.exe && \
    rm -rf /var/lib/apt/lists/*

RUN curl --proto '=https' --tlsv1.2 -fsSL https://sh.rustup.rs | \
    sh -s -- -y --default-toolchain=${RUST_VERSION} --profile=minimal && \
    rustup component add llvm-tools && \
    rustup target add \
        aarch64-unknown-linux-musl x86_64-unknown-linux-musl \
        i686-unknown-linux-musl arm-unknown-linux-musleabihf \
        powerpc64le-unknown-linux-musl riscv64gc-unknown-linux-musl \
        s390x-unknown-linux-gnu x86_64-unknown-freebsd \
        aarch64-pc-windows-msvc x86_64-pc-windows-msvc i686-pc-windows-msvc

RUN cargo install --locked cargo-xwin@${CARGO_XWIN_VERSION} && \
    cargo xwin cache xwin \
        --xwin-sdk-version 10.0.22621 --xwin-crt-version 14.44.17.14 \
        --xwin-arch x86,x86_64,aarch64

RUN curl --proto '=https' --tlsv1.2 -fsSL \
        https://ziglang.org/download/0.16.0/zig-x86_64-linux-0.16.0.tar.xz -o /app/zig.tar.xz && \
    echo '70e49664a74374b48b51e6f3fdfbf437f6395d42509050588bd49abe52ba3d00  /app/zig.tar.xz' | sha256sum -c - && \
    mkdir -p /opt/zig && tar -xJf /app/zig.tar.xz -C /opt/zig --strip-components=1 && \
    rm /app/zig.tar.xz

RUN curl --proto '=https' --tlsv1.2 -fsSL \
        https://download.freebsd.org/releases/amd64/14.3-RELEASE/base.txz -o /app/freebsd.txz && \
    echo 'e38b5cf756d60086a6c2f736eff19cc7685f7e2313e31d14342fc8df57200a92  /app/freebsd.txz' | sha256sum -c - && \
    mkdir -p /opt/freebsd && tar -xJf /app/freebsd.txz -C /opt/freebsd ./lib ./usr/lib ./usr/include && \
    rm /app/freebsd.txz

WORKDIR /work
