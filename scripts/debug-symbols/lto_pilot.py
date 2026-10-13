"""Compare fat/thin instrumentation with no/full debug on one Windows allocation."""

import argparse
import json
import os
import re
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
from study import benchmark, fingerprints
from telemetry import machine_info


def check_compiler_flags(path, names, lto, debug_level, *, instrumented):
    commands = {}
    for name in names:
        matches = [
            line
            for line in path.read_text(encoding="utf-8").splitlines()
            if f"--crate-name {name.removesuffix('.exe')} " in line
            and "--crate-type bin" in line
            and "Running" in line
        ]
        if len(matches) != 1:
            raise RuntimeError(f"Expected one compiler invocation for {name} in {path}")
        command = matches[0]
        for setting, expected in {
            "lto": lto,
            "codegen-units": "1",
            "opt-level": "3",
            "panic": "abort",
            "target-feature": "+crt-static",
        }.items():
            values = re.findall(rf"-C\s*{setting}=([^\s`]+)", command)
            if not values or values[-1] != expected:
                raise RuntimeError(
                    f"Unexpected effective {setting} for {name}: {values}"
                )
        debug = re.findall(r"-C\s*debuginfo=([^\s`]+)", command)
        if debug_level == "full" and (not debug or debug[-1] != "2"):
            raise RuntimeError(f"Full debug information missing for {name}")
        if debug_level == "none" and debug and debug[-1] not in {"0", "none"}:
            raise RuntimeError(f"No-debug control has debug information: {name}")
        required = "profile-generate" if instrumented else "profile-use"
        forbidden = "profile-use" if instrumented else "profile-generate"
        if not re.search(rf"-C\s*{required}=", command) or re.search(
            rf"-C\s*{forbidden}=", command
        ):
            raise RuntimeError(f"Unexpected PGO flags for {name}")
        commands[name] = command
    return commands


def verify_control(directory, report, tools, system):
    measured = report["builds"]["baseline"]
    for name, record in measured["executables"].items():
        binary = directory / name
        if (
            digest(binary) != record["sha256"]
            or sbom_digest(binary, tools, system) != record["sbom_sha256"]
        ):
            raise RuntimeError(f"Control executable changed: {name}")
        imports = run([tools["llvm-readobj"], "--coff-imports", binary])
        if re.search(
            r"(?:vcruntime|msvcp|msvcr|ucrtbase|api-ms-win-crt)[^\s]*\.dll",
            imports,
            re.IGNORECASE,
        ):
            raise RuntimeError(f"Control imports a dynamic CRT: {name}")
        record["static_crt"] = True
    verify_install(directory, measured, report["executables"], system)
    report["verified"] = True


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--output", type=Path, default=ROOT / "target" / "debug-symbols-lto-pilot"
    )
    args = parser.parse_args()
    plan = json.loads(Path(__file__).with_name("lto-pilot-plan.json").read_text())
    system, host, compiler, tools = build_context(True)
    if host != plan["target"] or os.environ.get("CARGO_BUILD_JOBS") != "4":
        raise RuntimeError("This pilot requires Windows x86-64 with four Cargo jobs")
    source = fingerprints()
    if source != plan["source_fingerprints"]:
        raise RuntimeError("Source differs from the pinned pilot revision")
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    commit = run(["git", "rev-parse", "HEAD"], cwd=ROOT).strip()
    summary = {
        "schema": 1,
        "study": plan["study_id"],
        "phase": "pilot",
        "plan_sha256": plan_fingerprint(plan),
        "commit": commit,
        "main_source_revision": plan["main_source_revision"],
        "source_fingerprints": source,
        "target": host,
        "platform": system,
        "machine": machine_info(),
        "order": plan["order"],
        "treatments": {},
        "failures": {},
        "verified": False,
        "runner": {
            key: os.environ.get(key)
            for key in (
                "GITHUB_RUN_ID",
                "GITHUB_RUN_ATTEMPT",
                "GITHUB_JOB",
                "RUNNER_NAME",
            )
        },
    }
    save_report(output, summary)
    run(
        ["cargo", "fetch", "--locked", "--target", host],
        cwd=ROOT,
        log=output / "fetch.log",
    )
    maturin = run(["maturin", "--version"]).strip()
    for name in plan["order"]:
        settings = plan["treatments"][name]
        mode = "baseline" if settings["debug_level"] == "none" else "symbols"
        directory = output / "variants" / name
        directory.mkdir(parents=True)
        report = {
            "platform": system,
            "target": host,
            "rustc": compiler,
            "maturin": maturin,
            "commit": commit,
            "tools": tools,
            "builds": {},
            "debug_level": settings["debug_level"],
            "training_lto": settings["training_lto"],
            "codegen_units": 1,
            "pgo": True,
            "telemetry": True,
            "verified": False,
        }
        summary["treatments"][name] = report
        save_report(output, summary)
        try:
            build(directory, report, tools, system, host, modes=(mode,))
            report["compiler_commands"] = {
                "instrumented": check_compiler_flags(
                    directory / mode / "pgo" / "training.log",
                    report["executables"],
                    settings["training_lto"],
                    settings["debug_level"],
                    instrumented=True,
                ),
                "final": check_compiler_flags(
                    directory / mode / "build.log",
                    report["executables"],
                    "fat",
                    settings["debug_level"],
                    instrumented=False,
                ),
            }
        except (RuntimeError, OSError, ValueError) as error:
            # Independent variants still run; every failed outcome remains visible.
            summary["failures"][name] = f"{type(error).__name__}: {error}"
        finally:
            save_report(directory, report)
            save_report(output, summary)
    if summary["failures"]:
        raise RuntimeError(f"Pilot variants failed: {list(summary['failures'])}")
    for lto in ("fat", "thin"):
        control = summary["treatments"][f"{lto}-none"]
        verify_control(
            output / "variants" / f"{lto}-none" / "baseline", control, tools, system
        )
        symbols = summary["treatments"][f"{lto}-full"]
        checks = output / "checks" / lto
        checks.mkdir(parents=True)
        paired = symbols | {
            "builds": {
                "baseline": control["builds"]["baseline"],
                "symbols": symbols["builds"]["symbols"],
            }
        }
        verify(
            checks,
            paired,
            tools,
            system,
            directories={
                "baseline": output / "variants" / f"{lto}-none" / "baseline",
                "symbols": output / "variants" / f"{lto}-full" / "symbols",
            },
        )
        symbols["verified"] = paired["verified"]
        save_report(output, summary)
    summary["benchmarks"] = benchmark(
        output,
        summary["treatments"],
        0,
        host,
        treatments=tuple(plan["treatments"]),
        reference="fat-none",
    )
    for report in summary["treatments"].values():
        for measured in report["builds"].values():
            for resources in (measured["resources"], measured["pgo"]["resources"]):
                if resources["samples"] < 3 or not resources["observed_processes"]:
                    raise RuntimeError("Incomplete resource sampling")
    summary["verified"] = True
    save_report(output, summary)
    print(
        json.dumps(
            {"study": plan["study_id"], "verified": True, "order": plan["order"]}
        )
    )


if __name__ == "__main__":
    main()
