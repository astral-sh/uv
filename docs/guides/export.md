---
title: Exporting a lockfile
description: Exporting a lockfile to different formats
---

# Exporting a lockfile

uv can export a lockfile to different formats for integration with other tools and workflows. The `uv export` command supports multiple output formats, each suited to different use cases.

For more details on lockfiles and how they're created, see the
[project layout](../concepts/projects/layout.md) and
[locking and syncing](../concepts/projects/sync.md) documentation.

## Overview of export formats

uv supports three export formats:

- `requirements.txt`: The traditional pip-compatible [requirements file format](https://pip.pypa.io/en/stable/reference/requirements-file-format/).
- `pylock.toml`: The standardized Python lockfile format defined in [PEP 751](https://peps.python.org/pep-0751/).
- `CycloneDX`: An industry-standard [Software Bill of Materials (SBOM)](https://cyclonedx.org/) format.

The format can be specified with the `--format` flag:

```console
$ uv export --format requirements.txt
$ uv export --format pylock.toml
$ uv export --format cyclonedx1.5
```

!!! tip

    By default, `uv export` prints to stdout. Use `--output-file` to write to a file for any format:

    ```console
    $ uv export --format requirements.txt --output-file requirements.txt
    $ uv export --format pylock.toml --output-file pylock.toml
    $ uv export --format cyclonedx1.5 --output-file sbom.json
    ```

## `requirements.txt` format

The `requirements.txt` format is the most widely supported format for Python dependencies. It can be used with `pip` and other Python package managers.

### Basic usage

```console
$ uv export --format requirements.txt
```

The generated `requirements.txt` file can then be installed via `uv pip install`, or with other tools like `pip`.

!!! note

    In general, we recommend against using both a `uv.lock` and a `requirements.txt` file. The
    `uv.lock` format is more powerful and includes features that cannot be expressed in
    `requirements.txt`. If you find yourself exporting a `uv.lock` file, consider opening an issue
    to discuss your use case.

## `pylock.toml` format

[PEP 751](https://peps.python.org/pep-0751/) defines a TOML-based lockfile format for Python dependencies. uv can export your project's dependency lockfile to this format.

### Basic usage

```console
$ uv export --format pylock.toml
```

## CycloneDX SBOM format

uv can export your project's dependency lockfile as a Software Bill of Materials (SBOM) in CycloneDX format. SBOMs provide a comprehensive inventory of all software components in your application, which is useful for security auditing, compliance, and supply chain transparency.

!!! important

    Support for exporting to CycloneDX is in _preview_. This means the behavior is experimental
    and subject to change.

### What is CycloneDX?

[CycloneDX](https://cyclonedx.org/) is an industry-standard format for creating Software Bill of Materials. CycloneDX is machine readable and widely supported by security scanning tools, vulnerability databases, and Software Composition Analysis (SCA) platforms.

### Basic usage

To export your project's lockfile as a CycloneDX SBOM:

```console
$ uv export --format cyclonedx1.5
```

This will generate a JSON-encoded CycloneDX v1.5 document containing your project and all of its dependencies.

### SBOM Structure

The generated SBOM follows the [CycloneDX specification](https://cyclonedx.org/specification/overview/). uv also includes the following custom properties on components:

- `uv:package:marker`: Environment markers (e.g., `python_version >= "3.8"`)
- `uv:workspace:path`: Relative path for workspace members

## Next steps

To learn more about lockfiles and exporting, see the
[locking and syncing](../concepts/projects/sync.md) documentation and the
[command reference](../reference/cli.md#uv-export).

Or, read on to learn how to [build and publish your project to a package index](./package.md).
