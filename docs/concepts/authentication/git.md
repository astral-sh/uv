# Git credentials

uv allows packages to be installed from private Git repositories using SSH or HTTP authentication.

## SSH authentication

To authenticate using an SSH key, use the `ssh://` protocol:

- `git+ssh://git@<hostname>/...` (e.g., `git+ssh://git@github.com/astral-sh/uv`)
- `git+ssh://git@<host>/...` (e.g., `git+ssh://git@github.com-key-2/astral-sh/uv`)

SSH authentication requires using the username `git`.

See the
[GitHub SSH documentation](https://docs.github.com/en/authentication/connecting-to-github-with-ssh/about-ssh)
for more details on how to configure SSH.

### HTTP authentication

To authenticate over HTTP Basic authentication using a password or token:

- `git+https://<user>:<token>@<hostname>/...` (e.g.,
  `git+https://git:github_pat_asdf@github.com/astral-sh/uv`)
- `git+https://<token>@<hostname>/...` (e.g., `git+https://github_pat_asdf@github.com/astral-sh/uv`)
- `git+https://<user>@<hostname>/...` (e.g., `git+https://git@github.com/astral-sh/uv`)

!!! note

    When using a GitHub personal access token, the username is arbitrary. GitHub doesn't allow you to
    use your account name and password in URLs like this, although other hosts may.

If there are no credentials present in the URL and authentication is needed, the
[Git credential helper](#git-credential-helpers) will be queried.

## Persistence of credentials

When using `uv add`, uv _will not_ persist Git credentials to the `pyproject.toml` or `uv.lock`.
These files are often included in source control and distributions, so it is generally unsafe to
include credentials in them.

If you have a Git credential helper configured, your credentials may be automatically persisted,
resulting in successful subsequent fetches of the dependency. However, if you do not have a Git
credential helper or the project is used on a machine without credentials seeded, uv will fail to
fetch the dependency.

You _may_ force uv to persist Git credentials by passing the `--raw` option to `uv add`. However, we
strongly recommend setting up a [credential helper](#git-credential-helpers) instead.

## Git credential helpers

Git credential helpers are used to store and retrieve Git credentials. See the
[Git documentation](https://git-scm.com/doc/credential-helpers) to learn more.

If you're using GitHub, the simplest way to set up a credential helper is to
[install the `gh` CLI](https://github.com/cli/cli#installation) and use:

```console
$ gh auth login
```

See the [`gh auth login`](https://cli.github.com/manual/gh_auth_login) documentation for more
details.

!!! note

    When using `gh auth login` interactively, the credential helper will be configured automatically.
    But when using `gh auth login --with-token`, as in the uv
    [GitHub Actions guide](../../guides/integration/github.md#private-repos), the
    [`gh auth setup-git`](https://cli.github.com/manual/gh_auth_setup-git) command will need to be
    run afterwards to configure the credential helper.
