# Python Resolution Policy (Proposal for #20759)

This change introduces an opt-in policy to restrict Python interpreter candidates during resolution:

Precedence: CLI flags > `.python-version` > `uv.toml` configuration.

- `--resolve-python-only 3.11.9`
- `--resolve-python-range ">=3.10,<3.13"`

Configuration (uv.toml):

```toml
[python.resolution]
only = "3.11.9" # alternatively
# range = ">=3.10,<3.13"
```

A `.python-version` file in the repo root is detected automatically when CLI is not provided.
