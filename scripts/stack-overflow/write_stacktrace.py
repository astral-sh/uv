import lldb


def save_backtrace(debugger, command, result, dict):
    process = debugger.GetSelectedTarget().GetProcess()
    if not process:
        result.SetError("No process found")
        return

    with open("scripts/stack-overflow/stacktrace.csv", "w") as f:
        f.write("id,stack_pointer,name,location\n")
        for thread in process:
            if thread.GetStopReason() != lldb.eStopReasonSignal:
                continue
            signal = thread.GetStopReasonDataAtIndex(0)
            # Signal 11 is SIGSEGV (segmentation fault), raised for stack overflows
            if signal != 11:
                continue

            for frame in thread:
                f.write(f"{frame.idx},{frame.sp},{frame.name},{frame.line_entry}\n")
            break


def __lldb_init_module(debugger, dict):
    debugger.HandleCommand(
        "command script add -f write_stacktrace.save_backtrace save_backtrace"
    )
    print("The 'save_backtrace' command has been installed.")
