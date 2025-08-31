# TLS certificates

By default, uv loads certificates from the bundled `webpki-roots` crate. The `webpki-roots` are a
reliable set of trust roots from Mozilla, and including them in uv improves portability and
performance (especially on macOS, where reading the system trust store incurs a significant delay).

## System certificates

In some cases, you may want to use the platform's native certificate store, especially if you're
relying on a corporate trust root (e.g., for a mandatory proxy) that's included in your system's
certificate store. To instruct uv to use the system's trust store, run uv with the `--native-tls`
command-line flag, or set the `UV_NATIVE_TLS` environment variable to `true`.

## Custom certificates

If a direct path to the certificate is required (e.g., in CI), set the `SSL_CERT_FILE` environment
variable to the path of the certificate bundle, to instruct uv to use that file instead of the
system's trust store.

If client certificate authentication (mTLS) is desired, set the `SSL_CLIENT_CERT` environment
variable to the path of the PEM formatted file containing the certificate followed by the private
key.

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
