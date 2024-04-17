from setuptools import setup

setup(
    name="extras",
    version="0.0.1",
    install_requires=[
        "iniconfig",
    ],
    extras_require={
        "dev": ["anyio"],
    }
)
