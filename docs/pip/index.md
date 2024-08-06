# The pip interface

uv provides a drop-in replacement for common `pip`, `pip-tools`, and `virtualenv` commands. These
commands work directly with the virtual environment, in contrast to uv's primary interfaces where
the virtual environment is managed automatically. The `uv pip` interface exposes the speed and
functionality of uv to power users and projects that are not ready to transition away from `pip` and
`pip-tools`.

The following sections discuss the basics of using `uv pip`:

-   [Creating and using environments](./environments.md)
-   [Installing and managing packages](./packages.md)
-   [Inspecting environments and packages](./inspection.md)
-   [Declaring package dependencies](./dependencies.md)
-   [Locking and syncing environments](./compile.md)

Please note these commands do not _exactly_ implement the interfaces and behavior of the tools they
are based on. The further you stray from common workflows, the more likely you are to encounter
differences. Consult the [pip-compatibility guide](./compatibility.md) for details.
