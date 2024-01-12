"""
A build backend which errors when a hook is called.
"""


def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    raise ValueError("Oh no")


def build_sdist(sdist_directory, config_settings=None):
    raise ValueError("Oh no")


def get_requires_for_build_wheel(config_settings=None):
    raise ValueError("Oh no")


def prepare_metadata_for_build_wheel(metadata_directory, config_settings=None):
    raise ValueError("Oh no")


def get_requires_for_build_sdist(config_settings=None):
    raise ValueError("Oh no")
