"""Run inside LLDB to record capabilities without evaluating target expressions."""

import json
import shlex
import traceback
from pathlib import Path

import lldb

SCENARIOS = (
    ("live_values", "live_values"),
    ("optimized_values", "optimized_values"),
    ("shadow_inner", "shadowed_values"),
    ("shadow_outer", "shadowed_values"),
    ("inline_chain", "inline_leaf"),
    ("split_record", "split_record"),
    ("before_last_use", "last_use"),
    ("after_last_use", "last_use"),
    ("native_values", "capability_native"),
    ("line_probe", "line_probe"),
    ("tail_call", "tail_leaf"),
)


def command(debugger, text):
    result = lldb.SBCommandReturnObject()
    debugger.GetCommandInterpreter().HandleCommand(text, result)
    return {
        "command": text,
        "success": result.Succeeded(),
        "output": result.GetOutput(),
        "error": result.GetError(),
    }


def value_record(value, depth=0):
    formatted = value
    value = value.GetNonSyntheticValue()
    error = lldb.SBError()
    unsigned = value.GetValueAsUnsigned(error)
    record = {
        "name": value.GetName(),
        "type": value.GetTypeName(),
        "valid": value.IsValid(),
        "in_scope": value.IsInScope(),
        "value": value.GetValue(),
        "summary": value.GetSummary(),
        "error": value.GetError().GetCString(),
        "location": value.GetLocation(),
        "bytes": value.GetByteSize(),
        "unsigned": unsigned if error.Success() else None,
    }
    if depth < 4 and value.IsValid():
        record["children"] = [
            value_record(value.GetChildAtIndex(index), depth + 1)
            for index in range(min(value.GetNumChildren(), 16))
        ]
    if depth < 2 and formatted.IsSynthetic():
        record["formatted_children"] = [
            value_record(formatted.GetChildAtIndex(index), depth + 1)
            for index in range(min(formatted.GetNumChildren(), 16))
        ]
    return record


def frame_record(frame):
    line = frame.GetLineEntry()
    return {
        "function": frame.GetFunctionName(),
        "inline": frame.IsInlined(),
        "artificial": frame.IsArtificial(),
        "file": str(line.GetFileSpec()) if line.IsValid() else None,
        "line": line.GetLine() if line.IsValid() else None,
        "pc": frame.GetPC(),
        "variables": [
            value_record(value) for value in frame.GetVariables(True, True, False, True)
        ],
    }


def snapshot(process):
    thread = process.GetSelectedThread()
    return {
        "state": process.GetState(),
        "stop_reason": thread.GetStopReason(),
        "frames": [
            frame_record(thread.GetFrameAtIndex(index))
            for index in range(min(thread.GetNumFrames(), 14))
        ],
    }


def create_target(debugger, configuration, symbols=True):
    target = debugger.CreateTarget(configuration["binary"])
    if not target.IsValid():
        raise RuntimeError("LLDB could not create the fixture target")
    loaded = None
    if symbols and configuration.get("symbols"):
        loaded = command(
            debugger, "target symbols add " + shlex.quote(configuration["symbols"])
        )
        if not loaded["success"]:
            raise RuntimeError("Cannot load companion symbols: " + str(loaded))
    return target, loaded


def launch(target, configuration):
    information = lldb.SBLaunchInfo(["17"])
    information.SetWorkingDirectory(str(Path(configuration["binary"]).parent))
    information.SetLaunchFlags(
        information.GetLaunchFlags() & ~lldb.eLaunchFlagDisableASLR
    )
    error = lldb.SBError()
    process = target.Launch(information, error)
    if error.Fail() or not process.IsValid():
        raise RuntimeError("Debugger launch failed: " + str(error))
    return process


def observe(debugger, configuration):
    debugger.SetAsync(False)
    settings = [
        command(debugger, "settings set target.disable-aslr false"),
        command(debugger, "settings set symbols.enable-external-lookup false"),
    ]
    if not all(item["success"] for item in settings):
        raise RuntimeError("Cannot establish isolated debugger settings")
    formatters = [
        command(
            debugger,
            "command script import " + shlex.quote(configuration["formatter_script"]),
        ),
        command(
            debugger,
            "command source " + shlex.quote(configuration["formatter_commands"]),
        ),
    ]
    if not all(item["success"] for item in formatters):
        raise RuntimeError("Rust debugger formatters could not be loaded")
    target, loaded = create_target(debugger, configuration)
    breakpoint = target.BreakpointCreateByName("capability_checkpoint")
    if breakpoint.GetNumLocations() != 1:
        raise RuntimeError(
            f"Expected one exported checkpoint, found {breakpoint.GetNumLocations()}"
        )
    observations = []
    process = launch(target, configuration)
    try:
        for scenario, function in SCENARIOS:
            if process.GetState() != lldb.eStateStopped:
                raise RuntimeError(
                    f"Fixture did not stop at {scenario}: state={process.GetState()}"
                )
            item = snapshot(process)
            if item["stop_reason"] != lldb.eStopReasonBreakpoint:
                raise RuntimeError(f"Unexpected stop at {scenario}: {item}")
            item["scenario"] = scenario
            item["expected_function"] = function
            item["matching_frames"] = [
                frame
                for frame in item["frames"]
                if function in (frame["function"] or "")
            ]
            observations.append(item)
            process.Continue()
        if process.GetState() != lldb.eStateExited or process.GetExitStatus() != 0:
            raise RuntimeError(
                "Fixture did not exit successfully after all checkpoints"
            )
        stdout = process.GetSTDOUT(65536)
        stderr = process.GetSTDERR(65536)
    finally:
        if process.IsValid() and process.GetState() != lldb.eStateExited:
            process.Kill()
    types = {}
    for name in (
        "capability_fixture::Record",
        "capability_fixture::Choice",
        "native_record",
    ):
        found = target.FindTypes(name)
        types[name] = [
            {
                "name": found.GetTypeAtIndex(index).GetName(),
                "bytes": found.GetTypeAtIndex(index).GetByteSize(),
                "fields": [
                    {
                        "name": field.GetName(),
                        "offset_bytes": field.GetOffsetInBytes(),
                        "type": field.GetType().GetName(),
                    }
                    for field in found.GetTypeAtIndex(index).fields
                ],
            }
            for index in range(found.GetSize())
        ]
    debugger.DeleteTarget(target)

    target, _ = create_target(debugger, configuration)
    line_breakpoint = target.BreakpointCreateByLocation(
        configuration["source"], configuration["step_line"]
    )
    stepping = {
        "requested_line": configuration["step_line"],
        "locations": line_breakpoint.GetNumLocations(),
        "stops": [],
    }
    if line_breakpoint.GetNumLocations():
        process = launch(target, configuration)
        try:
            for _ in range(6):
                if process.GetState() != lldb.eStateStopped:
                    break
                stepping["stops"].append(snapshot(process))
                process.GetSelectedThread().StepOver()
        finally:
            if process.IsValid() and process.GetState() != lldb.eStateExited:
                process.Kill()
    debugger.DeleteTarget(target)
    return {
        "debugger": lldb.SBDebugger.GetVersionString(),
        "settings": settings,
        "formatters": formatters,
        "symbols_loaded": loaded,
        "observations": observations,
        "types": types,
        "stepping": stepping,
        "stdout": stdout,
        "stderr": stderr,
        "completed": True,
    }


def run_probe(debugger, text, result, internal_dictionary):
    path = Path(shlex.split(text)[0])
    configuration = json.loads(path.read_text())
    try:
        record = observe(debugger, configuration)
    except (RuntimeError, OSError, ValueError, TypeError, AttributeError):
        record = {"completed": False, "error": traceback.format_exc()}
    Path(configuration["output"]).write_text(json.dumps(record, indent=2) + "\n")
    if record["completed"]:
        result.AppendMessage(
            "Recorded debugger checkpoints and source-stepping observations."
        )
    else:
        result.SetError(record["error"])


def __lldb_init_module(debugger, internal_dictionary):
    debugger.HandleCommand(
        "command script add -f lldb_probe.run_probe capability-probe"
    )
