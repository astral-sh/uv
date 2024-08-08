"""
This is implementation has some very rudimentary python bindings
"""

from pep440_rs import Operator, Version, VersionSpecifier, VersionSpecifiers


def test_pep440():
    assert Version("1.1a1").any_prerelease()
    assert Version("1.1.dev2").any_prerelease()
    assert not Version("1.1").any_prerelease()
    assert VersionSpecifier(">=1.0").contains(Version("1.1a1"))
    assert not VersionSpecifier(">=1.1").contains(Version("1.1a1"))
    assert Version("1.1") >= Version("1.1a1")
    assert Version("2.0") in VersionSpecifier("==2")
    assert Version("2.1").major == 2
    assert Version("2.1").minor == 1
    assert Version("2.1").micro == 0


def test_version_specifier():
    assert VersionSpecifier(">=1.1").version == Version("1.1")
    assert VersionSpecifier(">=1.1").operator == Operator.GreaterThanEqual
    assert str(VersionSpecifier(">=1.1").operator) == ">="
    # Note: This removes the star
    assert VersionSpecifier("==1.1.*").version == Version("1.1")
    assert str(VersionSpecifier("==1.1.*").operator) == "=="
    assert {
        VersionSpecifier("==1.1.*"),
        VersionSpecifier("==1.1"),
        VersionSpecifier("==1.1"),
    } == {VersionSpecifier("==1.1.*"), VersionSpecifier("==1.1")}


def test_version_specifiers():
    assert str(VersionSpecifiers(">=1.1, <2.0")) == ">=1.1, <2.0"
    assert list(VersionSpecifiers(">=1.1, <2.0")) == [
        VersionSpecifier(">=1.1"),
        VersionSpecifier("<2.0"),
    ]


def test_normalization():
    assert str(Version("1.19-alpha.1")) == "1.19a1"
    assert str(VersionSpecifier(" >=1.19-alpha.1 ")) == ">=1.19a1"
    assert repr(Version("1.19-alpha.1")) == '<Version("1.19a1")>'
    assert (
        repr(VersionSpecifier(" >=1.19-alpha.1 ")) == '<VersionSpecifier(">=1.19a1")>'
    )
