# Writing scripts

## Creating a new script

Python recently added a standard format for
[inline script metadata](https://packaging.python.org/en/latest/specifications/inline-script-metadata/#inline-script-metadata).
It allows for selecting Python versions and defining dependencies. Use `uv init --script` to
initialize scripts with the inline metadata:

```console
$ uv init --script example.py --python 3.12
```

## Declaring script dependencies

The inline metadata format allows the dependencies for a script to be declared in the script itself.

uv supports adding and updating inline script metadata for you. Use `uv add --script` to declare the
dependencies for the script:

```console
$ uv add --script example.py 'requests<3' 'rich'
```

This will add a `script` section at the top of the script declaring the dependencies using TOML:

```python title="example.py"
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

!!! important

    When using inline script metadata, even if `uv run` is [used in a _project_](../concepts/projects.md#running-scripts), the project's dependencies will be ignored. The `--no-project` flag is not required.

## Declaring Python version requirements

uv also respects Python version requirements:

```python title="example.py"
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///

# Use some syntax added in Python 3.12
type Point = tuple[float, float]
print(Point)
```

!!! note

    The `dependencies` field must be provided even if empty.

`uv run` will search for and use the required Python version. The Python version will download if it
is not installed — see the documentation on [Python versions](../concepts/python-versions.md) for
more details.

## Improving reproducibility

uv supports an `exclude-newer` field in the `tool.uv` section of inline script metadata to limit uv
to only considering distributions released before a specific date. This is useful for improving the
reproducibility of your script when run at a later point in time.

The date must be specified as an [RFC 3339](https://www.rfc-editor.org/rfc/rfc3339.html) timestamp
(e.g., `2006-12-02T02:07:43Z`).

```python title="example.py"
# /// script
# dependencies = [
#   "requests",
# ]
# [tool.uv]
# exclude-newer = "2023-10-16T00:00:00Z"
# ///

import requests

print(requests.__version__)
```
