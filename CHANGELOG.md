# Changelog

## 0.1.10

### Enhancements

- Omit `--find-links` from annotation header unless requested ([#1898](https://github.com/astral-sh/uv/pull/1898))
- Write to stdout when `--output-file` is present ([#1892](https://github.com/astral-sh/uv/pull/1892))

### Bug fixes

- Retain authentication when making range requests ([#1902](https://github.com/astral-sh/uv/pull/1902))
- Fix uv-created venv detection ([#1908](https://github.com/astral-sh/uv/pull/1908))
- Fix Windows `py` failure from spurious stderr ([#1885](https://github.com/astral-sh/uv/pull/1885))
- Ignore Python 2 installations when querying for interpreters ([#1905](https://github.com/astral-sh/uv/pull/1905))

## 0.1.9

### Enhancements

- Add support for `config_settings` in PEP 517 hooks ([#1833](https://github.com/astral-sh/uv/pull/1833))
- feat: allow passing extra config k,v pairs for pyvenv.cfg when creating a venv ([#1852](https://github.com/astral-sh/uv/pull/1852))

### Bug fixes

- Ensure authentication is passed from the index url to distribution files ([#1886](https://github.com/astral-sh/uv/pull/1886))
- Use `rustls-tls-native-roots` in `uv` crate ([#1888](https://github.com/astral-sh/uv/pull/1888))
- pep440: fix version ordering ([#1883](https://github.com/astral-sh/uv/pull/1883))
- Hide index URLs from header if not emitted ([#1835](https://github.com/astral-sh/uv/pull/1835))

### Documentation

- Add changelog ([#1881](https://github.com/astral-sh/uv/pull/1881))

## 0.1.8

### Bug fixes

- Allow duplicate URLs that resolve to the same canonical URL ([#1877](https://github.com/astral-sh/uv/pull/1877))
- Retain authentication attached to URLs when making requests to the same host ([#1874](https://github.com/astral-sh/uv/pull/1874))
- Win Trampoline: Use Python executable path encoded in binary ([#1803](https://github.com/astral-sh/uv/pull/1803))
- Expose types to implement custom `ResolverProvider` ([#1862](https://github.com/astral-sh/uv/pull/1862))
- Search `PATH` when `python` can't be found with `py` ([#1711](https://github.com/astral-sh/uv/pull/1711))
- Avoid displaying "root" package when formatting terms ([#1871](https://github.com/astral-sh/uv/pull/1871))

### Documentation

- Use more universal windows install instructions ([#1811](https://github.com/astral-sh/uv/pull/1811))

### Rust API

- Expose types to implement custom ResolverProvider ([#1862](https://github.com/astral-sh/uv/pull/1862))

## 0.1.7

### Enhancements

- Stream zip archive when fetching non-range-request metadata ([#1792](https://github.com/astral-sh/uv/pull/1792))
- Support setting request timeout with `UV_HTTP_TIMEOUT` and `HTTP_TIMEOUT` ([#1780](https://github.com/astral-sh/uv/pull/1780))
- Improve error message when git ref cannot be fetched ([#1826](https://github.com/astral-sh/uv/pull/1826))

### Configuration

- Implement `--annotation-style` parameter for `uv pip compile` ([#1679](https://github.com/astral-sh/uv/pull/1679))

### Bug fixes

- Add fixup for `prefect<1.0.0` ([#1825](https://github.com/astral-sh/uv/pull/1825))
- Add support for `>dev` specifier ([#1776](https://github.com/astral-sh/uv/pull/1776))
- Avoid enforcing URL correctness for installed distributions ([#1793](https://github.com/astral-sh/uv/pull/1793))
- Don't expect pinned packages for editables with non-existent extras ([#1847](https://github.com/astral-sh/uv/pull/1847))
- Linker copies files as a fallback when ref-linking fails ([#1773](https://github.com/astral-sh/uv/pull/1773))
- Move conflicting dependencies into PubGrub ([#1796](https://github.com/astral-sh/uv/pull/1796))
- Normalize `VIRTUAL_ENV` path in activation scripts ([#1817](https://github.com/astral-sh/uv/pull/1817))
- Preserve executable bit when untarring archives ([#1790](https://github.com/astral-sh/uv/pull/1790))
- Retain passwords in Git URLs ([#1717](https://github.com/astral-sh/uv/pull/1717))
- Sort output when installing seed packages ([#1822](https://github.com/astral-sh/uv/pull/1822))
- Treat ARM wheels as higher-priority than universal ([#1843](https://github.com/astral-sh/uv/pull/1843))
- Use `git` command to fetch repositories instead of `libgit2` for robust SSH support ([#1781](https://github.com/astral-sh/uv/pull/1781))
- Use redirected URL as base for relative paths ([#1816](https://github.com/astral-sh/uv/pull/1816))
- Use the right marker for the `implementation` field of `pyvenv.cfg` ([#1785](https://github.com/astral-sh/uv/pull/1785))
- Wait for distribution metadata with `--no-deps` ([#1812](https://github.com/astral-sh/uv/pull/1812))
- platform-host: check /bin/sh, then /bin/dash and then /bin/ls ([#1818](https://github.com/astral-sh/uv/pull/1818))
- Ensure that builds within the cache aren't considered Git repositories ([#1782](https://github.com/astral-sh/uv/pull/1782))
- Strip trailing `+` from version number of local Python builds ([#1771](https://github.com/astral-sh/uv/pull/1771))

### Documentation

- Add docs for git authentication ([#1844](https://github.com/astral-sh/uv/pull/1844))
- Update venv activation for windows ([#1836](https://github.com/astral-sh/uv/pull/1836))
- Update README.md to include extras example ([#1806](https://github.com/astral-sh/uv/pull/1806))

## 0.1.6

### Enhancements

- Expose find_uv_bin and declare typing support ([#1728](https://github.com/astral-sh/uv/pull/1728))
- Implement `uv cache dir` ([#1734](https://github.com/astral-sh/uv/pull/1734))
- Support `venv --prompt` ([#1570](https://github.com/astral-sh/uv/pull/1570))
- Print activation instructions for a venv after one has been created ([#1580](https://github.com/astral-sh/uv/pull/1580))

### CLI

- Add shell completions generation ([#1675](https://github.com/astral-sh/uv/pull/1675))
- Move `uv clean` to `uv cache clean` ([#1733](https://github.com/astral-sh/uv/pull/1733))
- Allow `-f` alias for `--find-links` ([#1735](https://github.com/astral-sh/uv/pull/1735))

### Configuration

- Control pip timeout duration via environment variable ([#1694](https://github.com/astral-sh/uv/pull/1694))

### Bug fixes

- Add support for absolute paths on Windows ([#1725](https://github.com/astral-sh/uv/pull/1725))
- Don't preserve timestamp in streaming unzip ([#1749](https://github.com/astral-sh/uv/pull/1749))
- Ensure extras trigger an install ([#1727](https://github.com/astral-sh/uv/pull/1727))
- Only preserve the executable bit ([#1743](https://github.com/astral-sh/uv/pull/1743))
- Preserve trailing slash for `--find-links` URLs ([#1720](https://github.com/astral-sh/uv/pull/1720))
- Respect `--index-url` provided via requirements.txt ([#1719](https://github.com/astral-sh/uv/pull/1719))
- Set index URLs for seeding venv ([#1755](https://github.com/astral-sh/uv/pull/1755))
- Support dotted function paths for script entrypoints ([#1622](https://github.com/astral-sh/uv/pull/1622))
- Support recursive extras for URL dependencies ([#1729](https://github.com/astral-sh/uv/pull/1729))
- Better error message for missing space before semicolon in requirements ([#1746](https://github.com/astral-sh/uv/pull/1746))
- Add warning when dependencies are empty with Poetry metadata ([#1650](https://github.com/astral-sh/uv/pull/1650))
- Ignore invalid extras from PyPI ([#1731](https://github.com/astral-sh/uv/pull/1731))
- Improve Poetry warning ([#1730](https://github.com/astral-sh/uv/pull/1730))
- Remove uv version from uv pip compile header ([#1716](https://github.com/astral-sh/uv/pull/1716))
- Fix handling of range requests on servers that return "Method not allowed" ([#1713](https://github.com/astral-sh/uv/pull/1713))
- re-introduce cache healing when we see an invalid cache entry ([#1707](https://github.com/astral-sh/uv/pull/1707))

### Documentation

- Clarify Windows install command in README.md ([#1751](https://github.com/astral-sh/uv/pull/1751))
- Add instructions for installing on Arch Linux ([#1765](https://github.com/astral-sh/uv/pull/1765))

### Rust API

- Allow passing in a custom reqwest Client ([#1745](https://github.com/astral-sh/uv/pull/1745))

## 0.1.5

### Enhancements

- Add `CACHEDIR.TAG` to uv-created virtualenvs ([#1653](https://github.com/astral-sh/uv/pull/1653))

### Bug fixes

- Build source distributions in the cache directory instead of the global temporary directory ([#1628](https://github.com/astral-sh/uv/pull/1628))
- Do not remove uv itself on pip sync ([#1649](https://github.com/astral-sh/uv/pull/1649))
- Ensure we retain existing environment variables during `python -m uv` ([#1667](https://github.com/astral-sh/uv/pull/1667))
- Add yank warnings at end of messages ([#1669](https://github.com/astral-sh/uv/pull/1669))

### Documentation

- Add brew to readme ([#1629](https://github.com/astral-sh/uv/pull/1629))
- Document RUST_LOG=trace for additional logging verbosity ([#1670](https://github.com/astral-sh/uv/pull/1670))
- Document local testing instructions ([#1672](https://github.com/astral-sh/uv/pull/1672))
- Minimal markdown nits ([#1664](https://github.com/astral-sh/uv/pull/1664))
- Use `--override` rather than `-o` to specify overrides in README.md ([#1668](https://github.com/astral-sh/uv/pull/1668))
- Remove setuptools & wheel from seed packages on Python 3.12+ (#1602) ([#1613](https://github.com/astral-sh/uv/pull/1613))

## 0.1.4

### Enhancements

- Add CMD support ([#1523](https://github.com/astral-sh/uv/pull/1523))
- Improve tracing when encountering invalid `requires-python` values ([#1568](https://github.com/astral-sh/uv/pull/1568))

### Bug fixes

- Add graceful fallback for Artifactory indexes ([#1574](https://github.com/astral-sh/uv/pull/1574))
- Allow URL requirements in editable installs ([#1614](https://github.com/astral-sh/uv/pull/1614))
- Allow repeated dependencies when installing ([#1558](https://github.com/astral-sh/uv/pull/1558))
- Always run `get_requires_for_build_wheel` ([#1590](https://github.com/astral-sh/uv/pull/1590))
- Avoid propagating top-level options to sub-resolutions ([#1607](https://github.com/astral-sh/uv/pull/1607))
- Consistent use of `BIN_NAME` in activation scripts ([#1577](https://github.com/astral-sh/uv/pull/1577))
- Enforce URL constraints for non-URL dependencies ([#1565](https://github.com/astral-sh/uv/pull/1565))
- Allow non-nested archives for `hexdump` and others ([#1564](https://github.com/astral-sh/uv/pull/1564))
- Avoid using `white` coloring in terminal output ([#1576](https://github.com/astral-sh/uv/pull/1576))
- Bump simple metadata cache version ([#1617](https://github.com/astral-sh/uv/pull/1617))
- Better error messages on expect failures in resolver ([#1583](https://github.com/astral-sh/uv/pull/1583))

### Documentation

- Add license to activator scripts ([#1610](https://github.com/astral-sh/uv/pull/1610))

## 0.1.3

### Enhancements

- Add support for `UV_EXTRA_INDEX_URL` ([#1515](https://github.com/astral-sh/uv/pull/1515))
- Use the system trust store for HTTPS requests ([#1512](https://github.com/astral-sh/uv/pull/1512))
- Automatically detect virtual environments when used via `python -m uv` ([#1504](https://github.com/astral-sh/uv/pull/1504))
- Add warning for empty requirements files ([#1519](https://github.com/astral-sh/uv/pull/1519))
- Support MD5 hashes ([#1556](https://github.com/astral-sh/uv/pull/1556))

### Bug fixes

- Add support for extras in editable requirements ([#1531](https://github.com/astral-sh/uv/pull/1531))
- Apply percent-decoding to file-based URLs ([#1541](https://github.com/astral-sh/uv/pull/1541))
- Apply percent-decoding to filepaths in HTML find-links ([#1544](https://github.com/astral-sh/uv/pull/1544))
- Avoid attempting rename in copy fallback path ([#1546](https://github.com/astral-sh/uv/pull/1546))
- Fix list rendering in `venv --help` output ([#1459](https://github.com/astral-sh/uv/pull/1459))
- Fix trailing commas on `Requires-Python` in HTML indexes  ([#1507](https://github.com/astral-sh/uv/pull/1507))
- Read from `/bin/sh` if `/bin/ls` cannot be found when determing libc path ([#1433](https://github.com/astral-sh/uv/pull/1433))
- Remove URL encoding when determining file name ([#1555](https://github.com/astral-sh/uv/pull/1555))
- Support recursive extras ([#1435](https://github.com/astral-sh/uv/pull/1435))
- Use comparable representation for `PackageId` ([#1543](https://github.com/astral-sh/uv/pull/1543))
- fix OS detection for Alpine Linux ([#1545](https://github.com/astral-sh/uv/pull/1545))
- only parse /bin/sh (not /bin/ls) ([#1493](https://github.com/astral-sh/uv/pull/1493))
- pypi-types: fix lenient requirement parsing ([#1529](https://github.com/astral-sh/uv/pull/1529))
- Loosen package script regexp to match spec ([#1482](https://github.com/astral-sh/uv/pull/1482))
- Use string display instead of debug for url parse trace ([#1498](https://github.com/astral-sh/uv/pull/1498))

### Documentation

- Provide example of file based package install. ([#1424](https://github.com/astral-sh/uv/pull/1424))
- Adjust link ([#1434](https://github.com/astral-sh/uv/pull/1434))
- Add troubleshooting section to benchmarks guide ([#1485](https://github.com/astral-sh/uv/pull/1485))
- infra: source github templates ([#1425](https://github.com/astral-sh/uv/pull/1425))

## 0.1.2

### Enhancements

- Add `--upgrade` support to `pip install` ([#1379](https://github.com/astral-sh/uv/pull/1379))
- Add `-U`/`-P` short flags for `--upgrade`/`--upgrade-package` ([#1394](https://github.com/astral-sh/uv/pull/1394))
- Add `UV_NO_CACHE` environment variable ([#1383](https://github.com/astral-sh/uv/pull/1383))
- uv-cache: Add hidden alias for --no-cache-dir ([#1380](https://github.com/astral-sh/uv/pull/1380))

### Bug fixes

- Add fix-up for invalid star comparison with major-only version ([#1410](https://github.com/astral-sh/uv/pull/1410))
- Add fix-up for trailing comma with trailing space ([#1409](https://github.com/astral-sh/uv/pull/1409))
- Allow empty fragments in HTML parser ([#1443](https://github.com/astral-sh/uv/pull/1443))
- Fix search for `python.exe` on Windows ([#1381](https://github.com/astral-sh/uv/pull/1381))
- Ignore invalid extra named `.none` ([#1428](https://github.com/astral-sh/uv/pull/1428))
- Parse `-r` and `-c` entries as relative to containing file ([#1421](https://github.com/astral-sh/uv/pull/1421))
- Avoid import contextlib in `_virtualenv` ([#1406](https://github.com/astral-sh/uv/pull/1406))
- Decode HTML escapes when extracting SHA ([#1440](https://github.com/astral-sh/uv/pull/1440))
- Fix broken URLs parsed from relative paths in registries ([#1413](https://github.com/astral-sh/uv/pull/1413))
- Improve error message for invalid sdist archives ([#1389](https://github.com/astral-sh/uv/pull/1389))

### Documentation

- Re-add license badge to the README ([#1333](https://github.com/astral-sh/uv/pull/1333))
- Replace "novel" in README ([#1365](https://github.com/astral-sh/uv/pull/1365))
- Tweak some grammar in the README ([#1387](https://github.com/astral-sh/uv/pull/1387))
- Update README.md to include venv activate ([#1411](https://github.com/astral-sh/uv/pull/1411))
- Update wording and add `alt` tag ([#1423](https://github.com/astral-sh/uv/pull/1423))

## 0.1.1

### Bug fixes

- Fix bug where `python3` is not found in the global path ([#1351](https://github.com/astral-sh/uv/pull/1351))

### Documentation

- Fix diagram alignment ([#1354](https://github.com/astral-sh/uv/pull/1354))
- Grammer nit ([#1345](https://github.com/astral-sh/uv/pull/1345))
