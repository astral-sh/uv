FROM public.ecr.aws/docker/library/python:3.13-slim-bookworm@sha256:01f42367a0a94ad4bc17111776fd66e3500c1d87c15bbd6055b7371d39c124fb

# Don't pin apt versions; update before install; clean lists in the same layer.
RUN apt-get update && apt-get install -y --no-install-recommends \
    && rm -rf /var/lib/apt/lists/*

# Pin pip/npm versions. Bake test dependencies into the single image (pinned).
RUN pip install --no-cache-dir pytest==8.4.1 pytest-json-ctrf==0.3.5

# Put inputs in environment/data/ and copy them in.
# COPY data /app/data

# Pre-create the parent dir of every output path the agent writes, or artifact upload fails.
RUN mkdir -p /app

WORKDIR /app