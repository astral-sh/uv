# TLS certificates

By default, uv uses the rustls TLS backend with platform-verifier, which loads certificates from the
operating system's certificate store. This provides a balance of security, performance, and
compatibility with corporate environments that use custom root certificates.

## System certificates

In some cases, you may want to use the platform's native certificate store, especially if you're
relying on a corporate trust root (e.g., for a mandatory proxy) that's included in your system's
certificate store. To instruct uv to use the system's trust store, run uv with the `--native-tls`
command-line flag, or set the `UV_NATIVE_TLS` environment variable to `true`.

## Custom certificates

To use custom CA certificates, you can set the `SSL_CERT_FILE` environment variable to the path of a
certificate bundle (PEM format), or set `SSL_CERT_DIR` to a directory containing certificate files
(`.pem`, `.crt`, or `.cer` extensions).

When using the default TLS backend (rustls), these custom certificates are merged with the
platform's certificate store loaded via `rustls-platform-verifier`. When using `--native-tls`, the
custom certificates are used alongside the certificates loaded from the platform's native
certificate store.

The `SSL_CERT_FILE` can point to a single certificate or a bundle containing multiple certificates.
The `SSL_CERT_DIR` can contain multiple certificate files, and uv will load all valid certificates
from the directory.

If client certificate authentication (mTLS) is desired, set the `SSL_CLIENT_CERT` environment
variable to the path of a PEM formatted file containing the certificate followed by the private key.

## Insecure hosts

If you're using a setup in which you want to trust a self-signed certificate or otherwise disable
certificate verification, you can instruct uv to allow insecure connections to dedicated hosts via
the `allow-insecure-host` configuration option. For example, adding the following to
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
