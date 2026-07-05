#!/usr/bin/env python3
"""Generate the bytecode target metadata for the pinned CPython release."""

from __future__ import annotations

import argparse
import difflib
import hashlib
import re
import subprocess
import sys
from pathlib import Path


TARGET_TAG = "v3.14.5"
TARGET_COMMIT = "5607950ef232dad16d75c0cf53101d9649d89115"
INPUTS = (
    "Include/opcode_ids.h",
    "Include/internal/pycore_opcode_metadata.h",
    "Include/cpython/code.h",
    "Include/internal/pycore_code.h",
    "Include/internal/pycore_magic_number.h",
    "Include/patchlevel.h",
)
CRATE_ROOT = Path(__file__).resolve().parents[1]
OUTPUT = CRATE_ROOT / "src" / "target" / "cpython_3_14_5.rs"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--cpython",
        type=Path,
        required=True,
        help="path to a CPython checkout at the pinned commit",
    )
    parser.add_argument(
        "--check",
        action="store_true",
        help="fail instead of writing if the checked-in module is stale",
    )
    return parser.parse_args()


def require_match(pattern: str, source: str, description: str) -> re.Match[str]:
    match = re.search(pattern, source, re.MULTILINE)
    if match is None:
        raise ValueError(f"failed to find {description}")
    return match


def git_output(checkout: Path, *arguments: str) -> str:
    return subprocess.run(
        ["git", "-C", str(checkout), *arguments],
        check=True,
        stdout=subprocess.PIPE,
        text=True,
    ).stdout.strip()


def verify_checkout(checkout: Path) -> None:
    commit = git_output(checkout, "rev-parse", "HEAD^{commit}")
    if commit != TARGET_COMMIT:
        raise ValueError(
            f"CPython checkout is at {commit}, expected {TARGET_COMMIT} ({TARGET_TAG})"
        )
    try:
        tag_commit = git_output(checkout, "rev-parse", f"{TARGET_TAG}^{{commit}}")
    except subprocess.CalledProcessError:
        return
    if tag_commit != TARGET_COMMIT:
        raise ValueError(f"{TARGET_TAG} resolves to {tag_commit}, expected {TARGET_COMMIT}")


def read_inputs(checkout: Path) -> dict[str, str]:
    sources = {}
    for relative in INPUTS:
        path = checkout / relative
        try:
            sources[relative] = path.read_text()
        except OSError as error:
            raise ValueError(f"failed to read {path}: {error}") from error
    return sources


def parse_opcodes(source: str) -> list[tuple[str, int]]:
    section = source.split("/* Instruction opcodes for compiled code */", 1)[1]
    section = section.split("#define HAVE_ARGUMENT", 1)[0]
    opcodes = []
    ids = set()
    for name, value in re.findall(r"^#define\s+([A-Z][A-Z0-9_]*)\s+(\d+)\s*$", section, re.MULTILINE):
        code = int(value)
        if code > 128:
            continue
        if code in ids:
            raise ValueError(f"duplicate physical opcode ID {code}")
        ids.add(code)
        opcodes.append((name, code))
    if not opcodes or ("RESUME", 128) not in opcodes:
        raise ValueError("opcode table does not contain the expected RESUME instruction")
    return opcodes


def parse_caches(source: str) -> dict[str, int]:
    section = source.split("const uint8_t _PyOpcode_Caches[256] = {", 1)[1]
    section = section.split("};", 1)[0]
    return {
        name: int(value)
        for name, value in re.findall(
            r"^\s*\[([A-Z][A-Z0-9_]*)\]\s*=\s*(\d+),\s*$",
            section,
            re.MULTILINE,
        )
    }


def parse_stack_function(source: str, direction: str) -> dict[str, str]:
    start = f"int _PyOpcode_num_{direction}(int opcode, int oparg)  {{"
    section = source.split(start, 1)[1].split("#endif", 1)[0]
    effects = {
        name: " ".join(expression.split())
        for name, expression in re.findall(
            r"case\s+([A-Z][A-Z0-9_]*):\s*return\s+([^;]+);", section
        )
    }
    if not effects:
        raise ValueError(f"no {direction} stack effects found")
    return effects


def rust_stack_expression(expression: str) -> str:
    allowed = re.fullmatch(r"[0-9A-Fa-fxXoparg +\-*&>()]+", expression)
    if allowed is None:
        raise ValueError(f"unsupported stack-effect expression: {expression}")
    return re.sub(r"\boparg\b", "argument", expression).replace("0xFF", "0xff")


def format_rust(source: str) -> str:
    """Format generated Rust with the workspace toolchain before comparing it."""
    return subprocess.run(
        ["rustfmt", "--edition", "2024", "--emit", "stdout"],
        input=source,
        check=True,
        stdout=subprocess.PIPE,
        text=True,
    ).stdout


def parse_numeric_defines(source: str, prefix: str) -> list[tuple[str, int]]:
    definitions = []
    for name, hexadecimal, decimal in re.findall(
        rf"^#define\s+({prefix}[A-Z0-9_]*)\s+\(?(?:(0x[0-9A-Fa-f]+)|(\d+))\)?(?:\s|/|$)",
        source,
        re.MULTILINE,
    ):
        value = int(hexadecimal or decimal, 0)
        definitions.append((name, value))
    return definitions


def render(sources: dict[str, str]) -> str:
    opcode_ids = sources["Include/opcode_ids.h"]
    metadata = sources["Include/internal/pycore_opcode_metadata.h"]
    code_header = sources["Include/cpython/code.h"]
    local_header = sources["Include/internal/pycore_code.h"]
    magic_header = sources["Include/internal/pycore_magic_number.h"]
    patchlevel = sources["Include/patchlevel.h"]

    opcodes = parse_opcodes(opcode_ids)
    opcode_names = {name for name, _ in opcodes}
    caches = parse_caches(metadata)
    popped = parse_stack_function(metadata, "popped")
    pushed = parse_stack_function(metadata, "pushed")
    for name in opcode_names:
        if name not in popped or name not in pushed:
            raise ValueError(f"missing stack metadata for {name}")
    unknown_caches = set(caches) - {
        name
        for name, value in re.findall(
            r"^#define\s+([A-Z][A-Z0-9_]*)\s+(\d+)\s*$", opcode_ids, re.MULTILINE
        )
        if int(value) < 256
    }
    if unknown_caches:
        raise ValueError(f"cache metadata references unknown opcodes: {sorted(unknown_caches)}")

    version = tuple(
        int(require_match(rf"^#define\s+PY_{part}_VERSION\s+(\d+)$", patchlevel, part).group(1))
        for part in ("MAJOR", "MINOR", "MICRO")
    )
    if version != (3, 14, 5):
        raise ValueError(f"unexpected CPython version {version}")
    magic = int(
        require_match(r"^#define\s+PYC_MAGIC_NUMBER\s+(\d+)$", magic_header, "magic number").group(1)
    )
    magic_bytes = (magic & 0xff, magic >> 8, 0x0D, 0x0A)

    code_flags = [
        item
        for item in parse_numeric_defines(code_header, "CO_")
        if item[0] != "CO_MAXBLOCKS"
    ]
    local_kinds = parse_numeric_defines(local_header, "CO_FAST_")
    required_flags = {"CO_OPTIMIZED", "CO_METHOD", "CO_FUTURE_ANNOTATIONS"}
    if not required_flags.issubset({name for name, _ in code_flags}):
        raise ValueError("code-flag table is incomplete")
    required_kinds = {"CO_FAST_ARG_POS", "CO_FAST_LOCAL", "CO_FAST_FREE"}
    if not required_kinds.issubset({name for name, _ in local_kinds}):
        raise ValueError("local-kind table is incomplete")

    lines = [
        "// @generated by scripts/generate_cpython_target.py; do not edit.",
        f"// CPython {TARGET_TAG} ({TARGET_COMMIT}).",
    ]
    for relative in INPUTS:
        digest = hashlib.sha256(sources[relative].encode()).hexdigest()
        lines.append(f"// {relative}: sha256:{digest}")
    lines.extend(
        [
            "",
            "use super::Opcode;",
            "",
            f'pub(crate) const TARGET_TAG: &str = "{TARGET_TAG}";',
            f'pub(crate) const TARGET_COMMIT: &str = "{TARGET_COMMIT}";',
            'pub(crate) const TARGET_IMPLEMENTATION: &str = "cpython";',
            f"pub(crate) const TARGET_VERSION: (u8, u8, u8) = ({version[0]}, {version[1]}, {version[2]});",
            "pub(crate) const TARGET_MAGIC_NUMBER: [u8; 4] = "
            f"[0x{magic_bytes[0]:02x}, 0x{magic_bytes[1]:02x}, 0x{magic_bytes[2]:02x}, 0x{magic_bytes[3]:02x}];",
            "",
            "pub(crate) mod opcodes {",
            "    use super::Opcode;",
            "",
        ]
    )
    for name, code in opcodes:
        lines.append(
            f"    pub(crate) const {name}: Opcode = Opcode::new({code}, {caches.get(name, 0)});"
        )
    lines.extend(["}", "", "#[allow(dead_code)]", "pub(crate) mod code_flags {"])
    for name, value in code_flags:
        lines.append(f"    pub(crate) const {name}: u32 = 0x{value:08x};")
    lines.extend(["}", "", "pub(crate) mod local_kinds {"])
    for name, value in local_kinds:
        lines.append(f"    pub(crate) const {name}: u8 = 0x{value:02x};")
    lines.extend(["}", ""])

    for direction, effects in (("popped", popped), ("pushed", pushed)):
        lines.extend(
            [
                f"pub(crate) fn num_{direction}(opcode: Opcode, argument: u32) -> usize {{",
                "    let argument = i64::from(argument);",
                "    let value = match opcode {",
            ]
        )
        for name, _ in opcodes:
            expression = rust_stack_expression(effects[name])
            lines.append(f"        opcodes::{name} => {expression},")
        lines.extend(
            [
                '        _ => unreachable!("missing generated stack metadata for opcode {}", opcode.code()),',
                "    };",
                f'    usize::try_from(value).expect("negative generated stack-{direction} count")',
                "}",
                "",
            ]
        )
    return "\n".join(lines) + "\n"


def check_numeric_authority() -> None:
    paths = [CRATE_ROOT / "src" / "compiler.rs"]
    paths.extend(sorted((CRATE_ROOT / "src" / "assembler").rglob("*.rs")))
    for path in paths:
        relative = path.relative_to(CRATE_ROOT)
        source = path.read_text()
        if re.search(r"Opcode::new\s*\(", source):
            raise ValueError(f"{relative} constructs Opcode outside the target module")
        for name in parse_opcodes_from_generated_names():
            pattern = rf"^\s*const\s+{re.escape(name)}\s*:\s*u8\s*=\s*\d+"
            if re.search(pattern, source, re.MULTILINE):
                raise ValueError(f"{relative} duplicates numeric opcode {name}")


def parse_opcodes_from_generated_names() -> set[str]:
    if not OUTPUT.exists():
        return set()
    return set(
        re.findall(r"^\s*pub\(crate\) const ([A-Z][A-Z0-9_]*): Opcode", OUTPUT.read_text(), re.MULTILINE)
    )


def main() -> int:
    options = parse_args()
    try:
        verify_checkout(options.cpython)
        generated = format_rust(render(read_inputs(options.cpython)))
        if options.check:
            current = OUTPUT.read_text() if OUTPUT.exists() else ""
            if current != generated:
                sys.stderr.writelines(
                    difflib.unified_diff(
                        current.splitlines(keepends=True),
                        generated.splitlines(keepends=True),
                        fromfile=str(OUTPUT),
                        tofile=f"{OUTPUT} (generated)",
                    )
                )
                return 1
            check_numeric_authority()
            return 0
        OUTPUT.parent.mkdir(parents=True, exist_ok=True)
        OUTPUT.write_text(generated)
        return 0
    except (OSError, subprocess.CalledProcessError, ValueError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
