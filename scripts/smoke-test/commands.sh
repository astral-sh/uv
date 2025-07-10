# NOTE this is not a real shell-script, it's parsed by `smoke-test/__main__.py` and executed
# serially via Python for cross-platform support.

# Show the uv version
uv --version

# Use any Python 3.13 version
uv python pin 3.13

# Create a virtual environment and install a package with `uv pip`
uv venv -v
uv pip install ruff -v

# Install a package with extension modules, e.g., `numpy` and make sure it's importable
uv pip install numpy -v
uv run python -c "import numpy; print(numpy.__version__)"

# Show the `uvx` version
uvx --version

# Run a package via `uvx`
uvx -v ruff --version
