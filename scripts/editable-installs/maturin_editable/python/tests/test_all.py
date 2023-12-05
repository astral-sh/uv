import pytest
import maturin_editable


def test_sum_as_string():
    assert maturin_editable.sum_as_string(1, 1) == "2"
