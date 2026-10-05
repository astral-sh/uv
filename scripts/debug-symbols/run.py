"""Build and verify optimized Rust+C binaries with separately published symbols.

Run with `uv run --no-project --with maturin==1.15.0 python scripts/debug-symbols/run.py`.
Only this standalone fixture is compiled; the uv workspace is not built.
"""

import argparse
import base64
import csv
import hashlib
import io
import json
import os
import platform
import re
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path
from zipfile import ZipFile

FIXTURE = Path(__file__).resolve().parent
FUNCTIONS = {"rust_frame": "main.rs", "native_frame": "native.c"}


def run(
    arguments,
    *,
    cwd=FIXTURE,
    env=None,
    log=None,
    allowed_exit_codes=(0,),
    merge_stderr=True,
):
    print("+", " ".join(map(str, arguments)), flush=True)
    # Keep partial build logs on disk even if a large build is interrupted.
    if log:
        with (
            log.open("w", encoding="utf-8") as stream,
            subprocess.Popen(
                list(map(str, arguments)),
                cwd=cwd,
                env=env,
                encoding="utf-8",
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT if merge_stderr else None,
            ) as process,
        ):
            for line in process.stdout:
                stream.write(line)
                if line.lstrip().startswith(
                    (
                        "Compiling ",
                        "Finished ",
                        "error:",
                        "warning:",
                        "Prepared ",
                        "Preparing training ",
                        "Building instrumented ",
                        "Training uv workload:",
                        "Profiled ",
                        "Merged ",
                    )
                ):
                    print(line.rstrip(), flush=True)
            returncode = process.wait()
        output = log.read_text(encoding="utf-8")
    else:
        result = subprocess.run(
            list(map(str, arguments)),
            cwd=cwd,
            env=env,
            encoding="utf-8",
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT if merge_stderr else None,
            check=False,
        )
        returncode = result.returncode
        output = result.stdout
    if returncode not in allowed_exit_codes:
        tail = "\n".join(output.splitlines()[-80:])
        raise RuntimeError(f"Command failed ({returncode}):\n{tail}")
    return output


def tool(name, sysroot, host):
    executable = name + (".exe" if os.name == "nt" else "")
    candidates = [
        Path(os.environ.get("LLVM_BIN", "/nonexistent")) / executable,
        sysroot / "lib" / "rustlib" / host / "bin" / executable,
        Path("C:/Program Files/LLVM/bin") / executable,
    ]
    if found := shutil.which(name):
        return found
    for candidate in candidates:
        if candidate.is_file():
            return str(candidate)
    if sys.platform == "darwin":
        return run(["xcrun", "--find", name]).strip()
    raise RuntimeError(f"Missing tool {name}; install LLVM or set LLVM_BIN")


def save_build_script_logs(target, destination):
    # Maturin does not forward all build-script output, even with Cargo verbosity.
    for source in target.glob("**/build/*/output"):
        log = destination / "build-scripts" / source.relative_to(target)
        log.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, log)


def addresses(binary, tools, system):
    if system == "Windows":
        output = run([tools["llvm-readobj"], "--coff-exports", binary])
        found = {
            name: int(address, 16)
            for name, address in re.findall(
                r"Name: (\w+)\s+RVA: (0x[0-9A-Fa-f]+)", output
            )
        }
    else:
        output = run([tools["llvm-nm"], "--defined-only", "--format=posix", binary])
        found = {}
        for line in output.splitlines():
            fields = line.split()
            if len(fields) >= 3:
                name = fields[0].removeprefix("_")
                if name in FUNCTIONS:
                    found[name] = int(fields[2], 16)
    if not FUNCTIONS.keys() <= found.keys():
        raise RuntimeError(f"Missing fixture function addresses:\n{output}")
    return {name: found[name] for name in FUNCTIONS}


def repack_wheel(source, destination, binaries):
    """Replace executables and regenerate the wheel's RECORD."""
    replacements = {binary.name: binary for binary in binaries}
    replaced = set()
    with ZipFile(source) as original, ZipFile(destination, "w") as wheel:
        records = []
        for entry in original.infolist():
            if entry.filename.endswith("/RECORD"):
                record_entry = entry
                continue
            replacement = None
            if ".data/scripts/" in entry.filename:
                name = entry.filename.rsplit("/", 1)[-1]
                if name in replacements:
                    replacement = replacements[name]
                    replaced.add(name)
            data = replacement.read_bytes() if replacement else original.read(entry)
            digest = base64.urlsafe_b64encode(hashlib.sha256(data).digest()).rstrip(
                b"="
            )
            records.append((entry.filename, f"sha256={digest.decode()}", len(data)))
            wheel.writestr(entry, data)
        if replaced != replacements.keys():
            raise RuntimeError(
                f"Missing wheel executables: {replacements.keys() - replaced}"
            )
        record = io.StringIO(newline="")
        writer = csv.writer(record, lineterminator="\n")
        writer.writerows(records)
        writer.writerow((record_entry.filename, "", ""))
        wheel.writestr(record_entry, record.getvalue())


def symbolize(binary, symbols, address, tools, system):
    if system == "Darwin":
        # atos reads the packed DWARF directly; UUID equality is verified separately.
        if symbols.exists():
            (source,) = (symbols / "Contents" / "Resources" / "DWARF").iterdir()
        else:
            source = binary
        output = run(["atos", "-o", source, hex(address)])
        return output, bool(re.search(r"\([^)]*:[1-9][0-9]*\)", output))
    arguments = [
        tools["llvm-symbolizer"],
        "--output-style=JSON",
        "--no-inlines",
        f"--obj={binary}",
    ]
    if system == "Windows":
        arguments.append("--relative-address")
    missing_pdb = system == "Windows" and not symbols.exists()
    output = run(
        [*arguments, hex(address)],
        allowed_exit_codes=(0, 1) if missing_pdb else (0,),
        # Symbolizer diagnostics must not be parsed as part of its JSON response.
        merge_stderr=False,
    )
    try:
        decoded = json.loads(output)
    except json.JSONDecodeError as error:
        raise RuntimeError(f"Invalid symbolizer JSON output:\n{output}") from error
    # LLVM reports a missing PDB as an error instead of returning an unknown frame.
    if missing_pdb and isinstance(decoded, dict):
        expected = f"'{symbols.name}': no such file or directory"
        if decoded.get("Error", {}).get("Message") == expected:
            return output, False
        raise RuntimeError(f"Unexpected symbolizer error:\n{output}")
    (result,) = decoded
    frames = result["Symbol"]
    return output, any(frame.get("Line", 0) > 0 for frame in frames)


def separate_symbols(binary, built, tools, system):
    if system == "Linux":
        symbols = binary.with_name(binary.name + ".debug")
        run([tools["llvm-objcopy"], "--only-keep-debug", binary, symbols])
        # GNU semantics retain non-debug metadata, including cargo-auditable's SBOM.
        run([tools["llvm-strip"], "--strip-all-gnu", binary])
        run([tools["llvm-objcopy"], f"--add-gnu-debuglink={symbols}", binary])
        identities = [
            run([tools["llvm-readobj"], "--notes", path]) for path in (binary, symbols)
        ]
        build_ids = [
            re.findall(r"Build ID: ([a-fA-F0-9]+)", identity) for identity in identities
        ]
        if not build_ids[0] or build_ids[0] != build_ids[1]:
            raise RuntimeError(f"ELF build IDs differ: {build_ids}")
        identity = build_ids[0]
    elif system == "Darwin":
        symbols = binary.with_suffix(".dSYM")
        shutil.copytree(built.with_suffix(".dSYM"), symbols)
        run(["strip", "-S", "-x", binary])
        run(["codesign", "--force", "--sign", "-", binary])
        identities = [
            run([tools["llvm-dwarfdump"], "--uuid", path]) for path in (binary, symbols)
        ]
        identifiers = [
            re.findall(r"UUID: ([A-Fa-f0-9-]+)", identity) for identity in identities
        ]
        if not identifiers[0] or identifiers[0] != identifiers[1]:
            raise RuntimeError(f"Mach-O UUIDs differ: {identifiers}")
        identity = identifiers[0]
    else:
        symbols = binary.with_suffix(".pdb")
        shutil.copyfile(built.with_suffix(".pdb"), symbols)
        identity = run([tools["llvm-readobj"], "--coff-debug-directory", binary])
    return symbols, identity


def verify_symbols(binary, symbols, function_addresses, functions, tools, system):
    lookups = {}
    for name, address in function_addresses.items():
        lookup, resolved = symbolize(binary, symbols, address, tools, system)
        if not resolved or functions[name] not in lookup:
            raise RuntimeError(f"Cannot resolve {name} to {functions[name]}:\n{lookup}")
        lookups[name] = lookup
    hidden = symbols.with_name(symbols.name + ".hidden")
    symbols.rename(hidden)
    negative_lookups = {}
    try:
        for name, address in function_addresses.items():
            lookup, resolved = symbolize(binary, symbols, address, tools, system)
            if resolved:
                raise RuntimeError(
                    f"{name} still resolves without companion symbols:\n{lookup}"
                )
            negative_lookups[name] = lookup
    finally:
        hidden.rename(symbols)

    return lookups, negative_lookups


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--output",
        type=Path,
        default=FIXTURE.parents[1] / "target" / "debug-symbols-experiment",
    )
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    system = platform.system()
    if system not in {"Linux", "Darwin", "Windows"}:
        raise RuntimeError(f"Unsupported experiment platform: {system}")
    compiler_version = run(["rustc", "-vV"])
    host = next(
        line.removeprefix("host: ")
        for line in compiler_version.splitlines()
        if line.startswith("host: ")
    )
    sysroot = Path(run(["rustc", "--print", "sysroot"]).strip())
    required = {"llvm-readobj"} if system == "Windows" else {"llvm-nm"}
    required |= {"llvm-dwarfdump"} if system == "Darwin" else {"llvm-symbolizer"}
    if system == "Linux":
        required |= {"llvm-objcopy", "llvm-strip", "llvm-readobj"}
    tools = {name: tool(name, sysroot, host) for name in required}
    report = {
        "platform": system,
        "target": host,
        "rustc": compiler_version,
        "maturin": run(["maturin", "--version"]).strip(),
        "tools": tools,
        "builds": {},
    }
    binary_name = "symbol_fixture.exe" if system == "Windows" else "symbol_fixture"

    # Delete all build outputs before lookups so debuggers cannot find original symbols.
    with tempfile.TemporaryDirectory(prefix="uv-debug-symbols-") as temporary:
        for mode in ("baseline", "symbols"):
            directory = output / mode
            directory.mkdir()
            target = Path(temporary) / mode
            environment = os.environ.copy()
            environment.update(
                {
                    "CARGO_TARGET_DIR": str(target),
                    "CARGO_PROFILE_RELEASE_DEBUG": "full"
                    if mode == "symbols"
                    else "none",
                    "CARGO_PROFILE_RELEASE_STRIP": "none"
                    if mode == "symbols"
                    else "symbols",
                    "CARGO_PROFILE_RELEASE_SPLIT_DEBUGINFO": "off"
                    if system == "Linux"
                    else "packed",
                    "CC_ENABLE_DEBUG_OUTPUT": "1",
                }
            )
            started = time.monotonic()
            run(
                [
                    "maturin",
                    "build",
                    "--release",
                    "--locked",
                    "--target",
                    host,
                    "--strip=false" if mode == "symbols" else "--strip=true",
                    "--out",
                    directory,
                    "--verbose",
                ],
                env=environment,
                log=directory / "build.log",
            )
            elapsed = time.monotonic() - started
            save_build_script_logs(target, directory)
            (wheel,) = directory.glob("*.whl")
            binary = directory / binary_name
            with ZipFile(wheel) as archive:
                member = next(
                    name
                    for name in archive.namelist()
                    if name.endswith(f".data/scripts/{binary_name}")
                )
                binary.write_bytes(archive.read(member))
            binary.chmod(0o755)
            report["builds"][mode] = {
                "seconds": elapsed,
                "executable_bytes": binary.stat().st_size,
                "wheel_bytes": wheel.stat().st_size,
            }
            if run([binary]).strip() != "59":
                raise RuntimeError(f"Unexpected {mode} fixture output")
            if mode == "baseline":
                if system == "Darwin":
                    # Compare both executables after the same signing operation.
                    run(["codesign", "--force", "--sign", "-", binary])
                original = wheel.with_suffix(".original.whl")
                wheel.rename(original)
                repack_wheel(original, wheel, [binary])
                report["builds"][mode].update(
                    {
                        "stripped_executable_bytes": binary.stat().st_size,
                        "stripped_wheel_bytes": wheel.stat().st_size,
                    }
                )
                continue
            function_addresses = addresses(binary, tools, system)
            built = target / host / "release" / binary_name
            if binary.read_bytes() != built.read_bytes():
                raise RuntimeError("Maturin changed the linked fixture executable")
            symbols, report["identity"] = separate_symbols(binary, built, tools, system)
            unprocessed_wheel = wheel.with_suffix(".original.whl")
            wheel.rename(unprocessed_wheel)
            repack_wheel(unprocessed_wheel, wheel, [binary])
            report["builds"][mode].update(
                {
                    "stripped_executable_bytes": binary.stat().st_size,
                    "stripped_wheel_bytes": wheel.stat().st_size,
                    "symbol_bytes": sum(
                        path.stat().st_size
                        for path in symbols.rglob("*")
                        if path.is_file()
                    )
                    if symbols.is_dir()
                    else symbols.stat().st_size,
                }
            )

    lookups, negative_lookups = verify_symbols(
        binary, symbols, function_addresses, FUNCTIONS, tools, system
    )

    with tempfile.TemporaryDirectory(prefix="uv-symbol-wheel-") as temporary:
        environment = Path(temporary) / "venv"
        run([sys.executable, "-m", "venv", "--without-pip", environment])
        executable_directory = environment / (
            "Scripts" if system == "Windows" else "bin"
        )
        python = executable_directory / (
            "python.exe" if system == "Windows" else "python"
        )
        run(["uv", "pip", "install", "--python", python, "--no-index", wheel])
        installed = executable_directory / binary_name
        if (
            installed.read_bytes() != binary.read_bytes()
            or run([installed]).strip() != "59"
        ):
            raise RuntimeError(
                "Installed wheel does not contain the verified executable"
            )
    report["lookups"] = lookups
    report["lookups_without_symbols"] = negative_lookups
    report["negative_control"] = (
        "Neither Rust nor C source lines resolve without companion symbols"
    )
    baseline = report["builds"]["baseline"]
    measured = report["builds"]["symbols"]
    report["executable_size_delta_bytes"] = (
        measured["stripped_executable_bytes"] - baseline["stripped_executable_bytes"]
    )
    report["wheel_size_delta_bytes"] = (
        measured["stripped_wheel_bytes"] - baseline["stripped_wheel_bytes"]
    )
    (output / "report.json").write_text(
        json.dumps(report, indent=2) + "\n", encoding="utf-8"
    )
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
