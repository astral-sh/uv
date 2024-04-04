FROM python:3.12-alpine
ENV LANG="C.UTF-8"
RUN apk add curl
ENV UV_EXTRA_INDEX_URL="https://wheels.home-assistant.io/musllinux-index/"
# time /io/target/x86_64-unknown-linux-musl/release/uv venv
# time /io/target/x86_64-unknown-linux-musl/release/uv pip install --no-build -r /io/requirements.txt
# time /io/target/x86_64-unknown-linux-musl/release/uv pip install --no-build -r /io/requirements_all.txt
