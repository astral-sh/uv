# PEP440 in rust

A library for python version numbers and specifiers, implementing
[PEP 440](https://peps.python.org/pep-0440)

```shell
pip install pep440_rs
```

```python
from pep440_rs import Version, VersionSpecifier

assert Version("1.1a1").any_prerelease()
assert Version("1.1.dev2").any_prerelease()
assert not Version("1.1").any_prerelease()
assert VersionSpecifier(">=1.0").contains(Version("1.1a1"))
assert not VersionSpecifier(">=1.1").contains(Version("1.1a1"))
assert Version("2.0") in VersionSpecifier("==2")
```

Unlike [pypa/packaging](https://github.com/pypa/packaging), this library always matches preleases. To only match final releases, filter with `.any_prelease()` beforehand.

PEP 440 has a lot of unintuitive features, including:

* An epoch that you can prefix the version which, e.g. `1!1.2.3`. Lower epoch always means lower
  version (`1.0 <=2!0.1`)
* post versions, which can be attached to both stable releases and prereleases
* dev versions, which can be attached to sbpth table releases and prereleases. When attached to a
  prerelease the dev version is ordered just below the normal prerelease, however when attached
  to a stable version, the dev version is sorted before a prereleases
* prerelease handling is a mess: "Pre-releases of any kind, including developmental releases,
  are implicitly excluded from all version specifiers, unless they are already present on the
  system, explicitly requested by the user, or if the only available version that satisfies
  the version specifier is a pre-release.". This means that we can't say whether a specifier
  matches without also looking at the environment
* prelease vs. prerelease incl. dev is fuzzy
* local versions on top of all the others, which are added with a + and have implicitly typed
  string and number segments
* no semver-caret (`^`), but a pseudo-semver tilde (`~=`)
* ordering contradicts matching: We have e.g. `1.0+local > 1.0` when sorting,
  but `==1.0` matches `1.0+local`. While the ordering of versions itself is a total order
  the version matching needs to catch all sorts of special cases
