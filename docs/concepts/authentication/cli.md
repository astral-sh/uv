# The `uv auth` CLI

uv provides a high-level interface for storing and retrieving credentials from services.

## Logging in to a service

To add credentials for service, use the `uv auth login` command:

```console
$ uv auth login example.com
```

This will prompt for the credentials.

The credentials can also be provided using the `--username` and `--password` options, or the
`--token` option for services which use a `__token__` or arbitrary username.

For example, to store username and password credentials:

```console
$ uv auth login --username foo --password bar example.com
```

To store a token instead:

```console
$ uv auth login --token pypi-... example.com
```

!!! note

    We recommend providing the secret via stdin. Use `-` to indicate the value should be read from
    stdin, e.g., for `--password`:

    ```console
    $ echo 'my-password' | uv auth login example.com --password -
    ```

    The same pattern can be used with `--token`.

Once credentials are added, uv will use them for packaging operations that require fetching content
from the given service. At this time, only HTTPS Basic authentication is supported. The credentials
will not yet be used for Git requests.

!!! note

    The credentials will not be validated, i.e., incorrect credentials will not fail.

## Logging out of a service

To remove credentials, use the `uv auth logout` command:

For username and password credentials, provide the username that was used to log in:

```console
$ uv auth logout --username foo example.com
```

For token-based credentials, no username is required:

```console
$ uv auth logout example.com
```

When `--username` is omitted, uv first tries to remove token credentials for the service. If the
plaintext credential store is in use, uv can also remove username and password credentials when only
one set of credentials exists for the service. If multiple usernames match the same service, uv will
ask you to specify the credentials to remove with `--username`.

!!! note

    The credentials will not be invalidated with the remote server, i.e., they will only be removed
    from local storage not rendered unusable.

## Showing credentials for a service

To show the credential stored for a given URL, use the `uv auth token` command:

For token-based credentials:

```console
$ uv auth token example.com
```

If a username was used to log in, it must be provided as well:

```console
$ uv auth token --username foo example.com
```

For example, these commands line up with the corresponding login flows:

```console
$ uv auth login --token pypi-... example.com
$ uv auth token example.com
```

```console
$ uv auth login --username foo --password bar example.com
$ uv auth token --username foo example.com
```

## Configuring the storage backend

Credentials are persisted to the uv [credentials store](./http.md#the-uv-credentials-store).

By default, credentials are written to a plaintext file. An encrypted system-native storage backend
can be enabled with `UV_PREVIEW_FEATURES=native-auth`.
