"""Observe the production PGO helper and select instrumentation LTO for a pilot."""

import argparse
import functools
import importlib.util
import json
import sys
import time
from pathlib import Path

from run_uv import ROOT, digest

TRAINING_FLAGS = "-Clto=thin -Cembed-bitcode=yes"


def load_helper():
    path = ROOT / "scripts" / "build_uv_pgo.py"
    specification = importlib.util.spec_from_file_location("pilot_production_pgo", path)
    if specification is None or specification.loader is None:
        raise RuntimeError("Cannot load the production PGO helper")
    module = importlib.util.module_from_spec(specification)
    sys.modules[specification.name] = module
    specification.loader.exec_module(module)
    return module


def observe(module, training_lto, report_path):
    if training_lto not in {"fat", "thin"}:
        raise ValueError("Training LTO must be fat or thin")
    report = {
        "training_lto": training_lto,
        "production_helper_sha256": digest(ROOT / "scripts" / "build_uv_pgo.py"),
        "flag_selections": 0,
        "stages": {},
        "complete": False,
    }

    def save():
        report_path.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")

    def timed(name, function):
        @functools.wraps(function)
        def measured(*args, **kwargs):
            started = time.monotonic()
            record = {"completed": False}
            report["stages"].setdefault(name, []).append(record)
            save()
            try:
                result = function(*args, **kwargs)
                record["completed"] = True
                return result
            finally:
                record["seconds"] = time.monotonic() - started
                save()

        return measured

    append_flags = module.append_flags

    def selected_flags(existing, additional):
        if additional == TRAINING_FLAGS:
            report["flag_selections"] += 1
            additional = f"-Clto={training_lto} -Cembed-bitcode=yes"
        return append_flags(existing, additional)

    original_run = module.run

    def instrumented_run(command, *, environment, allowed_exit_codes=(0,)):
        flags = environment.get("RUSTFLAGS", "")
        if (
            "build" in command
            and "--package" in command
            and "-Cprofile-generate=" in flags
        ):
            if f"-Clto={training_lto}" not in flags or "-Cprofile-use=" in flags:
                raise RuntimeError("Unexpected instrumented build flags")
            report["instrumented_environment"] = {
                key: value
                for key, value in environment.items()
                if key.startswith("CARGO_PROFILE_")
                or key
                in {
                    "RUSTFLAGS",
                    "CARGO_BUILD_JOBS",
                    "CARGO_TERM_VERBOSE",
                    "CFLAGS",
                    "CXXFLAGS",
                }
            }
            return timed("instrumented_build", original_run)(
                [*command, "--verbose"],
                environment=environment,
                allowed_exit_codes=allowed_exit_codes,
            )
        return original_run(
            command, environment=environment, allowed_exit_codes=allowed_exit_codes
        )

    module.append_flags = selected_flags
    module.run = instrumented_run
    for name in ("prepare_corpus", "train_uv", "merge_profiles"):
        setattr(module, name, timed(name, getattr(module, name)))
    save()
    return report, save


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--training-lto", choices=("fat", "thin"), required=True)
    parser.add_argument("--stage-report", type=Path, required=True)
    args, production_arguments = parser.parse_known_args()
    if "--train-only" not in production_arguments or "--debug" in production_arguments:
        parser.error("The pilot driver only supports optimized --train-only builds")
    module = load_helper()
    report, save = observe(module, args.training_lto, args.stage_report)
    sys.argv = [str(ROOT / "scripts" / "build_uv_pgo.py"), *production_arguments]
    try:
        module.main()
        expected = {
            "prepare_corpus",
            "instrumented_build",
            "train_uv",
            "merge_profiles",
        }
        if report["flag_selections"] != 1 or set(report["stages"]) != expected:
            raise RuntimeError(
                "Production helper stages differ from the pilot contract"
            )
        if any(
            len(items) != 1 or not items[0]["completed"]
            for items in report["stages"].values()
        ):
            raise RuntimeError("Incomplete or repeated PGO stage")
        report["complete"] = True
    finally:
        save()


if __name__ == "__main__":
    main()
