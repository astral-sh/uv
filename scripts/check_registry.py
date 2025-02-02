"""Check that adding uv's python-build-standalone distributions are successfully added
and removed from the Windows registry following PEP 514."""

import re
import subprocess
import sys
from argparse import ArgumentParser

# We apply the same download URL/hash redaction to the actual output, too. We don't
# redact the path inside the runner, if the runner configuration changes
# (or uv's installation paths), please update the snapshots.
expected_registry = [
    # Our company key
    r"""
Name                           Property
----                           --------
Astral                         DisplayName : Astral Software Inc.
                               SupportUrl  : https://github.com/astral-sh/uv
""",
    # The actual Python installations
    r"""
    Hive: HKEY_CURRENT_USER\Software\Python


Name                           Property
----                           --------
Astral                         DisplayName : Astral Software Inc.
                               SupportUrl  : https://github.com/astral-sh/uv


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
InstallPath                    (default)              : C:\Users\runneradmin\AppData\Roaming\uv\python\cpython-3.11.11-windows-x86_64-none
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
InstallPath                    (default)              : C:\Users\runneradmin\AppData\Roaming\uv\python\cpython-3.12.8-windows-x86_64-none
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
InstallPath                    (default)              : C:\Users\runneradmin\AppData\Roaming\uv\python\cpython-3.13.1-windows-x86_64-none
                               ExecutablePath         : C:\Users\runneradmin\AppData\Roaming\uv\python\cpython-3.13.1-windows-x86_64-none\python.exe
                               WindowedExecutablePath : C:\Users\runneradmin\AppData\Roaming\uv\python\cpython-3.13.1-windows-x86_64-none\pythonw.exe
""",
]


def filter_snapshot(snapshot: str) -> str:
    # Trim only newlines, there's leading whitespace before the `Hive:` entry
    snapshot = snapshot.strip("\n\r")
    # Trim trailing whitespace, Windows pads lines up to length
    snapshot = "\n".join(line.rstrip() for line in snapshot.splitlines())
    # Long URLs can be wrapped into multiple lines
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
    # `py --list-paths` output
    py_311_line = r" -V:Astral/CPython3.11.11 C:\Users\runneradmin\AppData\Roaming\uv\python\cpython-3.11.11-windows-x86_64-none\python.exe"
    py_312_line = r" -V:Astral/CPython3.12.8 C:\Users\runneradmin\AppData\Roaming\uv\python\cpython-3.12.8-windows-x86_64-none\python.exe"
    py_313_line = r" -V:Astral/CPython3.13.1 C:\Users\runneradmin\AppData\Roaming\uv\python\cpython-3.13.1-windows-x86_64-none\python.exe"

    # Use the powershell command to get an outside view on the registry values we wrote
    # By default, powershell wraps the output at terminal size
    list_registry_command = r"Get-ChildItem -Path HKCU:\Software\Python -Recurse | Format-Table | Out-String -width 1000"

    # Check 1: Install interpreters and check that all their keys are set in the
    # registry and that the Python launcher for Windows finds it.
    # Check 1a: Install new interpreters.
    # Check 1b: Request installation of already installed interpreters.
    for _ in range(2):
        print("Installing Python 3.11.11, 3.12.8, and 3.13.1")
        subprocess.check_call([uv, "python", "install", "-v", "--preview", "3.11.11"])
        subprocess.check_call([uv, "python", "install", "-v", "--preview", "3.12.8"])
        subprocess.check_call([uv, "python", "install", "-v", "--preview", "3.13.1"])
        # The default shell for a subprocess is not powershell
        actual_registry = subprocess.check_output(
            ["powershell", "-Command", list_registry_command], text=True
        )
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
        listed_interpreters = subprocess.check_output(["py", "--list-paths"], text=True)
        py_listed = set(listed_interpreters.splitlines())
        if (
            py_311_line not in py_listed
            or py_312_line not in py_listed
            or py_313_line not in py_listed
        ):
            print(
                "Python launcher interpreter mismatch: "
                f"{py_listed} vs. {py_311_line}, {py_312_line}, {py_313_line}"
            )
            sys.exit(1)

    # Check 2: Remove a single interpreter and check that its gone.
    # Check 2a: Removing an existing interpreter.
    # Check 2b: Remove a missing interpreter.
    for _ in range(2):
        print("Removing Python 3.11.11")
        subprocess.check_call([uv, "python", "uninstall", "-v", "--preview", "3.11.11"])
        listed_interpreters = subprocess.check_output(["py", "--list-paths"], text=True)
        py_listed = set(listed_interpreters.splitlines())
        if (
            py_311_line in py_listed
            or py_312_line not in py_listed
            or py_313_line not in py_listed
        ):
            print(
                "Python launcher interpreter not removed: "
                f"{py_listed} vs. {py_312_line}, {py_313_line}"
            )
            sys.exit(1)

    # Check 3: Remove all interpreters and check that they are all gone.
    # Check 3a: Clear a used registry.
    # Check 3b: Clear an empty registry.
    subprocess.check_call([uv, "python", "uninstall", "-v", "--preview", "--all"])
    for _ in range(2):
        print("Removing all Pythons")
        empty_registry = subprocess.check_output(
            ["powershell", "-Command", list_registry_command], text=True
        )
        if empty_registry.strip():
            print("Registry not cleared:")
            print("=" * 80)
            print(empty_registry)
            print("=" * 80)
            sys.exit(1)
        listed_interpreters = subprocess.check_output(["py", "--list-paths"], text=True)
        py_listed = set(listed_interpreters.splitlines())
        if (
            py_311_line in py_listed
            or py_312_line in py_listed
            or py_313_line in py_listed
        ):
            print(f"Python launcher interpreter not cleared: {py_listed}")
            sys.exit(1)


if __name__ == "__main__":
    parser = ArgumentParser()
    parser.add_argument("--uv", default="./uv.exe")
    args = parser.parse_args()
    main(args.uv)
