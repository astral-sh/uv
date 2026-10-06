//! Install the native ty executable from a Python package index without requiring Python.

use std::cmp::Reverse;
#[cfg(unix)]
use std::fs::Permissions;
use std::io;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::str::FromStr;

use futures::TryStreamExt;
use reqwest_retry::policies::ExponentialBackoff;
use tokio::io::AsyncRead;
use tokio::sync::Semaphore;
use tokio_util::compat::FuturesAsyncReadCompatExt;

use uv_cache::{Cache, CacheBucket, CacheEntry};
use uv_cache_key::hash_digest;
use uv_client::{
    BaseClient, FlatIndexClient, MetadataFormat, OwnedArchive, RegistryClient,
    fetch_with_url_fallback,
};
use uv_configuration::{ExcludeNewer, IndexStrategy};
use uv_distribution_filename::{
    DistFilename, LegacySourceDistExtension, SourceDistExtension, WheelFilename,
};
use uv_distribution_types::{ArchiveHashPolicy, File, IndexCapabilities, IndexLocations};
use uv_extract::hash::{HashReader, Hasher};
use uv_extract::stream;
use uv_normalize::PackageName;
use uv_pep440::{Version, VersionSpecifier, VersionSpecifiers};
use uv_platform::{Platform, wheel_platform};
use uv_platform_tags::{AbiTag, LanguageTag, PlatformTag, compatible_tags};
use uv_pypi_types::HashDigest;
use uv_redacted::DisplaySafeUrl;

use crate::{BinVersion, Error, ProgressReader, Reporter};

/// A ty wheel selected from a package index.
pub struct ResolvedTy {
    pub version: Version,
    file: File,
}

/// Select a ty wheel using the normal index client and native platform compatibility.
pub async fn resolve(
    version: &BinVersion,
    client: &RegistryClient,
    indexes: &IndexLocations,
    strategy: IndexStrategy,
    exclude_newer: &ExcludeNewer,
    cache: &Cache,
    download_concurrency: &Semaphore,
) -> Result<ResolvedTy, Error> {
    let package = PackageName::from_str("ty").map_err(io::Error::other)?;
    let platform_tags = compatible_tags(&wheel_platform()?).map_err(io::Error::other)?;
    let constraints = match version {
        BinVersion::Default => [
            VersionSpecifier::greater_than_equal_version(Version::new([0, 0])),
            VersionSpecifier::less_than_version(Version::new([0, 1])),
        ]
        .into_iter()
        .collect(),
        BinVersion::Pinned(version) => VersionSpecifier::equals_version(version.clone()).into(),
        BinVersion::Constraint(constraints) => constraints.clone(),
        BinVersion::Latest => VersionSpecifiers::empty(),
    };

    let mut candidates = Vec::new();
    let flat = FlatIndexClient::new(client.cached_client(), client.connectivity(), cache)
        .fetch_all(indexes.flat_indexes().map(|index| &index.url))
        .await
        .map_err(io::Error::other)?;
    for entry in flat.into_parts().0 {
        let (filename, file, index) = entry.into_parts();
        if let DistFilename::WheelFilename(wheel) = filename
            && wheel.name == package
        {
            candidates.push((wheel, file, index));
        }
    }

    let metadata = client
        .simple_detail(
            &package,
            None,
            &IndexCapabilities::default(),
            download_concurrency,
        )
        .await;
    let metadata = match metadata {
        Ok(metadata) => metadata,
        Err(err)
            if matches!(
                err.kind(),
                uv_client::ErrorKind::NoIndex(_) | uv_client::ErrorKind::RemotePackageNotFound(_)
            ) =>
        {
            Vec::new()
        }
        Err(err) => return Err(Error::Registry(Box::new(err))),
    };
    let mut best = select(
        candidates.iter().map(|(wheel, file, index)| {
            let cutoff = exclude_newer
                .exclude_newer_package_for_index(&package, indexes.exclude_newer_for(index));
            (wheel, file, cutoff)
        }),
        version,
        &constraints,
        &platform_tags,
    );
    for (index, metadata) in metadata {
        let files = match metadata {
            MetadataFormat::Simple(metadata) => {
                let metadata = OwnedArchive::deserialize(&metadata);
                metadata
                    .into_iter()
                    .flat_map(|entry| entry.files.wheels)
                    .filter_map(|file| {
                        let wheel = WheelFilename::from_str(file.filename()).ok()?;
                        Some((wheel, File::from(file)))
                    })
                    .collect::<Vec<_>>()
            }
            MetadataFormat::Flat(entries) => entries
                .into_iter()
                .filter_map(|entry| {
                    let (filename, file, _) = entry.into_parts();
                    if let DistFilename::WheelFilename(wheel) = filename {
                        Some((wheel, file))
                    } else {
                        None
                    }
                })
                .collect(),
        };
        let cutoff = exclude_newer
            .exclude_newer_package_for_index(&package, indexes.exclude_newer_for(index));
        if let Some(candidate) = select(
            files.iter().map(|(wheel, file)| (wheel, file, cutoff)),
            version,
            &constraints,
            &platform_tags,
        ) {
            if best
                .as_ref()
                .is_none_or(|best| candidate.version > best.version)
            {
                best = Some(candidate);
            }
            if strategy != IndexStrategy::UnsafeBestMatch {
                break;
            }
        }
    }
    best.ok_or_else(|| Error::NoTyWheel {
        constraints,
        platform: Platform::from_env()
            .map(|platform| platform.as_cargo_dist_triple())
            .unwrap_or_else(|_| std::env::consts::ARCH.to_owned()),
    })
}

fn select<'a>(
    files: impl IntoIterator<Item = (&'a WheelFilename, &'a File, Option<jiff::Timestamp>)>,
    version: &BinVersion,
    constraints: &VersionSpecifiers,
    platform_tags: &[PlatformTag],
) -> Option<ResolvedTy> {
    files
        .into_iter()
        .filter_map(|(wheel, file, cutoff)| {
            // ty's py3-none wheels contain a native executable. Their Requires-Python metadata
            // governs the Python launcher, which we do not install or run.
            if !constraints.contains(&wheel.version)
                || !wheel.python_tags().contains(&LanguageTag::Python {
                    major: 3,
                    minor: None,
                })
                || !wheel.abi_tags().contains(&AbiTag::None)
            {
                return None;
            }
            if !matches!(version, BinVersion::Pinned(_)) {
                if file
                    .yanked
                    .as_ref()
                    .is_some_and(|yanked| yanked.is_yanked())
                {
                    return None;
                }
                if let Some(cutoff) = cutoff
                    && file
                        .upload_time_utc_ms
                        .is_none_or(|uploaded| uploaded > cutoff.as_millisecond())
                {
                    return None;
                }
            }
            let priority = platform_tags
                .iter()
                .position(|tag| wheel.platform_tags().contains(tag))?;
            Some((wheel, file, Reverse(priority)))
        })
        .max_by_key(|(wheel, _, priority)| (&wheel.version, *priority, wheel.build_tag()))
        .map(|(wheel, file, _)| ResolvedTy {
            version: wheel.version.clone(),
            file: file.clone(),
        })
}

impl ResolvedTy {
    /// Download, verify, and extract ty into a cache distinct from standalone release archives.
    pub async fn install(
        &self,
        client: &BaseClient,
        retry_policy: &ExponentialBackoff,
        cache: &Cache,
        reporter: &dyn Reporter,
    ) -> Result<PathBuf, Error> {
        // Require an advertised digest even when the executable is already cached.
        if self.file.hashes.is_empty() {
            return Err(Error::InvalidTyWheel(
                "ty wheel does not advertise a supported hash",
            ));
        }
        let executable = format!("ty{}", std::env::consts::EXE_SUFFIX);
        let url = self.file.url.to_url().map_err(io::Error::other)?;
        let entry = CacheEntry::new(
            cache
                .bucket(CacheBucket::Binaries)
                .join("ty")
                .join("wheels")
                .join(hash_digest(&(&self.file.url, &self.file.hashes))),
            &executable,
        );
        let _lock = entry.with_file(".lock").lock().await?;
        let package = PackageName::from_str("ty").map_err(io::Error::other)?;
        if entry.path().is_file() && cache.freshness(&entry, Some(&package), None)?.is_fresh() {
            return Ok(entry.into_path_buf());
        }
        fs_err::tokio::create_dir_all(entry.dir()).await?;
        fetch_with_url_fallback(&[url], *retry_policy, "`ty`", |url| {
            self.download(url, client, cache, reporter, &entry, &executable)
        })
        .await
    }

    async fn download(
        &self,
        url: DisplaySafeUrl,
        client: &BaseClient,
        cache: &Cache,
        reporter: &dyn Reporter,
        entry: &CacheEntry,
        executable: &str,
    ) -> Result<PathBuf, Error> {
        let temp_dir = tempfile::tempdir_in(cache.bucket(CacheBucket::Binaries))?;
        let reader: Box<dyn AsyncRead + Unpin + Send> = if url.scheme() == "file" {
            let path = url
                .to_file_path()
                .map_err(|()| io::Error::other("Invalid wheel file URL"))?;
            Box::new(fs_err::tokio::File::open(path).await?)
        } else {
            let response = client
                .for_host(&url)
                .get(url::Url::from(url.clone()))
                .send()
                .await
                .map_err(|source| Error::Download {
                    url: url.clone(),
                    source,
                })?
                .error_for_status()
                .map_err(|err| Error::Download {
                    url: url.clone(),
                    source: err.into(),
                })?;
            Box::new(
                response
                    .bytes_stream()
                    .map_err(move |source| {
                        io::Error::other(Error::Stream {
                            url: url.clone(),
                            source,
                        })
                    })
                    .into_async_read()
                    .compat(),
            )
        };
        let id = reporter.on_download_start("ty", &self.version, self.file.size);
        let reader = ProgressReader::new(reader, id, reporter);
        let policy = ArchiveHashPolicy::All(self.file.hashes.as_slice());
        let mut hashers = policy
            .algorithms()
            .into_iter()
            .map(Hasher::from)
            .collect::<Vec<_>>();
        let mut reader = HashReader::new(reader, &mut hashers);
        let (temp_dir, _) = stream::archive(
            &mut reader,
            SourceDistExtension::Legacy(LegacySourceDistExtension::Zip),
            temp_dir,
        )
        .await
        .map_err(|source| Error::Extract { source })?;
        reader.finish().await?;
        if self
            .file
            .size
            .is_some_and(|size| reader.bytes_read() != size)
        {
            return Err(Error::InvalidTyWheel(
                "Downloaded wheel has an unexpected size",
            ));
        }
        let hashes = hashers
            .into_iter()
            .map(HashDigest::from)
            .collect::<Vec<_>>();
        if !policy.matches(&hashes) {
            return Err(Error::InvalidTyWheel(
                "Downloaded wheel does not match its advertised hashes",
            ));
        }
        let path = temp_dir
            .path()
            .join(format!("ty-{}.data", self.version))
            .join("scripts")
            .join(executable);
        if !fs_err::tokio::symlink_metadata(&path)
            .await?
            .file_type()
            .is_file()
        {
            return Err(Error::InvalidTyWheel(
                "Wheel does not contain a regular ty executable",
            ));
        }
        #[cfg(unix)]
        fs_err::tokio::set_permissions(&path, Permissions::from_mode(0o755)).await?;
        fs_err::tokio::rename(&path, entry.path()).await?;
        reporter.on_download_complete(id);
        Ok(entry.path().to_path_buf())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fmt::Write;

    use insta::assert_snapshot;
    use serde_json::json;
    use uv_client::{BaseClientBuilder, RegistryClientBuilder};
    use uv_distribution_types::{FileLocation, Index, IndexUrl};
    use uv_platform_tags::{Arch, Os, Platform as WheelPlatform};
    use uv_preview::PreviewFeature;
    use uv_preview::test::with_features;
    use uv_pypi_types::{HashAlgorithm, HashDigests, Yanked};
    use uv_test::packse::generate_wheel_with_files;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    struct NoopReporter;

    impl Reporter for NoopReporter {
        fn on_download_start(&self, _: &str, _: &Version, _: Option<u64>) -> usize {
            0
        }
        fn on_download_progress(&self, _: usize, _: u64) {}
        fn on_download_complete(&self, _: usize) {}
    }

    fn file(filename: &str) -> Result<(WheelFilename, File), Box<dyn std::error::Error>> {
        Ok((
            WheelFilename::from_str(filename)?,
            File {
                filename: filename.into(),
                url: FileLocation::new(
                    format!("https://example.com/{filename}").into(),
                    &"https://example.com/".into(),
                ),
                hashes: HashDigests::empty(),
                size: None,
                requires_python: None,
                dist_info_metadata: None,
                upload_time_utc_ms: Some(
                    "2026-01-01T00:00:00Z"
                        .parse::<jiff::Timestamp>()?
                        .as_millisecond(),
                ),
                yanked: None,
            },
        ))
    }

    #[test]
    fn native_wheel_selection() -> TestResult {
        let mut files = [
            "ty-0.0.1-py3-none-manylinux_2_17_x86_64.whl",
            "ty-0.0.2-py3-none-manylinux_2_28_x86_64.whl",
            "ty-0.0.3-cp313-cp313-manylinux_2_17_x86_64.whl",
            "ty-0.0.4-py3-none-win_amd64.whl",
            "ty-0.0.5-py3-none-macosx_11_0_arm64.whl",
            "ty-0.0.6-py3-none-musllinux_1_2_x86_64.whl",
            "ty-0.0.7-py3-none-manylinux_2_17_x86_64.whl",
            "ty-0.0.8-py3-none-manylinux_2_17_x86_64.whl",
        ]
        .into_iter()
        .map(file)
        .collect::<Result<Vec<_>, _>>()?;
        files[6].1.yanked = Some(Box::new(Yanked::Bool(true)));
        files[7].1.upload_time_utc_ms = Some(
            "2027-01-01T00:00:00Z"
                .parse::<jiff::Timestamp>()?
                .as_millisecond(),
        );
        let cutoff = Some("2026-06-01T00:00:00Z".parse()?);
        let mut selected = Vec::new();
        for platform in [
            WheelPlatform::new(
                Os::Manylinux {
                    major: 2,
                    minor: 17,
                },
                Arch::X86_64,
            ),
            WheelPlatform::new(
                Os::Manylinux {
                    major: 2,
                    minor: 28,
                },
                Arch::X86_64,
            ),
            WheelPlatform::new(Os::Musllinux { major: 1, minor: 2 }, Arch::X86_64),
            WheelPlatform::new(Os::Windows, Arch::X86_64),
            WheelPlatform::new(
                Os::Macos {
                    major: 11,
                    minor: 0,
                },
                Arch::Aarch64,
            ),
        ] {
            let wheel = select(
                files.iter().map(|(wheel, file)| (wheel, file, cutoff)),
                &BinVersion::Latest,
                &VersionSpecifiers::empty(),
                &compatible_tags(&platform)?,
            )
            .ok_or("No compatible wheel")?;
            selected.push(wheel.file.filename);
        }
        assert_snapshot!(selected.join("\n"), @"
        ty-0.0.1-py3-none-manylinux_2_17_x86_64.whl
        ty-0.0.2-py3-none-manylinux_2_28_x86_64.whl
        ty-0.0.6-py3-none-musllinux_1_2_x86_64.whl
        ty-0.0.4-py3-none-win_amd64.whl
        ty-0.0.5-py3-none-macosx_11_0_arm64.whl
        ");
        Ok(())
    }

    #[tokio::test]
    async fn flat_indexes_respect_exclude_newer() -> TestResult {
        let _preview = with_features(&[PreviewFeature::IndexExcludeNewer]);
        let cache = Cache::temp()?;
        let server = MockServer::start().await;
        let tags = compatible_tags(&wheel_platform()?)?;
        let tag = tags.first().ok_or("No native platform tag")?;
        let mut links = String::new();
        for (version, uploaded) in [
            ("0.0.17", " data-upload-time=\"2026-01-01T00:00:00Z\""),
            ("0.0.18", " data-upload-time=\"2026-09-01T00:00:00Z\""),
            ("0.0.19", ""),
        ] {
            let filename = format!("ty-{version}-py3-none-{tag}.whl");
            writeln!(links, "<a href=\"{filename}\"{uploaded}>{filename}</a>")?;
        }
        Mock::given(method("GET"))
            .and(path("/flat"))
            .respond_with(ResponseTemplate::new(200).set_body_string(links))
            .mount(&server)
            .await;

        let mut selected = Vec::new();
        for (label, index_cutoff, package_cutoff, version) in [
            ("global", None, None, BinVersion::Latest),
            ("index", Some("2026-12-31"), None, BinVersion::Latest),
            (
                "package",
                Some("2026-12-31"),
                Some("ty=2026-06-01"),
                BinVersion::Latest,
            ),
            (
                "pinned",
                None,
                None,
                BinVersion::Pinned(Version::new([0, 0, 19])),
            ),
        ] {
            let mut index =
                Index::from_find_links(IndexUrl::from_str(&format!("{}/flat", server.uri()))?);
            index.exclude_newer = index_cutoff.map(str::parse).transpose()?;
            let indexes = IndexLocations::new(vec![index.clone()], vec![index], true);
            let client = RegistryClientBuilder::new(BaseClientBuilder::default(), cache.clone())
                .index_locations(indexes.clone())
                .build()?;
            let exclude_newer = ExcludeNewer::from_args(
                Some("2026-06-01".parse()?),
                package_cutoff
                    .map(str::parse)
                    .transpose()?
                    .into_iter()
                    .collect(),
            );
            let resolved = resolve(
                &version,
                &client,
                &indexes,
                IndexStrategy::default(),
                &exclude_newer,
                &cache,
                &Semaphore::new(1),
            )
            .await?;
            selected.push(format!("{label}: {}", resolved.version));
        }
        assert_snapshot!(selected.join("\n"), @"
        global: 0.0.17
        index: 0.0.18
        package: 0.0.17
        pinned: 0.0.19
        ");
        Ok(())
    }

    #[tokio::test]
    async fn install_verifies_hashes_and_separates_sources() -> TestResult {
        let cache = Cache::temp()?;
        let server = MockServer::start().await;
        let executable = format!("ty{}", std::env::consts::EXE_SUFFIX);
        let tags = compatible_tags(&wheel_platform()?)?;
        let tag = tags.first().ok_or("No native platform tag")?;
        let (filename, wheel) = generate_wheel_with_files(
            &PackageName::from_str("ty")?,
            &Version::new([0, 0, 17]),
            &[],
            &BTreeMap::new(),
            None,
            &format!("py3-none-{tag}"),
            &[(&format!("ty-0.0.17.data/scripts/{executable}"), "native ty")],
        );
        let mut hasher = Hasher::from(HashAlgorithm::Sha256);
        hasher.update(&wheel);
        let hash = HashDigest::from(hasher);
        for route in ["first", "second", "unhashed", "unsupported"] {
            let hashes = match route {
                "unhashed" => json!({}),
                "unsupported" => json!({"unsupported": hash.digest()}),
                _ => json!({"sha256": hash.digest()}),
            };
            Mock::given(method("GET"))
                .and(path(format!("/{route}/ty.whl")))
                .respond_with(ResponseTemplate::new(200).set_body_bytes(wheel.clone()))
                .expect(match route {
                    "unhashed" | "unsupported" => 0,
                    "second" => 2,
                    _ => 1,
                })
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path(format!("/{route}/ty/")))
                .respond_with(
                    ResponseTemplate::new(200).set_body_raw(
                        json!({"name": "ty", "files": [{
                            "filename": filename,
                            "url": format!("{}/{route}/ty.whl", server.uri()),
                            "hashes": hashes, "size": wheel.len()
                        }]})
                        .to_string(),
                        "application/vnd.pypi.simple.v1+json",
                    ),
                )
                .mount(&server)
                .await;
        }
        let builder = BaseClientBuilder::default().retries(0);
        for route in ["first", "second", "unhashed", "unsupported"] {
            let indexes = IndexLocations::new(
                vec![Index::from_index_url(IndexUrl::from_str(&format!(
                    "{}/{route}",
                    server.uri()
                ))?)],
                vec![],
                false,
            );
            let client = RegistryClientBuilder::new(builder.clone(), cache.clone())
                .index_locations(indexes.clone())
                .build()?;
            let mut resolved = resolve(
                &BinVersion::Pinned(Version::new([0, 0, 17])),
                &client,
                &indexes,
                IndexStrategy::default(),
                &ExcludeNewer::default(),
                &cache,
                &Semaphore::new(1),
            )
            .await?;
            if matches!(route, "unhashed" | "unsupported") {
                let err = resolved
                    .install(
                        client.cached_client().uncached(),
                        &builder.retry_policy(),
                        &cache,
                        &NoopReporter,
                    )
                    .await
                    .err()
                    .ok_or("Wheel without a supported hash was accepted")?;
                insta::allow_duplicates! {
                    assert_snapshot!(err.to_string(), @"ty wheel does not advertise a supported hash");
                }
            } else {
                // Different URLs with the same digest must each download their wheel.
                let path = resolved
                    .install(
                        client.cached_client().uncached(),
                        &builder.retry_policy(),
                        &cache,
                        &NoopReporter,
                    )
                    .await?;
                assert_eq!(fs_err::read_to_string(&path)?, "native ty");
                if route == "second" {
                    // A new digest for the same URL must not reuse the cached executable.
                    resolved.file.hashes = HashDigests::from(vec![HashDigest::new(
                        HashAlgorithm::Sha256,
                        "0".repeat(64),
                    )?]);
                    let err = resolved
                        .install(
                            client.cached_client().uncached(),
                            &builder.retry_policy(),
                            &cache,
                            &NoopReporter,
                        )
                        .await
                        .err()
                        .ok_or("Invalid hash was accepted")?;
                    assert_snapshot!(err.to_string(), @"Downloaded wheel does not match its advertised hashes");
                    continue;
                }
                // Repeat installation to verify the executable cache avoids a second download.
                let cached = resolved
                    .install(
                        client.cached_client().uncached(),
                        &builder.retry_policy(),
                        &cache,
                        &NoopReporter,
                    )
                    .await?;
                assert_eq!(path, cached);

                // A cached executable cannot bypass the advertised digest requirement.
                resolved.file.hashes = HashDigests::empty();
                let unhashed = cache.entry(
                    CacheBucket::Binaries,
                    PathBuf::from("ty")
                        .join("wheels")
                        .join(hash_digest(&(&resolved.file.url, &resolved.file.hashes))),
                    &executable,
                );
                fs_err::create_dir_all(unhashed.dir())?;
                fs_err::write(unhashed.path(), "unverified ty")?;
                let err = resolved
                    .install(
                        client.cached_client().uncached(),
                        &builder.retry_policy(),
                        &cache,
                        &NoopReporter,
                    )
                    .await
                    .err()
                    .ok_or("Unhashed cached executable was accepted")?;
                assert_snapshot!(err.to_string(), @"ty wheel does not advertise a supported hash");
            }
        }
        Ok(())
    }
}
