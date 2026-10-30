from setuptools import Extension, setup

setup(
    name="uv-test-native-extension",
    version="0.1.0",
    ext_modules=[Extension("uv_test_native_extension", ["extension.c"])],
)
