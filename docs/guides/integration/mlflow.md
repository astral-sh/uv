---
title: Using uv with MLflow
description:
  A guide to using uv with MLflow for ML experiment tracking and model management, including
  automatic dependency inference, reproducible environments, and CI/CD integration.
---

# Using uv with MLflow

[MLflow](https://mlflow.org/) is a popular open-source platform for managing the end-to-end machine
learning lifecycle, including experiment tracking, model versioning, and deployment. uv integrates
seamlessly with MLflow to provide fast, reproducible dependency management for ML projects.

!!! note

    MLflow's uv integration requires MLflow version 3.10 or later.

## Automatic dependency inference

When you log a model with MLflow, it automatically captures your project's dependencies to ensure
reproducibility. If MLflow detects a uv project (indicated by the presence of both `uv.lock` and
`pyproject.toml`), it will use uv to infer dependencies instead of analyzing Python imports.

This means you can log models without manually specifying dependencies:

```python
import mlflow

# MLflow automatically detects uv.lock + pyproject.toml
# and captures dependencies via `uv export`
mlflow.pyfunc.log_model(
    name="model",
    python_model=my_model,
)
```

MLflow runs `uv export --frozen --no-dev --no-hashes --no-header --no-emit-project --no-annotate` to
generate a pinned `requirements.txt` that exactly matches your lock file.

## Reproducibility artifacts

In addition to exporting dependencies, MLflow logs your uv project files as artifacts for full
reproducibility:

- `uv.lock`: the complete lock file with all resolved dependencies
- `pyproject.toml`: your project configuration
- `.python-version`: the Python version specification (if present)

These artifacts enable anyone to recreate your exact environment using `uv sync --frozen`.

## Environment restoration

When loading a logged model, MLflow automatically detects the uv lock file artifact and restores the
environment using `uv sync --frozen --no-dev`. This happens transparently when you call
`mlflow.pyfunc.load_model()`, no manual intervention is needed.

If uv is not available at load time, MLflow falls back to `pip install -r requirements.txt`.

Using uv for restoration is significantly faster than pip because uv:

- Uses parallel downloads
- Leverages its global cache
- Performs optimized dependency resolution

## CI/CD integration

For reproducible model training in CI/CD pipelines, combine uv with MLflow:

```yaml
# .github/workflows/train.yml
name: Train Model

on: [push]

jobs:
  train:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install uv
        uses: astral-sh/setup-uv@v5
        with:
          enable-cache: true

      - name: Install dependencies
        run: uv sync --frozen

      - name: Train and log model
        run: uv run python train.py
        env:
          MLFLOW_TRACKING_URI: ${{ secrets.MLFLOW_TRACKING_URI }}
```

The `--frozen` flag ensures CI uses exactly the same dependencies as your local environment.

## Monorepo support

For monorepos where `uv.lock` is not in the current working directory, use the `uv_project_path`
parameter to point to the directory containing your `uv.lock` and `pyproject.toml`:

```python
mlflow.pyfunc.log_model(
    name="model",
    python_model=my_model,
    uv_project_path="../my-subproject",  # Directory containing uv.lock + pyproject.toml
)
```

## Disabling uv integration

If you need to fall back to MLflow's default import-based dependency inference, disable uv
auto-detection entirely:

```bash
export MLFLOW_UV_AUTO_DETECT=false
```

## Disabling uv file logging

For large projects where logging uv files as artifacts is not desired (but you still want uv-based
dependency inference), disable file logging:

```bash
export MLFLOW_LOG_UV_FILES=false
```

MLflow will still use uv for dependency inference but won't copy the lock files as artifacts.

## Best practices

### Lock your dependencies before logging

Always ensure your `uv.lock` is up to date before logging models:

```bash
uv lock
uv run python train.py  # Train and log model
```

### Use frozen installs in production

When deploying models, use `--frozen` to ensure exact reproducibility:

```bash
uv sync --frozen
```

## Troubleshooting

### MLflow not detecting uv project

Ensure both `uv.lock` and `pyproject.toml` exist in your project root. MLflow checks for both files
to confirm it's a uv project.

### Dependencies not matching expected versions

Run `uv lock` to update your lock file, then verify with:

```bash
uv export --frozen --no-dev --no-hashes --no-header --no-emit-project --no-annotate
```

This shows exactly what MLflow will capture.

### Environment variable not taking effect

Environment variables must be set before running your script:

```bash
export MLFLOW_UV_AUTO_DETECT=false
python train.py
```
