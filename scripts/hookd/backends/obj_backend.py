"""
A build backend in an object namespace.
"""


class Class:
    def build_wheel(
        self, wheel_directory, config_settings=None, metadata_directory=None
    ):
        return "build_wheel_fake_path"

    def build_sdist(self, sdist_directory, config_settings=None):
        return "build_sdist_fake_path"

    def get_requires_for_build_wheel(self, config_settings=None):
        return ["fake", "build", "wheel", "requires"]

    def prepare_metadata_for_build_wheel(
        self, metadata_directory, config_settings=None
    ):
        return "prepare_metadata_fake_dist_info_path"

    def get_requires_for_build_sdist(self, config_settings=None):
        return ["fake", "build", "sdist", "requires"]


obj = Class()
