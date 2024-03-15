# Slow bio_embeddings[all] benchmark reproducer

FROM amazonlinux:2023

WORKDIR /root

RUN yum install -y git gcc cmake perl tar zstd
RUN curl -s -L https://github.com/indygreg/python-build-standalone/releases/download/20240224/cpython-3.12.2+20240224-x86_64-unknown-linux-gnu-pgo+lto-full.tar.zst | tar -I zstd -xf -
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain 1.76
ENV PATH="/root/.cargo/bin:$PATH"

RUN git clone https://github.com/astral-sh/uv
WORKDIR uv
RUN ../python/install/bin/python -m venv .venv
RUN cargo build --profile profiling
# RUN time target/profiling/uv pip compile scripts/requirements/bio_embeddings.in --exclude-newer 2024-03-01
# RUN time target/profiling/uv pip compile scripts/requirements/bio_embeddings.in --exclude-newer 2024-03-01

