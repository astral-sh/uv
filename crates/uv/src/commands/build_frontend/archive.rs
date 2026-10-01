use std::path::Path;

use anyhow::Result;
use async_zip::base::read::seek::ZipFileReader;
use tokio::io::BufReader;
use tokio_util::compat::TokioAsyncReadCompatExt;
use uv_distribution_filename::WheelFilename;
use uv_metadata::find_archive_dist_info;
use uv_pypi_types::ResolutionMetadata;

pub(super) async fn wheel_metadata(
    path: &Path,
    filename: &WheelFilename,
) -> Result<ResolutionMetadata> {
    let input = fs_err::tokio::File::open(path).await?;
    let mut archive = ZipFileReader::new(BufReader::new(input).compat()).await?;
    let (index, _) = find_archive_dist_info(
        filename,
        archive
            .file()
            .entries()
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| Some((index, entry.filename().as_str().ok()?))),
    )?;
    let mut contents = Vec::new();
    archive
        .reader_with_entry(index)
        .await?
        .read_to_end_checked(&mut contents)
        .await?;
    Ok(ResolutionMetadata::parse_metadata(&contents)?)
}
