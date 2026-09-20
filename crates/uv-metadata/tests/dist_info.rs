use std::error::Error;
use std::str::FromStr;

use async_zip::base::write::ZipFileWriter;
use async_zip::{Compression, ZipEntryBuilder};
use futures::executor::block_on;
use futures::io::Cursor;

use uv_distribution_filename::WheelFilename;
use uv_metadata::read_dist_info_file_async_stream;

fn wheel(entries: &[(&str, &[u8])]) -> Result<Vec<u8>, Box<dyn Error>> {
    block_on(async {
        let mut writer = ZipFileWriter::new(Vec::new());
        for (path, contents) in entries {
            let entry = ZipEntryBuilder::new((*path).into(), Compression::Stored);
            writer.write_entry_whole(entry, contents).await?;
        }
        Ok(writer.close().await?)
    })
}

#[test]
fn read_named_dist_info_file() -> Result<(), Box<dyn Error>> {
    let filename = WheelFilename::from_str("friendly_bard-1.0-py3-none-any.whl")?;
    let wheel = wheel(&[
        ("friendly_bard/WHEEL", b"not metadata"),
        ("friendly_bard-1.0.dist-info/METADATA", b"other metadata"),
        ("friendly_bard-1.0.dist-info/WHEEL", b"wheel metadata"),
    ])?;

    let contents = block_on(read_dist_info_file_async_stream(
        &filename,
        "WHEEL",
        Cursor::new(&wheel),
    ))?;
    assert_eq!(contents, b"wheel metadata");

    assert!(
        block_on(read_dist_info_file_async_stream(
            &filename,
            "RECORD",
            Cursor::new(&wheel),
        ))
        .is_err()
    );
    Ok(())
}

#[test]
fn reject_named_file_in_mismatched_dist_info() -> Result<(), Box<dyn Error>> {
    let filename = WheelFilename::from_str("friendly_bard-1.0-py3-none-any.whl")?;
    let wheel = wheel(&[("other_package-1.0.dist-info/WHEEL", b"wheel metadata")])?;

    let error = block_on(read_dist_info_file_async_stream(
        &filename,
        "WHEEL",
        Cursor::new(&wheel),
    ))
    .expect_err("mismatched distribution");
    assert_eq!(
        error.to_string(),
        "The .dist-info directory other_package-1.0 does not start with the normalized package name: friendly-bard"
    );
    Ok(())
}
