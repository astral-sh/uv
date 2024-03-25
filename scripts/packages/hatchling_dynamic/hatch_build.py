from hatchling.builders.hooks.plugin.interface import BuildHookInterface


class LiteraryBuildHook(BuildHookInterface):
    def initialize(self, version, build_data):
        build_data["dependencies"].append("anyio")
