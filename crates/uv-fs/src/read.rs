use std::io::{self, Read};
use std::path::Path;

/// Read a UTF-8 text file if it starts with `prefix`.
///
/// If the file does not start with `prefix`, no bytes after the prefix are read. If the prefix
/// matches, the rest of the file is read incrementally and rejected as soon as it contains a NUL
/// byte or invalid UTF-8.
pub fn read_utf_8_file_if_starts_with(
    path: impl AsRef<Path>,
    prefix: &str,
) -> io::Result<Option<Vec<u8>>> {
    let mut file = fs_err::File::open(path.as_ref())?;
    read_utf_8_if_starts_with(&mut file, prefix)
}

fn read_utf_8_if_starts_with(mut reader: impl Read, prefix: &str) -> io::Result<Option<Vec<u8>>> {
    const READ_BUFFER_SIZE: usize = 8 * 1024;

    if prefix.as_bytes().contains(&0) {
        return Ok(None);
    }

    let mut contents = vec![0; prefix.len()];
    match reader.read_exact(&mut contents) {
        Ok(()) if contents == prefix.as_bytes() => {}
        Ok(()) => return Ok(None),
        Err(err) if err.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(err) => return Err(err),
    }

    let mut valid_utf_8_len = contents.len();
    let mut buffer = [0u8; READ_BUFFER_SIZE];
    loop {
        let count = match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => count,
            Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
            Err(err) => return Err(err),
        };

        let chunk = &buffer[..count];
        if chunk.contains(&0) {
            return Ok(None);
        }
        contents.extend_from_slice(chunk);
        match std::str::from_utf8(&contents[valid_utf_8_len..]) {
            Ok(_) => valid_utf_8_len = contents.len(),
            Err(err) if err.error_len().is_some() => return Ok(None),
            Err(err) => valid_utf_8_len += err.valid_up_to(),
        }
    }

    Ok((valid_utf_8_len == contents.len()).then_some(contents))
}

#[cfg(test)]
mod tests {
    use std::io::{self, Cursor, Read};

    use super::read_utf_8_if_starts_with;

    struct ErrorReader;

    impl Read for ErrorReader {
        fn read(&mut self, _buffer: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("read past rejected content"))
        }
    }

    #[test]
    fn stops_at_prefix_mismatch() -> io::Result<()> {
        let reader = Cursor::new(b"# ").chain(ErrorReader);
        assert!(read_utf_8_if_starts_with(reader, "#!")?.is_none());
        Ok(())
    }

    #[test]
    fn stops_at_binary_content() -> io::Result<()> {
        for marker in [0, 0xff] {
            let reader = Cursor::new([b'#', b'!', marker]).chain(ErrorReader);
            assert!(read_utf_8_if_starts_with(reader, "#!")?.is_none());
        }
        Ok(())
    }

    #[test]
    fn handles_split_utf_8() -> io::Result<()> {
        let reader = Cursor::new(b"#!\xc3").chain(Cursor::new(b"\xa9"));
        assert_eq!(
            read_utf_8_if_starts_with(reader, "#!")?,
            Some(b"#!\xc3\xa9".to_vec())
        );
        Ok(())
    }
}
