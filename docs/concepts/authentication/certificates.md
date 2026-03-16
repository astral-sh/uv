# TLS certificates

uv uses TLS to securely communicate with package indexes and other HTTPS servers. TLS certificates
are used to verify the identity of these servers, ensuring that connections are not intercepted.

## TLS backend

The [`--tls-backend`](../../reference/cli.md#uv) flag controls which TLS implementation uv uses:

- **[`rustls`](https://github.com/rustls/rustls)** (default): A memory-safe TLS implementation.
- **`native`**: The platform's native TLS implementation, wrapped by the
  [`native-tls`](https://docs.rs/native-tls/latest/native_tls/) crate (SChannel on Windows, Secure
  Transport on macOS, OpenSSL on Linux).

When using `rustls`, uv uses [`aws-lc-rs`](https://github.com/aws/aws-lc-rs) as the underlying
cryptography provider. `aws-lc-rs` is a Rust wrapper around
[AWS-LC](https://github.com/aws/aws-lc), a general-purpose cryptographic library maintained by AWS.

## System certificates

By default, uv uses bundled Mozilla root certificates for TLS verification. In some cases, you may
want to use the platform's native certificate store instead — for example, if you're relying on a
corporate trust root (e.g., for a mandatory proxy) that's included in your system's certificate
store.

To use system certificates, pass the [`--system-certs`](../../reference/cli.md#uv) flag or set the
[`UV_SYSTEM_CERTS`](../../reference/environment.md#uv_system_certs) environment variable to `true`.

When using system certificates with the `rustls` backend, certificate verification is performed by
[`rustls-platform-verifier`](https://github.com/rustls/rustls-platform-verifier), which delegates
to the operating system's certificate verifier. When using the `native` backend, the platform's TLS
implementation handles verification directly.

## Custom certificates

To use custom CA certificates, you can set the
[`SSL_CERT_FILE`](../../reference/environment.md#ssl_cert_file) environment variable to the path of a
certificate bundle (PEM format), or set
[`SSL_CERT_DIR`](../../reference/environment.md#ssl_cert_dir) to a directory containing certificate
files (`.pem`, `.crt`, or `.cer` extensions).

Custom certificates are merged with the certificate store used by the active configuration, e.g.,
when using system certificates (via `--system-certs`), they are layered on top of the platform's
certificate store.

`SSL_CERT_FILE` can point to a single certificate or a bundle containing multiple certificates.
`SSL_CERT_DIR` can contain multiple certificate files, and uv will load all valid certificates from
the directory.

If client certificate authentication (mTLS) is desired, set the
[`SSL_CLIENT_CERT`](../../reference/environment.md#ssl_client_cert) environment variable to the path
of a PEM formatted file containing the certificate followed by the private key.

## Insecure hosts

If you're using a setup in which you want to trust a self-signed certificate or otherwise disable
certificate verification, you can instruct uv to allow insecure connections to dedicated hosts via
the [`allow-insecure-host`](../../reference/settings.md#allow-insecure-host) configuration option.
For example, adding the following to `pyproject.toml` will allow insecure connections to
`example.com`:

```toml
[tool.uv]
allow-insecure-host = ["example.com"]
```

`allow-insecure-host` expects to receive a hostname (e.g., `localhost`) or hostname-port pair (e.g.,
`localhost:8080`), and is only applicable to HTTPS connections, as HTTP connections are inherently
insecure.

Use `allow-insecure-host` with caution and only in trusted environments, as it can expose you to
security risks due to the lack of certificate verification.
