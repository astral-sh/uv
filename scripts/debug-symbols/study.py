"""Run one independent randomized block of optimized debug-level builds."""

import argparse
import json
import os
import random
import re
import statistics
import sys
import tempfile
import time
from pathlib import Path

from analyze_study import plan_fingerprint
from run import run
from run_uv import (
    ROOT,
    build,
    build_context,
    digest,
    save_report,
    sbom_digest,
    verify,
    verify_install,
)
from telemetry import machine_info

TREATMENTS = ("none-a", "none-b", "line-tables-only", "limited", "full")
SOURCE_REVISION = "238d6ba651d13f0dfddab0cc1826f963cdf21711"
DESIGN_SEED = 2063209


def treatment_order(block, target):
    """Independently randomize all five positions before observing any timings."""
    generator = random.Random(f"{DESIGN_SEED}:{target}:{block}")
    labels = list(TREATMENTS)
    generator.shuffle(labels)
    return labels


def fingerprints():
    return {
        **{
            path: run(["git", "rev-parse", f"HEAD:{path}"], cwd=ROOT).strip()
            for path in ("crates", "python", ".cargo", "test/requirements")
        },
        **{
            path: digest(ROOT / path)
            for path in (
                "Cargo.toml",
                "Cargo.lock",
                "rust-toolchain.toml",
                "scripts/build_uv_pgo.py",
                "scripts/cargo.sh",
                "scripts/cargo.cmd",
            )
        },
    }


def benchmark(output, reports, block, host):
    """Keep timings nested under their independent runner block, never as build replicates."""
    directory = output / "benchmarks"
    directory.mkdir()
    binaries = {
        name: output
        / "variants"
        / name
        / ("baseline" if name.startswith("none-") else "symbols")
        / report["executables"][0]
        for name, report in reports.items()
    }
    results = {}
    with tempfile.TemporaryDirectory(prefix="uv-study-cache-") as temporary:
        for workload in ("jupyter", "trio"):
            requirements = ROOT / "test" / "requirements" / f"{workload}.in"
            compiled = directory / f"{workload}.txt"
            arguments = [
                "--no-config",
                "--no-progress",
                "--cache-dir",
                temporary,
                "pip",
                "compile",
                requirements,
                "--python",
                sys.executable,
                "--exclude-newer",
                "2026-06-30T00:00:00Z",
                "--no-build",
                "--no-header",
                "--no-annotate",
                "--quiet",
                "--output-file",
                compiled,
            ]
            run([binaries["none-a"], *arguments], cwd=ROOT)
            expected = digest(compiled)
            samples = {name: [] for name in TREATMENTS}
            orders = []
            generator = random.Random(f"{DESIGN_SEED}:{host}:{block}:{workload}")
            for cycle in range(11):
                labels = list(TREATMENTS)
                generator.shuffle(labels)
                shifts = list(range(5))
                generator.shuffle(shifts)
                for shift in shifts:
                    order = labels[shift:] + labels[:shift]
                    orders.append(order)
                    for name in order:
                        compiled.unlink()
                        started = time.perf_counter()
                        run([binaries[name], *arguments, "--offline"], cwd=ROOT)
                        elapsed = time.perf_counter() - started
                        if digest(compiled) != expected:
                            raise RuntimeError(f"{name} changed {workload} output")
                        if cycle:
                            samples[name].append(elapsed)
            results[workload] = {
                "requirements_sha256": digest(requirements),
                "resolution_sha256": expected,
                "python": sys.version,
                "orders": orders,
                "warmups_per_treatment": 5,
                "samples_seconds": samples,
                "median_seconds": {
                    name: statistics.median(values) for name, values in samples.items()
                },
            }
            (directory / "report.json").write_text(json.dumps(results, indent=2) + "\n")
    return results


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--block", type=int, default=os.environ.get("STUDY_BLOCK"))
    parser.add_argument(
        "--phase", choices=("calibration", "confirmatory"), default="calibration"
    )
    parser.add_argument(
        "--output", type=Path, default=ROOT / "target" / "debug-symbols-study"
    )
    args = parser.parse_args()
    if args.block is None or args.block < 0:
        parser.error("block must be nonnegative")
    plan = json.loads((Path(__file__).with_name("study-plan.json")).read_text())
    if args.phase == "calibration" and args.block not in plan["calibration_blocks"]:
        parser.error("Unregistered calibration block")
    if args.phase == "confirmatory" and args.block < plan["confirmatory_block_start"]:
        parser.error("Confirmatory IDs must start at the registered offset")
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    system, host, compiler, tools = build_context(True)
    if host not in plan["targets"]:
        raise RuntimeError("Host target is outside the registered study")
    commit = run(["git", "rev-parse", "HEAD"], cwd=ROOT).strip()
    inputs = fingerprints()
    if inputs != plan["source_fingerprints"]:
        raise RuntimeError("Source inputs differ from the registered study")
    order = treatment_order(args.block, host)
    summary = {
        "schema": 1,
        "study": plan["study_id"],
        "plan_sha256": plan_fingerprint(plan),
        "phase": args.phase,
        "block": args.block,
        "target": host,
        "platform": system,
        "commit": commit,
        "main_source_revision": SOURCE_REVISION,
        "source_fingerprints": inputs,
        "design_seed": DESIGN_SEED,
        "order": order,
        "machine": machine_info(),
        "runner": {
            key: os.environ.get(key)
            for key in (
                "GITHUB_RUN_ID",
                "GITHUB_RUN_ATTEMPT",
                "GITHUB_JOB",
                "RUNNER_NAME",
            )
        },
        "treatments": {},
        "verified": False,
    }
    save_report(output, summary)
    # Make crate downloads a setup operation for every order, not part of a treatment.
    run(
        ["cargo", "fetch", "--locked", "--target", host],
        cwd=ROOT,
        log=output / "fetch.log",
    )
    maturin = run(["maturin", "--version"]).strip()
    for name in order:
        directory = output / "variants" / name
        directory.mkdir(parents=True)
        mode = "baseline" if name.startswith("none-") else "symbols"
        report = {
            "platform": system,
            "target": host,
            "rustc": compiler,
            "maturin": maturin,
            "commit": commit,
            "tools": tools,
            "builds": {},
            "debug_level": "none" if mode == "baseline" else name,
            "codegen_units": 1,
            "pgo": True,
            "telemetry": True,
            "study_treatment": name,
            "study_block": args.block,
        }
        build(directory, report, tools, system, host, modes=(mode,))
        summary["treatments"][name] = report
        save_report(output, summary)
    # Verify without copying potentially gigabytes of companions into pair directories.
    for name, report in summary["treatments"].items():
        directory = output / "variants" / name
        if name.startswith("none-"):
            for executable, record in report["builds"]["baseline"][
                "executables"
            ].items():
                binary = directory / "baseline" / executable
                if (
                    digest(binary) != record["sha256"]
                    or sbom_digest(binary, tools, system) != record["sbom_sha256"]
                ):
                    raise RuntimeError(f"Control executable changed: {executable}")
                if system == "Windows":
                    imports = run([tools["llvm-readobj"], "--coff-imports", binary])
                    if re.search(
                        r"vcruntime[\d_]*\.dll|ucrtbase\.dll|api-ms-win-crt-",
                        imports,
                        re.IGNORECASE,
                    ):
                        raise RuntimeError(
                            "Control dynamically links the Visual C++ runtime"
                        )
                    record["static_crt"] = True
                if system == "Darwin":
                    run(["codesign", "--verify", binary])
            verify_install(
                directory / "baseline",
                report["builds"]["baseline"],
                report["executables"],
                system,
            )
            report["verified"] = True
        else:
            verification = output / "checks" / name
            verification.mkdir(parents=True)
            paired = report | {
                "builds": {
                    "baseline": summary["treatments"]["none-a"]["builds"]["baseline"],
                    "symbols": report["builds"]["symbols"],
                }
            }
            verify(
                verification,
                paired,
                tools,
                system,
                directories={
                    "baseline": output / "variants" / "none-a" / "baseline",
                    "symbols": directory / "symbols",
                },
            )
            report["verified"] = paired["verified"]
        save_report(directory, report)
        save_report(output, summary)
    summary["benchmarks"] = benchmark(output, summary["treatments"], args.block, host)
    for report in summary["treatments"].values():
        for measured in report["builds"].values():
            for resources in (measured["resources"], measured["pgo"]["resources"]):
                if resources["samples"] < 3 or not resources["observed_processes"]:
                    raise RuntimeError(
                        "Resource telemetry is missing; retain the completed build data"
                    )
    summary["verified"] = True
    save_report(output, summary)
    print(
        json.dumps(
            {"block": args.block, "target": host, "order": order, "verified": True}
        )
    )


if __name__ == "__main__":
    main()
