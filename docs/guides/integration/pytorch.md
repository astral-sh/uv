# Installing pytorch with uv

[PyTorch](https://pytorch.org/) is a popular open-source deep-learning learning library that can run
both on CPUs and GPUs. For this reason, PyTorch can be notoriously cumbersome to install, especially
on Linux/Windows.

[PyTorch website](https://pytorch.org/get-started/locally/) offers a simple tool to determine what
`pip` command you should run to install PyTorch. This guide should help you do the same with `uv`.
If the following instructions fail, you might want to check the original source for any difference,
and open an issue to report the discrepancy. In this case, or if you run into any other issue,
please refer to [Getting Help](../../getting-started/help.md).

!!! tip "TL;DR"

    As of 09/2024:

    * Use `uv add torch` or `uv pip install torch` for:
        * on macOS (CPU only)
        * on Linux (CUDA 12.1 only)
        * on Windows (CPU only)
    * Use `uv add --extra-index-url=https://download.pytorch.org/whl/cpu torch` or `uv pip install --extra-index-url=https://download.pytorch.org/whl/cpu torch` for:
        * on Linux (CPU only)
    * `uv add --extra-index-url=https://download.pytorch.org/whl/cu118 torch` or `uv pip install --extra-index-url=https://download.pytorch.org/whl/cu118 torch` will work:
        * on Windows and Linux (CUDA 11.8 only)
    * `uv add --extra-index-url=https://download.pytorch.org/whl/cu121 torch` or `uv pip install --extra-index-url=https://download.pytorch.org/whl/cu121 torch` will work:
        * on Windows (CUDA 12.1 only)
    * `uv add --extra-index-url=https://download.pytorch.org/whl/cu124 torch` or `uv pip install --extra-index-url=https://download.pytorch.org/whl/cu124 torch` will work:
        * on Windows (CUDA 12.4 only)
        * on Linux (CUDA 12.4 only)


!!! info "Why `--extra-index-url`?"

    uv `--extra-index-url` behaves slightly differently from `pip --extra-index-url`. See more [here](https://docs.astral.sh/uv/pip/compatibility/#packages-that-exist-on-multiple-indexes).

!!! warning "About lockfile cross-compatibility"

    Currently, uv does not support pinning a package to a specific index (though progress is tracked [here](https://github.com/astral-sh/uv/issues/171)). As a result,
    the lockfiles are not guaranteed to be cross-compatible between different platforms. For example: suppose a Windows user runs `uv add torch` and then a Linux user runs
    `uv sync` to synchronise the lockfile. The Windows user will get CPU-only PyTorch, while the Linux user will get CUDA 12.1 PyTorch.

    If you specify the `[tool.uv.sources]` section in your `pyproject.toml`, users from different platform might not be able to solve for the package dependencies.

## On macOS

Since CUDA is not available on macOS, you should just run:

```sh
# in a project
uv add torch

# make sure there is a virtual environment in the directory
uv pip install torch
```

## On Linux

### CPU Only

```sh
uv add --extra-index-url=https://download.pytorch.org/whl/cpu torch

uv pip install --extra-index-url=https://download.pytorch.org/whl/cpu torch
```

### CUDA 11.8 Support

```sh
# in a project
uv add --extra-index-url=https://download.pytorch.org/whl/cu118 torch

# make sure there is a virtual environment in the directory
uv pip install --extra-index-url=https://download.pytorch.org/whl/cu118 torch
```

### CUDA 12.1 Support

Currently, wheels on PyPI for PyTorch on Linux come with CUDA 12.1.

```sh
# in a project
uv add torch

# make sure there is a virtual environment in the directory
uv pip install torch
```

### CUDA 12.4 Support

```sh
# in a project
uv add --extra-index-url=https://download.pytorch.org/whl/cu124 torch

# make sure there is a virtual environment in the directory
uv pip install --extra-index-url=https://download.pytorch.org/whl/cu124 torch
```

## On Windows

### CPU Only

```sh
# in a project
uv add torch

# make sure there is a virtual environment in the directory
uv pip install torch
```

### CUDA 11.8 Support

```sh
# in a project
uv add --extra-index-url=https://download.pytorch.org/whl/cu118 torch

# make sure there is a virtual environment in the directory
uv pip install --extra-index-url=https://download.pytorch.org/whl/cu118 torch
```

### CUDA 12.1 Support

```sh
# in a project
uv add --extra-index-url=https://download.pytorch.org/whl/cu121 torch

# make sure there is a virtual environment in the directory
uv pip install --extra-index-url=https://download.pytorch.org/whl/cu121 torch
```

### CUDA 12.4 Support

```sh
# in a project
uv add --extra-index-url=https://download.pytorch.org/whl/cu124 torch

# make sure there is a virtual environment in the directory
uv pip install --extra-index-url=https://download.pytorch.org/whl/cu124 torch
```
