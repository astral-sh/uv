# Python support

## Python versions

uv supports and is tested against the following Python versions:

- 3.8
- 3.9
- 3.10
- 3.11
- 3.12
- 3.13
- 3.14
- 3.15

## Python implementations

uv has Tier 1 support for the following Python implementations:

- CPython

As with [platforms](./platforms.md), Tier 1 support can be thought of "guaranteed to work". uv
supports managed installations of these implementations, and the builds are maintained by Astral.

uv has Tier 2 support for:

- PyPy
- GraalPy
- Pyodide

uv is "expected to work" with these implementations. uv also supports managed installations of these
Python implementations, but the builds are not maintained by Astral.

uv has Tier 3 support for:

- Pyston

uv "should work" with these implementations, but stability may vary.
