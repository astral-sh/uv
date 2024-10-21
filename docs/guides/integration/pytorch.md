# Installing PyTorch with uv

[PyTorch](https://pytorch.org/) is a popular open-source deep-learning learning framework that has first-class support for acceleration via GPUs. Installation, however, can be complex, as you won't find all the wheels for PyTorch on PyPI and you have to manage this through external indexes. This guide aims to help you set up a `pyproject.toml` file using `uv` [indexes features](../../configuration/indexes.md).

!!! info "Available PyTorch indexes"

    By default, from PyPI you will download:
    * CPU-only wheels on Windows and macOS.
    * CUDA (i.e. GPU) on Linux. At the time of writing, for PyTorch stable version (2.5.0) this defaults to CUDA 12.4. On older versions, this might be different.

    If you want to install CPU-only PyTorch wheels on Linux, or a PyTorch version that supports a different CUDA version, you need to resort to external indexes:

    * https://download.pytorch.org/whl/cu118
    * https://download.pytorch.org/whl/cu121
    * https://download.pytorch.org/whl/cu124
    * https://download.pytorch.org/whl/cpu
    * https://download.pytorch.org/whl/rocm6.2 (AMD GPUs, only available on Linux)


The [PyTorch website](https://pytorch.org/get-started/locally/) offers a simple interface to determine what `pip` command you should run to install PyTorch. This guide should help you do the same with `uv`. If the following instructions fail, you might want to check the link for any difference, and open an issue to report the discrepancy. In this case, or if you run into any other issue, please refer to [Getting Help](../../getting-started/help.md).

## Initialise a project

Create a new project with `uv init`:

```sh
uv init
```

!!! tip "Supported Python versions"

    Make sure to use a Python version that is supported by PyTorch. You can find the compatibility matrix [here](https://github.com/pytorch/pytorch/blob/main/RELEASE.md#release-compatibility-matrix).


## Add PyTorch indexes

Open the `pyproject.toml` file, and create a custom index matching the CUDA version you have available to instruct `uv` where to find the PyTorch wheels.

=== "CPU-only"

    ```toml
    [[tool.uv.index]]
    name = "torch-cpu"
    url = "https://download.pytorch.org/whl/cpu"
    explicit = true
    ```

=== "CUDA 11.8"

    ```toml
    [[tool.uv.index]]
    name = "torch-cu118"
    url = "https://download.pytorch.org/whl/cu118"
    explicit = true
    ```

=== "CUDA 12.1"

    ```toml
    [[tool.uv.index]]
    name = "torch-cu121"
    url = "https://download.pytorch.org/whl/cu121"
    explicit = true
    ```

=== "CUDA 12.4"

    ```toml
    [[tool.uv.index]]
    name = "torch-cu124"
    url = "https://download.pytorch.org/whl/cu124"
    explicit = true
    ```

=== "ROCm6"

    ```toml
    [[tool.uv.index]]
    name = "torch-rocm"
    url = "https://download.pytorch.org/whl/rocm6.2"
    explicit = true
    ```

Note that we also specify the `explicit` option: this prevents packages from being installed from that index unless explicitly pinned to it (see the step below). This means that only PyTorch will be installed from this index, while all other packages will be looked up on PyPI.

## Pin PyTorch to the custom index

Now we need to pin specific PyTorch versions to the appropriate indexes. We do this by adding/editing the `sources` section in the `pyproject.toml`.

=== "CPU-only"

    Note that we use the `platform_system` marker to instruct `uv` to look into this index on Linux and Windows.

    ```toml
    [tool.uv.sources]
    torch = [
      { index = "torch-cpu", marker = "platform_system != 'Darwin'"},
    ]
    ```

=== "CUDA 11.8"

    Note that we use the `platform_system` marker to instruct `uv` to look into this index on Linux and Windows.

    ```toml
    [tool.uv.sources]
    torch = [
      { index = "torch-cu118", marker = "platform_system != 'Darwin'"},
    ]
    ```

=== "CUDA 12.1"

    Note that we use the `platform_system` marker to instruct `uv` to look into this index on Linux and Windows.

    ```toml
    [tool.uv.sources]
    torch = [
      { index = "torch-cu121", marker = "platform_system != 'Darwin'"},
    ]
    ```

=== "CUDA 12.4"

    Note that we use the `platform_system` marker to instruct `uv` to look into this index on Linux and Windows.

    ```toml
    [tool.uv.sources]
    torch = [
      { index = "torch-cu124", marker = "platform_system != 'Darwin'"},
    ]
    ```

=== "ROCm6"

    Note that we use the `platform_system` marker to instruct `uv` to look into this index when on Linux only.

    ```toml
    [tool.uv.sources]
    torch = [
      { index = "torch-rocm", marker = "platform_system == 'Linux'"},
    ]
    ```

## Add PyTorch to your dependencies

Finally, we can add PyTorch to the `project.dependencies` section of the `pyproject.toml`. You can do this by hand, or using `uv`:

```sh
uv add torch
```

However, if you want to be more explicit, you could also:

```toml
[project]
dependencies = [
  "torch==2.5.0 ; platform_system == 'Darwin'",
  "torch==2.5.0+cu124 ; platform_system != 'Darwin'",
]
```

This will install PyTorch 2.5.0 on macOS, and PyTorch 2.5.0+cu124 on Linux and Windows.

!!! warning "PyTorch on Intel Macs"

    Note that the last version to support Intel Macs is PyTorch 2.3.0. In other words, if you try to install PyTorch 2.4.0 or more on an Intel Mac, you will get an error.
