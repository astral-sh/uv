"""Focused checks for statistical validity and real resource collection."""

import json
import math
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from analyze_study import analyze, classify, median_interval, plan_fingerprint
from run import symbolize
from study import TREATMENTS, treatment_order
from telemetry import PhaseSampler


class StudyChecks(unittest.TestCase):
    def cohort(self):
        """A complete synthetic experiment with fixed, known multiplicative effects."""
        plan = json.loads(Path(__file__).with_name("study-plan.json").read_text())
        records = []
        values = {
            "none-a": 100.0,
            "none-b": 100.0,
            "line-tables-only": 110.0,
            "limited": 112.0,
            "full": 160.0,
        }
        for target in plan["targets"]:
            for block in range(1000, 1020):
                records.append(
                    {
                        "target": target,
                        "block": block,
                        "phase": "confirmatory",
                        "study": plan["study_id"],
                        "plan_sha256": plan_fingerprint(plan),
                        "source_fingerprints": plan["source_fingerprints"],
                        "verified": True,
                        "commit": "fixed-experiment",
                        "runner": {"RUNNER_NAME": f"{target}-{block}"},
                        "treatments": {
                            name: {"builds": {"measured": {"total_seconds": value}}}
                            for name, value in values.items()
                        },
                        "benchmarks": {
                            workload: {"median_seconds": values}
                            for workload in ("jupyter", "trio")
                        },
                    }
                )
        return plan, records

    def test_complete_family_and_controls(self):
        plan, records = self.cohort()
        result = analyze(records, plan, 20)
        self.assertEqual(len(result["results"]), 84)
        self.assertTrue(result["resolved"])
        self.assertEqual(result["look_alpha"], 0.025)

    def test_duplicate_runner_block_is_not_another_sample(self):
        plan, records = self.cohort()
        with self.assertRaisesRegex(ValueError, "more than once"):
            analyze([*records, records[0]], plan, 20)

    def test_missing_scheduled_block_prevents_inference(self):
        plan, records = self.cohort()
        with self.assertRaisesRegex(ValueError, "Missing predeclared blocks"):
            analyze(records[:-1], plan, 20)

    def test_changed_plan_prevents_inference(self):
        plan, records = self.cohort()
        plan["build_equivalence_margin"] = 0.1
        with self.assertRaisesRegex(ValueError, "analysis plan changed"):
            analyze(records, plan, 20)

    def test_order_contains_both_controls_and_all_levels(self):
        order = treatment_order(0, "x86_64-pc-windows-msvc")
        self.assertCountEqual(order, TREATMENTS)
        self.assertEqual(order, treatment_order(0, "x86_64-pc-windows-msvc"))
        self.assertNotEqual(order, treatment_order(1, "x86_64-pc-windows-msvc"))

    def test_exact_median_interval(self):
        self.assertEqual(median_interval(list(range(20)), 0.05), (5, 14))

    def test_two_observations_cannot_support_tight_inference(self):
        self.assertEqual(median_interval([0.1, 0.2], 0.05 / 84), (-math.inf, math.inf))

    def test_non_significance_is_not_equivalence(self):
        self.assertEqual(classify(math.log(0.7), math.log(1.4), 0.05), "unresolved")
        self.assertEqual(
            classify(math.log(0.98), math.log(1.02), 0.05), "equivalent_within_margin"
        )
        self.assertEqual(
            classify(math.log(1.1), math.log(1.2), 0.05), "difference_detected"
        )
        self.assertEqual(
            classify(math.log(1.1), math.log(1.2), 0.05, control=True), "unresolved"
        )

    def test_disabled_sampler_keeps_legacy_execution_available(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "resources.jsonl"
            with PhaseSampler(path, enabled=False) as sampler:
                self.assertIsNone(sampler.summary)
            self.assertFalse(path.exists())

    @unittest.skipUnless(sys.platform == "darwin", "Mach-O debug information")
    def test_macos_negative_lookup_detects_embedded_debug_information(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            source = directory / "native.c"
            source.write_text("int symbol_frame(int value) { return value + 7; }\n")
            binary = directory / "native.o"
            subprocess.run(
                ["xcrun", "clang", "-g", "-O1", "-c", source, "-o", binary],
                check=True,
            )
            dwarf_tool = subprocess.check_output(
                ["xcrun", "--find", "llvm-dwarfdump"], text=True
            ).strip()
            symbols = directory / "missing.dSYM"
            tools = {"llvm-dwarfdump": dwarf_tool}
            _, resolved = symbolize(binary, symbols, 1, tools, "Darwin")
            self.assertTrue(resolved)
            subprocess.run(["strip", "-S", binary], check=True)
            _, resolved = symbolize(binary, symbols, 1, tools, "Darwin")
            self.assertFalse(resolved)

    @unittest.skipUnless(sys.platform == "darwin", "Mach-O debug information")
    def test_macos_missing_input_is_not_an_unknown_source_location(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            dwarf_tool = subprocess.check_output(
                ["xcrun", "--find", "llvm-dwarfdump"], text=True
            ).strip()
            with self.assertRaises(RuntimeError):
                symbolize(
                    directory / "missing.o",
                    directory / "missing.dSYM",
                    1,
                    {"llvm-dwarfdump": dwarf_tool},
                    "Darwin",
                )

    def test_sampler_observes_real_child_cpu_and_memory(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "resources.jsonl"
            with PhaseSampler(path, interval=0.05) as sampler:
                subprocess.run(
                    [
                        sys.executable,
                        "-c",
                        "import time\nallocation=bytearray(16*1024*1024)\nallocation[::4096]=b'x'*4096\nend=time.monotonic()+1.5\nwhile time.monotonic()<end: sum(range(2000))\n",
                    ],
                    check=True,
                )
            self.assertGreater(sampler.summary["samples"], 2)
            self.assertGreater(sampler.summary["observed_processes"], 0)
            self.assertGreater(
                sampler.summary["sampled_peak_tree_rss_bytes"], 16 * 1024 * 1024
            )
            self.assertGreater(
                sampler.summary["sampled_tree_counters_lower_bound"][
                    "cpu_user_seconds"
                ],
                0,
            )
            self.assertTrue(path.with_suffix(".json").is_file())


if __name__ == "__main__":
    unittest.main()
