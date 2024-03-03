# Use bullseye
FROM debian:bullseye

RUN apt-get update
RUN apt-get install python3 python3-pip python3-venv -y
COPY schema.py /app/schema.py

# Install Rust.
RUN apt-get install -y curl
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"
RUN rustup default 1.76.0
RUN rustup update
