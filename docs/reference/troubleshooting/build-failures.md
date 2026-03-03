# Troubleshooting build failures

uv needs to build packages when there is not a compatible wheel (a pre-built distribution of the
package) available. Building packages can fail for many reasons, some of which may be unrelated to
uv itself.

## Recognizing a build failure

An example build failure can be produced by trying to install and old version of numpy on a new,
unsupported version of Python:

```console
$ uv pip install -p 3.13 'numpy<1.20'
Resolved 1 package in 62ms
  × Failed to build `numpy==1.19.5`
  ├─▶ The build backend returned an error
  ╰─▶ Call to `setuptools.build_meta:__legacy__.build_wheel()` failed (exit status: 1)

      [stderr]
      Traceback (most recent call last):
        File "<string>", line 8, in <module>
          from setuptools.build_meta import __legacy__ as backend
        File "/home/konsti/.cache/uv/builds-v0/.tmpi4bgKb/lib/python3.13/site-packages/setuptools/__init__.py", line 9, in <module>
          import distutils.core
      ModuleNotFoundError: No module named 'distutils'

      hint: `distutils` was removed from the standard library in Python 3.12. Consider adding a constraint (like `numpy >1.19.5`) to avoid building a version of `numpy` that depends
      on `distutils`.
```

Notice that the error message is prefaced by "The build backend returned an error".

The build failure includes the `[stderr]` (and `[stdout]`, if present) from the build backend that
was used for the build. The error logs are not from uv itself.

The message following the `╰─▶` is a hint provided by uv, to help resolve common build failures. A
hint will not be available for all build failures.

## Confirming that a build failure is specific to uv

Build failures are usually related to your system and the build backend. It is rare that a build
failure is specific to uv. You can confirm that the build failure is not related to uv by attempting
to reproduce it with pip:

```console
$ uv venv -p 3.13 --seed
$ source .venv/bin/activate
$ pip install --use-pep517 --no-cache --force-reinstall 'numpy==1.19.5'
Collecting numpy==1.19.5
  Using cached numpy-1.19.5.zip (7.3 MB)
  Installing build dependencies ... done
  Getting requirements to build wheel ... done
ERROR: Exception:
Traceback (most recent call last):
  ...
  File "/Users/example/.cache/uv/archive-v0/3783IbOdglemN3ieOULx2/lib/python3.13/site-packages/pip/_vendor/pyproject_hooks/_impl.py", line 321, in _call_hook
    raise BackendUnavailable(data.get('traceback', ''))
pip._vendor.pyproject_hooks._impl.BackendUnavailable: Traceback (most recent call last):
  File "/Users/example/.cache/uv/archive-v0/3783IbOdglemN3ieOULx2/lib/python3.13/site-packages/pip/_vendor/pyproject_hooks/_in_process/_in_process.py", line 77, in _build_backend
    obj = import_module(mod_path)
  File "/Users/example/.local/share/uv/python/cpython-3.13.0-macos-aarch64-none/lib/python3.13/importlib/__init__.py", line 88, in import_module
    return _bootstrap._gcd_import(name[level:], package, level)
           ~~~~~~~~~~~~~~~~~~~~~~^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  File "<frozen importlib._bootstrap>", line 1387, in _gcd_import
  File "<frozen importlib._bootstrap>", line 1360, in _find_and_load
  File "<frozen importlib._bootstrap>", line 1310, in _find_and_load_unlocked
  File "<frozen importlib._bootstrap>", line 488, in _call_with_frames_removed
  File "<frozen importlib._bootstrap>", line 1387, in _gcd_import
  File "<frozen importlib._bootstrap>", line 1360, in _find_and_load
  File "<frozen importlib._bootstrap>", line 1331, in _find_and_load_unlocked
  File "<frozen importlib._bootstrap>", line 935, in _load_unlocked
  File "<frozen importlib._bootstrap_external>", line 1022, in exec_module
  File "<frozen importlib._bootstrap>", line 488, in _call_with_frames_removed
  File "/private/var/folders/6p/k5sd5z7j31b31pq4lhn0l8d80000gn/T/pip-build-env-vdpjme7d/overlay/lib/python3.13/site-packages/setuptools/__init__.py", line 9, in <module>
    import distutils.core
ModuleNotFoundError: No module named 'distutils'
```

!!! important

    The `--use-pep517` flag should be included with the `pip install` invocation to ensure the same
    build isolation behavior. uv always uses [build isolation by default](../../pip/compatibility.md#pep-517-build-isolation).

    We also recommend including the `--force-reinstall` and `--no-cache` options when reproducing
    failures.

Since this build failure occurs in pip too, it is not likely to be a bug with uv.

If a build failure is reproducible with another installer, you should investigate upstream (in this
example, `numpy` or `setuptools`), find a way to avoid building the package in the first place, or
make the necessary adjustments to your system for the build to succeed.

## Why does uv build a package?

When generating the cross-platform lockfile, uv needs to determine the dependencies of all packages,
even those only installed on other platforms. uv tries to avoid package builds during resolution. It
uses any wheel if exist for that version, then tries to find static metadata in the source
distribution (mainly pyproject.toml with static `project.version`, `project.dependencies` and
`project.optional-dependencies` or METADATA v2.2+). Only if all of that fails, it builds the
package.

When installing, uv needs to have a wheel for the current platform for each package. If no matching
wheel exists in the index, uv tries to build the source distribution.

You can check which wheels exist for a PyPI project under “Download Files”, e.g.
https://pypi.org/project/numpy/2.1.1.md#files. Wheels with `...-py3-none-any.whl` filenames work
everywhere, others have the operating system and platform in the filename. In the linked `numpy`
example, you can see that there are pre-built distributions for Python 3.10 to 3.13 on macOS, Linux
and Windows.

## Common build failures

The following examples demonstrate common build failures and how to resolve them.

### Command is not found

If the build error mentions a missing command, for example, `gcc`:

<!-- docker run --platform linux/x86_64 -it ghcr.io/astral-sh/uv:python3.10-trixie-slim /bin/bash -c "uv pip install --system pysha3==1.0.2" -->

```hl_lines="17"
× Failed to build `pysha3==1.0.2`
├─▶ The build backend returned an error
╰─▶ Call to `setuptools.build_meta:__legacy__.build_wheel` failed (exit status: 1)

    [stdout]
    running bdist_wheel
    running build
    running build_py
    creating build/lib.linux-x86_64-cpython-310
    copying sha3.py -> build/lib.linux-x86_64-cpython-310
    running build_ext
    building '_pysha3' extension
    creating build/temp.linux-x86_64-cpython-310/Modules/_sha3
    gcc -Wno-unused-result -Wsign-compare -DNDEBUG -g -fwrapv -O3 -Wall -fPIC -DPY_WITH_KECCAK=1 -I/root/.cache/uv/builds-v0/.tmp8V4iEk/include -I/usr/local/include/python3.10 -c
    Modules/_sha3/sha3module.c -o build/temp.linux-x86_64-cpython-310/Modules/_sha3/sha3module.o

    [stderr]
    error: command 'gcc' failed: No such file or directory
```

Then, you'll need to install it with your system package manager, e.g., to resolve the error above:

```console
$ apt install gcc
```

!!! tip

    When using the uv-managed Python versions, it's common to need `clang` installed instead of
    `gcc`.

    Many Linux distributions provide a package that includes all the common build dependencies.
    You can address most build requirements by installing it, e.g., for Debian or Ubuntu:

    ```console
    $ apt install build-essential
    ```

### Header or library is missing

If the build error mentions a missing header or library, e.g., a `.h` file, then you'll need to
install it with your system package manager.

For example, installing `pygraphviz` requires Graphviz to be installed:

<!-- docker run --platform linux/x86_64 -it ghcr.io/astral-sh/uv:python3.12-trixie /bin/bash -c "uv pip install --system 'pygraphviz'" -->

```hl_lines="18-19"
× Failed to build `pygraphviz==1.14`
├─▶ The build backend returned an error
╰─▶ Call to `setuptools.build_meta.build_wheel` failed (exit status: 1)

  [stdout]
  running bdist_wheel
  running build
  running build_py
  ...
  gcc -fno-strict-overflow -Wsign-compare -DNDEBUG -g -O3 -Wall -fPIC -DSWIG_PYTHON_STRICT_BYTE_CHAR -I/root/.cache/uv/builds-v0/.tmpgLYPe0/include -I/usr/local/include/python3.12 -c pygraphviz/graphviz_wrap.c -o
  build/temp.linux-x86_64-cpython-312/pygraphviz/graphviz_wrap.o

  [stderr]
  ...
  pygraphviz/graphviz_wrap.c:9: warning: "SWIG_PYTHON_STRICT_BYTE_CHAR" redefined
      9 | #define SWIG_PYTHON_STRICT_BYTE_CHAR
        |
  <command-line>: note: this is the location of the previous definition
  pygraphviz/graphviz_wrap.c:3023:10: fatal error: graphviz/cgraph.h: No such file or directory
    3023 | #include "graphviz/cgraph.h"
        |          ^~~~~~~~~~~~~~~~~~~
  compilation terminated.
  error: command '/usr/bin/gcc' failed with exit code 1

  hint: This error likely indicates that you need to install a library that provides "graphviz/cgraph.h" for `pygraphviz@1.14`
```

To resolve this error on Debian, you'd install the `libgraphviz-dev` package:

```console
$ apt install libgraphviz-dev
```

Note that installing the `graphviz` package is not sufficient, the development headers need to be
installed.

!!! tip

    To resolve an error where `Python.h` is missing, install the [`python3-dev` package](https://packages.debian.org/trixie/python3-dev).

### Module is missing or cannot be imported

If the build error mentions a failing import, consider
[disabling build isolation](../../concepts/projects/config.md#build-isolation).

For example, some packages assume that `pip` is available without declaring it as a build
dependency:

<!-- docker run --platform linux/x86_64 -it ghcr.io/astral-sh/uv:python3.12-trixie-slim /bin/bash -c "uv pip install --system chumpy" -->

```hl_lines="7"
  × Failed to build `chumpy==0.70`
  ├─▶ The build backend returned an error
  ╰─▶ Call to `setuptools.build_meta:__legacy__.build_wheel` failed (exit status: 1)

    [stderr]
    Traceback (most recent call last):
      File "<string>", line 9, in <module>
    ModuleNotFoundError: No module named 'pip'

    During handling of the above exception, another exception occurred:

    Traceback (most recent call last):
      File "<string>", line 14, in <module>
      File "/root/.cache/uv/builds-v0/.tmpvvHaxI/lib/python3.12/site-packages/setuptools/build_meta.py", line 334, in get_requires_for_build_wheel
        return self._get_build_requires(config_settings, requirements=[])
                ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
      File "/root/.cache/uv/builds-v0/.tmpvvHaxI/lib/python3.12/site-packages/setuptools/build_meta.py", line 304, in _get_build_requires
        self.run_setup()
      File "/root/.cache/uv/builds-v0/.tmpvvHaxI/lib/python3.12/site-packages/setuptools/build_meta.py", line 522, in run_setup
        super().run_setup(setup_script=setup_script)
      File "/root/.cache/uv/builds-v0/.tmpvvHaxI/lib/python3.12/site-packages/setuptools/build_meta.py", line 320, in run_setup
        exec(code, locals())
      File "<string>", line 11, in <module>
    ModuleNotFoundError: No module named 'pip'
```

To resolve this error, pre-install the build dependencies then disable build isolation for the
package:

```console
$ uv pip install pip setuptools
$ uv pip install chumpy --no-build-isolation-package chumpy
```

Note you will need to install the missing package, e.g., `pip`, _and_ all the other build
dependencies of the package, e.g, `setuptools`.

### Old version of the package is built

If a package fails to build during resolution and the version that failed to build is older than the
version you want to use, try adding a [constraint](../settings.md#constraint-dependencies) with a
lower bound (e.g., `numpy>=1.17`). Sometimes, due to algorithmic limitations, the uv resolver tries
to find a fitting version using unreasonably old packages, which can be prevented by using lower
bounds.

For example, when resolving the following dependencies on Python 3.10, uv attempts to build an old
version of `apache-beam`.

```title="requirements.txt"
dill<0.3.9,>=0.2.2
apache-beam<=2.49.0
```

<!-- docker run --platform linux/x86_64 -it ghcr.io/astral-sh/uv:python3.10-trixie-slim /bin/bash -c "printf 'dill<0.3.9,>=0.2.2\napache-beam<=2.49.0' | uv pip compile -" -->

```hl_lines="1"
× Failed to build `apache-beam==2.0.0`
├─▶ The build backend returned an error
╰─▶ Call to `setuptools.build_meta:__legacy__.build_wheel` failed (exit status: 1)

    [stderr]
    ...
```

Adding a lower bound constraint, e.g., `apache-beam<=2.49.0,>2.30.0`, resolves this build failure as
uv will avoid using an old version of `apache-beam`.

Constraints can also be defined for indirect dependencies using `constraints.txt` files or the
[`constraint-dependencies`](../settings.md#constraint-dependencies) setting.

### Old Version of a build dependency is used

If a package fails to build because `uv` selects an incompatible or outdated version of a build-time
dependency, you can enforce constraints specifically for build dependencies. The
[`build-constraint-dependencies`](../settings.md#build-constraint-dependencies) setting (or an
analogous `build-constraints.txt` file) can be used to ensure that `uv` selects an appropriate
version of a given build requirements.

For example, the issue described in
[#5551](https://github.com/astral-sh/uv/issues/5551#issuecomment-2256055975) could be addressed by
specifying a build constraint that excludes `setuptools` version `72.0.0`:

```toml title="pyproject.toml"
[tool.uv]
# Prevent setuptools version 72.0.0 from being used as a build dependency.
build-constraint-dependencies = ["setuptools!=72.0.0"]
```

The build constraint will thus ensure that any package requiring `setuptools` during the build
process will avoid using the problematic version, preventing build failures caused by incompatible
build dependencies.

### Package is only needed for an unused platform

If locking fails due to building a package from a platform you do not need to support, consider
[limiting resolution](../../concepts/projects/config.md#limited-resolution-environments) to your
supported platforms.

### Package does not support all Python versions

If you support a large range of Python versions, consider using markers to use older versions for
older Python versions and newer versions for newer Python version. For example, `numpy` only
supports four Python minor version at a time, so to support a wider range of Python versions, e.g.,
Python 3.8 to 3.13, the `numpy` requirement needs to be split:

```
numpy>=1.23; python_version >= "3.10"
numpy<1.23; python_version < "3.10"
```

### Package is only usable on a specific platform

If locking fails due to building a package that is only usable on another platform, you can
[provide dependency metadata manually](../settings.md#dependency-metadata) to skip the build. uv can
not verify this information, so it is important to specify correct metadata when using this
override.
