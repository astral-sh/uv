v0.4.0 (in development)
-----------------------
- Drop support for Python 3.6
- Support Python 3.11

v0.3.0 (2022-08-12)
-------------------
- The `@pytest.mark.fail_slow()` marker now errors if not given exactly one
  argument.  Previously, it would either use the first argument or, if no
  arguments were given, it would be ignored.

v0.2.0 (2022-04-25)
-------------------
- Test against pytest 7
- Added `@pytest.mark.fail_slow(DURATION)` marker for making individual tests
  fail if they take too long to run

v0.1.0 (2021-12-10)
-------------------
Initial release
