#!/usr/bin/env python3
"""Rebuild embedded Python shims, independently of normal uv builds."""

from __future__ import annotations

import argparse
import hashlib
import os
import re
import shlex
import shutil
import subprocess
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CRATES = ("uv-python-shim", "uv-windows")
TARGETS = (
    "aarch64-apple-darwin",
    "x86_64-apple-darwin",
    "aarch64-unknown-linux-musl",
    "x86_64-unknown-linux-musl",
    "i686-unknown-linux-musl",
    "arm-unknown-linux-musleabihf",
    "powerpc64le-unknown-linux-musl",
    "riscv64gc-unknown-linux-musl",
    "s390x-unknown-linux-gnu",
    "x86_64-unknown-freebsd",
    "aarch64-pc-windows-msvc",
    "x86_64-pc-windows-msvc",
    "i686-pc-windows-msvc",
)


def run(*args: str, cwd: Path, env: dict[str, str] | None = None) -> None:
    subprocess.run(args, cwd=cwd, env=env, check=True)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    selection = parser.add_mutually_exclusive_group(required=True)
    selection.add_argument("--target", action="append", choices=TARGETS)
    selection.add_argument("--group", choices=("macos", "cross"))
    parser.add_argument(
        "--check", action="store_true", help="Compare without changing committed assets"
    )
    parser.add_argument("--work-dir", type=Path, required=True)
    parser.add_argument("--freebsd-sysroot", type=Path)
    parser.add_argument(
        "--output-dir",
        type=Path,
        default=ROOT / "crates/uv-trampoline-builder/python-shims",
    )
    args = parser.parse_args()
    args.output_dir = args.output_dir.resolve()
    work_dir = args.work_dir.resolve()
    work_dir.mkdir(parents=True, exist_ok=True)
    if any(work_dir.iterdir()):
        parser.error("--work-dir must be empty so checks cannot reuse build artifacts")
    targets = args.target or [
        target
        for target in TARGETS
        if ("apple-darwin" in target) == (args.group == "macos")
    ]
    source = work_dir / "source"
    source.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(ROOT / "Cargo.toml", source / "Cargo.toml")
    for crate in CRATES:
        shutil.copytree(
            ROOT / "crates" / crate,
            source / "crates" / crate,
            dirs_exist_ok=True,
            ignore=shutil.ignore_patterns("target"),
        )

    # As with the Windows trampolines, uv release versions must not change the
    # launcher's crate hashes. Keep registry dependency versions locked.
    for manifest in source.rglob("Cargo.toml"):
        text = re.sub(
            r'^version = "[^"]+"',
            'version = "0.0.0"',
            manifest.read_text(),
            flags=re.MULTILINE,
        )
        text = re.sub(r'version = "[^"]+"(, path = ")', r'version = "0.0.0"\1', text)
        manifest.write_text(text)
    crate_dir = source / "crates/uv-python-shim"
    lockfile = crate_dir / "Cargo.lock"
    text = lockfile.read_text()
    for crate in CRATES:
        text = re.sub(rf'(name = "{crate}"\nversion = ")[^"]+', r"\g<1>0.0.0", text)
    lockfile.write_text(text)
    toolchain = tomllib.loads((crate_dir / "rust-toolchain.toml").read_text())[
        "toolchain"
    ]["channel"]
    rustc = subprocess.check_output(
        ["rustup", "which", "--toolchain", toolchain, "rustc"], text=True
    ).strip()
    compiler = dict(
        line.split(": ", 1)
        for line in subprocess.check_output([rustc, "-vV"], text=True).splitlines()
        if ": " in line
    )
    host = compiler["host"]
    rust_lld = Path(rustc).parent.parent / "lib/rustlib" / host / "bin/rust-lld"
    # Ignore ambient compiler/profile settings; caches and registry configuration
    # can still be inherited without changing the build contract.
    build_env = {
        key: value
        for key, value in os.environ.items()
        if not key.startswith(("CARGO_PROFILE_", "CARGO_TARGET_", "CARGO_BUILD_"))
        and key
        not in {
            "RUSTFLAGS",
            "CARGO_ENCODED_RUSTFLAGS",
            "RUSTC",
            "RUSTC_WRAPPER",
            "RUSTC_WORKSPACE_WRAPPER",
        }
    }
    temporary = work_dir / "tmp"
    temporary.mkdir()
    build_env.update(TMPDIR=str(temporary), CARGO_INCREMENTAL="0", ZERO_AR_DATE="1")
    build_outputs = work_dir / "assets"
    build_outputs.mkdir()
    windows_outputs = []
    for target in targets:
        env = build_env.copy()
        # Populate registry sources before enumerating path remaps, including
        # when CI starts with an empty Cargo home.
        run(
            "cargo",
            f"+{toolchain}",
            "fetch",
            "--locked",
            "--target",
            target,
            cwd=crate_dir,
            env=env,
        )
        flags = [
            f"--remap-path-prefix={source}=/uv",
            f"--remap-path-prefix={Path(rustc).parent.parent}/lib/rustlib/src/rust=/rustc/{compiler['commit-hash']}",
        ]
        registry = Path(env.get("CARGO_HOME", Path.home() / ".cargo")) / "registry/src"
        if registry.is_dir():
            flags.extend(
                f"--remap-path-prefix={path}=/cargo/registry/src"
                for path in sorted(registry.iterdir())
                if path.is_dir()
            )
        env["CARGO_TARGET_DIR"] = str(work_dir / "target")
        linker_key = f"CARGO_TARGET_{target.upper().replace('-', '_')}_LINKER"
        cargo = ["cargo", f"+{toolchain}"]
        if target.endswith("windows-msvc"):
            cargo.extend(
                [
                    "xwin",
                    "build",
                    "--xwin-sdk-version",
                    "10.0.22621",
                    "--xwin-crt-version",
                    "14.44.17.14",
                ]
            )
            if shutil.which("mt.exe") is None:
                manifest_tool = shutil.which("llvm-mt")
                if manifest_tool is None:
                    parser.error("Windows builds require mt.exe or llvm-mt on PATH")
                tools_dir = work_dir / "tools"
                tools_dir.mkdir(exist_ok=True)
                (tools_dir / "mt.exe").write_text(
                    f'#!/bin/sh\nexec {shlex.quote(manifest_tool)} "$@"\n'
                )
                (tools_dir / "mt.exe").chmod(0o755)
                env["PATH"] = str(tools_dir) + os.pathsep + env["PATH"]
            if target.startswith("i686-"):
                cargo.extend(["--xwin-arch", "x86"])
            flags.extend(
                [
                    "-Ctarget-feature=+crt-static",
                    "-Clink-arg=/Brepro",
                    "-Clink-arg=/DEBUG:NONE",
                ]
            )
        else:
            cargo.append("build")
            if "linux-musl" in target:
                env[linker_key] = str(rust_lld)
                flags.extend(
                    ["-Ctarget-feature=+crt-static", "-Clink-self-contained=yes"]
                )
            elif target in ("s390x-unknown-linux-gnu", "x86_64-unknown-freebsd"):
                if target.endswith("freebsd"):
                    clang = shutil.which("clang")
                    if clang is None or args.freebsd_sysroot is None:
                        parser.error(
                            "FreeBSD builds require clang and --freebsd-sysroot"
                        )
                    sysroot = args.freebsd_sysroot.resolve()
                    linker_args = [
                        clang,
                        f"--target={target}",
                        f"--sysroot={sysroot}",
                        f"-fuse-ld={rust_lld.parent}/gcc-ld/ld.lld",
                        f"-B{sysroot}/usr/lib",
                        f"-L{sysroot}/lib",
                        f"-L{sysroot}/usr/lib",
                    ]
                else:
                    zig = shutil.which("zig")
                    if zig is None:
                        parser.error(f"Zig is required to link {target}")
                    linker_args = [zig, "cc", "-target", "s390x-linux-gnu.2.17"]
                linker = work_dir / f"link-{target}"
                linker.write_text(f'#!/bin/sh\nexec {shlex.join(linker_args)} "$@"\n')
                linker.chmod(0o755)
                env[linker_key] = str(linker)
            elif "apple-darwin" in target:
                sdk_version = subprocess.check_output(
                    ["xcrun", "--sdk", "macosx26.5", "--show-sdk-version"], text=True
                ).strip()
                if sdk_version != "26.5":
                    parser.error(f"Expected macOS SDK 26.5, found {sdk_version}")
                env[linker_key] = subprocess.check_output(
                    ["xcrun", "--sdk", "macosx26.5", "--find", "clang"], text=True
                ).strip()
                env["SDKROOT"] = subprocess.check_output(
                    ["xcrun", "--sdk", "macosx26.5", "--show-sdk-path"], text=True
                ).strip()
                # LLD hashes the output basename into LC_UUID. Cargo's basename
                # contains a path-dependent hash, so give LLD a stable final name.
                flags.extend(
                    [
                        f"-Clink-arg=-fuse-ld={rust_lld.parent}/gcc-ld/ld64.lld",
                        "-Clink-arg=-Wl,-S",
                        "-Clink-arg=-Wl,-final_output,uv-python",
                    ]
                )
                env["MACOSX_DEPLOYMENT_TARGET"] = (
                    "11.0" if target.startswith("aarch64-") else "10.12"
                )
        env["CARGO_ENCODED_RUSTFLAGS"] = "\x1f".join(flags)
        run(
            *cargo,
            "--locked",
            "--profile",
            "shim",
            "--target",
            target,
            cwd=crate_dir,
            env=env,
        )
        suffix = ".exe" if target.endswith("windows-msvc") else ""
        output = build_outputs / f"uv-python-{target}{suffix}"
        shutil.copyfile(
            work_dir / "target" / target / "shim" / f"uv-python{suffix}", output
        )
        if suffix:
            windows_outputs.append(str(output))
        elif "linux" in target or target.endswith("freebsd"):
            # LLD's version string includes a build-host-specific source URL.
            # This section is not loaded and has no effect on the launcher.
            run(
                str(rust_lld.parent / "llvm-objcopy"),
                "--remove-section=.comment",
                str(output),
                cwd=ROOT,
            )
    if windows_outputs:
        build_env["CARGO_TARGET_DIR"] = str(work_dir / "normalize-target")
        run(
            "cargo",
            f"+{toolchain}",
            "run",
            "--locked",
            "--profile",
            "no-debug",
            "-p",
            "uv-trampoline-builder",
            "--bin",
            "normalize-pe-timestamps",
            "--",
            *windows_outputs,
            cwd=ROOT,
            env=build_env,
        )
    if args.check:
        failures = []
        # Catch added/removed assets as well as stale contents. Each CI group
        # checks its own bytes, and together the groups cover the complete catalog.
        expected = {
            f"uv-python-{target}{'.exe' if target.endswith('windows-msvc') else ''}"
            for target in TARGETS
        }
        actual = (
            {path.name for path in args.output_dir.iterdir()}
            if args.output_dir.is_dir()
            else set()
        )
        if actual != expected:
            failures.append(
                f"Asset catalog differs: missing={sorted(expected - actual)}, extra={sorted(actual - expected)}"
            )
        for output in sorted(build_outputs.iterdir()):
            committed = args.output_dir / output.name
            if not committed.is_file() or committed.read_bytes() != output.read_bytes():
                failures.append(
                    f"{output.name} differs from its clean build ({hashlib.sha256(output.read_bytes()).hexdigest()})"
                )
        if failures:
            parser.exit(1, "\n".join(failures) + "\n")
        print(f"All {len(targets)} selected shim assets match their clean builds.")
    else:
        args.output_dir.mkdir(parents=True, exist_ok=True)
        for output in build_outputs.iterdir():
            shutil.copyfile(output, args.output_dir / output.name)


if __name__ == "__main__":
    main()
