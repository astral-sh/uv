use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures::future::{AbortHandle, Aborted};
use futures::io::AsyncRead;

/// Check cancellation between reads, including reads of already-buffered, decompressed data.
///
/// With an abort handle, each read is bounded to limit work between checks. This adapter does not
/// register a waker; pair it with [`futures::future::Abortable`] to wake a pending read on cancellation.
pub(super) struct AbortReader<'a, R> {
    reader: R,
    abort: Option<&'a AbortHandle>,
}

impl<'a, R> AbortReader<'a, R> {
    pub(super) fn new(reader: R, abort: Option<&'a AbortHandle>) -> Self {
        Self { reader, abort }
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for AbortReader<'_, R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        check_aborted(self.abort)?;
        // `read_to_end` can supply an arbitrarily large buffer. Bound the work between checks.
        let size = if self.abort.is_some() {
            buffer.len().min(super::DEFAULT_BUF_SIZE)
        } else {
            buffer.len()
        };
        Pin::new(&mut self.reader).poll_read(cx, &mut buffer[..size])
    }
}

pub(super) fn check_aborted(abort: Option<&AbortHandle>) -> io::Result<()> {
    if abort.is_some_and(AbortHandle::is_aborted) {
        return Err(io::Error::other(Aborted));
    }
    Ok(())
}
