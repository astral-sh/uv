# Dependency bots

## Renovate

uv is supported by [Renovate](https://github.com/renovatebot/renovate).

With `lockFileMaintenance` enabled, Renovate will automatically detect `uv.lock` and suggest updates
to `uv.lock` files.

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

## Dependabot

Support for uv is [in progress](https://github.com/dependabot/dependabot-core/issues/10039).
