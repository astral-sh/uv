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

    MLflow's uv integration requires MLflow version 2.20 or later.

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
    artifact_path="model",
    python_model=my_model,
)
```

MLflow runs `uv export --frozen --no-dev --no-hashes` to generate a pinned `requirements.txt` that
exactly matches your lock file.

## Reproducibility artifacts

In addition to exporting dependencies, MLflow logs your uv project files as artifacts for full
reproducibility:

- `uv.lock` — The complete lock file with all resolved dependencies
- `pyproject.toml` — Your project configuration
- `.python-version` — The Python version specification (if present)

These artifacts enable anyone to recreate your exact environment using `uv sync --frozen`.

## Configuring dependency groups

MLflow supports uv's dependency groups and extras. You can control which groups are included when
logging models via environment variables:

```bash
# Include specific dependency groups
export MLFLOW_UV_GROUPS="dev,test"

# Include only specific groups (exclude default dependencies)
export MLFLOW_UV_ONLY_GROUPS="ml"

# Include optional extras
export MLFLOW_UV_EXTRAS="cuda,optimization"
```

These map directly to uv's `--group`, `--only-group`, and `--extra` flags.

## Environment restoration

When loading a logged model, MLflow can restore the environment using uv for faster installation:

```bash
# Restore environment from logged artifacts
uv sync --frozen
```

This is significantly faster than `pip install -r requirements.txt` because uv:

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

## Disabling uv integration

If you need to fall back to MLflow's default import-based dependency inference, you can disable uv
detection:

```bash
export MLFLOW_UV_AUTO_DETECT=false
```

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

### Separate training and inference dependencies

Use dependency groups to keep training-only packages out of your inference environment:

```toml
# pyproject.toml
[project]
dependencies = [
    "mlflow>=2.20",
    "scikit-learn>=1.0",
]

[dependency-groups]
train = [
    "jupyter",
    "matplotlib",
    "optuna",
]
```

Then log models without the training group:

```bash
export MLFLOW_UV_ONLY_GROUPS=""  # Exclude all groups, use only core dependencies
uv run python log_model.py
```

## Troubleshooting

### MLflow not detecting uv project

Ensure both `uv.lock` and `pyproject.toml` exist in your project root. MLflow checks for both files
to confirm it's a uv project.

### Dependencies not matching expected versions

Run `uv lock` to update your lock file, then verify with:

```bash
uv export --frozen --no-dev --no-hashes
```

This shows exactly what MLflow will capture.

### Environment variable not taking effect

Environment variables must be set before importing MLflow:

```bash
export MLFLOW_UV_GROUPS="dev"
python -c "import mlflow; mlflow.pyfunc.log_model(...)"
```
