# Antithesis

This configuration runs uv in [Antithesis](https://antithesis.com/) with a hermetic package index.
The `resolver-installer` test template repeatedly installs a dependency graph whose newest root
version is unsatisfiable. A successful run must backtrack to the older root version, install the
expected packages, and pass `uv pip check`. The `interrupted-reinstall` template kills a reinstall
after distribution metadata becomes visible but before the full wheel payload does, then verifies
that an ordinary install repairs the environment. The `interrupted-uninstall` template kills an
uninstall after uv removes the wheel's `RECORD` but before it removes the package payload, then
verifies that retrying the same uninstall completes the removal.

Parallel drivers run from two client containers and share uv's cache between otherwise isolated
virtual environments. The cache is stored under `/state/cache` so it survives container restarts.
Wheels contain a 2 MiB payload and the selected root wheel contains 10,000 small files. The package
index streams wheels in delayed chunks, giving Antithesis meaningful windows in which to interrupt
both downloads and installation. Each operation atomically journals its phases and originating
container under `/state/operations`; interrupted environments are preserved for inspection.

The test reports explicit Antithesis properties for successful environments, dependency checks,
journal integrity, persisted evidence of interrupted operations, and recovery. The eventual verifier
first checks whether a cache populated by a successful driver remains usable with `--offline`, then
performs an online recovery and verifies a second fresh offline installation.

## Run locally

Build and start the package index and test client:

```console
docker compose \
  -f antithesis/docker-compose.yaml \
  -f antithesis/docker-compose.local.yaml \
  up --build --detach
```

Run the driver and recovery verifier:

```console
docker compose \
  -f antithesis/docker-compose.yaml \
  -f antithesis/docker-compose.local.yaml \
  exec client \
  /opt/antithesis/test/v1/resolver-installer/parallel_driver_install.py

docker compose \
  -f antithesis/docker-compose.yaml \
  -f antithesis/docker-compose.local.yaml \
  exec client \
  /opt/antithesis/test/v1/resolver-installer/eventually_verify_install.py
```

Run the interrupted-reinstall reproducer. The singleton command fails on affected uv builds because
the recovery command returns success without restoring the complete wheel payload:

```console
docker compose \
  -f antithesis/docker-compose.yaml \
  -f antithesis/docker-compose.local.yaml \
  exec client \
  /opt/antithesis/test/v1/interrupted-reinstall/first_initialize.py

docker compose \
  -f antithesis/docker-compose.yaml \
  -f antithesis/docker-compose.local.yaml \
  exec client \
  /opt/antithesis/test/v1/interrupted-reinstall/singleton_driver_recover.py
```

Run the interrupted-uninstall reproducer. The singleton command fails on affected uv builds because
the first uninstall removes its own `RECORD`, leaving the retry unable to remove the remaining wheel
payload:

```console
docker compose \
  -f antithesis/docker-compose.yaml \
  -f antithesis/docker-compose.local.yaml \
  exec client \
  /opt/antithesis/test/v1/interrupted-uninstall/first_initialize.py
docker compose \
  -f antithesis/docker-compose.yaml \
  -f antithesis/docker-compose.local.yaml \
  exec client \
  /opt/antithesis/test/v1/interrupted-uninstall/singleton_driver_recover.py
```

When using the local Compose override, SDK lifecycle events and assertion evaluations are written to
`/state/antithesis-sdk-client*.jsonl` in the shared state volume.

Remove the containers and test state when finished:

```console
docker compose \
  -f antithesis/docker-compose.yaml \
  -f antithesis/docker-compose.local.yaml \
  down --volumes
```

## Deploy

The Dockerfile has three deployment targets:

- `index` contains the hermetic package index.
- `client` contains the uv build and Antithesis test template.
- `config` contains `/docker-compose.yaml`, as required by Antithesis.

Build each target for `linux/amd64`, tag it for the Antithesis registry, and push it. Launch the
Antithesis `basic_test` with the `config` image in `antithesis.config_image` and the `index` and
`client` images in `antithesis.images`. The image names must remain `uv-antithesis-index` and
`uv-antithesis-client` so they replace the tags in `docker-compose.yaml`.
