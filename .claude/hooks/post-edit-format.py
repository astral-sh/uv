# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///

"""Post-edit hook to auto-format files after Claude edits."""

import json
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
            ["npx", "prettier", "--write", file_path], cwd=cwd, capture_output=True
        )
    except FileNotFoundError:
        pass


def main() -> None:
    import os

    input_data = json.load(sys.stdin)

    tool_name = input_data.get("tool_name")
    tool_input = input_data.get("tool_input", {})
    file_path = tool_input.get("file_path")

    # Only process Write, Edit, and MultiEdit tools
    if tool_name not in ("Write", "Edit", "MultiEdit"):
        return

    if not file_path:
        return

    cwd = os.environ.get("CLAUDE_PROJECT_DIR", os.getcwd())
    path = Path(file_path)
    ext = path.suffix

    if ext == ".rs":
        format_rust(file_path, cwd)
    elif ext in (".py", ".pyi"):
        format_python(file_path, cwd)
    elif ext in (".json5", ".yaml", ".yml", ".md"):
        format_prettier(file_path, cwd)


if __name__ == "__main__":
    main()
