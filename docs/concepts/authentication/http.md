# HTTP credentials

uv supports credentials over HTTP when querying package registries.

Authentication can come from the following sources, in order of precedence:

- The URL, e.g., `https://<user>:<password>@<hostname>/...`
- A [`.netrc`](https://everything.curl.dev/usingcurl/netrc) configuration file
- A [keyring](https://github.com/jaraco/keyring) provider (requires opt-in)

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

## Persistence of credentials

If authentication is found for a single index URL or net location (scheme, host, and port), it will
be cached for the duration of the command and used for other queries to that index or net location.
Authentication is not cached across invocations of uv.

When using `uv add`, uv _will not_ persist index credentials to the `pyproject.toml` or `uv.lock`.
These files are often included in source control and distributions, so it is generally unsafe to
include credentials in them. However, uv _will_ persist credentials for direct URLs, i.e.,
`package @ https://username:password:example.com/foo.whl`, as there is not currently a way to
otherwise provide those credentials.

If credentials were attached to an index URL during `uv add`, uv may fail to fetch dependencies from
indexes which require authentication on subsequent operations. See the
[index authentication documentation](../indexes.md#authentication) for details on persistent
authentication for indexes.

## Learn more

See the [index authentication documentation](../indexes.md#authentication) for details on
authenticating index URLs.

See the [`pip` compatibility guide](../../pip/compatibility.md#registry-authentication) for details
on differences from `pip`.
