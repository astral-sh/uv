from typing import List
import pytest

CASES = [
    (
        "{decor}def test_func():\n    assert 2 + 2 == 4\n",
        True,
        False,
    ),
    (
        "from time import sleep\n"
        "{decor}def test_func():\n"
        "    sleep(5)\n"
        "    assert 2 + 2 == 4\n",
        True,
        True,
    ),
    (
        "{decor}def test_func():\n    assert 2 + 2 == 5\n",
        False,
        False,
    ),
    (
        "from time import sleep\n"
        "{decor}def test_func():\n"
        "    sleep(5)\n"
        "    assert 2 + 2 == 5\n",
        False,
        False,
    ),
    (
        "from time import sleep\n"
        "import pytest\n"
        "\n"
        "@pytest.fixture\n"
        "def slow_setup():\n"
        "    sleep(3)\n"
        "    yield\n"
        "    sleep(3)\n"
        "\n"
        "{decor}def test_func(slow_setup):\n"
        "    assert 2 + 2 == 4\n",
        True,
        False,
    ),
]


@pytest.mark.parametrize("src,success,slow", CASES)
def test_fail_slow_no_threshold(
    pytester, src: str, success: bool, slow: bool  # noqa: U100
) -> None:
    pytester.makepyfile(test_func=src.format(decor=""))
    result = pytester.runpytest()
    if success:
        result.assert_outcomes(passed=1)
    else:
        result.assert_outcomes(failed=1)
    result.stdout.no_fnmatch_line("*Test passed but took too long to run*")


@pytest.mark.parametrize("src,success,slow", CASES)
@pytest.mark.parametrize(
    "args,decor,limitrgx",
    [
        (["--fail-slow=2"], "", r"2\.\d+s"),
        (["--fail-slow=2.0"], "", r"2\.\d+s"),
        (["--fail-slow=0.0333m"], "", r"1\.9\d+s"),
        ([], "import pytest\n@pytest.mark.fail_slow(2)\n", r"2s"),
        ([], "import pytest\n@pytest.mark.fail_slow(2.0)\n", r"2\.\d+s"),
        ([], "import pytest\n@pytest.mark.fail_slow('0.0333m')\n", r"1\.9\d+s"),
        (["--fail-slow=30"], "import pytest\n@pytest.mark.fail_slow(2)\n", r"2s"),
        (["--fail-slow=0.01"], "import pytest\n@pytest.mark.fail_slow(2)\n", r"2s"),
    ],
)
def test_fail_slow_threshold(
    pytester,
    src: str,
    success: bool,
    slow: bool,
    args: List[str],
    decor: str,
    limitrgx: str,
) -> None:
    pytester.makepyfile(test_func=src.format(decor=decor))
    result = pytester.runpytest(*args)
    if success and not slow:
        result.assert_outcomes(passed=1)
    else:
        result.assert_outcomes(failed=1)
    if slow:
        result.stdout.re_match_lines(
            [
                r"_+ test_func _+$",
                "Test passed but took too long to run:"
                rf" Duration \d+\.\d+s > {limitrgx}$",
            ],
            consecutive=True,
        )
    else:
        result.stdout.no_fnmatch_line("*Test passed but took too long to run*")


@pytest.mark.parametrize("args", ["", "42, 'foo'"])
def test_fail_slow_marker_bad_args(pytester, args: str) -> None:
    pytester.makepyfile(
        test_func=(
            "import pytest\n"
            "\n"
            f"@pytest.mark.fail_slow({args})\n"
            "def test_func():\n"
            "    assert 2 + 2 == 4\n"
        )
    )
    result = pytester.runpytest()
    result.assert_outcomes()
    result.stdout.no_fnmatch_line("?")
    result.stderr.fnmatch_lines(
        ["ERROR: @pytest.mark.fail_slow() takes exactly one argument"]
    )
