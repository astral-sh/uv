# Static build requirements in the PyPI ecosystem

Warehouse needs **at least three distinct setuptools builds in addition to the
version in its runtime lock** if all the following packages are built from
source. Transformers also needs different versions of NumPy at build time and
runtime. These are conflicts in published source distributions, not just
synthetic examples:

| Existing uv lock                                                                                                                        | Locked at runtime                     | Build requirements observed in that lock's source distributions                                                                                                                                                                                                                                           |
| --------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| [Warehouse](https://github.com/astral-sh/uv/blob/7f396534d5/crates/uv/tests/it/snapshots/it__ecosystem__warehouse-lock-file.snap)       | `setuptools==81.0.0`                  | [psycopg-c 3.3.4](https://pypi.org/project/psycopg-c/3.3.4/#files) pins `==80.3.1`; [zope.schema 8.1](https://pypi.org/project/zope.schema/8.1/#files) pins `==78.1.1`; [hpack 4.2.0](https://pypi.org/project/hpack/4.2.0/#files) needs `>=82`. No two of those three requirements can share a solution. |
| [Transformers](https://github.com/astral-sh/uv/blob/7f396534d5/crates/uv/tests/it/snapshots/it__ecosystem__transformers-lock-file.snap) | `numpy==1.26.4`; `setuptools==81.0.0` | [h5py 3.14.0](https://pypi.org/project/h5py/3.14.0/#files) needs `numpy>=2,<3`. [ml-dtypes 0.3.2](https://pypi.org/project/ml-dtypes/0.3.2/#files) needs `setuptools~=68.1.0`, while [yarl 1.24.2](https://pypi.org/project/yarl/1.24.2/#files) needs `setuptools>=82.0.1`.                               |

We compared the hashes of downloaded source distributions to the hashes already
in the Black, JupyterLab, Transformers and Warehouse snapshots. All 875
comparisons matched. Each build requirement cited above is unconditional. A
locked package need not be installed on every platform or in every extras
selection; these are requirements for a **universal lock**, not a claim that an
ordinary sync builds all of them.

## Survey coverage

Of 15,000 pinned releases, **8,240** declared build requirements, **4,440** had
no root `pyproject.toml`, and **539** had no `[build-system]` table. A missing
table is different from declaring `requires = []`: only **8** of the 8,240
explicitly required nothing. **120** declarations used markers, **455**
requested an extra, and **50** used an in-tree backend.

Two cases deserve special attention.
[pyproject-toml 0.1.0](https://pypi.org/project/pyproject-toml/0.1.0/#files)
lists **itself** as a build requirement. Walking build edges must terminate even
before deciding whether a wheel breaks the cycle.
[power-grid-model 1.13.28](https://pypi.org/project/power-grid-model/1.13.28/#files)
uses a direct wheel URL containing `releases/latest`. Pinning the requirement
string would not pin the bytes; the lock must enforce the artifact hash or
reject that source. No direct reference was fetched.

The remaining **1,781** releases had no sdist (1,033), exceeded the 4 MiB
archive cap (674), lacked accessible release metadata (33, including four inventory entries with no version), failed archive or metadata
checks (21), or had only yanked or too-new sdists (20). The scanner caps
unpacked tar size at 32 MiB and individual declarations at 512 KiB.

The survey only reads static declarations. It cannot see what a build hook will
request, whether a wheel will be available at install time, or how long a
resolver run will take. Large source distributions are underrepresented, so
these counts are not failure rates or performance results.

## How the sample was selected

The scan used the
[pinned 15,000-release inventory](https://github.com/astral-sh/uv-ecosystem-testing/blob/4ae32dc2f34d250b64c0cca7c6f2217517087ff7/data/top-15k-pypi-latest-version.csv)
(SHA-256 `3ede20286c4dcddc24c82f0c13a548d4ae241aefb1ee0170961aa9ea2a0e0229`).
Only a network-isolated container opened the archives. The scan excluded yanked files and
uploads at or after midnight UTC on November 1, 2026, and recorded unavailable releases
instead of substituting another version. Each successfully inspected row includes the
archive's SHA-256 digest.
