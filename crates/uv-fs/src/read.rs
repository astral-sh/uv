use std::io::{self, Read};

const READ_BUFFER_SIZE: usize = 8 * 1024;

/// A reader that validates its contents while reading.
#[must_use]
pub struct ValidatedReader<Reader> {
    reader: Reader,
    prefix: Option<String>,
    require_utf8: bool,
}

impl<Reader> ValidatedReader<Reader> {
    /// Create a new validated reader.
    pub fn new(reader: Reader) -> Self {
        Self {
            reader,
            prefix: None,
            require_utf8: false,
        }
    }

    /// Require the contents to start with `prefix`.
    ///
    /// If the prefix does not match, [`Self::read`] does not read any bytes after the prefix.
    pub fn require_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.prefix = Some(prefix.into());
        self
    }

    /// Require the contents to be valid UTF-8 text.
    ///
    /// The contents are read incrementally and rejected as soon as a NUL byte or invalid UTF-8 is
    /// encountered.
    pub fn require_utf8(mut self) -> Self {
        self.require_utf8 = true;
        self
    }
}

impl<Reader: Read> ValidatedReader<Reader> {
    /// Read the contents, returning `None` if a configured validation fails.
    pub fn read(self) -> io::Result<Option<Vec<u8>>> {
        let Self {
            mut reader,
            prefix,
            require_utf8,
        } = self;

        let mut contents = if let Some(prefix) = prefix {
            let mut contents = vec![0; prefix.len()];
            match reader.read_exact(&mut contents) {
                Ok(()) if contents == prefix.as_bytes() => {}
                Ok(()) => return Ok(None),
                Err(err) if err.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
                Err(err) => return Err(err),
            }
            contents
        } else {
            Vec::new()
        };

        if !require_utf8 {
            reader.read_to_end(&mut contents)?;
            return Ok(Some(contents));
        }

        if contents.contains(&0) {
            return Ok(None);
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
}

#[cfg(test)]
mod tests {
    use std::io::{self, Cursor, Read};

    use super::ValidatedReader;

    struct ErrorReader;

    impl Read for ErrorReader {
        fn read(&mut self, _buffer: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("read past rejected content"))
        }
    }

    #[test]
    fn stops_at_prefix_mismatch() -> io::Result<()> {
        let reader = Cursor::new(b"# ").chain(ErrorReader);
        assert!(
            ValidatedReader::new(reader)
                .require_prefix("#!")
                .read()?
                .is_none()
        );
        Ok(())
    }

    #[test]
    fn reads_without_utf_8_validation() -> io::Result<()> {
        assert_eq!(
            ValidatedReader::new(Cursor::new(b"#!\xff"))
                .require_prefix("#!")
                .read()?,
            Some(b"#!\xff".to_vec())
        );
        Ok(())
    }

    #[test]
    fn stops_at_binary_content() -> io::Result<()> {
        for marker in [0, 0xff] {
            let reader = Cursor::new([b'#', b'!', marker]).chain(ErrorReader);
            assert!(
                ValidatedReader::new(reader)
                    .require_prefix("#!")
                    .require_utf8()
                    .read()?
                    .is_none()
            );
        }
        Ok(())
    }

    #[test]
    fn handles_split_utf_8() -> io::Result<()> {
        let reader = Cursor::new(b"#!\xc3").chain(Cursor::new(b"\xa9"));
        assert_eq!(
            ValidatedReader::new(reader)
                .require_prefix("#!")
                .require_utf8()
                .read()?,
            Some(b"#!\xc3\xa9".to_vec())
        );
        Ok(())
    }
}
