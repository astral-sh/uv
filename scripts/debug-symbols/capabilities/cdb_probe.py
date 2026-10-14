"""Observe retained Windows fixtures with the Windows debugger engine."""

import json
import os
import re
import subprocess
from pathlib import Path

VARIABLES = {
    "seed": "seed",
    "live_scalar": "live_scalar",
    "constant": "constant",
    "record_count": "record.count",
    "record_tag": "record.tag",
    "shadowed": "shadowed",
    "temporary": "temporary",
    "inline_value": "inline_value",
    "bucket": "bucket",
    "lower_byte": "lower_byte",
    "unused_value": "unused_value",
    "consumed": "consumed",
    "partial_count": "partial.count",
    "partial_tag": "partial.tag",
    "native_local": "native_local",
}


def observe(binary, destination):
    debugger = Path(os.environ["CDB"])
    checkpoint_file = destination / "checkpoint.commands"
    commands = ['.printf "CAP_BEGIN %I64u\\n", @rcx', "kn"]
    for index in range(6):
        commands.extend([f".echo CAP_FRAME {index}", f".frame /r {index}", "dv /t /v"])
        commands.extend(
            f'.printf "CAP_VALUE {name} %I64x\\n", @@c++({expression})'
            for name, expression in VARIABLES.items()
        )
        if index == 1:
            commands.extend(
                [
                    ".echo CAP_AGGREGATES",
                    "?? record",
                    "?? partial",
                    "?? choice",
                    "?? optional",
                    "?? text",
                    "?? items",
                ]
            )
    commands.extend([".echo CAP_END", "gc"])
    checkpoint_file.write_text("\n".join(commands) + "\n")
    initial_file = destination / "initial.commands"
    initial_file.write_text(
        "\n".join(
            [
                ".version",
                ".symopt+0x410",
                ".lines -e",
                f".sympath {binary.parent}",
                ".echo CAP_TYPES",
                "dt capability_fixture!capability_fixture::Record",
                "dt capability_fixture!capability_fixture::Choice",
                "dt capability_fixture!native_record",
                f'bp capability_fixture!capability_checkpoint "$$><{checkpoint_file}"',
                "g",
            ]
        )
        + "\n"
    )
    log = destination / "cdb.log"
    environment = {
        **os.environ,
        "_NT_SYMBOL_PATH": str(binary.parent),
        "_NT_ALT_SYMBOL_PATH": "",
    }
    with (destination / "cdb-console.log").open("w") as stream:
        result = subprocess.run(
            [
                str(debugger),
                "-G",
                "-logo",
                str(log),
                "-cf",
                str(initial_file),
                str(binary),
                "17",
            ],
            cwd=destination,
            env=environment,
            stdout=stream,
            stderr=subprocess.STDOUT,
            check=False,
            timeout=180,
        )
    text = log.read_text(errors="replace") if log.exists() else ""
    cases = {}
    current = None
    frame = None
    for line in text.splitlines():
        if match := re.fullmatch(r"CAP_BEGIN ([0-9]+)", line.strip()):
            current = {"frames": {}, "transcript": []}
            cases[int(match[1])] = current
            frame = None
        elif line.strip() == "CAP_END":
            current = None
        elif current is not None:
            current["transcript"].append(line)
            if match := re.fullmatch(r"CAP_FRAME ([0-9]+)", line.strip()):
                frame = {"values": {}}
                current["frames"][match[1]] = frame
            elif frame is not None and (
                match := re.fullmatch(r"CAP_VALUE (\w+) ([0-9a-fA-F`]+)", line.strip())
            ):
                frame["values"][match[1]] = int(match[2].replace("`", ""), 16)
    report = {
        "debugger": str(debugger),
        "exit_code": result.returncode,
        "cases": cases,
        "completed": set(cases) == set(range(1, 12)),
        "type_output": text.split("CAP_TYPES", 1)[-1].split("CAP_BEGIN", 1)[0],
    }
    live = cases.get(1, {}).get("frames", {})
    expected = {"seed": 17, "live_scalar": 58, "record_count": 58, "record_tag": 23}
    report["positive_control_values"] = any(
        all(frame["values"].get(name) == value for name, value in expected.items())
        for frame in live.values()
    )
    (destination / "cdb.json").write_text(json.dumps(report, indent=2) + "\n")
    if not report["completed"]:
        raise RuntimeError(f"CDB did not record eleven checkpoints; see {log}")
    return report
