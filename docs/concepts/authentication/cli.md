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

```console
$ uv auth logout example.com
```

!!! note

    The credentials will not be invalidated with the remote server, i.e., they will only be removed
    from local storage not rendered unusable.

## Showing credentials for a service

To show the credential stored for a given URL, use the `uv auth token` command:

```console
$ uv auth token example.com
```

If a username was used to log in, it will need to be provided as well, e.g.:

```console
$ uv auth token --username foo example.com
```

## Configuring the storage backend

Credentials are persisted to the uv [credentials store](./http.md#the-uv-credentials-store).

By default, credentials are written to a plaintext file. An encrypted system-native storage backend
can be enabled with `UV_PREVIEW_FEATURES=native-auth`.
