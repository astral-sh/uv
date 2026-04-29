use std::borrow::Cow;
use std::io::stdout;
use std::path::Path;

use anstream::AutoStream;

/// A multicasting writer that writes to both the standard output and an output file, if present.
pub struct OutputWriter<'a> {
    stdout: Option<AutoStream<std::io::Stdout>>,
    output_file: Option<&'a Path>,
    buffer: Vec<u8>,
}

impl<'a> OutputWriter<'a> {
    /// Create a new output writer.
    pub fn new(include_stdout: bool, output_file: Option<&'a Path>) -> Self {
        let stdout = include_stdout.then(|| AutoStream::<std::io::Stdout>::auto(stdout()));
        Self {
            stdout,
            output_file,
            buffer: Vec::new(),
        }
    }

    /// Commit the buffer to the output file.
    pub async fn commit(self) -> std::io::Result<()> {
        if let Some(output_file) = self.output_file {
            if let Some(parent_dir) = output_file.parent() {
                fs_err::create_dir_all(parent_dir)?;
            }

            // If the output file is an existing symlink, write to the destination instead.
            let output_file = fs_err::read_link(output_file)
                .map(Cow::Owned)
                .unwrap_or(Cow::Borrowed(output_file));
            let stream = anstream::adapter::strip_bytes(&self.buffer).into_vec();
            uv_fs::write_atomic(output_file, &stream).await?;
        }
        Ok(())
    }
}

impl std::io::Write for OutputWriter<'_> {
    /// Write to both standard output and the output buffer, if present.
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        // Write to the buffer.
        if self.output_file.is_some() {
            self.buffer.write_all(buf)?;
        }

        // Write to standard output.
        if let Some(stdout) = &mut self.stdout {
            stdout.write_all(buf)?;
        }

        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        if let Some(stdout) = &mut self.stdout {
            stdout.flush()?;
        }
        Ok(())
    }
}
