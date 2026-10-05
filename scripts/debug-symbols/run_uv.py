"""Compare native optimized uv builds with and without separate debug symbols."""

import argparse
import hashlib
import json
import os
import platform
import re
import shutil
import sys
import tempfile
import time
import zlib
from pathlib import Path
from zipfile import ZipFile

from run import (
    repack_wheel,
    run,
    save_build_script_logs,
    separate_symbols,
    symbolize,
    tool,
    verify_symbols,
)

ROOT = Path(__file__).resolve().parents[2]


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def sbom_digest(binary, tools, system):
    sections = run([tools["llvm-readobj"], "--sections", binary])
    for section in sections.split("Section {")[1:]:
        if not re.search(r"Name: \.dep-v0(?:\s|$)", section):
            continue
        offset_field, size_field = (
            ("PointerToRawData", "RawDataSize")
            if system == "Windows"
            else ("Offset", "Size")
        )
        (offset,) = re.findall(rf"\b{offset_field}: (0x[0-9A-Fa-f]+|\d+)", section)
        (size,) = re.findall(rf"\b{size_field}: (0x[0-9A-Fa-f]+|\d+)", section)
        with binary.open("rb") as stream:
            stream.seek(int(offset, 0))
            data = stream.read(int(size, 0))
        # Validate the embedded JSON as well as recording its exact section bytes.
        json.loads(zlib.decompress(data))
        return hashlib.sha256(data).hexdigest()
    raise RuntimeError(f"Missing embedded SBOM in {binary}")


def symbol_candidates(binary, symbols, tools, system):
    if system == "Windows":
        sections = run([tools["llvm-readobj"], "--sections", binary])
        section_addresses = {
            int(number): int(address, 16)
            for number, address in re.findall(
                r"Number: (\d+).*?VirtualAddress: (0x[0-9A-Fa-f]+)", sections, re.DOTALL
            )
        }
        records = run(
            [tools["llvm-pdbutil"], "dump", "--publics", "--symbols", symbols]
        )
        functions = re.findall(
            r"S_PUB32[^\n]*`([^`]+)`\s+flags = function, addr = (\d+):(\d+)",
            records,
        )
        # Rust entry points may appear only in module procedure records.
        functions += re.findall(
            r"S_[GL]PROC32(?:_ID)?[^\n]*`([^`]+)`\s+parent = [^\n]*?addr = (\d+):(\d+)",
            records,
        )
        # PDB addresses are decimal section:offset pairs, not PE RVAs.
        return {
            name: section_addresses[int(section)] + int(offset)
            for name, section, offset in functions
            if int(section) in section_addresses
        }
    source = symbols
    if system == "Darwin":
        (source,) = (symbols / "Contents" / "Resources" / "DWARF").iterdir()
    output = run(
        [tools["llvm-nm"], "--defined-only", "--demangle", "--format=posix", source]
    )
    return {
        name: int(address, 16)
        for name, address in re.findall(
            r"^(.+?) [tT] ([0-9a-fA-F]+)(?: .*)?$", output, re.MULTILINE
        )
    }


def select_functions(binary, symbols, tools, system):
    candidates = symbol_candidates(binary, symbols, tools, system)
    stem = binary.stem
    queries = {
        "rust": (rf"(?:{stem}::main|{len(stem)}{stem}4main)", ".rs"),
    }
    if stem == "uv":
        queries["aws_lc"] = (r"aws_lc.*_RAND_bytes$", "rand.c")
        queries["jitterentropy"] = (r"(?:^|_)jent_read_entropy$", "jitterentropy")
    selected = {}
    for category, (pattern, expected_source) in queries.items():
        matches = sorted(
            (
                (name, address)
                for name, address in candidates.items()
                if re.search(pattern, name)
            ),
            key=lambda item: len(item[0]),
        )
        if not matches and category == "jitterentropy":
            selected[category] = {"status": "not linked"}
            continue
        attempts = []
        for name, address in matches[:20]:
            lookup, resolved = symbolize(binary, symbols, address, tools, system)
            attempts.append({"name": name, "address": address, "lookup": lookup})
            if resolved and expected_source in lookup:
                selected[category] = {
                    "name": name,
                    "address": address,
                    "source": expected_source,
                }
                break
        else:
            raise RuntimeError(
                f"Cannot find {category} source location in {binary.name}: {attempts}"
            )
    return selected


def smoke_test(directory, names, python):
    uv = directory / names[0]
    version = run([uv, "--version"], cwd=ROOT).strip()
    if not version.startswith("uv "):
        raise RuntimeError(f"Unexpected uv version: {version}")
    for name in names:
        run([directory / name, "--help"], cwd=ROOT)
    with tempfile.TemporaryDirectory(prefix="uv-symbol-smoke-") as temporary:
        environment = Path(temporary) / "venv"
        run(
            [uv, "--no-config", "--offline", "venv", "--python", python, environment],
            cwd=ROOT,
        )
        executable = environment / (
            "Scripts/python.exe" if os.name == "nt" else "bin/python"
        )
        output = run([executable, "-c", "print('uv-symbols-ok')"], cwd=ROOT)
        if output.strip() != "uv-symbols-ok":
            raise RuntimeError("uv-created environment failed its smoke test")
    return version


def save_report(output, report):
    (output / "report.json").write_text(
        json.dumps(report, indent=2) + "\n", encoding="utf-8"
    )


def train_pgo(temporary, directory, environment, tools, host):
    training = Path(temporary) / "pgo"
    retained = directory / "pgo"
    retained.mkdir()
    started = time.monotonic()
    try:
        run(
            [
                sys.executable,
                ROOT / "scripts" / "build_uv_pgo.py",
                "--target",
                host,
                "--target-dir",
                training,
                "--llvm-profdata",
                tools["llvm-profdata"],
                "--train-only",
            ],
            cwd=ROOT,
            env=environment,
            log=retained / "training.log",
        )
    finally:
        save_build_script_logs(training / "instrumented", retained)
    seconds = time.monotonic() - started
    profile = retained / "uv.profdata"
    shutil.copyfile(training / "uv.profdata", profile)
    summary = run(
        [tools["llvm-profdata"], "show", "--detailed-summary", profile],
        log=retained / "profile-summary.log",
    )
    (functions,) = re.findall(r"Total functions: (\d+)", summary)
    (maximum_count,) = re.findall(r"Maximum internal block count: (\d+)", summary)
    (total_count,) = re.findall(r"Total count: (\d+)", summary)
    if not int(functions) or not int(total_count):
        raise RuntimeError("PGO training produced no executed functions")
    profiles = list((training / "profiles").glob("uv-*.profraw"))
    measured = {
        "training_seconds": seconds,
        "profile": str(profile.relative_to(directory)),
        "profile_sha256": digest(profile),
        "profile_bytes": profile.stat().st_size,
        "raw_profiles": len(profiles),
        "raw_profile_bytes": sum(path.stat().st_size for path in profiles),
        "functions": int(functions),
        "maximum_internal_block_count": int(maximum_count),
        "total_count": int(total_count),
    }
    # Only the merged profile is needed by the final build. Remove the instrumented
    # binaries and training corpus before compiling again to limit disk usage.
    shutil.rmtree(training)
    return profile, measured


def profile_diagnostics(log):
    diagnostics = [
        line
        for line in log.splitlines()
        if "warning:" in line and re.search(r"profile|pgo", line, re.IGNORECASE)
    ]
    return {
        "warnings": len(diagnostics),
        "missing": sum("no profile data available" in line for line in diagnostics),
        "mismatch": sum("mismatch" in line for line in diagnostics),
        "examples": diagnostics[:20],
    }


def build(output, report, tools, system, host):
    names = ["uv.exe", "uvx.exe", "uvw.exe"] if system == "Windows" else ["uv", "uvx"]
    report["executables"] = names
    for mode in ("baseline", "symbols"):
        directory = output / mode
        directory.mkdir()
        # Separate target directories make both builds cold. Delete intermediates
        # before symbol lookups and before starting the next build to limit disk use.
        with tempfile.TemporaryDirectory(
            prefix="uv-symbol-build-", dir=output
        ) as temporary:
            environment = os.environ | {
                "CARGO_TARGET_DIR": temporary,
                "CARGO_INCREMENTAL": "0",
                "CARGO_TERM_COLOR": "never",
                "CARGO_PROFILE_RELEASE_DEBUG": "full" if mode == "symbols" else "none",
                "CARGO_PROFILE_RELEASE_STRIP": "none"
                if mode == "symbols"
                else "symbols",
                "CARGO_PROFILE_RELEASE_SPLIT_DEBUGINFO": "off"
                if system == "Linux"
                else "packed",
                "AWS_LC_SYS_USE_SYSTEM": "0",
                "CC_ENABLE_DEBUG_OUTPUT": "1",
                "CARGO": str(
                    ROOT
                    / "scripts"
                    / ("cargo.cmd" if system == "Windows" else "cargo.sh")
                ),
            }
            if system == "Windows":
                environment["AWS_LC_SYS_PREBUILT_NASM"] = "0"
                if report["pgo"]:
                    # Setting RUSTFLAGS overrides the target's Cargo configuration.
                    environment["RUSTFLAGS"] = "-C target-feature=+crt-static"
            if system == "Darwin":
                environment["RUSTFLAGS"] = (
                    "-C linker=rust-lld -C linker-flavor=ld64.lld -C link-arg=--icf=safe"
                )
                environment["MACOSX_DEPLOYMENT_TARGET"] = "11.0"
                if report["pgo"]:
                    for variable in ("CFLAGS", "CXXFLAGS"):
                        environment[variable] = " ".join(
                            (
                                environment.get(variable, ""),
                                "-fno-profile-generate -fno-profile-use",
                            )
                        ).strip()
            if host == "aarch64-unknown-linux-gnu":
                environment["JEMALLOC_SYS_WITH_LG_PAGE"] = "16"
                if report["pgo"]:
                    # GNU ld cannot resolve long-range calls in the instrumented
                    # ARM64 binary; use the same bundled LLD as the release build.
                    linker_directory = Path(tools["llvm-profdata"]).parent / "gcc-ld"
                    if not (linker_directory / "ld.lld").is_file():
                        raise RuntimeError("Missing bundled ARM64 LLD")
                    environment["RUSTFLAGS"] = " ".join(
                        (
                            environment.get("RUSTFLAGS", ""),
                            f"-C link-arg=-B{linker_directory} -C link-arg=-fuse-ld=lld",
                        )
                    ).strip()
            if report["pgo"]:
                environment["RUSTFLAGS"] = " ".join(
                    (
                        environment.get("RUSTFLAGS", ""),
                        "-Cllvm-args=-pgo-warn-missing-function",
                    )
                ).strip()
            measured = {"executables": {}}
            report["builds"][mode] = measured
            save_report(output, report)
            if report["pgo"]:
                profile, measured["pgo"] = train_pgo(
                    temporary, directory, environment, tools, host
                )
                environment["RUSTFLAGS"] += f" -Cprofile-use={profile}"
                save_report(output, report)
            features = (
                "self-update,windows-gui-bin" if system == "Windows" else "self-update"
            )
            started = time.monotonic()
            build_log = run(
                [
                    "maturin",
                    "build",
                    "--release",
                    "--locked",
                    "--target",
                    host,
                    "--features",
                    features,
                    "--strip=false" if mode == "symbols" else "--strip=true",
                    "--out",
                    directory,
                    "--verbose",
                ],
                cwd=ROOT,
                env=environment,
                log=directory / "build.log",
            )
            measured.update(
                {
                    "seconds": time.monotonic() - started,
                    "environment": {
                        key: environment[key]
                        for key in sorted(environment)
                        if key.startswith(
                            ("CARGO_PROFILE_", "AWS_LC_SYS_", "JEMALLOC_SYS_")
                        )
                        or key
                        in {
                            "RUSTFLAGS",
                            "MACOSX_DEPLOYMENT_TARGET",
                            "CARGO_BUILD_JOBS",
                            "CFLAGS",
                            "CXXFLAGS",
                        }
                    },
                }
            )
            if report["pgo"]:
                measured["pgo"]["diagnostics"] = profile_diagnostics(build_log)
                measured["total_seconds"] = (
                    measured["seconds"] + measured["pgo"]["training_seconds"]
                )
            save_build_script_logs(Path(temporary), directory)
            (wheel,) = directory.glob("*.whl")
            measured["wheel_bytes"] = wheel.stat().st_size
            measured["wheel"] = wheel.name
            with ZipFile(wheel) as archive:
                for name in names:
                    binary = directory / name
                    (member,) = (
                        entry
                        for entry in archive.namelist()
                        if entry.endswith(f".data/scripts/{name}")
                    )
                    with (
                        archive.open(member) as source,
                        binary.open("wb") as destination,
                    ):
                        shutil.copyfileobj(source, destination)
                    binary.chmod(0o755)
                    record = {
                        "executable_bytes": binary.stat().st_size,
                        "sbom_sha256": sbom_digest(binary, tools, system),
                    }
                    measured["executables"][name] = record
                    if mode == "symbols":
                        built = Path(temporary) / host / "release" / name
                        if digest(binary) != digest(built):
                            raise RuntimeError(f"Maturin changed {name}")
                        symbols, record["identity"] = separate_symbols(
                            binary, built, tools, system
                        )
                        record["symbols"] = symbols.name
                        record["symbol_bytes"] = (
                            sum(
                                path.stat().st_size
                                for path in symbols.rglob("*")
                                if path.is_file()
                            )
                            if symbols.is_dir()
                            else symbols.stat().st_size
                        )
                    elif system == "Darwin":
                        run(["codesign", "--force", "--sign", "-", binary])
                    if sbom_digest(binary, tools, system) != record["sbom_sha256"]:
                        raise RuntimeError(f"Processing changed the SBOM in {name}")
                    record["stripped_executable_bytes"] = binary.stat().st_size
                    record["sha256"] = digest(binary)
            original = wheel.with_suffix(".original.whl")
            wheel.rename(original)
            repack_wheel(original, wheel, [directory / name for name in names])
            measured["stripped_wheel_bytes"] = wheel.stat().st_size
            measured["version"] = smoke_test(directory, names, sys.executable)
            save_report(output, report)


def verify(output, report, tools, system):
    directory = output / "symbols"
    names = report["executables"]
    for mode, measured in report["builds"].items():
        for name, record in measured["executables"].items():
            binary = output / mode / name
            # Artifact downloads do not retain executable permissions.
            binary.chmod(0o755)
            if sbom_digest(binary, tools, system) != record["sbom_sha256"]:
                raise RuntimeError(f"The SBOM changed in {mode}/{name}")
    for name in names:
        binary = directory / name
        record = report["builds"]["symbols"]["executables"][name]
        if digest(binary) != record["sha256"]:
            raise RuntimeError(f"{name} changed since the build")
        symbols = directory / record["symbols"]
        record["selected_functions"] = select_functions(binary, symbols, tools, system)
        functions = {
            item["name"]: item["source"]
            for item in record["selected_functions"].values()
            if "name" in item
        }
        addresses = {
            item["name"]: item["address"]
            for item in record["selected_functions"].values()
            if "name" in item
        }
        record["lookups"], record["lookups_without_symbols"] = verify_symbols(
            binary, symbols, addresses, functions, tools, system
        )
        if system == "Windows":
            imports = run([tools["llvm-readobj"], "--coff-imports", binary])
            if re.search(
                r"vcruntime[\d_]*\.dll|ucrtbase\.dll|api-ms-win-crt-",
                imports,
                re.IGNORECASE,
            ):
                raise RuntimeError(f"{name} dynamically links the Visual C++ runtime")
            record["static_crt"] = True
        if system == "Darwin":
            run(["codesign", "--verify", binary])
        save_report(output, report)

    with tempfile.TemporaryDirectory(prefix="uv-symbol-install-") as temporary:
        environment = Path(temporary) / "venv"
        run([sys.executable, "-m", "venv", "--without-pip", environment])
        executable_directory = environment / (
            "Scripts" if system == "Windows" else "bin"
        )
        python = executable_directory / (
            "python.exe" if system == "Windows" else "python"
        )
        wheel = directory / report["builds"]["symbols"]["wheel"]
        run(["uv", "pip", "install", "--python", python, "--no-index", wheel])
        for name in names:
            if digest(executable_directory / name) != digest(directory / name):
                raise RuntimeError(f"Installed {name} differs from verified executable")
        smoke_test(executable_directory, names, python)
        run([python, "-m", "uv", "--help"])
    report["verified"] = True
    baseline = report["builds"]["baseline"]
    measured = report["builds"]["symbols"]
    report["wheel_size_delta_bytes"] = (
        measured["stripped_wheel_bytes"] - baseline["stripped_wheel_bytes"]
    )
    report["executable_size_delta_bytes"] = {
        name: measured["executables"][name]["stripped_executable_bytes"]
        - baseline["executables"][name]["stripped_executable_bytes"]
        for name in names
    }
    save_report(output, report)
    print(json.dumps(report, indent=2))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--pgo",
        action="store_true",
        help="Train separate release PGO profiles for the baseline and symbols builds",
    )
    parser.add_argument(
        "--output", type=Path, default=ROOT / "target" / "uv-debug-symbols-experiment"
    )
    parser.add_argument(
        "--verify-only",
        action="store_true",
        help="Verify retained outputs without rebuilding",
    )
    args = parser.parse_args()
    output = args.output.resolve()
    system = platform.system()
    if system not in {"Linux", "Darwin", "Windows"}:
        raise RuntimeError(f"Unsupported experiment platform: {system}")
    compiler_version = run(["rustc", "-vV"], cwd=ROOT)
    host = next(
        line.removeprefix("host: ")
        for line in compiler_version.splitlines()
        if line.startswith("host: ")
    )
    sysroot = Path(run(["rustc", "--print", "sysroot"], cwd=ROOT).strip())
    required = {
        "Linux": {
            "llvm-nm",
            "llvm-symbolizer",
            "llvm-readobj",
            "llvm-objcopy",
            "llvm-strip",
        },
        "Darwin": {"llvm-nm", "llvm-dwarfdump", "llvm-readobj"},
        "Windows": {"llvm-pdbutil", "llvm-symbolizer", "llvm-readobj"},
    }
    tools = {name: tool(name, sysroot, host) for name in required[system]}
    if args.pgo:
        # Profiles must be read by the same LLVM version that wrote them.
        profiler = (
            sysroot
            / "lib"
            / "rustlib"
            / host
            / "bin"
            / ("llvm-profdata.exe" if system == "Windows" else "llvm-profdata")
        )
        if not profiler.is_file():
            raise RuntimeError("Install llvm-tools-preview for PGO training")
        tools["llvm-profdata"] = str(profiler)
    if args.verify_only:
        report = json.loads((output / "report.json").read_text(encoding="utf-8"))
        if report["target"] != host:
            raise RuntimeError("Verification requires the original native target")
    else:
        output.mkdir(parents=True, exist_ok=False)
        report = {
            "platform": system,
            "target": host,
            "rustc": compiler_version,
            "maturin": run(["maturin", "--version"]).strip(),
            "commit": run(["git", "rev-parse", "HEAD"], cwd=ROOT).strip(),
            "tools": tools,
            "builds": {},
            "pgo": args.pgo,
            "scope": "Native release profile, fat LTO, self-update, cargo-auditable; optional release PGO training; no manylinux container or release signing",
        }
        build(output, report, tools, system, host)
    verify(output, report, tools, system)


if __name__ == "__main__":
    main()
