FROM docker.io/library/python:3.12-slim-bookworm@sha256:76d4b7b6305788c6b4c6a19d6a22a3921bf802e9af4d5e1e5bd771208dba74bf AS runtime

FROM runtime AS index
COPY antithesis/generate-packages.py /tmp/generate-packages.py
RUN python /tmp/generate-packages.py /index \
  && rm /tmp/generate-packages.py
COPY --chmod=755 antithesis/index_server.py /usr/local/bin/uv-antithesis-index
CMD ["uv-antithesis-index"]

FROM docker.io/library/rust:1.96.0-bookworm@sha256:19817ead3289c8c631c73df281e18b59b172f6a31f4f563290f69cddd06c30e9 AS uv-build
WORKDIR /uv
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates crates
ARG TARGETARCH
RUN --mount=type=cache,id=uv-antithesis-cargo-registry,target=/usr/local/cargo/registry,sharing=locked \
  --mount=type=cache,id=uv-antithesis-target-${TARGETARCH},target=/uv/target,sharing=locked \
  cargo build --locked --profile fast-build --bin uv \
  && cp target/fast-build/uv /uv/uv

FROM runtime AS client
COPY --from=uv-build /uv/uv /usr/local/bin/uv
RUN python -m pip install --disable-pip-version-check --no-cache-dir \
  antithesis==0.2.0 \
  cffi==2.0.0 \
  pycparser==3.0
COPY --chmod=755 antithesis/entrypoint.sh /usr/local/bin/uv-antithesis-entrypoint
COPY --chmod=755 antithesis/test /opt/antithesis/test
RUN ln -s /opt/antithesis/test /opt/antithesis/catalog
ENV UV_CACHE_DIR=/state/cache \
  UV_NO_CONFIG=1 \
  UV_NO_PROGRESS=1 \
  PYTHONPATH=/opt/antithesis/test/v1/resolver-installer
ENTRYPOINT ["uv-antithesis-entrypoint"]

FROM scratch AS config
COPY antithesis/docker-compose.yaml /docker-compose.yaml
