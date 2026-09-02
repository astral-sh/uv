FROM python:3.12.12-slim-bookworm@sha256:593bd06efe90efa80dc4eee3948be7c0fde4134606dd40d8dd8dbcade98e669c AS base

ENV HOME=/tmp

FROM base AS build
COPY build-requirements.txt /requirements.txt
RUN python -m pip install --no-cache-dir --only-binary=:all: --require-hashes -r /requirements.txt
COPY build.py /scripts/build.py

FROM base AS keyring
COPY keyring-requirements.txt /requirements.txt
RUN python -m pip install --no-cache-dir --only-binary=:all: --require-hashes -r /requirements.txt
ENV PYTHON_KEYRING_BACKEND=keyrings.alt.file.PlaintextKeyring
COPY keyring.py /scripts/keyring.py
