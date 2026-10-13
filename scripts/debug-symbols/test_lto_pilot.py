"""Check the pilot's LTO isolation against the production PGO helper."""

import json
import os
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from lto_pilot import check_compiler_flags, select_target
from pgo_lto_driver import load_helper, observe
from study import TREATMENTS, benchmark


class PilotChecks(unittest.TestCase):
    def test_production_helper_selects_only_instrumentation_lto(self):
        for lto in ("fat", "thin"):
            with self.subTest(lto=lto), tempfile.TemporaryDirectory() as temporary:
                directory = Path(temporary)
                target = "x86_64-pc-windows-msvc"
                module = load_helper()
                calls = []
                training_environments = []

                def run_command(
                    command,
                    *,
                    environment,
                    allowed_exit_codes=(0,),
                    calls=calls,
                    directory=directory,
                    target=target,
                ):
                    calls.append((command, environment.copy()))
                    binaries = directory / "instrumented" / target / "release"
                    binaries.mkdir(parents=True)
                    for name in ("uv.exe", "uvx.exe"):
                        (binaries / name).write_bytes(b"fixture")

                def training(
                    *args, environment, training_environments=training_environments
                ):
                    training_environments.append(environment.copy())
                    return []

                with (
                    patch.object(module, "rustc_host", return_value=target),
                    patch.object(
                        module,
                        "find_llvm_profdata",
                        return_value=directory / "llvm-profdata.exe",
                    ),
                    patch.object(module, "prepare_corpus", return_value=None),
                    patch.object(module, "train_uv", side_effect=training),
                    patch.object(module, "merge_profiles", return_value=None),
                    patch.object(module, "run", side_effect=run_command),
                    patch.dict(
                        os.environ, {"RUSTFLAGS": "-C target-feature=+crt-static"}
                    ),
                    patch.object(
                        sys,
                        "argv",
                        ["helper", "--target-dir", str(directory), "--train-only"],
                    ),
                ):
                    report, save = observe(module, lto, directory / "stages.json")
                    module.main()
                    save()
                self.assertEqual(len(calls), 1)
                self.assertIn("--verbose", calls[0][0])
                self.assertIn(f"-Clto={lto}", calls[0][1]["RUSTFLAGS"])
                self.assertIn("-Cprofile-generate=", calls[0][1]["RUSTFLAGS"])
                self.assertNotIn("-Cprofile-use=", calls[0][1]["RUSTFLAGS"])
                self.assertIn("-C target-feature=+crt-static", calls[0][1]["RUSTFLAGS"])
                self.assertEqual(training_environments, [calls[0][1]])
                self.assertEqual(report["flag_selections"], 1)
                self.assertEqual(
                    set(report["stages"]),
                    {
                        "prepare_corpus",
                        "instrumented_build",
                        "train_uv",
                        "merge_profiles",
                    },
                )
                self.assertTrue(
                    all(
                        len(v) == 1 and v[0]["completed"]
                        for v in report["stages"].values()
                    )
                )
                self.assertEqual(
                    json.loads((directory / "stages.json").read_text()), report
                )

    def test_balanced_runtime_samples_for_pilot_and_original_study(self):
        pilot = ("fat-none", "fat-full", "thin-none", "thin-full")
        for treatments, reference, count in (
            (("thin-none", "thin-line-tables", "thin-limited"), "thin-none", 30),
            (pilot, "fat-none", 40),
            (TREATMENTS, "none-a", 50),
        ):
            with (
                self.subTest(treatments=treatments),
                tempfile.TemporaryDirectory() as temporary,
            ):
                reports = {
                    name: {
                        "executables": ["uv.exe"],
                        "builds": {
                            "baseline"
                            if name in {"none-a", "none-b", "fat-none", "thin-none"}
                            else "symbols": {}
                        },
                    }
                    for name in treatments
                }

                def fake_run(arguments, **kwargs):
                    output = Path(arguments[arguments.index("--output-file") + 1])
                    output.write_text("identical-resolution")
                    return ""

                with patch("study.run", side_effect=fake_run):
                    results = benchmark(
                        Path(temporary),
                        reports,
                        0,
                        "test-host",
                        treatments=treatments,
                        reference=reference,
                    )
                for workload in results.values():
                    self.assertEqual(workload["warmups_per_treatment"], len(treatments))
                    self.assertEqual(set(workload["samples_seconds"]), set(treatments))
                    self.assertTrue(
                        all(
                            len(values) == count
                            for values in workload["samples_seconds"].values()
                        )
                    )
                    for start in range(0, len(workload["orders"]), len(treatments)):
                        batch = workload["orders"][start : start + len(treatments)]
                        for position in range(len(treatments)):
                            self.assertCountEqual(
                                [order[position] for order in batch], treatments
                            )

    def test_registered_targets_jobs_and_orders(self):
        for filename in ("lto-pilot-plan.json", "lower-debug-pilot-plan.json"):
            plan = json.loads(Path(__file__).with_name(filename).read_text())
            targets = plan.get("targets", {plan.get("target"): plan})
            for host, target in targets.items():
                with self.subTest(plan=filename, host=host):
                    self.assertEqual(
                        select_target(plan, host, str(target["cargo_jobs"])), target
                    )
                    with self.assertRaisesRegex(RuntimeError, "parallelism"):
                        select_target(plan, host, "1")
                    invalid = {**plan, "targets": {host: {**target, "order": []}}}
                    with self.assertRaisesRegex(RuntimeError, "exactly once"):
                        select_target(invalid, host, str(target["cargo_jobs"]))
            with self.assertRaisesRegex(RuntimeError, "outside"):
                select_target(plan, "unregistered", "4")

    def test_reduced_debug_flags_on_native_platforms(self):
        for system, executable in (
            ("Linux", "uv"),
            ("Darwin", "uv"),
            ("Windows", "uv.exe"),
        ):
            for level, flag in (
                ("limited", "1"),
                ("line-tables-only", "line-tables-only"),
            ):
                with (
                    self.subTest(system=system, level=level),
                    tempfile.TemporaryDirectory() as temporary,
                ):
                    path = Path(temporary) / "training.log"
                    command = (
                        "Running rustc --crate-name uv --crate-type bin "
                        "-C opt-level=3 -C lto=fat -C codegen-units=1 -C panic=abort "
                        f"-C debuginfo={flag} -Cprofile-generate=profiles -Clto=thin"
                    )
                    path.write_text(command)
                    if system == "Windows":
                        with self.assertRaisesRegex(RuntimeError, "target-feature"):
                            check_compiler_flags(
                                path,
                                [executable],
                                "thin",
                                level,
                                instrumented=True,
                                system=system,
                            )
                        command += " -C target-feature=+crt-static"
                        path.write_text(command)
                    self.assertEqual(
                        check_compiler_flags(
                            path,
                            [executable],
                            "thin",
                            level,
                            instrumented=True,
                            system=system,
                        ),
                        {executable: command},
                    )
                    path.write_text(command + " -C debuginfo=2")
                    with self.assertRaisesRegex(RuntimeError, "debug flags"):
                        check_compiler_flags(
                            path,
                            [executable],
                            "thin",
                            level,
                            instrumented=True,
                            system=system,
                        )

    def test_effective_compiler_flags_use_the_last_lto_setting(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "training.log"
            command = (
                "Running rustc --crate-name uv --crate-type bin "
                "-C opt-level=3 -C lto=fat -C codegen-units=1 -C panic=abort "
                "-C target-feature=+crt-static -C debuginfo=2 "
                "-Cprofile-generate=profiles -Clto=thin -Cembed-bitcode=yes"
            )
            path.write_text(command)
            result = check_compiler_flags(
                path, ["uv.exe"], "thin", "full", instrumented=True
            )
            self.assertEqual(result, {"uv.exe": command})
            with self.assertRaisesRegex(RuntimeError, "effective lto"):
                check_compiler_flags(path, ["uv.exe"], "fat", "full", instrumented=True)
            with self.assertRaisesRegex(RuntimeError, "No-debug control"):
                check_compiler_flags(
                    path, ["uv.exe"], "thin", "none", instrumented=True
                )

    def test_final_build_cannot_use_thin_lto_or_instrumentation(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "build.log"
            command = (
                "Running rustc --crate-name uv --crate-type bin "
                "-C opt-level=3 -C lto=fat -C codegen-units=1 -C panic=abort "
                "-C target-feature=+crt-static -Cprofile-use=uv.profdata"
            )
            path.write_text(command)
            self.assertEqual(
                check_compiler_flags(
                    path, ["uv.exe"], "fat", "none", instrumented=False
                ),
                {"uv.exe": command},
            )
            path.write_text(command + " -Clto=thin")
            with self.assertRaisesRegex(RuntimeError, "effective lto"):
                check_compiler_flags(
                    path, ["uv.exe"], "fat", "none", instrumented=False
                )
            path.write_text(command + " -Cprofile-generate=profiles")
            with self.assertRaisesRegex(RuntimeError, "Unexpected PGO"):
                check_compiler_flags(
                    path, ["uv.exe"], "fat", "none", instrumented=False
                )


if __name__ == "__main__":
    unittest.main()
