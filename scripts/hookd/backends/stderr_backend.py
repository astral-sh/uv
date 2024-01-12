"""
A build backend which writes to stderr
"""
import sys


def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    print("hello", file=sys.stderr)
    print("world", file=sys.stderr)
    return "build_wheel_fake_path"
