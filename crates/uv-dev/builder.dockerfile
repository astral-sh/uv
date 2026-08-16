# Provide isolation for source distribution builds
# https://moyix.blogspot.com/2022/09/someones-been-messing-with-my-subnormals.html

FROM ubuntu:22.04
# Feel free to add build dependencies you need
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        autoconf \
        build-essential \
        cmake \
        curl \
        make \
        pkg-config \
        python3 \
        python3-dev \
        python3-pip \
        python3-venv \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*
ARG RUSTUP_VERSION=1.29.0
ARG RUSTUP_SHA256=4acc9acc76d5079515b46346a485974457b5a79893cfb01112423c89aeb5aa10
RUN curl --proto '=https' --tlsv1.2 -sSf \
        --output /tmp/rustup-init \
        "https://static.rust-lang.org/rustup/archive/${RUSTUP_VERSION}/x86_64-unknown-linux-gnu/rustup-init" \
    && printf '%s  %s\n' "$RUSTUP_SHA256" /tmp/rustup-init | sha256sum --check \
    && chmod +x /tmp/rustup-init \
    && /tmp/rustup-init -y \
    && rm /tmp/rustup-init
ENV HOME="/root"
WORKDIR /app
RUN python3 -m venv $HOME/venv-docker
ENV VIRTUAL_ENV="$HOME/venv-docker"
ENV PATH="$HOME/.cargo/bin:$HOME/venv-docker/bin:$PATH"
RUN rustup default 1.75.0
RUN rustup show
