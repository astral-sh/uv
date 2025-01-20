"""Check that adding uv's python-build-standalone distributions are successfully added
and removed from the Windows registry following PEP 514."""

import re
import subprocess
import sys
from argparse import ArgumentParser

# This is the snapshot as of python build standalone 20250115, we redact URL and hash
# below. We don't redact the path inside the runner, if the runner configuration changes
# (or uv's installation paths), please update the snapshot.
expected_registry = [
    r"""
Name                           Property
----                           --------
Astral                         DisplayName : Astral Software Inc.
                               SupportUrl  : https://github.com/astral-sh/uv
""",
    r"""
    Hive: HKEY_CURRENT_USER\Software\Python\Astral


Name                           Property
----                           --------
CPython3.11.11                 DisplayName     : CPython 3.11.11 (64-bit)
                               SupportUrl      : https://github.com/astral-sh/uv
                               Version         : 3.11.11
                               SysVersion      : 3.11.11
                               SysArchitecture : 64bit
                               DownloadUrl     : <downloadUrl>
                               DownloadSha256  : <downloadSha256>


Hive: HKEY_CURRENT_USER\Software\Python\Astral\CPython3.11.11


Name                           Property
----                           --------
InstallPath                    (default)              :
C:\Users\runneradmin\AppData\Roaming\uv\python\cpython-3.11.11-windows-x86_64-none
ExecutablePath         : C:\Users\runneradmin\AppData\Roaming\uv\python\cpython-3.11.11-windows-x86_64-none\python.exe
WindowedExecutablePath : C:\Users\runneradmin\AppData\Roaming\uv\python\cpython-3.11.11-windows-x86_64-none\pythonw.exe
""",
    r"""
    Hive: HKEY_CURRENT_USER\Software\Python\Astral


Name                           Property
----                           --------
CPython3.12.8                  DisplayName     : CPython 3.12.8 (64-bit)
                               SupportUrl      : https://github.com/astral-sh/uv
                               Version         : 3.12.8
                               SysVersion      : 3.12.8
                               SysArchitecture : 64bit
                               DownloadUrl     : <downloadUrl>
                               DownloadSha256  : <downloadSha256>


    Hive: HKEY_CURRENT_USER\Software\Python\Astral\CPython3.12.8


Name                           Property
----                           --------
InstallPath                    (default)              :
                               C:\Users\runneradmin\AppData\Roaming\uv\python\cpython-3.12.8-windows-x86_64-none
                               ExecutablePath         : C:\Users\runneradmin\AppData\Roaming\uv\python\cpython-3.12.8-windows-x86_64-none\python.exe
                               WindowedExecutablePath : C:\Users\runneradmin\AppData\Roaming\uv\python\cpython-3.12.8-windows-x86_64-none\pythonw.exe
""",
    r"""
    Hive: HKEY_CURRENT_USER\Software\Python\Astral


Name                           Property
----                           --------
CPython3.13.1                  DisplayName     : CPython 3.13.1 (64-bit)
                               SupportUrl      : https://github.com/astral-sh/uv
                               Version         : 3.13.1
                               SysVersion      : 3.13.1
                               SysArchitecture : 64bit
                               DownloadUrl     : <downloadUrl>
                               DownloadSha256  : <downloadSha256>


    Hive: HKEY_CURRENT_USER\Software\Python\Astral\CPython3.13.1


Name                           Property
----                           --------
InstallPath                    (default)              :
                               C:\Users\runneradmin\AppData\Roaming\uv\python\cpython-3.13.1-windows-x86_64-none
                               ExecutablePath         : C:\Users\runneradmin\AppData\Roaming\uv\python\cpython-3.13.1-windows-x86_64-none\python.exe
                               WindowedExecutablePath : C:\Users\runneradmin\AppData\Roaming\uv\python\cpython-3.13.1-windows-x86_64-none\pythonw.exe
""",
]


def filter_snapshot(snapshot: str) -> str:
    snapshot = snapshot.strip()
    # Trim trailing whitespace
    snapshot = "\n".join(line.rstrip() for line in snapshot.splitlines())
    # Long URLs are wrapped into multiple lines
    snapshot = re.sub(
        "DownloadUrl ( *): .*(\n.*)+?(\n +)DownloadSha256",
        r"DownloadUrl \1: <downloadUrl>\3DownloadSha256",
        snapshot,
    )
    snapshot = re.sub(
        "DownloadSha256 ( *): .*", r"DownloadSha256 \1: <downloadSha256>", snapshot
    )
    return snapshot


def main(uv: str):
    # Check 1: Install interpreters and check that all their keys are set in the
    # registry and that the Python launcher for Windows finds it.
    print("Installing Python 3.11.11, 3.12.8, and 3.13.1")
    subprocess.check_call([uv, "python", "install", "--preview", "3.11.11"])
    subprocess.check_call([uv, "python", "install", "--preview", "3.12.8"])
    subprocess.check_call([uv, "python", "install", "--preview", "3.13.1"])
    # Use the powershell command to get an outside view on the registry values we wrote
    list_registry_command = [
        "powershell",
        "-Command",
        # By default, powershell wraps the output at terminal size
        r"Get-ChildItem -Path HKCU:\Software\Python -Recurse | Format-Table | Out-String -width 1000",
    ]
    actual_registry = subprocess.check_output(list_registry_command, text=True)
    for expected in expected_registry:
        if filter_snapshot(expected) not in filter_snapshot(actual_registry):
            print("Registry mismatch:")
            print("Expected Snippet:")
            print("=" * 80)
            print(filter_snapshot(expected))
            print("=" * 80)
            print("Actual:")
            print("=" * 80)
            print(filter_snapshot(actual_registry))
            print("=" * 80)
            sys.exit(1)
    py_311_line = r" -V:Astral/CPython3.11.11 C:\Users\runneradmin\AppData\Roaming\uv\python\cpython-3.11.11-windows-x86_64-none\python.exe"
    py_312_line = r" -V:Astral/CPython3.12.8 C:\Users\runneradmin\AppData\Roaming\uv\python\cpython-3.12.8-windows-x86_64-none\python.exe"
    py_313_line = r" -V:Astral/CPython3.13.1 C:\Users\runneradmin\AppData\Roaming\uv\python\cpython-3.13.1-windows-x86_64-none\python.exe"
    listed_interpreters = subprocess.check_output(["py", "--list-paths"], text=True)
    py_listed = set(listed_interpreters.splitlines())
    if (
        py_311_line not in py_listed
        or py_312_line not in py_listed
        or py_313_line not in py_listed
    ):
        print(
            f"Python launcher interpreter mismatch: {py_listed} vs. {py_311_line}, {py_312_line}, {py_313_line}"
        )
        sys.exit(1)

    # Check 2: Remove a single interpreter and check that its gone.
    print("Removing Python 3.11.11")
    subprocess.check_call([uv, "python", "uninstall", "--preview", "3.11.11"])
    listed_interpreters = subprocess.check_output(["py", "--list-paths"], text=True)
    py_listed = set(listed_interpreters.splitlines())
    if (
        py_311_line in py_listed
        or py_312_line not in py_listed
        or py_313_line not in py_listed
    ):
        print(
            f"Python launcher interpreter not removed: {py_listed} vs. {py_312_line}, {py_313_line}"
        )
        sys.exit(1)

    # Check 3: Remove all interpreters and check that they are all gone.
    subprocess.check_call([uv, "python", "uninstall", "--preview", "--all"])
    empty_registry = subprocess.check_output(list_registry_command, text=True)
    if empty_registry.strip():
        print("Registry not cleared:")
        print("=" * 80)
        print(empty_registry)
        print("=" * 80)
        sys.exit(1)
    listed_interpreters = subprocess.check_output(["py", "--list-paths"], text=True)
    py_listed = set(listed_interpreters.splitlines())
    if py_311_line in py_listed or py_312_line in py_listed or py_313_line in py_listed:
        print(f"Python launcher interpreter not cleared: {py_listed}")
        sys.exit(1)


if __name__ == "__main__":
    parser = ArgumentParser()
    parser.add_argument("--uv", default="./uv.exe")
    args = parser.parse_args()
    main(args.uv)
