use std::path::Path;

use async_zip::base::write::ZipFileWriter;
use async_zip::{Compression, ZipEntryBuilder};
use futures::executor::block_on;

#[test]
fn seekable_mixed_size_entries() -> anyhow::Result<()> {
    let archive = tempfile::NamedTempFile::new()?;
    let mut writer = ZipFileWriter::new(Vec::new());
    let mut expected = Vec::new();
    for index in 0..16u8 {
        let name = format!("package/file-{index}.txt");
        let contents = vec![index; usize::from(index % 4) * 64 * 1024 + usize::from(index)];
        let compression = if index % 2 == 0 {
            Compression::Stored
        } else {
            Compression::Deflate
        };
        block_on(writer.write_entry_whole(
            ZipEntryBuilder::new(name.clone().into(), compression),
            &contents,
        ))?;
        expected.push((name, contents));
    }
    fs_err::write(archive.path(), block_on(writer.close())?)?;

    // A single worker guarantees that an archive reader is reused across several entries.
    uv_configuration::initialize_rayon_once();
    let pool = rayon::ThreadPoolBuilder::new().num_threads(1).build()?;
    let unhashed = tempfile::tempdir()?;
    let reader = fs_err::File::open(archive.path())?;
    let files = pool.install(|| uv_extract::unzip(reader, unhashed.path()))?;
    let hashed = tempfile::tempdir()?;
    let reader = fs_err::File::open(archive.path())?;
    let (hashed_files, tree) =
        pool.install(|| uv_extract::unzip_and_hash(reader, hashed.path()))?;

    let expected_files = expected
        .iter()
        .map(|(name, contents)| (Path::new(name), contents.len() as u64))
        .collect::<Vec<_>>();
    assert_eq!(
        files
            .iter()
            .map(|file| (file.path(), file.size()))
            .collect::<Vec<_>>(),
        expected_files
    );
    assert_eq!(
        hashed_files
            .iter()
            .map(|file| (file.path(), file.size()))
            .collect::<Vec<_>>(),
        expected_files
    );
    for (name, contents) in expected {
        assert_eq!(fs_err::read(unhashed.path().join(&name))?, contents);
        assert_eq!(fs_err::read(hashed.path().join(&name))?, contents);
    }
    assert_eq!(
        tree.hash(),
        uv_extract::dirhash::dirhash_path(hashed.path())?
    );
    Ok(())
}
