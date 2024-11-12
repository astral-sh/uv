# Crates

## [uv-bench](./uv-bench)

Functionality for benchmarking uv.

## [uv-cache-key](./uv-cache-key)

Generic functionality for caching paths, URLs, and other resources across platforms.

## [uv-distribution-filename](./uv-distribution-filename)

Parse built distribution (wheel) and source distribution (sdist) filenames to extract structured
metadata.

## [uv-distribution-types](./uv-distribution-types)

Abstractions for representing built distributions (wheels) and source distributions (sdists), and
the sources from which they can be downloaded.

## [uv-install-wheel-rs](./uv-install-wheel)

Install built distributions (wheels) into a virtual environment.

## [uv-once-map](./uv-once-map)

A [`waitmap`](https://github.com/withoutboats/waitmap)-like concurrent hash map for executing tasks
exactly once.

## [uv-pep440-rs](./uv-pep440)

Utilities for interacting with Python version numbers and specifiers.

## [uv-pep508-rs](./uv-pep508)

Utilities for parsing and evaluating
[dependency specifiers](https://packaging.python.org/en/latest/specifications/dependency-specifiers/),
previously known as [PEP 508](https://peps.python.org/pep-0508/).

## [uv-platform-tags](./uv-platform-tags)

Functionality for parsing and inferring Python platform tags as per
[PEP 425](https://peps.python.org/pep-0425/).

## [uv-cli](./uv-cli)

Command-line interface for the uv package manager.

## [uv-build-frontend](./uv-build-frontend)

A [PEP 517](https://www.python.org/dev/peps/pep-0517/)-compatible build frontend for uv.

## [uv-cache](./uv-cache)

Functionality for caching Python packages and associated metadata.

## [uv-client](./uv-client)

Client for interacting with PyPI-compatible HTTP APIs.

## [uv-dev](./uv-dev)

Development utilities for uv.

## [uv-dispatch](./uv-dispatch)

A centralized `struct` for resolving and building source distributions in isolated environments.
Implements the traits defined in `uv-types`.

## [uv-distribution](./uv-distribution)

Client for interacting with built distributions (wheels) and source distributions (sdists). Capable
of fetching metadata, distribution contents, etc.

## [uv-extract](./uv-extract)

Utilities for extracting files from archives.

## [uv-fs](./uv-fs)

Utilities for interacting with the filesystem.

## [uv-git](./uv-git)

Functionality for interacting with Git repositories.

## [uv-installer](./uv-installer)

Functionality for installing Python packages into a virtual environment.

## [uv-python](./uv-python)

Functionality for detecting and leveraging the current Python interpreter.

## [uv-normalize](./uv-normalize)

Normalize package and extra names as per Python specifications.

## [uv-requirements](./uv-requirements)

Utilities for reading package requirements from `pyproject.toml` and `requirements.txt` files.

## [uv-resolver](./uv-resolver)

Functionality for resolving Python packages and their dependencies.

## [uv-shell](./uv-shell)

Utilities for detecting and manipulating shell environments.

## [uv-types](./uv-types)

Shared traits for uv, to avoid circular dependencies.

## [uv-pypi-types](./uv-pypi-types)

General-purpose type definitions for types used in PyPI-compatible APIs.

## [uv-virtualenv](./uv-virtualenv)

A `venv` replacement to create virtual environments in Rust.

## [uv-warnings](./uv-warnings)

User-facing warnings for uv.

## [uv-workspace](./uv-workspace)

Workspace abstractions for uv.

## [uv-requirements-txt](./uv-requirements-txt)

Functionality for parsing `requirements.txt` files.
