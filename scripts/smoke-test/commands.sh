# Note this is not a real shell-script, it's parsed by `smoke-test/__main__.py` and executed
# serially via Python.

uv venv -v
uv pip install ruff -v
uvx -v ruff --version

