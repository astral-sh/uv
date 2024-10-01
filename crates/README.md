# Crates

## [bench](./bench)

Functionality for benchmarking uv.

## [cache-key](./cache-key)

Generic functionality for caching paths, URLs, and other resources across platforms.

## [distribution-filename](./distribution-filename)

Parse built distribution (wheel) and source distribution (sdist) filenames to extract structured
metadata.

## [distribution-types](./distribution-types)

Abstractions for representing built distributions (wheels) and source distributions (sdists), and
the sources from which they can be downloaded.

## [install-wheel-rs](./install-wheel-rs)

Install built distributions (wheels) into a virtual environment.]

## [once-map](./once-map)

A [`waitmap`](https://github.com/withoutboats/waitmap)-like concurrent hash map for executing tasks
exactly once.

## [pep440-rs](./pep440-rs)

Utilities for interacting with Python version numbers and specifiers.

## [pep508-rs](./pep508-rs)

Utilities for interacting with [PEP 508](https://peps.python.org/pep-0508/) dependency specifiers.

## [platform-host](./platform-host)

Functionality for detecting the current platform (operating system, architecture, etc.).

## [platform-tags](./platform-tags)

Functionality for parsing and inferring Python platform tags as per
[PEP 425](https://peps.python.org/pep-0425/).

## [uv](./uv)

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

## [uv-package](./uv-package)

Types and functionality for working with Python packages, e.g., parsing wheel files.

## [uv-requirements](./uv-requirements)

Utilities for reading package requirements from `pyproject.toml` and `requirements.txt` files.

## [uv-resolver](./uv-resolver)

Functionality for resolving Python packages and their dependencies.

## [uv-shell](./uv-shell)

Utilities for detecting and manipulating shell environments.

## [uv-types](./uv-types)

Shared traits for uv, to avoid circular dependencies.

## [pypi-types](./pypi-types)

General-purpose type definitions for types used in PyPI-compatible APIs.

## [uv-virtualenv](./uv-virtualenv)

A `venv` replacement to create virtual environments in Rust.

## [uv-warnings](./uv-warnings)

User-facing warnings for uv.

## [uv-workspace](./uv-workspace)

Workspace abstractions for uv.

## [requirements-txt](./requirements-txt)

Functionality for parsing `requirements.txt` files.
