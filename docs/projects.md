# Projects

## Project metadata

`pyproject.toml`

```
uv init
```

## Project environments

`.venv`

```
uv sync
```

## Lock files

```
uv lock
```

## Adding dependencies

```
uv add
```

### Updating existing dependencies

<!-- What happens when the same dependency is added multiple times? -->

## Removing dependencies

```
uv remove
```

## Running commands

```
uv run
```

### Running commands with additional dependencies

### Running scripts

Scripts that declare inline metadata are automatically executed in environments isolated from the project. See the [scripts guide](./guides/scripts.md#declaring-script-dependencies) for more details.

## Projects with many packages

See the [workspaces](./preview/workspaces.md) documentation.
