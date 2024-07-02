# Authentication

## Git authentication

uv allows packages to be installed from Git and supports the following schemes for authenticating with private
repositories.

Using SSH:

- `git+ssh://git@<hostname>/...` (e.g. `git+ssh://git@github.com/astral-sh/uv`)
- `git+ssh://git@<host>/...` (e.g. `git+ssh://git@github.com-key-2/astral-sh/uv`)

See the [GitHub SSH documentation](https://docs.github.com/en/authentication/connecting-to-github-with-ssh/about-ssh) for more details on how to configure SSH.

Using a password or token:

- `git+https://<user>:<token>@<hostname>/...` (e.g. `git+https://git:github_pat_asdf@github.com/astral-sh/uv`)
- `git+https://<token>@<hostname>/...` (e.g. `git+https://github_pat_asdf@github.com/astral-sh/uv`)
- `git+https://<user>@<hostname>/...` (e.g. `git+https://git@github.com/astral-sh/uv`)

When using a GitHub personal access token, the username is arbitrary. GitHub does not support logging in with password directly, although other hosts may. If a username is provided without credentials, you will be prompted to enter them.

If there are no credentials present in the URL and authentication is needed, the [Git credential helper](https://git-scm.com/doc/credential-helpers) will be queried.

## HTTP authentication

uv supports credentials over HTTP when querying package registries.

Authentication can come from the following sources, in order of precedence:

- The URL, e.g., `https://<user>:<password>@<hostname>/...`
- A [`netrc`](https://everything.curl.dev/usingcurl/netrc) configuration file
- A [keyring](https://github.com/jaraco/keyring) provider (requires opt-in)

If authentication is found for a single net location (scheme, host, and port), it will be cached for the duration
of the command and used for other queries to that net location. Authentication is not cached across invocations of
uv.

Note `--keyring-provider subprocess` or `UV_KEYRING_PROVIDER=subprocess` must be provided to enable keyring-based
authentication.

Authentication may be used for hosts specified in the following contexts:

- `index-url`
- `extra-index-url`
- `find-links`
- `package @ https://...`

See the [`pip` compatibility guide](./pip/compatibility.md#registry-authentication) for details on differences from
`pip`.

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
variable to the path of the PEM formatted file containing the certificate followed by the private key.
