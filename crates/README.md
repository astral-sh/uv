# Crates

## [bench](./bench)

Functionality for benchmarking Puffin.

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

## [pep440-rs](./pep440-rs)

Utilities for interacting with Python version numbers and specifiers.

## [pep508-rs](./pep508-rs)

Utilities for interacting with [PEP 508](https://peps.python.org/pep-0508/) dependency specifiers.

## [platform-host](./platform-host)

Functionality for detecting the current platform (operating system, architecture, etc.).

## [platform-tags](./platform-tags)

Functionality for parsing and inferring Python platform tags as per [PEP 425](https://peps.python.org/pep-0425/).

## [puffin-build](./puffin-build)

A [PEP 517](https://www.python.org/dev/peps/pep-0517/)-compatible build frontend for Puffin.

## [puffin-cache](./puffin-cache)

Functionality for caching Python packages and associated metadata.

## [puffin-cli](./puffin-cli)

Command-line interface for the Puffin package manager.

## [puffin-client](./puffin-client)

Client for interacting with PyPI-compatible HTTP APIs.

## [puffin-dev](./puffin-dev)

Development utilities for Puffin.

## [puffin-dispatch](./puffin-dispatch)

A centralized `struct` for resolving and building source distributions in isolated environments.
Implements the traits defined in `puffin-traits`.

## [puffin-distribution](./puffin-distribution)

Client for interacting with built distributions (wheels) and source distributions (sdists).
Capable of fetching metadata, distribution contents, etc.

## [puffin-fs](./puffin-fs)

Utilities for interacting with the filesystem.

## [puffin-git](./puffin-git)

Functionality for interacting with Git repositories.

## [puffin-installer](./puffin-installer)

Functionality for installing Python packages into a virtual environment.

## [puffin-interpreter](./puffin-interpreter)

Functionality for detecting and leveraging the current Python interpreter.

## [puffin-normalize](./puffin-normalize)

Normalize package and extra names as per Python specifications.

## [puffin-package](./puffin-package)

Types and functionality for working with Python packages, e.g., parsing wheel files.

## [puffin-resolver](./puffin-resolver)

Functionality for resolving Python packages and their dependencies.

## [puffin-traits](./puffin-traits)

Shared traits for Puffin, to avoid circular dependencies.

## [pypi-types](./pypi-types)

General-purpose type definitions for types used in PyPI-compatible APIs.

## [puffin-warnings](./puffin-warnings)

User-facing warnings for Puffin.

## [requirements-txt](./requirements-txt)

Functionality for parsing `requirements.txt` files.
