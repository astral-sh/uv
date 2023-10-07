from collections import namedtuple
from unittest import mock

import pytest
from pep508_rs import Requirement, MarkerEnvironment, Pep508Error, VersionSpecifier


def test_pep508():
    req = Requirement("numpy; python_version >= '3.7'")
    assert req.name == "numpy"
    env = MarkerEnvironment.current()
    assert req.evaluate_markers(env, [])
    req2 = Requirement("numpy; python_version < '3.7'")
    assert not req2.evaluate_markers(env, [])

    requests = Requirement(
        'requests [security,tests] >=2.8.1, ==2.8.* ; python_version > "3.8"'
    )
    assert requests.name == "requests"
    assert requests.extras == ["security", "tests"]
    assert requests.version_or_url == [
        VersionSpecifier(">=2.8.1"),
        VersionSpecifier("==2.8.*"),
    ]
    assert requests.marker == "python_version > '3.8'"


def test_marker():
    env = MarkerEnvironment.current()
    assert not Requirement("numpy; extra == 'science'").evaluate_markers(env, [])
    assert Requirement("numpy; extra == 'science'").evaluate_markers(env, ["science"])
    assert not Requirement(
        "numpy; extra == 'science' and extra == 'arrays'"
    ).evaluate_markers(env, ["science"])
    assert Requirement(
        "numpy; extra == 'science' or extra == 'arrays'"
    ).evaluate_markers(env, ["science"])


class FakeVersionInfo(
    namedtuple("FakeVersionInfo", ["major", "minor", "micro", "releaselevel", "serial"])
):
    pass


@pytest.mark.parametrize(
    ("version", "version_str"),
    [
        (FakeVersionInfo(3, 10, 11, "final", 0), "3.10.11"),
        (FakeVersionInfo(3, 10, 11, "rc", 1), "3.10.11rc1"),
    ],
)
def test_marker_values(version, version_str):
    with mock.patch("sys.implementation.version", version):
        env = MarkerEnvironment.current()
        assert str(env.implementation_version.version) == version_str


def test_marker_values_current_platform():
    MarkerEnvironment.current()


def test_errors():
    with pytest.raises(
        Pep508Error,
        match="Expected an alphanumeric character starting the extra name, found 'ö'",
    ):
        Requirement("numpy[ö]; python_version < '3.7'")


def test_warnings(caplog):
    env = MarkerEnvironment.current()
    assert not Requirement("numpy; '3.6' < '3.7'").evaluate_markers(env, [])
    assert caplog.messages == [
        "Comparing two quoted strings with each other doesn't make sense: "
        "'3.6' < '3.7', evaluating to false"
    ]
    caplog.clear()
    assert not Requirement("numpy; 'a' < 'b'").evaluate_markers(env, [])
    assert caplog.messages == [
        "Comparing two quoted strings with each other doesn't make sense: "
        "'a' < 'b', evaluating to false"
    ]
    caplog.clear()
    Requirement("numpy; python_version >= '3.9.'").evaluate_markers(env, [])
    assert caplog.messages == [
        "Expected PEP 440 version to compare with python_version, found '3.9.', "
        "evaluating to false: Version `3.9.` doesn't match PEP 440 rules"
    ]
    caplog.clear()
    # pickleshare 0.7.5
    Requirement("numpy; python_version in '2.6 2.7 3.2 3.3'").evaluate_markers(env, [])
    assert caplog.messages == [
        "Expected PEP 440 version to compare with python_version, "
        "found '2.6 2.7 3.2 3.3', "
        "evaluating to false: Version `2.6 2.7 3.2 3.3` doesn't match PEP 440 rules"
    ]
