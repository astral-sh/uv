---
title: Using uv with Bazel
description: Using uv to power package resolution with Bazel
---

# Using uv with Bazel

For broader Bazel workflows with uv, see the
[`rules_py` uv guide](https://github.com/aspect-build/rules_py#dependency-resolution-with-uv) or the
[`rules_python` uv guide](https://rules-python.readthedocs.io/en/latest/pypi/lock.html#uv-pip-compile-bzlmod-only).

## Authentication

Bazel 7 and newer supports credential helpers via the `--credential_helper` option. To use
credentials stored by uv for Bazel fetches, first authenticate uv with the service that hosts the
files Bazel needs to fetch:

```console
$ uv auth login https://packages.example.com
```

Then, configure Bazel to invoke
[`uv auth helper`](../../concepts/authentication/cli.md#using-credentials-with-external-tools) for
matching hosts:

```text title=".bazelrc"
common --credential_helper=packages.example.com=%workspace%/bazel/uv-auth-helper
common --credential_helper=files.example.com=%workspace%/bazel/uv-auth-helper
```

Replace the host patterns with the hosts that serve the index and files Bazel will fetch.

Finally, add the wrapper script referenced by `.bazelrc`:

```bash title="bazel/uv-auth-helper"
#!/usr/bin/env bash
exec uv --preview-features auth-helper auth helper --protocol=bazel "$@"
```

The script must be executable:

```console
$ chmod +x bazel/uv-auth-helper
```
