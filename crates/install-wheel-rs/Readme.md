Reimplementation of wheel installing in rust. Supports both classical venvs and monotrail.

There are simple python bindings:

```python
from install_wheel_rs import LockedVenv

locked_venv = LockedVenv("path/to/.venv")
locked_venv.install_wheel("path/to/some_tagged_wheel.whl")
```

and there's only one function: `install_wheels_venv(wheels: List[str], venv: str)`, where `wheels` is a list of paths to wheel files and `venv` is the location of the venv to install the packages in.

See monotrail for benchmarks.
