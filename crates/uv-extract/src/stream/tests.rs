use std::thread;
use std::time::{Duration, Instant};

use async_zip::base::write::ZipFileWriter;
use async_zip::{Compression, ZipEntryBuilder};
use futures::executor::block_on;
use futures::future::{AbortHandle, Aborted};
use futures::io::AsyncWriteExt;

use super::{unzip_blocking, unzip_blocking_and_hash};
use crate::Error;

#[test]
fn cancel_buffered_decompression() -> anyhow::Result<()> {
    const UNCOMPRESSED_SIZE: u64 = 128 * 1024 * 1024;

    let mut archive = ZipFileWriter::new(Vec::new());
    let mut entry = block_on(archive.write_entry_stream(ZipEntryBuilder::new(
        "payload.bin".into(),
        Compression::Deflate,
    )))?;
    let chunk = vec![0; 1024 * 1024];
    for _ in 0..128 {
        block_on(entry.write_all(&chunk))?;
    }
    block_on(entry.close())?;
    let bytes = block_on(archive.close())?;
    assert!(bytes.len() < 1024 * 1024);

    for hash_contents in [false, true] {
        let target = tempfile::tempdir()?;
        let payload = target.path().join("payload.bin");
        let (abort, registration) = AbortHandle::new_pair();
        let result = thread::scope(|scope| {
            let worker = scope.spawn(|| {
                if hash_contents {
                    unzip_blocking_and_hash(bytes.as_slice(), target.path(), registration)
                        .map(|_| ())
                } else {
                    unzip_blocking(bytes.as_slice(), target.path(), registration).map(|_| ())
                }
            });
            let started = Instant::now();
            while fs_err::metadata(&payload).map_or(0, |metadata| metadata.len()) < 1024 * 1024
                && !worker.is_finished()
                && started.elapsed() < Duration::from_secs(10)
            {
                thread::sleep(Duration::from_millis(1));
            }
            abort.abort();
            worker
                .join()
                .map_err(|_| anyhow::anyhow!("extraction worker panicked"))
        })?;
        let Err(Error::Io(error)) = result else {
            anyhow::bail!("expected extraction to be aborted");
        };
        assert!(matches!(error.get_ref(), Some(error) if error.is::<Aborted>()));
        assert!(fs_err::metadata(payload)?.len() < UNCOMPRESSED_SIZE);
    }
    Ok(())
}
