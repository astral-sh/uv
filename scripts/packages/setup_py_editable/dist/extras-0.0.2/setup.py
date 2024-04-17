from setuptools import setup

1/0

setup(
    name="extras",
    version="0.0.2",
    install_requires=[
        "httpx",
    ],
    extras_require={
        "dev": ["anyio"],
    }
)
