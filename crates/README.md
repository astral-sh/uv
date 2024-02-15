# Crates

## [bench](./bench)

Functionality for benchmarking Axi.

## [cache-key](./cache-key)

Generic functionality for caching paths, URLs, and other resources across platforms.

## [distribution-filename](./distribution-filename)

Parse built distribution (wheel) and source distribution (sdist) filenames to extract structured
metadata.

## [distribution-types](./distribution-types)

Abstractions for representing built distributions (wheels) and source distributions (sdists), and
the sources from which they can be downloaded.

## [gourgeist](./gourgeist)

A `venv` replacement to create virtual environments in Rust.

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

Functionality for parsing and inferring Python platform tags as per [PEP 425](https://peps.python.org/pep-0425/).

## [axi](./axi)

Command-line interface for the Axi package manager.

## [axi-build](./axi-build)

A [PEP 517](https://www.python.org/dev/peps/pep-0517/)-compatible build frontend for Axi.

## [axi-cache](./axi-cache)

Functionality for caching Python packages and associated metadata.

## [axi-client](./axi-client)

Client for interacting with PyPI-compatible HTTP APIs.

## [axi-dev](./axi-dev)

Development utilities for Axi.

## [axi-dispatch](./axi-dispatch)

A centralized `struct` for resolving and building source distributions in isolated environments.
Implements the traits defined in `axi-traits`.

## [axi-distribution](./axi-distribution)

Client for interacting with built distributions (wheels) and source distributions (sdists).
Capable of fetching metadata, distribution contents, etc.

## [axi-extract](./axi-extract)

Utilities for extracting files from archives.

## [axi-fs](./axi-fs)

Utilities for interacting with the filesystem.

## [axi-git](./axi-git)

Functionality for interacting with Git repositories.

## [axi-installer](./axi-installer)

Functionality for installing Python packages into a virtual environment.

## [axi-interpreter](./axi-interpreter)

Functionality for detecting and leveraging the current Python interpreter.

## [axi-normalize](./axi-normalize)

Normalize package and extra names as per Python specifications.

## [axi-package](./axi-package)

Types and functionality for working with Python packages, e.g., parsing wheel files.

## [axi-resolver](./axi-resolver)

Functionality for resolving Python packages and their dependencies.

## [axi-traits](./axi-traits)

Shared traits for Axi, to avoid circular dependencies.

## [pypi-types](./pypi-types)

General-purpose type definitions for types used in PyPI-compatible APIs.

## [axi-warnings](./axi-warnings)

User-facing warnings for Axi.

## [requirements-txt](./requirements-txt)

Functionality for parsing `requirements.txt` files.
