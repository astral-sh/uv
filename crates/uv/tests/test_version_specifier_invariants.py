import pytest

def normalize_semver(ver: str) -> tuple[int, int, int]:
    cleaned = ver.strip().lstrip("vV")
    parts = cleaned.split(".")
    if len(parts) != 3:
        raise ValueError(f"Expected 3 version components, got: {ver}")
    return (int(parts[0]), int(parts[1]), int(parts[2]))

def test_valid_semver():
    assert normalize_semver("1.2.3") == (1, 2, 3)
    assert normalize_semver("v2.0.1") == (2, 0, 1)

def test_invalid_semver():
    with pytest.raises(ValueError):
        normalize_semver("1.2")
