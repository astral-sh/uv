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

    * If want PyTorch on macOS, just run `uv add torch`/`uv pip install torch`.
    * If want PyTorch on Windows *with CPU support*, just run `uv add torch`/`uv pip install torch`.
    * If want to install PyTorch on Linux with CPU support, add `--extra-index-url=https://download.pytorch.org/whl/cpu` to the `uv add`/`uv pip install` command.
    * For Windows and Linux:
        1. If you need CUDA 11.8, add `--extra-index-url=https://download.pytorch.org/whl/cu118` to the `uv add`/`uv pip install` command.
        2. If you need CUDA 12.1, add `--extra-index-url=https://download.pytorch.org/whl/cu121` to the `uv add`/`uv pip install` command.
        3. If you need CUDA 12.4, add `--extra-index-url=https://download.pytorch.org/whl/cu124` to the `uv add`/`uv pip install` command.

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
