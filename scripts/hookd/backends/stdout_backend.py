"""
A build backend which writes to stdout

If `config_settings` is populated, its contents will be written.
"""


def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    if not config_settings:
        print("hello")
        print("world")
    else:
        for key, value in config_settings.items():
            print("writing config_settings")
            print(key, "=", value)
    return "build_wheel_fake_path"
