# Dependency bots

## Renovate

uv is supported by [Renovate](https://github.com/renovatebot/renovate).

Renovate uses the presence of a `uv.lock` file to determine that uv is used for managing
dependencies, and will suggest upgrades to
[project dependencies](../../concepts/dependencies.md#project-dependencies),
[optional dependencies](../../concepts/dependencies.md#optional-dependencies) and
[development dependencies](../../concepts/dependencies.md#development-dependencies). Renovate will
update both the `pyproject.toml` and `uv.lock` files.

The lockfile on also be refreshed on a regular basis (for instance to update transitive
dependencies) by enabling the
[`lockFileMaintenance`](https://docs.renovatebot.com/configuration-options/#lockfilemaintenance)
option:

```json5 title="renovate.json5"
{
  $schema: "https://docs.renovatebot.com/renovate-schema.json",
  lockFileMaintenance: {
    enabled: true,
  },
}
```

!!! note

    `uv pip compile` outputs such as `requirements.txt` are not yet supported by Renovate.
    Progress can be tracked at [renovatebot/renovate#30909](https://github.com/renovatebot/renovate/issues/30909).

## Dependabot

Support for uv  is not yet available. Progress can be tracked at
[dependabot/dependabot-core#10039](https://github.com/dependabot/dependabot-core/issues/10039).
