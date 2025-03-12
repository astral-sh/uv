# NOTE this is not a real shell-script, it's parsed by `smoke-test/__main__.py` and executed
# serially via Python for cross-platform support.

# Create a virtual environment and install a package with `uv pip`
uv venv -v
uv pip install ruff -v

# Run a package via `uvx`
uvx -v ruff --version

