"""Report the default heap of the native rustc process on 64-bit Windows CI."""

import ctypes
import subprocess
import sys
import time
from ctypes import wintypes


def main():
    pointer = ctypes.c_void_p
    size = ctypes.c_size_t
    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    read = kernel.ReadProcessMemory
    read.argtypes = [wintypes.HANDLE, pointer, pointer, size, ctypes.POINTER(size)]
    query = ctypes.WinDLL("ntdll").NtQueryInformationProcess
    query.argtypes = [wintypes.HANDLE, wintypes.ULONG, pointer, wintypes.ULONG, pointer]
    assert ctypes.sizeof(pointer) == 8
    rustc = subprocess.check_output(["rustup", "which", "rustc"], text=True).strip()
    heaps = {0xFFEEFFEE: "NT", 0xDDEEDDEE: "segment"}

    # rustc reads source from stdin, so it stays alive until the probe finishes.
    with subprocess.Popen([rustc, "--print", "crate-name", "-"], stdin=subprocess.PIPE) as proc:
        try:
            info = (pointer * 6)()
            status = query(int(proc._handle), 0, ctypes.byref(info), ctypes.sizeof(info), None)
            if status or not info[1]:
                raise RuntimeError(f"ProcessBasicInformation returned {status:#x}")
            # Native x64/Arm64 PEB.ProcessHeap and the heap signatures used by x64dbg.
            # https://github.com/x64dbg/x64dbg/blob/c44297200731995c51dbc75ab3098e2f0094b282/src/dbg/memory.cpp#L305-L324
            heap = pointer()
            signature = wintypes.DWORD()
            for _ in range(100):
                if (read(int(proc._handle), info[1] + 0x30, ctypes.byref(heap), 8, None)
                        and heap.value
                        and read(int(proc._handle), heap.value + 0x10, ctypes.byref(signature), 4, None)
                        and signature.value in heaps):
                    name = heaps[signature.value]
                    print(f"rustc default heap = {name} (0x{signature.value:08x})")
                    if sys.argv[1] == "segment" and name != "segment":
                        raise RuntimeError("rustc did not select the segment heap")
                    return
                time.sleep(0.05)
            raise RuntimeError(f"Unknown rustc heap signature 0x{signature.value:08x}")
        finally:
            proc.stdin.close()


if __name__ == "__main__":
    main()
