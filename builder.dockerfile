# Provide isolation for source distribution builds
# https://moyix.blogspot.com/2022/09/someones-been-messing-with-my-subnormals.html

FROM ubuntu:22.04
# Feel free to add build dependencies you need
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        python3 \
        python3-dev \
        python3-pip \
        python3-venv \
        build-essential \
        make \
        autoconf \
        curl \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV HOME="/root"
WORKDIR /app
RUN python3 -m venv $HOME/venv-docker
ENV VIRTUAL_ENV="$HOME/venv-docker"
ENV PATH="$HOME/.cargo/bin:$HOME/venv-docker/bin:$PATH"
COPY rust-toolchain.toml rust-toolchain.toml
RUN rustup show
