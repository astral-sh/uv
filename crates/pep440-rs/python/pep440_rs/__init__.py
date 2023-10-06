"""
A PEP440 reimplementation in rust

```python
from pep440_rs import Version, VersionSpecifier


assert Version("1.1a1").any_prerelease()
assert Version("1.1.dev2").any_prerelease()
assert not Version("1.1").any_prerelease()
assert VersionSpecifier(">=1.0").contains(Version("1.1a1"))
assert not VersionSpecifier(">=1.1").contains(Version("1.1a1"))
assert Version("2.0") in VersionSpecifier("==2")
```

"""

from ._pep440_rs import *

__doc__ = _pep440_rs.__doc__
if hasattr(_pep440_rs, "__all__"):
    __all__ = _pep440_rs.__all__
