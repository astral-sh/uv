# Authentication

## Git authentication

uv allows packages to be installed from Git and supports the following schemes for authenticating
with private repositories.

Using SSH:

- `git+ssh://git@<hostname>/...` (e.g., `git+ssh://git@github.com/astral-sh/uv`)
- `git+ssh://git@<host>/...` (e.g., `git+ssh://git@github.com-key-2/astral-sh/uv`)

See the
[GitHub SSH documentation](https://docs.github.com/en/authentication/connecting-to-github-with-ssh/about-ssh)
for more details on how to configure SSH.

Using a password or token:

- `git+https://<user>:<token>@<hostname>/...` (e.g.,
  `git+https://git:github_pat_asdf@github.com/astral-sh/uv`)
- `git+https://<token>@<hostname>/...` (e.g., `git+https://github_pat_asdf@github.com/astral-sh/uv`)
- `git+https://<user>@<hostname>/...` (e.g., `git+https://git@github.com/astral-sh/uv`)

When using a GitHub personal access token, the username is arbitrary. GitHub doesn't allow you to
use your account name and password in URLs like this, although other hosts may.

If there are no credentials present in the URL and authentication is needed, the
[Git credential helper](#git-credential-helpers) will be queried.

!!! important

    When using `uv add`, uv _will not_ persist Git credentials to the `pyproject.toml` or `uv.lock`.
    These files are often included in source control and distributions, so it is generally unsafe
    to include credentials in them.

    If you have a Git credential helper configured, your credentials may be automatically persisted,
    resulting in successful subsequent fetches of the dependency. However, if you do not have a Git
    credential helper or the project is used on a machine without credentials seeded, uv will fail to
    fetch the dependency.

    You _may_ force uv to persist Git credentials by passing the `--raw` option to `uv add`. However,
    we strongly recommend setting up a [credential helper](#git-credential-helpers) instead.

### Git credential helpers

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
    [GitHub Actions guide](../guides/integration/github.md#private-repos), the
    [`gh auth setup-git`](https://cli.github.com/manual/gh_auth_setup-git) command will need to be
    run afterwards to configure the credential helper.

## HTTP authentication

uv supports credentials over HTTP when querying package registries.

Authentication can come from the following sources, in order of precedence:

- The URL, e.g., `https://<user>:<password>@<hostname>/...`
- A [`.netrc`](https://everything.curl.dev/usingcurl/netrc) configuration file
- A [keyring](https://github.com/jaraco/keyring) provider (requires opt-in)

If authentication is found for a single index URL or net location (scheme, host, and port), it will
be cached for the duration of the command and used for other queries to that index or net location.
Authentication is not cached across invocations of uv.

`.netrc` authentication is enabled by default, and will respect the `NETRC` environment variable if
defined, falling back to `~/.netrc` if not.

To enable keyring-based authentication, pass the `--keyring-provider subprocess` command-line
argument to uv, or set `UV_KEYRING_PROVIDER=subprocess`.

Authentication may be used for hosts specified in the following contexts:

- `[index]`
- `index-url`
- `extra-index-url`
- `find-links`
- `package @ https://...`

See the [index authentication documentation](./indexes.md#authentication) for details on
authenticating index URLs.

See the [`pip` compatibility guide](../pip/compatibility.md#registry-authentication) for details on
differences from `pip`.

!!! important

    When using `uv add`, uv _will not_ persist index credentials to the `pyproject.toml` or `uv.lock`.
    These files are often included in source control and distributions, so it is generally unsafe
    to include credentials in them. However, uv _will_ persist credentials for direct URLs, i.e.,
    `package @ https://username:password:example.com/foo.whl`, as there is not currently a way to
    otherwise provide those credentials.

    If credentials were attached to an index URL during `uv add`, uv may fail to fetch dependencies
    from indexes which require authentication on subsequent operations. See the
    [index authentication documentation](./indexes.md#authentication) for details on persistent
    authentication for indexes.

## Authentication with alternative package indexes

See the [alternative indexes integration guide](../guides/integration/alternative-indexes.md) for
details on authentication with popular alternative Python package indexes.

## Custom CA certificates

By default, uv loads certificates from the bundled `webpki-roots` crate. The `webpki-roots` are a
reliable set of trust roots from Mozilla, and including them in uv improves portability and
performance (especially on macOS, where reading the system trust store incurs a significant delay).

However, in some cases, you may want to use the platform's native certificate store, especially if
you're relying on a corporate trust root (e.g., for a mandatory proxy) that's included in your
system's certificate store. To instruct uv to use the system's trust store, run uv with the
`--native-tls` command-line flag, or set the `UV_NATIVE_TLS` environment variable to `true`.

If a direct path to the certificate is required (e.g., in CI), set the `SSL_CERT_FILE` environment
variable to the path of the certificate bundle, to instruct uv to use that file instead of the
system's trust store.

If client certificate authentication (mTLS) is desired, set the `SSL_CLIENT_CERT` environment
variable to the path of the PEM formatted file containing the certificate followed by the private
key.

Finally, if you're using a setup in which you want to trust a self-signed certificate or otherwise
disable certificate verification, you can instruct uv to allow insecure connections to dedicated
hosts via the `allow-insecure-host` configuration option. For example, adding the following to
`pyproject.toml` will allow insecure connections to `example.com`:

```toml
[tool.uv]
allow-insecure-host = ["example.com"]
```

`allow-insecure-host` expects to receive a hostname (e.g., `localhost`) or hostname-port pair (e.g.,
`localhost:8080`), and is only applicable to HTTPS connections, as HTTP connections are inherently
insecure.

Use `allow-insecure-host` with caution and only in trusted environments, as it can expose you to
security risks due to the lack of certificate verification.
