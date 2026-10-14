"""Guard debugger observations whose naming and scope differ by debug level."""

import unittest

from run_capabilities import inline_frames, variable


class CapabilityObservations(unittest.TestCase):
    def test_line_tables_have_unqualified_inline_frame_names(self):
        observation = {
            "frames": [
                {"function": "black_box<u64>", "inline": True},
                {"function": "capability_checkpoint", "inline": False},
                {"function": "inline_leaf", "inline": True},
                {"function": "inline_middle", "inline": True},
                {"function": "inline_root", "inline": False},
            ]
        }
        self.assertEqual(inline_frames(observation), ["inline_leaf", "inline_middle"])

    def test_limited_debug_has_qualified_inline_frame_names(self):
        observation = {
            "frames": [
                {"function": "capability_fixture::inline_leaf", "inline": True},
                {"function": "capability_fixture::inline_middle", "inline": True},
                {"function": "capability_fixture::inline_root", "inline": False},
            ]
        }
        self.assertEqual(
            inline_frames(observation),
            ["capability_fixture::inline_leaf", "capability_fixture::inline_middle"],
        )

    def test_shadowed_name_uses_debugger_lexical_lookup(self):
        outer = {"name": "shadowed", "unsigned": 27}
        inner = {"name": "shadowed", "unsigned": 37}
        observation = {
            "matching_frames": [
                {"variables": [outer, inner], "lexical_lookup": {"shadowed": inner}}
            ]
        }
        self.assertEqual(variable(observation, "shadowed"), inner)


if __name__ == "__main__":
    unittest.main()
