"""Measure debugger capabilities of optimized Rust+C fixtures, with live controls."""

import argparse
import hashlib
import json
import os
import platform
import shlex
import shutil
import subprocess
import tempfile
import traceback
from pathlib import Path

from lto_pilot import check_compiler_flags
from run import run, separate_symbols, tool

ROOT = Path(__file__).resolve().parents[2]
FIXTURE = Path(__file__).with_name("capabilities")
VARIANTS = {
    "unoptimized-full": ("full", False),
    "optimized-none": ("none", True),
    "optimized-line-tables": ("line-tables-only", True),
    "optimized-limited": ("limited", True),
    "optimized-full": ("full", True),
}


def save(path, data):
    path.write_text(json.dumps(data, indent=2) + "\n")


def digest(path):
    checksum = hashlib.sha256()
    with path.open("rb") as stream:
        while chunk := stream.read(1024 * 1024):
            checksum.update(chunk)
    return checksum.hexdigest()


def debugger_probe(debugger, binary, symbols, destination, sysroot):
    source = FIXTURE / "src" / "main.rs"
    configuration = {
        "binary": str(binary),
        "symbols": str(symbols) if symbols else None,
        "source": str(source),
        "step_line": next(
            index
            for index, line in enumerate(source.read_text().splitlines(), 1)
            if "STEP_START" in line
        ),
        "output": str(destination / "debugger.json"),
        "formatter_script": str(sysroot / "lib/rustlib/etc/lldb_lookup.py"),
    }
    save(destination / "debugger-config.json", configuration)
    arguments = [
        debugger,
        "--batch",
        "--no-lldbinit",
        "-o",
        "command script import " + shlex.quote(str(FIXTURE / "lldb_probe.py")),
        "-o",
        "capability-probe " + shlex.quote(str(destination / "debugger-config.json")),
    ]
    with (destination / "debugger.log").open("w") as stream:
        result = subprocess.run(
            arguments, stdout=stream, stderr=subprocess.STDOUT, check=False, timeout=180
        )
    path = destination / "debugger.json"
    if not path.exists():
        raise RuntimeError(
            f"Debugger produced no report (exit {result.returncode}); see {destination / 'debugger.log'}"
        )
    report = json.loads(path.read_text())
    if result.returncode or not report["completed"]:
        raise RuntimeError(f"Debugger observation failed: {report.get('error')}")
    return report


def variable(observation, name):
    return next(
        (
            value
            for frame in observation["matching_frames"]
            for value in frame["variables"]
            if value["name"] == name
        ),
        None,
    )


def summarize(probe):
    observations = {item["scenario"]: item for item in probe["observations"]}
    live = observations["live_values"]
    fields = {}
    for name in (
        "seed",
        "live_scalar",
        "constant",
        "record",
        "tuple",
        "optional",
        "choice",
        "text",
        "items",
    ):
        fields[name] = variable(live, name)
    record = fields["record"]
    struct_fields = {
        child["name"]: child["unsigned"] for child in (record or {}).get("children", [])
    }
    values = lambda item: {
        value["name"]: value
        for frame in item["matching_frames"]
        for value in frame["variables"]
    }
    return {
        "source_locations": {
            name: any(
                frame["line"]
                and frame["file"]
                and (
                    frame["file"].endswith("main.rs")
                    or frame["file"].endswith("native.c")
                )
                for frame in item["matching_frames"]
            )
            for name, item in observations.items()
        },
        "live_scalar_correct": (fields["live_scalar"] or {}).get("unsigned") == 58,
        "argument_correct": (fields["seed"] or {}).get("unsigned") == 17,
        "record_fields_correct": struct_fields.get("count") == 58
        and struct_fields.get("tag") == 23,
        "live_variables": fields,
        "optimized_variables": values(observations["optimized_values"]),
        "partial_record": variable(observations["split_record"], "partial"),
        "before_last_use": variable(observations["before_last_use"], "temporary"),
        "after_last_use": variable(observations["after_last_use"], "temporary"),
        "shadow_inner": variable(observations["shadow_inner"], "shadowed"),
        "shadow_outer": variable(observations["shadow_outer"], "shadowed"),
        "inline_frames": [
            frame["function"]
            for frame in observations["inline_chain"]["frames"]
            if frame["inline"]
            and "capability_fixture::inline_" in (frame["function"] or "")
        ],
        "native_variables": values(observations["native_values"]),
        "tail_frames": [
            {
                key: frame[key]
                for key in ("function", "inline", "artificial", "file", "line")
            }
            for frame in observations["tail_call"]["frames"]
            if "tail_" in (frame["function"] or "")
        ],
        "types": probe["types"],
        "source_breakpoint_locations": probe["stepping"]["locations"],
        "stepping_stops": [
            {
                "function": stop["frames"][0]["function"],
                "file": stop["frames"][0]["file"],
                "line": stop["frames"][0]["line"],
            }
            for stop in probe["stepping"]["stops"]
        ],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--output", type=Path, default=ROOT / "target" / "debug-symbols-capabilities"
    )
    parser.add_argument(
        "--build-only",
        action="store_true",
        help="Retain prepared binaries where debugger attachment is unavailable",
    )
    parser.add_argument(
        "--variants", nargs="+", choices=VARIANTS, default=list(VARIANTS)
    )
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    system = platform.system()
    compiler = run(["rustc", "-vV"])
    host = next(
        line.removeprefix("host: ")
        for line in compiler.splitlines()
        if line.startswith("host: ")
    )
    sysroot = Path(run(["rustc", "--print", "sysroot"]).strip())
    suffix = ".exe" if system == "Windows" else ""
    bundled = sysroot / "lib" / "rustlib" / host / "bin"
    tools = {}
    required = {"llvm-profdata", "llvm-nm", "llvm-readobj"}
    required |= (
        {"llvm-dwarfdump"} if system == "Darwin" else {"llvm-objcopy", "llvm-strip"}
    )
    for name in required:
        path = bundled / (name + suffix)
        tools[name] = str(path) if path.exists() else tool(name, sysroot, host)
    debugger = os.environ.get("LLDB") or shutil.which("lldb")
    if system == "Darwin":
        debugger = run(["xcrun", "--find", "lldb"]).strip()
    if not debugger and not args.build_only:
        raise RuntimeError("LLDB is unavailable")
    report = {
        "commit": run(["git", "rev-parse", "HEAD"], cwd=ROOT).strip(),
        "source_sha256": {
            str(path.relative_to(FIXTURE)): digest(path)
            for path in (
                FIXTURE / "src/main.rs",
                FIXTURE / "native.c",
                FIXTURE / "build.rs",
                FIXTURE / "Cargo.toml",
                FIXTURE / "Cargo.lock",
                FIXTURE / "lldb_probe.py",
            )
        },
        "platform": system,
        "target": host,
        "rustc": compiler,
        "tools": tools,
        "debugger": debugger,
        "variants": {},
        "failures": {},
        "runner": {
            name: os.environ.get(name)
            for name in ("GITHUB_RUN_ID", "GITHUB_RUN_ATTEMPT", "RUNNER_NAME")
        },
        "build_only": args.build_only,
    }
    save(output / "report.json", report)
    for name in args.variants:
        level, optimized = VARIANTS[name]
        destination = output / name
        destination.mkdir()
        try:
            with tempfile.TemporaryDirectory(prefix="uv-capability-") as temporary:
                temporary = Path(temporary)
                environment = os.environ.copy()
                environment.pop("CARGO_ENCODED_RUSTFLAGS", None)
                environment.update(
                    {
                        "CARGO_INCREMENTAL": "0",
                        "CARGO_BUILD_JOBS": "4",
                        "CARGO_PROFILE_RELEASE_DEBUG": level,
                        "CARGO_PROFILE_RELEASE_CODEGEN_UNITS": "1",
                        "CARGO_PROFILE_RELEASE_STRIP": "none",
                        "CARGO_PROFILE_DEV_STRIP": "none",
                        "CARGO_PROFILE_RELEASE_SPLIT_DEBUGINFO": "off"
                        if system == "Linux"
                        else "packed",
                        "CARGO_PROFILE_DEV_SPLIT_DEBUGINFO": "off"
                        if system == "Linux"
                        else "packed",
                    }
                )
                rustflags = (
                    "-Ctarget-feature=+crt-static" if system == "Windows" else ""
                )
                if system == "Darwin":
                    # Match the production helper: Apple Clang's profile runtime
                    # must not be mixed with Rust's LLVM instrumentation runtime.
                    for variable_name in ("CFLAGS", "CXXFLAGS"):
                        environment[variable_name] = (
                            "-fno-profile-generate -fno-profile-use"
                        )
                cargo = [
                    "cargo",
                    "build",
                    "--manifest-path",
                    FIXTURE / "Cargo.toml",
                    "--locked",
                    "--target",
                    host,
                    "--verbose",
                ]
                profile = "release" if optimized else "debug"
                if optimized:
                    cargo.append("--release")
                    raw = destination / "profiles"
                    raw.mkdir()
                    environment["CARGO_TARGET_DIR"] = str(temporary / "instrumented")
                    environment["RUSTFLAGS"] = (
                        f"{rustflags} -Cprofile-generate={raw} -Clto=thin -Cembed-bitcode=yes".strip()
                    )
                    run(
                        cargo,
                        cwd=FIXTURE,
                        env=environment,
                        log=destination / "instrumented.log",
                    )
                    instrumented = (
                        temporary
                        / "instrumented"
                        / host
                        / profile
                        / ("capability_fixture" + suffix)
                    )
                    train_environment = {
                        **environment,
                        "LLVM_PROFILE_FILE": str(raw / "%m-%p.profraw"),
                    }
                    run(
                        [instrumented, "train"],
                        cwd=destination,
                        env=train_environment,
                        log=destination / "training.log",
                    )
                    files = sorted(raw.glob("*.profraw"))
                    if not files:
                        raise RuntimeError("Fixture produced no PGO profiles")
                    merged = destination / "fixture.profdata"
                    run(
                        [
                            tools["llvm-profdata"],
                            "merge",
                            "-sparse",
                            *files,
                            "-o",
                            merged,
                        ],
                        log=destination / "merge.log",
                    )
                    environment["RUSTFLAGS"] = (
                        f"{rustflags} -Cprofile-use={merged} -Cllvm-args=-pgo-warn-missing-function".strip()
                    )
                else:
                    environment["RUSTFLAGS"] = rustflags
                environment["CARGO_TARGET_DIR"] = str(temporary / "final")
                if level == "none":
                    environment["CARGO_PROFILE_RELEASE_STRIP"] = "symbols"
                run(cargo, cwd=FIXTURE, env=environment, log=destination / "final.log")
                built = (
                    temporary
                    / "final"
                    / host
                    / profile
                    / ("capability_fixture" + suffix)
                )
                binary = destination / built.name
                shutil.copyfile(built, binary)
                binary.chmod(0o755)
                symbols = None
                identity = None
                if level != "none":
                    symbols, identity = separate_symbols(binary, built, tools, system)
                stdout = run([binary, "17"], cwd=destination).strip()
                if stdout != "536":
                    raise RuntimeError(f"Incorrect fixture output: {stdout}")
                measured = {
                    "debug_level": level,
                    "optimized": optimized,
                    "binary_sha256": digest(binary),
                    "binary_bytes": binary.stat().st_size,
                    "identity": identity,
                    "stdout": stdout,
                    "environment": {
                        key: value
                        for key, value in environment.items()
                        if key.startswith("CARGO_PROFILE")
                        or key in ("CARGO_BUILD_JOBS", "RUSTFLAGS")
                    },
                }
                if symbols:
                    measured["symbol_files"] = {
                        str(path.relative_to(destination)): {
                            "bytes": path.stat().st_size,
                            "sha256": digest(path),
                        }
                        for path in (
                            symbols.rglob("*") if symbols.is_dir() else [symbols]
                        )
                        if path.is_file()
                    }
                if optimized:
                    measured["compiler_commands"] = {
                        "instrumented": check_compiler_flags(
                            destination / "instrumented.log",
                            [binary.name],
                            "thin",
                            level,
                            instrumented=True,
                            system=system,
                        ),
                        "final": check_compiler_flags(
                            destination / "final.log",
                            [binary.name],
                            "fat",
                            level,
                            instrumented=False,
                            system=system,
                        ),
                    }
                    measured["profile_sha256"] = digest(merged)
                report["variants"][name] = measured
            # Remove original compilation paths before loading the separated companion.
            if not args.build_only:
                probe = debugger_probe(debugger, binary, symbols, destination, sysroot)
                measured["capabilities"] = summarize(probe)
                negative = destination / "without-symbols"
                negative.mkdir()
                shutil.copy2(binary, negative / binary.name)
                negative_probe = debugger_probe(
                    debugger, negative / binary.name, None, negative, sysroot
                )
                negative_summary = summarize(negative_probe)
                measured["without_symbols"] = negative_summary
                if (
                    any(negative_summary["source_locations"].values())
                    or negative_summary["source_breakpoint_locations"]
                ):
                    raise RuntimeError(
                        "Source information leaked into the missing-companion control"
                    )
                if name == "unoptimized-full" and not all(
                    measured["capabilities"][key]
                    for key in (
                        "live_scalar_correct",
                        "argument_correct",
                        "record_fields_correct",
                    )
                ):
                    raise RuntimeError(
                        "Unoptimized full-debug positive control failed; inspect tool/fixture compatibility"
                    )
            measured["completed"] = True
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
        args.variants
    )
    save(output / "report.json", report)
    if not report["completed"]:
        raise SystemExit(
            "Capability fixture has failed builds or debugger checks; partial evidence retained"
        )


if __name__ == "__main__":
    main()
