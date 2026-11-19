//! Read just the build declaration from a wheel when the index supports range requests.

use async_http_range_reader::AsyncHttpRangeReader;
use futures::FutureExt;
use futures::io::{AsyncReadExt, BufReader};
use http::HeaderMap;
use reqwest::{Request, Response};
use tokio_util::compat::TokioAsyncReadCompatExt;

use uv_cache::{CacheBucket, WheelCache};
use uv_distribution_types::{IndexCapabilities, RegistryBuiltWheel};
use uv_redacted::SafeUrl;

use super::RegistryWheelRequest;
use crate::{Error, ErrorKind, RegistryClient, RetryState};

/// Larger declarations are read from the source archive instead.
const MAX_PYPROJECT_SIZE: u64 = 1024 * 1024;

impl RegistryClient {
    pub(super) fn wheel_range_request(&self, url: &SafeUrl) -> Result<(Request, HeaderMap), Error> {
        let req = self
            .uncached_client(url)
            .head(url.clone())
            .header(
                "accept-encoding",
                http::HeaderValue::from_static("identity"),
            )
            .build()
            .map_err(|err| {
                ErrorKind::from_reqwest(url.clone(), err, self.client.certificate_source())
            })?;

        // Copy authorization headers from the HEAD request to subsequent requests
        let mut headers = HeaderMap::default();
        if let Some(authorization) = req.headers().get("authorization") {
            headers.append("authorization", authorization.clone());
        }
        // Byte offsets refer to the ZIP, not to its HTTP-compressed representation. Otherwise
        // transparent HTTP decompression can cause the range reader to decompress twice:
        // https://github.com/astral-sh/async_http_range_reader/pull/3#discussion_r2700194798
        headers.insert(
            reqwest::header::ACCEPT_ENCODING,
            reqwest::header::HeaderValue::from_static("identity"),
        );

        Ok((req, headers))
    }

    /// Return the embedded project declaration, caching negative results too. Do not stream a
    /// whole binary wheel on range failure: the caller can use the source archive instead.
    pub async fn wheel_pyproject(
        &self,
        wheel: &RegistryBuiltWheel,
        capabilities: &IndexCapabilities,
    ) -> Result<Option<String>, Error> {
        let (url, index) = RegistryWheelRequest::for_wheel(wheel, &self.indexes)?.into_parts();
        if url.scheme() == "file" || !capabilities.supports_range_requests(index) {
            return Ok(None);
        }
        let filename = &wheel.filename;
        let entry = self.cache.entry(
            CacheBucket::Wheels,
            WheelCache::Index(index).wheel_dir(filename.name.as_ref()),
            format!("{}.pyproject.msgpack", filename.cache_key()),
        );
        let policy =
            self.wheel_metadata_cache_control(&entry, &filename.name, Some(&wheel.index))?;
        let (req, headers) = self.wheel_range_request(&url)?;
        let read = |response: Response, _: &mut RetryState| {
            async {
                let mut reader = AsyncHttpRangeReader::from_head_response(
                    self.uncached_client(&url).clone(),
                    response,
                    url.clone().into_unsafe_url(),
                    headers.clone(),
                )
                .await
                .map_err(|err| ErrorKind::AsyncHttpRangeReader(url.clone(), err))?;
                reader
                    .prefetch(reader.len().saturating_sub(16384)..reader.len())
                    .await;
                let mut zip = async_zip::base::read::seek::ZipFileReader::new(BufReader::new(
                    reader.compat(),
                ))
                .await
                .map_err(|err| ErrorKind::Zip(filename.clone(), err))?;
                let entries = zip.file().entries();
                let ((), stem) = uv_metadata::find_archive_dist_info(
                    filename,
                    entries
                        .iter()
                        .filter_map(|e| Some(((), e.filename().as_str().ok()?))),
                )
                .map_err(|err| ErrorKind::Metadata(url.to_string(), err))?;
                let path = format!("{stem}.dist-info/pyproject.toml");
                let index = entries
                    .iter()
                    .position(|e| e.filename().as_bytes() == path.as_bytes())
                    .or_else(|| {
                        entries
                            .iter()
                            .position(|e| e.filename().as_bytes() == b"pyproject.toml")
                    });
                let Some(index) = index else {
                    return Ok::<_, Error>(None);
                };
                // This is an optimization. Fall back to the sdist for unusually large declarations.
                if entries[index].uncompressed_size() > MAX_PYPROJECT_SIZE {
                    return Ok(None);
                }
                let crc = entries[index].crc32();
                let mut entry = zip
                    .reader_with_entry(index)
                    .await
                    .map_err(|err| ErrorKind::Zip(filename.clone(), err))?;
                let mut contents = String::new();
                (&mut entry)
                    .take(MAX_PYPROJECT_SIZE + 1)
                    .read_to_string(&mut contents)
                    .await
                    .map_err(ErrorKind::Io)?;
                Ok(
                    (contents.len() as u64 <= MAX_PYPROJECT_SIZE && entry.compute_hash() == crc)
                        .then_some(contents),
                )
            }
            .boxed_local()
        };
        match self
            .cached_client()
            .get_serde_with_retry(req, &entry, policy, read)
            .await
            .map_err(Error::from)
        {
            Err(err) if err.is_http_range_requests_unsupported(&url, Some(index)) => {
                capabilities.set_no_range_requests(index.clone());
                Ok(None)
            }
            result => result,
        }
    }
}
