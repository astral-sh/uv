---
title: Using uv with Renovate
description: A guide to using uv with the Renovate dependency bot.
---

# Renovate

It is considered best practice to regularly update dependencies, to avoid being exposed to
vulnerabilities, limit incompatibilities between dependencies, and avoid complex upgrades when
upgrading from a too old version.

uv is supported by [Renovate](https://github.com/renovatebot/renovate).

## `uv.lock` output

Renovate uses the presence of a `uv.lock` file to determine that uv is used for managing
dependencies, and will suggest upgrades to
[project dependencies](../../concepts/projects/dependencies.md#project-dependencies),
[optional dependencies](../../concepts/projects/dependencies.md#optional-dependencies) and
[development dependencies](../../concepts/projects/dependencies.md#development-dependencies).
Renovate will update both the `pyproject.toml` and `uv.lock` files.

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

## Inline script metadata

Renovate supports updating dependencies defined using
[inline script metadata](../scripts.md/#declaring-script-dependencies).

Since it cannot automatically detect which Python files use inline script metadata, their locations
need to be explicitly defined using
[`managerFilePatterns`](https://docs.renovatebot.com/configuration-options/#managerfilepatterns),
like so:

```jsx title="renovate.json5"
{
  $schema: "https://docs.renovatebot.com/renovate-schema.json",
  pep723: {
    managerFilePatterns: [
      "docs/build.py",
      "scripts/**/*.py",
    ],
  },
}
```

!!! note

    Renovate does not yet support updating the lock file associated to the
    script (https://github.com/renovatebot/renovate/issues/33591), so if you rely on this feature
    for a script, the lock file will need to be manually updated.

## Dependency cooldown

If you use [`exclude-newer`](../../reference/settings.md#exclude-newer) option, it is recommended to
also set the equivalent
[`minimumReleaseAge`](https://docs.renovatebot.com/configuration-options/#minimumreleaseage) option
in Renovate, to avoid ending up with pull requests where uv would not be able to lock the
dependencies.

For instance, if you've set `exclude-newer` to `1 week`, you can set:

```jsx title="renovate.json5"
{
  $schema: "https://docs.renovatebot.com/renovate-schema.json",

  // Enable only for PyPI.
  packageRules: [
    {
      matchDatasources: ["pypi"],
      minimumReleaseAge: "1 week",
    },
  ],

  // Or enable for every ecosystem.
  minimumReleaseAge: "1 week",
}
```
