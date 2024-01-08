.. image:: http://www.repostatus.org/badges/latest/active.svg
    :target: http://www.repostatus.org/#active
    :alt: Project Status: Active — The project has reached a stable, usable
          state and is being actively developed.

.. image:: https://github.com/jwodder/pytest-fail-slow/workflows/Test/badge.svg?branch=master
    :target: https://github.com/jwodder/pytest-fail-slow/actions?workflow=Test
    :alt: CI Status

.. image:: https://codecov.io/gh/jwodder/pytest-fail-slow/branch/master/graph/badge.svg
    :target: https://codecov.io/gh/jwodder/pytest-fail-slow

.. image:: https://img.shields.io/pypi/pyversions/pytest-fail-slow.svg
    :target: https://pypi.org/project/pytest-fail-slow/

.. image:: https://img.shields.io/conda/vn/conda-forge/pytest-fail-slow.svg
    :target: https://anaconda.org/conda-forge/pytest-fail-slow
    :alt: Conda Version

.. image:: https://img.shields.io/github/license/jwodder/pytest-fail-slow.svg
    :target: https://opensource.org/licenses/MIT
    :alt: MIT License

`GitHub <https://github.com/jwodder/pytest-fail-slow>`_
| `PyPI <https://pypi.org/project/pytest-fail-slow/>`_
| `Issues <https://github.com/jwodder/pytest-fail-slow/issues>`_
| `Changelog <https://github.com/jwodder/pytest-fail-slow/blob/master/CHANGELOG.md>`_

``pytest-fail-slow`` is a pytest_ plugin for making tests fail that take too
long to run.  It adds a ``--fail-slow DURATION`` command-line option to pytest
that causes any & all otherwise-passing tests that run for longer than the
given duration to be marked as failures, and it adds a
``@pytest.mark.fail_slow(DURATION)`` marker for making an individual test fail
if it runs for longer than the given duration.  If ``--fail-slow`` is given and
a test has the ``@fail_slow()`` marker, the duration given by the marker takes
precedence for that test.

Note that slow tests will still be run to completion; if you want them to
instead be stopped early, use pytest-timeout_.

.. _pytest: https://docs.pytest.org
.. _pytest-timeout: https://github.com/pytest-dev/pytest-timeout

A duration can be supplied to the ``--fail-slow`` option as either a bare
floating-point number of seconds or as a floating-point number followed by one
of the following units (case insensitive):

- ``h``, ``hour``, ``hours``
- ``m``, ``min``, ``mins``, ``minute``, ``minutes``
- ``s``, ``sec``, ``secs``, ``second``, ``seconds``
- ``ms``, ``milli``, ``millisec``, ``milliseconds``
- ``us``, ``μs``, ``micro``, ``microsec``, ``microseconds``

Durations passed to the ``@pytest.mark.fail_slow()`` marker can be either
ints/floats (for a number of seconds) or strings in the same format as passed
to ``--fail-slow``.

If ``pytest-fail-slow`` marks a test as a failure, the output will include the
test's duration and the duration threshold, like so::

    ________________________________ test_func ________________________________
    Test passed but took too long to run: Duration 123.0s > 5.0s

**Note:** Only the durations for tests themselves are taken into consideration.
If a test passes in less than the specified duration, but one or more fixture
setups/teardowns take longer than the duration, the test will still be marked
as passing.


Installation
============
``pytest-fail-slow`` requires Python 3.7 or higher and pytest 6.0 or higher.
Just use `pip <https://pip.pypa.io>`_ for Python 3 (You have pip, right?) to
install it::

    python3 -m pip install pytest-fail-slow
