# Running scripts

A Python script is a file intended for standalone execution, e.g., with `python <script>.py`. Using uv to execute scripts will ensure that
script dependencies are properly managed inside and outside of projects.

## Running a script without dependencies

If a script has no dependencies, it can be executed with `uv run`:

```python
print("Hello world")
```

```console
$ uv run example.py
Hello world
```

Similarly, if the script depends on a module in the standard library, there's nothing more to do:

```python
import os

print(os.path.expanduser("~"))
```

```console
$ uv run example.py
/Users/astral
```

Arguments can be passed to the script:

```python
import sys

print(" ".join(sys.argv[1:]))
```

```console
$ uv run example.py test
test

$ uv run example.py hello world!
hello world!
```

Note that if `uv run` is used in a _project_, i.e. a directory with a `pyproject.toml`, it will install the current project before running the script. If the script does not depend on the project, use the `--isolated` flag to skip this:

```console
# Note, it is important that the flag comes _before_ the script
$ uv run --isolated example.py
```

See the [projects](./projects.md) guide for more details on working in projects.

## Running a script with dependencies

When a script requires dependencies, they must be installed into the environment that the script runs in. uv prefers to create these environments on-demand instead of maintaining a long-lived virtual environment with manually managed dependencies. This requires explicit declaration
of dependencies that are required for the script. Generally, it's recommended to use a [project](./projects.md) or [inline metadata](#declaring-script-dependencies) to declare dependencies, but uv supports requesting dependencies per invocation as well.

For example, the following script requires `rich`.

```python
import time
from rich.progress import track

for i in track(range(20), description="For example:"):
    time.sleep(0.05)
```

If executed without specifying a dependency, this script will fail:

```console
$ uv run --isolated example.py
Traceback (most recent call last):
  File "/Users/astral/example.py", line 2, in <module>
    from rich.progress import track
ModuleNotFoundError: No module named 'rich'
```

The dependency can be requested with the `--with` option:

```console
$ uv run --with rich example.py
For example: ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━ 100% 0:00:01
```

Constraints can be added to the requested dependency if specific versions are needed:

```consoleq
$ uv run --with 'rich>12,<13' example.py
```

Multiple dependencies can be requested by repeating with `--with` option.

Note that if `uv run` is used in a _project_, these dependencies will be included _in addition_ to the project's dependencies. To opt-out of this behavior, use the `--isolated` flag.

## Declaring script dependencies

Python recently added a standard format for [inline script metadata](https://packaging.python.org/en/latest/specifications/inline-script-metadata/#inline-script-metadata). This allows the dependencies for a script to be declared in the script itself.

To use inline script metadata, include a `script` section at the top of the script:

```python
# /// script
# dependencies = [
#   "requests<3",
#   "rich",
# ]
# ///

import requests
from rich.pretty import pprint

resp = requests.get("https://peps.python.org/api/peps.json")
data = resp.json()
pprint([(k, v["title"]) for k, v in data.items()][:10])
```

uv will automatically create an environment with the dependencies necessary to run the script, e.g.:

```console
$ uv run example.py
[
│   ('1', 'PEP Purpose and Guidelines'),
│   ('2', 'Procedure for Adding New Modules'),
│   ('3', 'Guidelines for Handling Bug Reports'),
│   ('4', 'Deprecation of Standard Modules'),
│   ('5', 'Guidelines for Language Evolution'),
│   ('6', 'Bug Fix Releases'),
│   ('7', 'Style Guide for C Code'),
│   ('8', 'Style Guide for Python Code'),
│   ('9', 'Sample Plaintext PEP Template'),
│   ('10', 'Voting Guidelines')
]
```

uv also supports Python version requirements:

```python
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///

# Use some syntax added in Python 3.12
type Point = tuple[float, float]
print(Point)
```

uv will fetch the required Python version if it is not installed — see the documentation on [Python versions](../python-versions.md) for more details. Note that the `dependencies` field must be provided even if empty.

Note that when using inline script metadata, even if `uv run` is used in a _project_, the project's dependencies will be ignored. The `--isolated` flag is not required.

## Using different Python versions

uv allows arbitrary Python versions to be requested on each script invocation, for example:

```python
import sys

print(".".join(map(str, sys.version_info[:3])))
```

```console
# Use the default Python version, may differ on your machine
$ uv run example.py
3.12.1
```

```console
# Use a specific Python version
$ uv run --python 3.10 example.py
3.10.13
```

See the [Python versions](../python-versions.md) documentation for more details on requesting Python versions.
