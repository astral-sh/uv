# Dependency bots

It is considered best practice to regularly update dependencies, to avoid being exposed to
vulnerabilities, limit incompatibilities between dependencies, and avoid complex upgrades when
upgrading from a too old version. A variety of tools can help staying up-to-date by creating
automated pull requests. Several of them support uv, or have work underway to support it.

## Renovate

uv is supported by [Renovate](https://github.com/renovatebot/renovate).

!!! note

    Updating `uv pip compile` outputs such as `requirements.txt` is not yet supported. Progress can
    be tracked
    at [renovatebot/renovate#30909](https://github.com/renovatebot/renovate/issues/30909).

### `uv.lock` output

Renovate uses the presence of a `uv.lock` file to determine that uv is used for managing
dependencies, and will suggest upgrades to
[project dependencies](../../concepts/dependencies.md#project-dependencies),
[optional dependencies](../../concepts/dependencies.md#optional-dependencies) and
[development dependencies](../../concepts/dependencies.md#development-dependencies). Renovate will
update both the `pyproject.toml` and `uv.lock` files.

The lockfile can also be refreshed on a regular basis (for instance to update transitive
dependencies) by enabling the
[`lockFileMaintenance`](https://docs.renovatebot.com/configuration-options/#lockfilemaintenance)
option:

```jsx title="renovate.json5"
{
  $schema: "https://docs.renovatebot.com/renovate-schema.json",
  lockFileMaintenance: {
    enabled: true,
  },
}
```

### Inline script metadata

Renovate supports updating dependencies defined using
[script inline metadata](../scripts.md/#declaring-script-dependencies).

Since it cannot automatically detect which Python files use script inline metadata, their locations
need to be explicitly defined using
[`fileMatch`](https://docs.renovatebot.com/configuration-options/#filematch), like so:

```jsx title="renovate.json5"
{
  $schema: "https://docs.renovatebot.com/renovate-schema.json",
  pep723: {
    fileMatch: [
      "scripts/generate_docs\\.py",
      "scripts/run_server\\.py",
    ],
  },
}
```

## Dependabot

Support for uv is not yet available. Progress can be tracked at:

- [dependabot/dependabot-core#10478](https://github.com/dependabot/dependabot-core/issues/10478) for
  `uv.lock` output
- [dependabot/dependabot-core#10039](https://github.com/dependabot/dependabot-core/issues/10039) for
  `uv pip compile` outputs
