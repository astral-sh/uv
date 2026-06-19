from functools import lru_cache
from pathlib import Path


@lru_cache(maxsize=1)
def pi() -> float:
    return float(Path(__file__).parent.joinpath("pi.txt").read_text().strip())


def area(radius: float) -> float:
    """Use a non-Python file (`pi.txt`)."""
    return radius**2 * pi()
