## Project dependencies

The `project.dependencies` table represents the dependencies that are used when uploading to PyPI or
building a wheel. Individual dependencies are specified using
[dependency specifiers](https://packaging.python.org/en/latest/specifications/dependency-specifiers/)
syntax, and the table follows the
[PEP 621](https://packaging.python.org/en/latest/specifications/pyproject-toml/) standard.

`project.dependencies` defines the list of packages that are required for the project, along with
the version constraints that should be used when installing them. Each entry includes a dependency
name and version. An entry may include extras or environment markers for platform-specific packages.
For example:

```toml title="pyproject.toml"
[project]
name = "albatross"
version = "0.1.0"
dependencies = [
  # Any version in this range
  "tqdm >=4.66.2,<5",
  # Exactly this version of torch
  "torch ==2.2.2",
  # Install transformers with the torch extra
  "transformers[torch] >=4.39.3,<5",
  # Only install this package on older python versions
  # See "Environment Markers" for more information
  "importlib_metadata >=7.1.0,<8; python_version < '3.10'",
  "mollymawk ==0.1.0"
]
```

## Dependency specifiers

uv uses standard
[dependency specifiers](https://packaging.python.org/en/latest/specifications/dependency-specifiers/),
originally defined in [PEP 508](https://peps.python.org/pep-0508/). A dependency specifier is
composed of, in order:

- The dependency name
- The extras you want (optional)
- The version specifier
- An environment marker (optional)

The version specifiers are comma separated and added together, e.g., `foo >=1.2.3,<2,!=1.4.0` is
interpreted as "a version of `foo` that's at least 1.2.3, but less than 2, and not 1.4.0".

Specifiers are padded with trailing zeros if required, so `foo ==2` matches foo 2.0.0, too.

A star can be used for the last digit with equals, e.g., `foo ==2.1.*` will accept any release from
the 2.1 series. Similarly, `~=` matches where the last digit is equal or higher, e.g., `foo ~=1.2`
is equal to `foo >=1.2,<2`, and `foo ~=1.2.3` is equal to `foo >=1.2.3,<1.3`.

Extras are comma-separated in square bracket between name and version, e.g.,
`pandas[excel,plot] ==2.2`. Whitespace between extra names is ignored.

Some dependencies are only required in specific environments, e.g., a specific Python version or
operating system. For example to install the `importlib-metadata` backport for the
`importlib.metadata` module, use `importlib-metadata >=7.1.0,<8; python_version < '3.10'`. To
install `colorama` on Windows (but omit it on other platforms), use
`colorama >=0.4.6,<5; platform_system == "Windows"`.

Markers are combined with `and`, `or`, and parentheses, e.g.,
`aiohttp >=3.7.4,<4; (sys_platform != 'win32' or implementation_name != 'pypy') and python_version >= '3.10'`.
Note that versions within markers must be quoted, while versions _outside_ of markers must _not_ be
quoted.
