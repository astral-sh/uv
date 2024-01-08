import pytest
from pytest_fail_slow import parse_duration


@pytest.mark.parametrize(
    "unit,mul",
    [
        ("", 1.0),
        ("h", 3600.0),
        ("hour", 3600.0),
        ("hours", 3600.0),
        ("m", 60.0),
        ("min", 60.0),
        ("mins", 60.0),
        ("minute", 60.0),
        ("minutes", 60.0),
        ("s", 1.0),
        ("sec", 1.0),
        ("secs", 1.0),
        ("second", 1.0),
        ("seconds", 1.0),
        ("ms", 0.001),
        ("msec", 0.001),
        ("msecs", 0.001),
        ("msecond", 0.001),
        ("mseconds", 0.001),
        ("milli", 0.001),
        ("millis", 0.001),
        ("millisec", 0.001),
        ("millisecs", 0.001),
        ("millisecond", 0.001),
        ("milliseconds", 0.001),
        ("us", 0.000001),
        ("usec", 0.000001),
        ("usecs", 0.000001),
        ("usecond", 0.000001),
        ("useconds", 0.000001),
        ("μs", 0.000001),
        ("μsec", 0.000001),
        ("μsecs", 0.000001),
        ("μsecond", 0.000001),
        ("μseconds", 0.000001),
        ("micro", 0.000001),
        ("micros", 0.000001),
        ("microsec", 0.000001),
        ("microsecs", 0.000001),
        ("microsecond", 0.000001),
        ("microseconds", 0.000001),
    ],
)
@pytest.mark.parametrize("number", ["42", "42.", "42.0", "3.14", "2e4"])
def test_parse_duration(number: str, unit: str, mul: float) -> None:
    assert parse_duration(number + unit) == float(number) * mul
    assert parse_duration(number + " " + unit) == float(number) * mul
    assert parse_duration(number + unit + " ") == float(number) * mul
    assert parse_duration(number + unit.upper()) == float(number) * mul


@pytest.mark.parametrize(
    "s",
    [
        "s",
        "10d",
        "1w",
        "100y",
        "500ns",
        "500n s",
        "123cm",
        "500u",
    ],
)
def test_parse_bad_duration(s: str) -> None:
    with pytest.raises(ValueError):
        parse_duration(s)
