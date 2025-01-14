import pytest
from built_by_uv import greet
from built_by_uv.arithmetic.circle import area


def test_circle():
    assert area(2) == pytest.approx(12.56636)


def test_greet():
    assert greet() == "Hello 👋"
