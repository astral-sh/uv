"""
A build backend in a class namespace.
"""


class Class:
    @staticmethod
    def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
        return "build_wheel_fake_path"

    @staticmethod
    def build_sdist(sdist_directory, config_settings=None):
        return "build_sdist_fake_path"

    @staticmethod
    def get_requires_for_build_wheel(config_settings=None):
        return ["fake", "build", "wheel", "requires"]

    @staticmethod
    def prepare_metadata_for_build_wheel(metadata_directory, config_settings=None):
        return "prepare_metadata_fake_dist_info_path"

    @staticmethod
    def get_requires_for_build_sdist(config_settings=None):
        return ["fake", "build", "sdist", "requires"]
