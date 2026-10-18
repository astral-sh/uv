// Copyright 2022 Google LLC

// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![expect(clippy::cast_sign_loss)]

use std::{
    io::{BufRead, Read, Seek, SeekFrom},
    sync::Arc,
};

#[cfg(unix)]
use fs_err::os::unix::fs::FileExt;
#[cfg(windows)]
use fs_err::os::windows::fs::FileExt;

// Chosen from extraction benchmarks to reduce read calls without adding too
// much per-clone buffering.
const BUFFER_SIZE: usize = 64 * 1024;

/// A [`Read`] which refers to its underlying stream by reference count,
/// and thus can be cloned cheaply. It supports seeking; each cloned instance
/// maintains its own position and uses positioned reads so clones can read
/// concurrently without sharing a file cursor.
pub(crate) struct CloneableSeekableReader {
    file: Arc<fs_err::File>,
    pos: u64,
    // TODO determine and store this once instead of per cloneable file
    file_length: Option<u64>,
    buffer: Box<[u8; BUFFER_SIZE]>,
    buffer_position: usize,
    buffer_length: usize,
}

impl Clone for CloneableSeekableReader {
    fn clone(&self) -> Self {
        Self {
            file: self.file.clone(),
            pos: self.pos,
            file_length: self.file_length,
            buffer: Box::new([0; BUFFER_SIZE]),
            buffer_position: 0,
            buffer_length: 0,
        }
    }
}

impl CloneableSeekableReader {
    /// Constructor. Takes ownership of the underlying file.
    /// You should pass in only files whose total length you expect
    /// to be fixed and unchanging. Odd behavior may occur if the length
    /// of the stream changes; any subsequent seeks will not take account
    /// of the changed stream length.
    pub(crate) fn new(file: fs_err::File) -> Self {
        Self {
            file: Arc::new(file),
            pos: 0u64,
            file_length: None,
            buffer: Box::new([0; BUFFER_SIZE]),
            buffer_position: 0,
            buffer_length: 0,
        }
    }

    /// Determine the length of the underlying stream.
    fn ascertain_file_length(&mut self) -> std::io::Result<u64> {
        if let Some(len) = self.file_length {
            return Ok(len);
        }
        let len = self.file.metadata()?.len();
        self.file_length = Some(len);
        Ok(len)
    }

    fn buffered_len(&self) -> usize {
        self.buffer_length.saturating_sub(self.buffer_position)
    }

    fn consume_buffer(&mut self, amount: usize) {
        let amount = amount.min(self.buffered_len());
        self.buffer_position += amount;
        self.pos += amount as u64;

        if self.buffer_position == self.buffer_length {
            self.buffer_position = 0;
            self.buffer_length = 0;
        }
    }

    fn clear_buffer(&mut self) {
        self.buffer_position = 0;
        self.buffer_length = 0;
    }
}

impl Read for CloneableSeekableReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.buffered_len() > 0 {
            let amount = buf.len().min(self.buffered_len());
            buf[..amount]
                .copy_from_slice(&self.buffer[self.buffer_position..self.buffer_position + amount]);
            self.consume_buffer(amount);
            return Ok(amount);
        }

        let read_result = read_at(&self.file, buf, self.pos);
        if let Ok(bytes_read) = read_result {
            // TODO, once stabilised, use checked_add_signed
            self.pos += bytes_read as u64;
        }
        read_result
    }
}

impl Seek for CloneableSeekableReader {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        let new_pos = match pos {
            SeekFrom::Start(pos) => pos,
            SeekFrom::End(offset_from_end) => {
                let file_len = self.ascertain_file_length()?;
                if -offset_from_end as u64 > file_len {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "Seek too far backwards",
                    ));
                }
                // TODO, once stabilised, use checked_add_signed
                file_len - (-offset_from_end as u64)
            }
            // TODO, once stabilised, use checked_add_signed
            SeekFrom::Current(offset_from_pos) => {
                if offset_from_pos > 0 {
                    self.pos + (offset_from_pos as u64)
                } else {
                    self.pos - ((-offset_from_pos) as u64)
                }
            }
        };
        self.pos = new_pos;
        self.clear_buffer();
        Ok(new_pos)
    }
}

impl BufRead for CloneableSeekableReader {
    fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
        if self.buffered_len() == 0 {
            self.buffer_length = read_at(&self.file, &mut *self.buffer, self.pos)?;
            self.buffer_position = 0;
        }

        Ok(&self.buffer[self.buffer_position..self.buffer_length])
    }

    fn consume(&mut self, amount: usize) {
        self.consume_buffer(amount);
    }
}

/// Read at an explicit offset without relying on the shared file cursor.
fn read_at(file: &fs_err::File, buf: &mut [u8], offset: u64) -> std::io::Result<usize> {
    // File offsets are signed. On Windows, negative offsets can also select the shared
    // cursor instead of an explicit position, so reject them before calling `seek_read`.
    if offset > i64::MAX as u64 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Seek position exceeds the maximum file offset",
        ));
    }
    #[cfg(unix)]
    {
        file.read_at(buf, offset)
    }
    #[cfg(windows)]
    {
        file.seek_read(buf, offset)
    }
}

#[cfg(test)]
mod test {
    use std::{
        io::{BufRead, Read, Seek, SeekFrom},
        sync::Barrier,
        thread,
    };

    use super::{BUFFER_SIZE, CloneableSeekableReader};

    #[test]
    fn test_cloneable_seekable_reader() -> std::io::Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let path = temp_dir.path().join("archive.zip");
        fs_err::write(&path, [0, 1, 2, 3, 4, 5, 6, 7, 8, 9])?;
        let mut reader = CloneableSeekableReader::new(fs_err::File::open(path)?);
        let mut out = vec![0; 2];
        assert!(reader.read_exact(&mut out).is_ok());
        assert_eq!(out[0], 0);
        assert_eq!(out[1], 1);
        assert!(reader.seek(SeekFrom::Start(0)).is_ok());
        assert!(reader.read_exact(&mut out).is_ok());
        assert_eq!(out[0], 0);
        assert_eq!(out[1], 1);
        assert!(reader.stream_position().is_ok());
        assert!(reader.read_exact(&mut out).is_ok());
        assert_eq!(out[0], 2);
        assert_eq!(out[1], 3);
        assert!(reader.seek(SeekFrom::End(-2)).is_ok());
        assert!(reader.read_exact(&mut out).is_ok());
        assert_eq!(out[0], 8);
        assert_eq!(out[1], 9);
        assert!(reader.read_exact(&mut out).is_err());

        // These positions must not become negative Windows file-offset sentinels.
        for position in [i64::MAX as u64 + 1, u64::MAX - 1, u64::MAX] {
            reader.seek(SeekFrom::Start(position))?;
            assert_eq!(
                reader.read(&mut out).unwrap_err().kind(),
                std::io::ErrorKind::InvalidInput
            );
            assert_eq!(
                reader.fill_buf().unwrap_err().kind(),
                std::io::ErrorKind::InvalidInput
            );
        }
        Ok(())
    }

    #[test]
    fn test_clones_have_independent_buffers_and_positions() -> std::io::Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let path = temp_dir.path().join("archive.zip");
        let contents: Vec<u8> = (0..=255).cycle().take(2 * BUFFER_SIZE + 17).collect();
        fs_err::write(&path, &contents)?;
        let mut reader = CloneableSeekableReader::new(fs_err::File::open(path)?);

        assert_eq!(reader.fill_buf()?, &contents[..BUFFER_SIZE]);
        reader.consume(13);
        let mut cloned = reader.clone();

        // A clone starts at the consumed position, not the end of the read-ahead buffer.
        let mut output = [0; 7];
        cloned.read_exact(&mut output)?;
        assert_eq!(output, contents[13..20]);
        reader.read_exact(&mut output)?;
        assert_eq!(output, contents[13..20]);

        // Filling a clone's buffer must not disturb the original reader's next refill.
        cloned.seek(SeekFrom::Start((BUFFER_SIZE + 3) as u64))?;
        assert_eq!(
            cloned.fill_buf()?,
            &contents[BUFFER_SIZE + 3..2 * BUFFER_SIZE + 3]
        );
        let mut remainder = Vec::new();
        reader.read_to_end(&mut remainder)?;
        assert_eq!(remainder, contents[20..]);

        // Seeking discards the old buffer, including when the next read reaches EOF.
        cloned.seek(SeekFrom::End(-7))?;
        assert_eq!(cloned.fill_buf()?, &contents[contents.len() - 7..]);
        cloned.consume(7);
        assert!(cloned.fill_buf()?.is_empty());
        assert_eq!(cloned.read(&mut output)?, 0);
        Ok(())
    }

    #[test]
    fn test_concurrent_clones_read_distinct_offsets() -> std::io::Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let path = temp_dir.path().join("archive.zip");
        let contents: Vec<u8> = (0..8)
            .flat_map(|value| std::iter::repeat_n(value, 4096))
            .collect();
        fs_err::write(&path, &contents)?;
        let reader = CloneableSeekableReader::new(fs_err::File::open(path)?);
        let barrier = Barrier::new(8);

        thread::scope(|scope| {
            let threads: Vec<_> = (0..8)
                .map(|index| {
                    let mut reader = reader.clone();
                    let contents = &contents;
                    let barrier = &barrier;
                    scope.spawn(move || -> std::io::Result<()> {
                        let mut output = [0; 4096];
                        barrier.wait();
                        for iteration in 0..32 {
                            reader.seek(SeekFrom::Start((index * 4096) as u64))?;
                            if iteration % 2 == 0 {
                                reader.read_exact(&mut output)?;
                                assert_eq!(output, contents[index * 4096..(index + 1) * 4096]);
                            } else {
                                assert_eq!(reader.fill_buf()?, &contents[index * 4096..]);
                                reader.consume(4096);
                            }
                        }
                        Ok(())
                    })
                })
                .collect();
            for thread in threads {
                thread
                    .join()
                    .map_err(|_| std::io::Error::other("reader thread panicked"))??;
            }
            Ok(())
        })
    }
}
