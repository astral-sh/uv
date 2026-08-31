#!/usr/bin/env python3
"""Rebuild embedded Python shims, independently of normal uv builds."""

from __future__ import annotations

import argparse
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
    parser.add_argument("--target", action="append", choices=TARGETS, required=True)
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
    args.output_dir.mkdir(parents=True, exist_ok=True)
    windows_outputs = []
    for target in args.target:
        env = os.environ.copy()
        env.pop("CARGO_ENCODED_RUSTFLAGS", None)
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
        output = args.output_dir / f"uv-python-{target}{suffix}"
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
        run(
            "cargo",
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
        )


if __name__ == "__main__":
    main()
