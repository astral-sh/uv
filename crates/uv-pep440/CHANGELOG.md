## 0.7

* Remove pyo3 bindings

## 0.6.6

* Add `VersionSpecifiers::empty()`, which is present in the uv version of this crate but missing
  here.

## 0.6.2 - 0.6.5

* CI fixes

## 0.6.1

* Update pyo3 to 0.22

## 0.6

* Update pyo3 to 0.21 and a minimum of python 3.8

## 0.5

The crate has been completely rewritten by [burntsushi](https://github.com/BurntSushi/).

* Faster version parsing.
* Faster version comparisons.
* `Version` field accessors are now methods.
* `Version` is an [`Arc`](https://doc.rust-lang.org/std/sync/struct.Arc.html) of its internal
  representation, so cloning is cheap.
* The internal representation of a version is split into a full representation and an optimized
  small variant that can handle 75% of the versions on pypi.
* Parse errors are now opaque.
* [rkyv](https://github.com/rkyv/rkyv) support.

## 0.4

* segments are now `u64` instead of `usize`. This ensures consistency between platforms and `u64`
  are required when timestamps are used as patch versions (e.g., `20230628214621`, the ISO 8601 "
  basic format")
* Faster version comparison
* Added `VersionSpecifier::equals_version` constructor for `==<version>`
* Added `VersionSpecifier::any_prerelease`: Whether the version marker includes a prerelease
* Updated to pyo3 0.20
* once_cell instead of lazy_static

## 0.3.12

- Implement `FromPyObject` for `Version`

## 0.3.11

- CI fix

## 0.3.10

- Update pyo3 to 0.19 and maturin to 1.0

## 0.3.7

- Add `major()`, `minor()` and `micro()` to `Version` by ischaojie
  ([#9](https://github.com/konstin/pep440-rs/pull/9))

- ## 0.3.6

- Fix Readme display

## 0.3.5

- Make string serialization look more like poetry's
- Implement `__hash__` for `VersionSpecifier`

## 0.3.4

- Python bindings for `VersionSpecifiers`

## 0.3.3

- Implement `Display` for `VersionSpecifiers`

## 0.3.2

- Expose `VersionSpecifier().operator` and `VersionSpecifier().version` to Python

## 0.3.1

- Expose `Version` from `PyVersion`

## 0.3.0

- Introduced a `PyVersion` wrapper specifically for the Python bindings to work around
  https://github.com/PyO3/pyo3/pull/2786
- Added `VersionSpecifiers::contains`
- Added `Version::from_release`, a constructor for a version that is just a release such as `3.8`.

## 0.2.0

- Added `VersionSpecifiers`, a thin wrapper around `Vec<VersionSpecifier>` with a serde
  implementation. `VersionSpecifiers::from_str` is now preferred over `parse_version_specifiers`.
- Reexport rust function for python module
