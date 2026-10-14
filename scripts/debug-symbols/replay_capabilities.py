"""Replay debugger probes against retained, hash-verified native fixture binaries."""

import argparse
import json
import os
import platform
import shutil
import subprocess
import traceback
from pathlib import Path

from capabilities.cdb_probe import observe as observe_cdb
from run import run
from run_capabilities import (
    FIXTURE,
    ROOT,
    VARIANTS,
    debugger_probe,
    digest,
    save,
    summarize,
)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument(
        "--output", type=Path, default=ROOT / "target/debug-symbols-capabilities-replay"
    )
    parser.add_argument("--backend", choices=("lldb", "cdb"), default="lldb")
    parser.add_argument(
        "--source-commit", default=os.environ.get("CAPABILITY_SOURCE_COMMIT")
    )
    args = parser.parse_args()
    source = args.input.resolve()
    original = json.loads((source / "report.json").read_text())
    if (
        original["commit"] != args.source_commit
        or original["platform"] != platform.system()
    ):
        raise RuntimeError(
            "Retained artifact source/platform differs from the requested replay"
        )
    for relative, expected in original["source_sha256"].items():
        relative = relative.replace("\\", "/")
        if relative != "lldb_probe.py" and digest(FIXTURE / relative) != expected:
            raise RuntimeError(f"Compiled source changed: {relative}")
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    shutil.copyfile(source / "report.json", output / "source-report.json")
    debugger = None
    sysroot = None
    if args.backend == "lldb":
        sysroot = Path(run(["rustc", "--print", "sysroot"]).strip())
        debugger = (
            run(["xcrun", "--find", "lldb"]).strip()
            if platform.system() == "Darwin"
            else shutil.which("lldb")
        )
        if not debugger:
            raise RuntimeError("Missing LLDB")
    report = {
        "source_commit": original["commit"],
        "observer_commit": run(["git", "rev-parse", "HEAD"], cwd=ROOT).strip(),
        "source_report_sha256": digest(source / "report.json"),
        "source_runner": original["runner"],
        "source_sha256": original["source_sha256"],
        "platform": original["platform"],
        "target": original["target"],
        "backend": args.backend,
        "runner": {
            name: os.environ.get(name)
            for name in ("GITHUB_RUN_ID", "GITHUB_RUN_ATTEMPT", "RUNNER_NAME")
        },
        "variants": {},
        "failures": {},
    }
    save(output / "report.json", report)
    for name in VARIANTS:
        destination = output / name
        destination.mkdir()
        try:
            build = original["variants"][name]
            basename = (
                "capability_fixture.exe"
                if platform.system() == "Windows"
                else "capability_fixture"
            )
            binary = destination / basename
            original_binary = source / name / basename
            if digest(original_binary) != build["binary_sha256"]:
                raise RuntimeError("Retained binary hash mismatch")
            shutil.copy2(original_binary, binary)
            binary.chmod(0o755)
            symbols = None
            for relative, expected in build.get("symbol_files", {}).items():
                path = Path(relative.replace("\\", "/"))
                original_symbol = source / name / path
                if (
                    digest(original_symbol) != expected["sha256"]
                    or original_symbol.stat().st_size != expected["bytes"]
                ):
                    raise RuntimeError("Retained companion hash/size mismatch")
                copied = destination / path
                copied.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(original_symbol, copied)
                symbols = destination / path.parts[0]
            if run([binary, "17"], cwd=destination).strip() != "536":
                raise RuntimeError("Retained fixture checksum mismatch")
            variant = {
                "binary_sha256": build["binary_sha256"],
                "debug_level": build["debug_level"],
                "optimized": build["optimized"],
            }
            report["variants"][name] = variant
            if args.backend == "lldb":
                probe = debugger_probe(debugger, binary, symbols, destination, sysroot)
                variant["capabilities"] = summarize(probe)
                if name == "unoptimized-full" and not all(
                    variant["capabilities"][key]
                    for key in (
                        "live_scalar_correct",
                        "argument_correct",
                        "record_fields_correct",
                    )
                ):
                    raise RuntimeError(
                        "Unoptimized full-debug LLDB positive control failed"
                    )
            else:
                probe = observe_cdb(binary, destination)
                variant["cdb"] = probe
                if name == "unoptimized-full" and not probe["positive_control_values"]:
                    raise RuntimeError(
                        "Unoptimized full-debug CDB positive control failed"
                    )
                if name == "optimized-full":
                    negative = destination / "without-symbols"
                    negative.mkdir()
                    shutil.copy2(binary, negative / binary.name)
                    absent = observe_cdb(negative / binary.name, negative)
                    variant["without_symbols"] = absent
                    if any(
                        frame["values"]
                        for case in absent["cases"].values()
                        for frame in case["frames"].values()
                    ):
                        raise RuntimeError(
                            "CDB recovered named locals without the companion PDB"
                        )
            variant["completed"] = True
        except (
            RuntimeError,
            OSError,
            ValueError,
            TypeError,
            AttributeError,
            KeyError,
            subprocess.SubprocessError,
        ):
            report["failures"][name] = traceback.format_exc()
            print(report["failures"][name], flush=True)
        save(output / "report.json", report)
    report["completed"] = not report["failures"] and len(report["variants"]) == len(
        VARIANTS
    )
    save(output / "report.json", report)
    if not report["completed"]:
        raise SystemExit(
            "Debugger replay has incomplete/failed controls; evidence retained"
        )


if __name__ == "__main__":
    main()
