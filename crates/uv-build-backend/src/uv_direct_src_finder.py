from __future__ import annotations

import sys
from importlib.abc import MetaPathFinder
from importlib.machinery import PathFinder
from importlib.metadata import DistributionFinder

TYPE_CHECKING = False
if TYPE_CHECKING:
    from importlib.metadata import Distribution
    from importlib.machinery import ModuleSpec
    from typing import Iterable, Sequence
    from types import ModuleType


class UvDirectSrcFinder(PathFinder, MetaPathFinder):
    """A meta path finder to add support for `src/__init__.py` layouts.

    By default, Python requires that a module is either defined in a file with its
    name (e.g. `foo.abi3.so`, `foo.py`) or there is a directory with the name, e.g.
    `foo/__init__.py`. We want to support `src/__init__.py`, to avoid duplicating the
    project name and ending up with structures such as `foo/src/foo/__init__.py`, where
    `foo/pyproject.toml` also exists."""

    name: str
    """The name of the module."""
    project_dir: str
    """The directory containing the `pyproject.toml` and the `src` folder."""

    def __init__(self, name: str, project_dir: str):
        self.name = name
        self.project_dir = project_dir

    def find_spec(
        self,
        fullname: str,
        path: Sequence[str] | None = None,
        target: ModuleType | None = None,
    ) -> ModuleSpec | None:
        from pathlib import Path
        from importlib.util import spec_from_file_location

        if fullname == self.name:
            init_py = Path(self.project_dir).joinpath("__init__.py")
            if not init_py.is_file():
                print(
                    f"Missing source editable `{self.name}` at `{self.project_dir}`, "
                    f"please run uv to refresh",
                    file=sys.stderr,
                )
                sys.exit(1)
            return spec_from_file_location(fullname, init_py)

    def find_distributions(
        self, context: DistributionFinder.Context = DistributionFinder.Context()
    ) -> Iterable[Distribution]:
        """https://docs.python.org/3/library/importlib.metadata.html#extending-the-search-algorithm

        The context has a name and a path attribute and we need to return an
        iterator with our Distribution object."""
        from importlib.metadata import PathDistribution
        from pathlib import Path

        if context.name is None or context.name == self.name:
            return iter([PathDistribution(Path(self.project_dir))])
        else:
            return iter([])
