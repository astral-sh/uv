# uv-keyring

This is vendored from [keyring-rs crate](https://github.com/open-source-cooperative/keyring-rs)
commit 9635a2f53a19eb7f188cdc4e38982dcb19caee00.

A cross-platform library to manage storage and retrieval of passwords (and other secrets) in the
underlying platform secure store, with a fully-developed example that provides a command-line
interface.

## Usage

You can use the `Entry::new` function to create a new keyring entry. The `new` function takes a
service name and a user's name which together identify the entry.

Passwords (strings) or secrets (binary data) can be added to an entry using its `set_password` or
`set_secret` methods, respectively. (These methods create or update an entry in the underlying
platform's persistent credential store.) The password or secret can then be read back using the
`get_password` or `get_secret` methods. The underlying credential (with its password/secret data)
can then be removed using the `delete_credential` method.

```rust
use keyring::{Entry, Result};

fn main() -> Result<()> {
    let entry = Entry::new("my-service", "my-name")?;
    entry.set_password("topS3cr3tP4$$w0rd").await?;
    let password = entry.get_password().await?;
    println!("My password is '{}'", password);
    entry.delete_credential().await?;
    Ok(())
}
```

## Errors

Creating and operating on entries can yield a `keyring::Error` which provides both a
platform-independent code that classifies the error and, where relevant, underlying platform errors
or more information about what went wrong.

## Platforms

This crate provides built-in implementations of the following platform-specific credential stores:

- _Linux_, _FreeBSD_, _OpenBSD_: The DBus-based Secret Service.
- _macOS_: Keychain Services.
- _Windows_: The Windows Credential Manager.

It can be built and used on other platforms, but will not provide a built-in credential store
implementation; you will have to bring your own.

### Platform-specific issues

If you use the _Secret Service_ as your credential store, be aware that every call to the Secret
Service is done via an inter-process call, which takes time (typically tens if not hundreds of
milliseconds).

If you use the _Windows-native credential store_, be careful about multi-threaded access, because
the Windows credential store does not guarantee your calls will be serialized in the order they are
made. Always access any single credential from just one thread at a time, and if you are doing
operations on multiple credentials that require a particular serialization order, perform all those
operations from the same thread.

The _macOS credential store_ does not allow service names or usernames to be empty, because empty
fields are treated as wildcards on lookup. Use some default, non-empty value instead.
