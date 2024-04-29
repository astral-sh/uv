//! Vendored from cachedir 0.3.1 to replace `std::fs` with `fs_err`.

use std::io::Write;
use std::{io, path};

use cachedir::HEADER;

/// Adds a tag to the specified `directory`.
///
/// Will return an error if:
///
/// * The `directory` exists and contains a `CACHEDIR.TAG` file, regardless of its content.
/// * The file can't be created for any reason (the `directory` doesn't exist, permission error,
///   can't write to the file etc.)
pub fn add_tag<P: AsRef<path::Path>>(directory: P) -> io::Result<()> {
    let directory = directory.as_ref();
    match fs_err::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(directory.join("CACHEDIR.TAG"))
    {
        Ok(mut cachedir_tag) => cachedir_tag.write_all(HEADER),
        Err(e) => Err(e),
    }
}

/// Ensures the tag exists in `directory`.
///
/// This function considers the `CACHEDIR.TAG` file in `directory` existing, regardless of its
/// content, as a success.
///
/// Will return an error if The tag file doesn't exist and can't be created for any reason
/// (the `directory` doesn't exist, permission error, can't write to the file etc.).
pub fn ensure_tag<P: AsRef<path::Path>>(directory: P) -> io::Result<()> {
    match add_tag(directory) {
        Err(e) => match e.kind() {
            io::ErrorKind::AlreadyExists => Ok(()),
            _ => Err(e),
        },
        other => other,
    }
}
