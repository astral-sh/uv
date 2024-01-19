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
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV HOME="/root"
WORKDIR /app
RUN python3 -m venv $HOME/venv-docker
ENV VIRTUAL_ENV="$HOME/venv-docker"
ENV PATH="$HOME/.cargo/bin:$HOME/venv-docker/bin:$PATH"
RUN rustup default 1.75.0
RUN rustup show
