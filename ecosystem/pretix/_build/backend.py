import tomli
from setuptools import build_meta as _orig
from setuptools.build_meta import *


def get_requires_for_build_wheel(config_settings=None):
    with open("pyproject.toml", "rb") as f:
        p = tomli.load(f)
    return [
        *_orig.get_requires_for_build_wheel(config_settings),
        *p['project']['dependencies']
    ]
