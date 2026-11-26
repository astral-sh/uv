#!/usr/bin/env python3
# /// script
# requires-python = ">=3.11"
# dependencies = []
# [tool.uv]
# no-build = true
# exclude-newer = "P7D"
# ///

"""Order the final uv link by first use during startup.

Use `python scripts/build_uv_pgo.py --order-startup` on native x86-64 Linux
with glibc, Rust's bundled LLD, cc and qemu-x86_64 (from qemu-user).
The linker runs while rustc's temporary objects are still available, so the
second link reuses the same PGO and fat LTO output.
"""

import os
import subprocess
import sys
import tempfile
from itertools import pairwise
from pathlib import Path


def train(binary: Path, directory: Path) -> list[str]:
    # Keep discovery, interpreter selection and the cache local to the fixture.
    environment = {
        "HOME": str(directory),
        "PATH": os.environ["PATH"],
        "UV_OFFLINE": "1",
        "UV_PYTHON": sys.executable,
        "UV_PYTHON_DOWNLOADS": "never",
    }
    if prefix := os.environ.get("QEMU_LD_PREFIX"):
        environment["QEMU_LD_PREFIX"] = prefix
    (directory / "pyproject.toml").write_text(
        '[project]\nname = "startup"\nversion = "1.0"\nrequires-python = ">=3.11"\n'
    )
    subprocess.run(
        [binary, "sync", "--quiet"],
        cwd=directory,
        env=environment,
        check=True,
        timeout=60,
    )

    trace = directory / "trace"
    names = {}
    for args in (["--version"], ["run", "--locked", "--", "/usr/bin/true"]):
        before = len(names)
        subprocess.run(
            ["qemu-x86_64", "-d", "in_asm", "-D", trace, binary, *args],
            cwd=directory,
            env=environment,
            stdout=subprocess.DEVNULL,
            check=True,
            timeout=120,
        )
        # QEMU labels translated blocks with the ELF symbol in the main
        # executable. Ignore unlabeled blocks, including shared libraries.
        with trace.open() as lines:
            for line in lines:
                if line.startswith("IN: ") and (name := line[4:].strip()):
                    names[name] = None
        if len(names) == before:
            raise RuntimeError(f"QEMU found no new symbols for uv {' '.join(args)}")
    return list(names)


def check_layout(names: list[str], link_map: Path) -> None:
    # LLD can finish successfully even if it ignored the requested order.
    # Read its map so the release binary can keep its normal stripping flags.
    wanted = set(names)
    addresses = {}
    with link_map.open() as lines:
        for line in lines:
            fields = line.split(maxsplit=4)
            if len(fields) == 5 and (name := fields[4].strip()) in wanted:
                addresses.setdefault(name, int(fields[0], 16))
    matched = [addresses[name] for name in names if name in addresses]
    increasing = sum(a < b for a, b in pairwise(matched))
    # Local aliases and functions sharing an input section can tie or move.
    if len(matched) < 0.8 * len(names) or increasing < 0.9 * (len(matched) - 1):
        raise RuntimeError(
            f"uv startup order did not take effect: {len(matched)}/{len(names)} "
            f"symbols matched, {increasing} adjacent pairs in order"
        )


def main() -> None:
    args = sys.argv[1:]
    if (
        os.environ.get("CARGO_BIN_NAME") != "uv"
        or os.environ.get("CARGO_PKG_NAME") != "uv"
    ):
        os.execvp("cc", ["cc", *args])

    with tempfile.TemporaryDirectory() as temporary:
        directory = Path(temporary)
        binary = directory / "uv"
        # Only the training link needs a symbol table.
        training_args = args.copy()
        training_args[training_args.index("-o") + 1] = str(binary)
        training_args = [arg for arg in training_args if arg != "-Wl,--strip-all"]
        subprocess.run(["cc", *training_args], check=True)

        names = train(binary, directory)
        order = directory / "uv.order"
        order.write_text("\n".join(names) + "\n")
        link_map = directory / "uv.map"
        subprocess.run(
            [
                "cc",
                *args,
                "-Xlinker",
                f"--symbol-ordering-file={order}",
                "-Wl,--no-warn-symbol-ordering,--no-demangle",
                "-Xlinker",
                f"--Map={link_map}",
            ],
            check=True,
        )
        check_layout(names, link_map)


if __name__ == "__main__":
    main()
