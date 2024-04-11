use async_http_range_reader::AsyncHttpRangeReader;
use futures::io::BufReader;
use tokio_util::compat::TokioAsyncReadCompatExt;

use distribution_filename::WheelFilename;
use install_wheel_rs::metadata::find_archive_dist_info;

use crate::{Error, ErrorKind};

/// Read the `.dist-info/METADATA` file from a async remote zip reader, so we avoid downloading the
/// entire wheel just for the one file.
///
/// This method is derived from `prefix-dev/rip`, which is available under the following BSD-3
/// Clause license:
///
/// ```text
/// BSD 3-Clause License
///
/// Copyright (c) 2023, prefix.dev GmbH
///
/// Redistribution and use in source and binary forms, with or without
/// modification, are permitted provided that the following conditions are met:
///
/// 1. Redistributions of source code must retain the above copyright notice, this
///    list of conditions and the following disclaimer.
///
/// 2. Redistributions in binary form must reproduce the above copyright notice,
///    this list of conditions and the following disclaimer in the documentation
///    and/or other materials provided with the distribution.
///
/// 3. Neither the name of the copyright holder nor the names of its
///    contributors may be used to endorse or promote products derived from
///    this software without specific prior written permission.
///
/// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
/// AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
/// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
/// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE
/// FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
/// DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
/// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
/// CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,
/// OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
/// OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
/// ```
///
/// Additional work and modifications to the originating source are available under the
/// Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or <https://www.apache.org/licenses/LICENSE-2.0>)
/// or MIT license ([LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/licenses/MIT>), as per the
/// rest of the crate.
pub(crate) async fn wheel_metadata_from_remote_zip(
    filename: &WheelFilename,
    reader: &mut AsyncHttpRangeReader,
) -> Result<String, Error> {
    // Make sure we have the back part of the stream.
    // Best guess for the central directory size inside the zip
    const CENTRAL_DIRECTORY_SIZE: u64 = 16384;
    // Because the zip index is at the back
    reader
        .prefetch(reader.len().saturating_sub(CENTRAL_DIRECTORY_SIZE)..reader.len())
        .await;

    // Construct a zip reader to uses the stream.
    let buf = BufReader::new(reader.compat());
    let mut reader = async_zip::base::read::seek::ZipFileReader::new(buf)
        .await
        .map_err(|err| ErrorKind::Zip(filename.clone(), err))?;

    let ((metadata_idx, metadata_entry), _dist_info_prefix) = find_archive_dist_info(
        filename,
        reader
            .file()
            .entries()
            .iter()
            .enumerate()
            .filter_map(|(idx, e)| Some(((idx, e), e.filename().as_str().ok()?))),
    )
    .map_err(ErrorKind::DistInfo)?;

    let offset = metadata_entry.header_offset();
    let size = metadata_entry.compressed_size()
        + 30 // Header size in bytes
        + metadata_entry.filename().as_bytes().len() as u64;

    // The zip archive uses as BufReader which reads in chunks of 8192. To ensure we prefetch
    // enough data we round the size up to the nearest multiple of the buffer size.
    let buffer_size = 8192;
    let size = ((size + buffer_size - 1) / buffer_size) * buffer_size;

    // Fetch the bytes from the zip archive that contain the requested file.
    reader
        .inner_mut()
        .get_mut()
        .get_mut()
        .prefetch(offset..offset + size)
        .await;

    // Read the contents of the METADATA file
    let mut contents = String::new();
    reader
        .reader_with_entry(metadata_idx)
        .await
        .map_err(|err| ErrorKind::Zip(filename.clone(), err))?
        .read_to_string_checked(&mut contents)
        .await
        .map_err(|err| ErrorKind::Zip(filename.clone(), err))?;

    Ok(contents)
}
