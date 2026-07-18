# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///

"""Post-edit hook to auto-format files after agent edits."""

import json
import os
import subprocess
import sys
from pathlib import Path


def format_rust(file_path: str, cwd: str) -> None:
    """Format Rust files with cargo fmt."""
    try:
        subprocess.run(
            ["cargo", "fmt", "--", file_path],
            cwd=cwd,
            capture_output=True,
        )
    except FileNotFoundError:
        pass


def format_python(file_path: str, cwd: str) -> None:
    """Format Python files with ruff."""
    try:
        subprocess.run(
            ["uvx", "ruff", "format", file_path],
            cwd=cwd,
            capture_output=True,
        )
    except FileNotFoundError:
        pass


def format_prettier(file_path: str, cwd: str) -> None:
    """Format files with prettier."""
    try:
        subprocess.run(
            ["npx", "prettier@3.9.0", "--write", file_path], cwd=cwd, capture_output=True
        )
    except FileNotFoundError:
        pass


def patch_file_paths(command: str) -> list[str]:
    """Return added or updated file paths from an `apply_patch` payload."""
    file_paths: list[str] = []
    current_update_index: int | None = None

    for line in command.splitlines():
        if line.startswith("*** Add File: "):
            file_paths.append(line.removeprefix("*** Add File: "))
            current_update_index = None
        elif line.startswith("*** Update File: "):
            file_paths.append(line.removeprefix("*** Update File: "))
            current_update_index = len(file_paths) - 1
        elif line.startswith("*** Move to: ") and current_update_index is not None:
            file_paths[current_update_index] = line.removeprefix("*** Move to: ")
            current_update_index = None
        elif line.startswith("*** Delete File: "):
            current_update_index = None

    return list(dict.fromkeys(file_paths))


def edited_file_paths(input_data: dict[str, object]) -> list[str]:
    tool_name = input_data.get("tool_name")
    tool_input = input_data.get("tool_input")
    if not isinstance(tool_input, dict):
        return []

    if tool_name in ("Write", "Edit", "MultiEdit"):
        file_path = tool_input.get("file_path")
        return [file_path] if isinstance(file_path, str) and file_path else []

    if tool_name == "apply_patch":
        command = tool_input.get("command")
        return patch_file_paths(command) if isinstance(command, str) else []

    return []


def format_file(file_path: str, cwd: str) -> None:
    ext = Path(file_path).suffix

    if ext == ".rs":
        format_rust(file_path, cwd)
    elif ext in (".py", ".pyi"):
        format_python(file_path, cwd)
    elif ext in (".json5", ".yaml", ".yml", ".md"):
        format_prettier(file_path, cwd)


def main() -> None:
    input_data = json.load(sys.stdin)
    cwd = input_data.get("cwd")
    if not isinstance(cwd, str) or not cwd:
        cwd = os.environ.get("CLAUDE_PROJECT_DIR", os.getcwd())

    for file_path in edited_file_paths(input_data):
        format_file(file_path, cwd)


if __name__ == "__main__":
    main()
