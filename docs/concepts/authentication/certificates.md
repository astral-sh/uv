# TLS certificates

uv uses TLS to securely communicate with package indexes and other HTTPS servers. TLS certificates
are used to verify the identity of these servers, ensuring that connections are not intercepted.

## TLS backend

uv uses [`rustls`](https://github.com/rustls/rustls), a memory-safe TLS implementation written in
Rust, with [`aws-lc-rs`](https://github.com/aws/aws-lc-rs) as the cryptography provider.

uv supports the following X.509 certificate signature algorithms:

- ECDSA (P-256, P-384, P-521) with SHA-256, SHA-384, or SHA-512
- Ed25519
- RSA PKCS#1 v1.5 (2048–8192 bit) with SHA-256, SHA-384, or SHA-512
- RSA-PSS (2048–8192 bit) with SHA-256, SHA-384, or SHA-512

## System certificates

By default, uv uses bundled Mozilla root certificates for TLS verification. In some cases, you may
want to use the platform's native certificate store instead — for example, if you're relying on a
corporate trust root (e.g., for a mandatory proxy) that's included in your system's certificate
store.

To use system certificates, pass the [`--system-certs`](../../reference/cli.md#uv) flag, set the
[`UV_SYSTEM_CERTS`](../../reference/environment.md#uv_system_certs) environment variable to `true`,
or set [`system-certs = true`](../../reference/settings.md#system-certs) in `uv.toml`.

When using system certificates, certificate verification is performed by
[`rustls-platform-verifier`](https://github.com/rustls/rustls-platform-verifier), which delegates to
the operating system's certificate verifier.

On Linux, the platform verifier discovers CA bundles from the filesystem (via `rustls-native-certs`
/ `openssl-probe`). In minimal environments where the system CA bundle is absent (e.g., scratch
containers), uv automatically merges the bundled Mozilla root certificates alongside the platform
verifier to ensure basic connectivity. On macOS and Windows, the platform's built-in trust store is
always available, so no additional roots are merged.

## Custom certificates

To use custom CA certificates, set the
[`SSL_CERT_FILE`](../../reference/environment.md#ssl_cert_file) environment variable to the path of
a PEM-encoded certificate bundle (e.g., `certs.pem`, `ca-bundle.crt`), or set
[`SSL_CERT_DIR`](../../reference/environment.md#ssl_cert_dir) to one or more directories containing
PEM-encoded certificate files. Multiple entries are supported, separated using a platform-specific
delimiter (`:` on Unix, `;` on Windows).

Certificates are usually stored with `.pem`, `.crt`, or `.cer` extensions, but uv will attempt to
read a certificate from any regular file in the provided `SSL_CERT_DIR`.

Files that cannot be parsed as PEM certificates are ignored. uv resolves symlinks and ignores
dangling symlinks.

DER-encoded files are not supported.

When set, these environment variables **override** the default certificate source entirely — only
the provided certificates will be trusted.

`SSL_CERT_FILE` can point to a single certificate or a bundle containing multiple certificates.
`SSL_CERT_DIR` can include multiple directory entries; uv will load all valid certificates from each
directory.

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
