use std::sync::Arc;

use uv_distribution_types::{
    ArtifactUrlMapError, BuiltDist, Dist, File, IndexLocations, IndexUrl, ProxyArtifactRoutes,
    ProxyIndexError, RegistryBuiltWheel, RegistrySourceDist, ResolvedDist, SourceDist,
};
use uv_normalize::PackageName;
use uv_pep508::MarkerTree;
use uv_redacted::DisplaySafeUrl;

use crate::lock::is_wheel_unreachable_for_marker;
use crate::resolution::{ResolutionGraphNode, ResolverOutput};
use crate::universal_marker::{ConflictMarker, UniversalMarker};

/// An error encountered while canonicalizing proxy artifact URLs for a new lock.
#[derive(Debug, thiserror::Error)]
pub enum ProxyCanonicalizationError {
    #[error(transparent)]
    ProxyIndex(#[from] ProxyIndexError),
    #[error(
        "Cannot canonicalize `{filename}` for `{package}` because canonical index `{canonical}` has no matching proxy declaration"
    )]
    MissingProxyIndex {
        package: PackageName,
        filename: String,
        canonical: Box<DisplaySafeUrl>,
    },
    #[error(
        "Cannot canonicalize `{filename}` for `{package}` because proxy index `{physical}` does not match the configured route for `{canonical}`"
    )]
    RouteMismatch {
        package: PackageName,
        filename: String,
        canonical: Box<DisplaySafeUrl>,
        physical: Box<DisplaySafeUrl>,
    },
    #[error(
        "Cannot lock `{filename}` for `{package}` from proxy index `{physical}` because it has no supported advertised digest"
    )]
    MissingDigest {
        package: PackageName,
        filename: String,
        physical: Box<DisplaySafeUrl>,
    },
    #[error(
        "Failed to canonicalize `{filename}` for `{package}` from proxy index `{physical}` against `{canonical}`"
    )]
    ArtifactUrl {
        package: PackageName,
        filename: String,
        canonical: Box<DisplaySafeUrl>,
        physical: Box<DisplaySafeUrl>,
        #[source]
        source: Box<ArtifactUrlMapError>,
    },
}

fn diagnostic_safe_index_url(index: &IndexUrl) -> DisplaySafeUrl {
    let mut url = index.url().clone();
    url.remove_credentials();
    url.set_query(None);
    url.set_fragment(None);
    url
}

impl ResolverOutput {
    /// Canonicalize proxy artifact URLs before constructing a new lock.
    pub fn canonicalize_proxy_artifact_urls_for_lock(
        &mut self,
        index_locations: &IndexLocations,
        supported_environments: &[MarkerTree],
    ) -> Result<(), ProxyCanonicalizationError> {
        let artifact_routes = ProxyArtifactRoutes::try_from(index_locations)?;
        let supported_environment = self.supported_environment(supported_environments);
        let mut replacements = Vec::new();

        for (node_index, annotated) in self.base_dists() {
            let ResolvedDist::Installable { dist, version } = &annotated.dist else {
                continue;
            };
            let selected_index = annotated.index();
            let mut canonicalized = dist.as_ref().clone();
            let mut wheel_marker = annotated.marker;
            if let Some(supported_environment) = supported_environment {
                wheel_marker.and(supported_environment);
            }
            if canonicalize_dist(
                &mut canonicalized,
                &annotated.name,
                &artifact_routes,
                selected_index,
                &self.requires_python,
                &wheel_marker,
            )? {
                replacements.push((
                    node_index,
                    ResolvedDist::Installable {
                        dist: Arc::new(canonicalized),
                        version: version.clone(),
                    },
                ));
            }
        }

        for (node_index, replacement) in replacements {
            if let Some(ResolutionGraphNode::Dist(annotated)) =
                self.graph.node_weight_mut(node_index)
            {
                annotated.dist = replacement;
            }
        }
        Ok(())
    }

    fn supported_environment(
        &self,
        supported_environments: &[MarkerTree],
    ) -> Option<UniversalMarker> {
        if supported_environments.is_empty() {
            return None;
        }
        let mut combined = MarkerTree::FALSE;
        for marker in supported_environments {
            combined.or(self.requires_python.complexify_markers(*marker));
        }
        Some(UniversalMarker::new(combined, ConflictMarker::TRUE))
    }
}

fn canonicalize_dist(
    dist: &mut Dist,
    package: &PackageName,
    artifact_routes: &ProxyArtifactRoutes,
    selected_index: Option<&IndexUrl>,
    requires_python: &uv_distribution_types::RequiresPython,
    wheel_marker: &UniversalMarker,
) -> Result<bool, ProxyCanonicalizationError> {
    match dist {
        Dist::Built(BuiltDist::Registry(registry)) => {
            let mut changed = false;
            if let Some(source_dist) = registry.sdist.as_mut()
                && selected_index.is_some_and(|index| *index == source_dist.index)
                && canonicalize_source_dist(source_dist, package, artifact_routes)?
            {
                changed = true;
            }
            if canonicalize_wheels(
                &mut registry.wheels,
                package,
                artifact_routes,
                selected_index,
                requires_python,
                wheel_marker,
            )? {
                changed = true;
            }
            Ok(changed)
        }
        Dist::Source(SourceDist::Registry(registry)) => {
            let mut changed = selected_index.is_some_and(|index| *index == registry.index)
                && canonicalize_source_dist(registry, package, artifact_routes)?;
            if canonicalize_wheels(
                &mut registry.wheels,
                package,
                artifact_routes,
                selected_index,
                requires_python,
                wheel_marker,
            )? {
                changed = true;
            }
            Ok(changed)
        }
        Dist::Built(_) | Dist::Source(_) => Ok(false),
    }
}

fn canonicalize_wheels(
    wheels: &mut [RegistryBuiltWheel],
    package: &PackageName,
    artifact_routes: &ProxyArtifactRoutes,
    selected_index: Option<&IndexUrl>,
    requires_python: &uv_distribution_types::RequiresPython,
    wheel_marker: &UniversalMarker,
) -> Result<bool, ProxyCanonicalizationError> {
    let mut changed = false;
    for wheel in wheels {
        if selected_index.is_none_or(|index| *index != wheel.index) {
            continue;
        }
        if is_wheel_unreachable_for_marker(&wheel.filename, requires_python, wheel_marker, None) {
            continue;
        }
        let lock_filename = wheel.filename.to_string();
        if canonicalize_registry_file(
            wheel.file.as_mut(),
            Some(&lock_filename),
            &wheel.index,
            &mut wheel.proxy,
            package,
            artifact_routes,
        )? {
            changed = true;
        }
    }
    Ok(changed)
}

fn canonicalize_source_dist(
    source_dist: &mut RegistrySourceDist,
    package: &PackageName,
    artifact_routes: &ProxyArtifactRoutes,
) -> Result<bool, ProxyCanonicalizationError> {
    canonicalize_registry_file(
        source_dist.file.as_mut(),
        None,
        &source_dist.index,
        &mut source_dist.proxy,
        package,
        artifact_routes,
    )
}

fn canonicalize_registry_file(
    file: &mut File,
    lock_filename: Option<&str>,
    canonical: &IndexUrl,
    proxy: &mut Option<IndexUrl>,
    package: &PackageName,
    artifact_routes: &ProxyArtifactRoutes,
) -> Result<bool, ProxyCanonicalizationError> {
    let Some(physical) = proxy.as_ref() else {
        return Ok(false);
    };
    let filename = lock_filename.unwrap_or(file.filename.as_ref());
    if file.hashes.is_empty() {
        return Err(ProxyCanonicalizationError::MissingDigest {
            package: package.clone(),
            filename: filename.to_string(),
            physical: Box::new(diagnostic_safe_index_url(physical)),
        });
    }
    let Some(artifact_route) = artifact_routes.route_for(canonical) else {
        return Err(ProxyCanonicalizationError::MissingProxyIndex {
            package: package.clone(),
            filename: filename.to_string(),
            canonical: Box::new(diagnostic_safe_index_url(canonical)),
        });
    };
    if !artifact_route.matches_physical(physical) {
        return Err(ProxyCanonicalizationError::RouteMismatch {
            package: package.clone(),
            filename: filename.to_string(),
            canonical: Box::new(diagnostic_safe_index_url(canonical)),
            physical: Box::new(diagnostic_safe_index_url(physical)),
        });
    }
    let canonicalization_error = |source| ProxyCanonicalizationError::ArtifactUrl {
        package: package.clone(),
        filename: filename.to_string(),
        canonical: Box::new(diagnostic_safe_index_url(canonical)),
        physical: Box::new(diagnostic_safe_index_url(physical)),
        source: Box::new(source),
    };
    let mut canonical_url = artifact_route
        .canonical_artifact_url(&file.url, file.filename.as_ref())
        .map_err(&canonicalization_error)?;
    // Lock deserialization normalizes wheel filenames, while proxy rediscovery matches them
    // exactly. Require the mapped URL to survive that round trip.
    if let Some(lock_filename) = lock_filename
        && lock_filename != file.filename.as_ref()
    {
        canonical_url = artifact_route
            .canonical_artifact_url(&file.url, lock_filename)
            .map_err(&canonicalization_error)?;
    }
    file.url = canonical_url;
    *proxy = None;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use std::error::Error;
    use std::path::Path;
    use std::str::FromStr;

    use petgraph::{Directed, graph::Graph, graph::NodeIndex};
    use serde_json::json;
    use uv_cache::Cache;
    use uv_client::{BaseClientBuilder, RegistryClientBuilder};
    use uv_configuration::{Constraints, Overrides};
    use uv_distribution_filename::{SourceDistExtension, WheelFilename};
    use uv_distribution_types::{
        ArtifactUrlMap, FileLocation, Index, IndexCapabilities, IndexReference, ProxyIndex,
        RegistryBuiltDist, RequiresPython, UrlString, Zstd,
    };
    use uv_normalize::{ExtraName, PackageName};
    use uv_pep440::{Version, VersionSpecifiers};
    use uv_pypi_types::{HashDigest, HashDigests, Yanked};
    use uv_redacted::DisplaySafeUrl;
    use wiremock::matchers::{method, path as request_path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::Options;
    use crate::resolution::AnnotatedDist;

    const CANONICAL_INDEX: &str = "https://canonical.example/simple";
    const PROXY_INDEX: &str = "https://proxy.example/simple";
    const PHYSICAL_PREFIX: &str = "https://files.proxy.example/packages";
    const CANONICAL_PREFIX: &str = "https://files.canonical.example/artifacts";
    const SHA256: &str = "sha256:1111111111111111111111111111111111111111111111111111111111111111";

    type ResolverFixture = (ResolverOutput, Vec<NodeIndex>, Option<NodeIndex>);

    #[test]
    fn canonicalizes_wheel_without_mutating_runtime_extra_or_canonical_nodes()
    -> Result<(), Box<dyn Error>> {
        let canonical = IndexUrl::from_str(CANONICAL_INDEX)?;
        let proxy = IndexUrl::from_str(PROXY_INDEX)?;
        let filename = "example-1.0.0-py3-none-any.whl";
        let wheel = registry_wheel(
            filename,
            &format!("{PHYSICAL_PREFIX}/{filename}#sha256=ignored"),
            &canonical,
            Some(&proxy),
            hashes()?,
        )?;
        let original_wheel = wheel.clone();
        let canonical_filename = "canonical-1.0.0-py3-none-any.whl";
        let canonical_wheel = registry_wheel(
            canonical_filename,
            &format!("https://canonical.example/files/{canonical_filename}"),
            &canonical,
            None,
            HashDigests::empty(),
        )?;
        let original_canonical_wheel = canonical_wheel.clone();
        let (mut resolution, base_indexes, extra_index) = resolver_output(
            vec![
                (
                    PackageName::from_str("example")?,
                    Dist::Built(BuiltDist::Registry(RegistryBuiltDist {
                        wheels: vec![wheel],
                        best_wheel_index: 0,
                        sdist: None,
                    })),
                ),
                (
                    PackageName::from_str("canonical")?,
                    Dist::Built(BuiltDist::Registry(RegistryBuiltDist {
                        wheels: vec![canonical_wheel],
                        best_wheel_index: 0,
                        sdist: None,
                    })),
                ),
            ],
            true,
        )?;
        let runtime_dist = installable_dist_at(&resolution, base_indexes[0])?.clone();

        let index_locations = index_locations()?;
        resolution.canonicalize_proxy_artifact_urls_for_lock(&index_locations, &[])?;

        let base_wheel = wheel_at(&resolution, base_indexes[0], 0)?;
        let mut expected_file = original_wheel.file.as_ref().clone();
        expected_file.url = FileLocation::AbsoluteUrl(UrlString::from(DisplaySafeUrl::parse(
            &format!("{CANONICAL_PREFIX}/{filename}"),
        )?));
        assert_eq!(base_wheel.file.as_ref(), &expected_file);
        assert_eq!(base_wheel.index, canonical);
        assert!(base_wheel.proxy.is_none());
        let extra_wheel = wheel_at(&resolution, extra_index.ok_or("expected an extra node")?, 0)?;
        assert_eq!(extra_wheel, &original_wheel);
        assert_eq!(
            registry_wheel_from_dist(runtime_dist.as_ref(), 0)?,
            &original_wheel
        );
        assert_eq!(
            wheel_at(&resolution, base_indexes[1], 0)?,
            &original_canonical_wheel
        );
        let lock = crate::Lock::from_resolution(
            &resolution,
            Path::new("."),
            Vec::new(),
            &index_locations,
        )?;
        let lock = lock.to_toml()?;
        insta::assert_snapshot!(lock, @r#"
        version = 1
        revision = 3
        requires-python = ">=3.12"

        [[package]]
        name = "canonical"
        version = "1.0.0"
        source = { registry = "https://canonical.example/simple" }
        wheels = [
            { url = "https://canonical.example/files/canonical-1.0.0-py3-none-any.whl", size = 42, upload-time = "1970-01-01T00:00:01.234Z", zstd = { hash = "sha256:1111111111111111111111111111111111111111111111111111111111111111", size = 21 } },
        ]

        [[package]]
        name = "example"
        version = "1.0.0"
        source = { registry = "https://canonical.example/simple" }
        wheels = [
            { url = "https://files.canonical.example/artifacts/example-1.0.0-py3-none-any.whl", hash = "sha256:1111111111111111111111111111111111111111111111111111111111111111", size = 42, upload-time = "1970-01-01T00:00:01.234Z", zstd = { hash = "sha256:1111111111111111111111111111111111111111111111111111111111111111", size = 21 } },
        ]
        "#);
        Ok(())
    }

    #[tokio::test]
    async fn fresh_lock_wheel_filename_is_replay_stable() -> Result<(), Box<dyn Error>> {
        let server = MockServer::start().await;
        let canonical = IndexUrl::from_str(CANONICAL_INDEX)?;
        let proxy = IndexUrl::from_str(&format!("{}/simple", server.uri()))?;
        let index_locations =
            IndexLocations::new(vec![Index::from(canonical.clone())], Vec::new(), false)
                .with_proxy_indexes(vec![ProxyIndex {
                    index: IndexReference::Url(canonical.clone()),
                    url: proxy.clone(),
                    artifact_url_map: ArtifactUrlMap::single(
                        DisplaySafeUrl::parse(PHYSICAL_PREFIX)?,
                        DisplaySafeUrl::parse(CANONICAL_PREFIX)?,
                    ),
                }]);

        let proxy_filename = "example-1.01.0-py3-none-any.whl";
        let lock_filename = WheelFilename::from_str(proxy_filename)?.to_string();
        assert_eq!(lock_filename, "example-1.1.0-py3-none-any.whl");
        let unstable_wheel = registry_wheel(
            proxy_filename,
            &format!("{PHYSICAL_PREFIX}/{proxy_filename}"),
            &canonical,
            Some(&proxy),
            hashes()?,
        )?;
        let (mut unstable_resolution, _, _) = resolver_output(
            vec![(
                PackageName::from_str("example")?,
                Dist::Built(BuiltDist::Registry(RegistryBuiltDist {
                    wheels: vec![unstable_wheel],
                    best_wheel_index: 0,
                    sdist: None,
                })),
            )],
            false,
        )?;

        let error = unstable_resolution
            .canonicalize_proxy_artifact_urls_for_lock(&index_locations, &[])
            .expect_err("an unstable wheel filename must not be emitted into a fresh lock");
        let ProxyCanonicalizationError::ArtifactUrl { source, .. } = &error else {
            return Err("expected artifact URL error".into());
        };
        let ArtifactUrlMapError::FilenameChanged {
            expected, actual, ..
        } = source.as_ref()
        else {
            return Err("expected filename stability error".into());
        };
        assert_eq!(expected, &lock_filename);
        assert_eq!(actual, proxy_filename);

        let stable_wheel = registry_wheel(
            &lock_filename,
            &format!("{PHYSICAL_PREFIX}/{lock_filename}"),
            &canonical,
            Some(&proxy),
            hashes()?,
        )?;
        let (mut stable_resolution, _, _) = resolver_output(
            vec![(
                PackageName::from_str("example")?,
                Dist::Built(BuiltDist::Registry(RegistryBuiltDist {
                    wheels: vec![stable_wheel],
                    best_wheel_index: 0,
                    sdist: None,
                })),
            )],
            false,
        )?;
        stable_resolution.canonicalize_proxy_artifact_urls_for_lock(&index_locations, &[])?;
        let lock = crate::Lock::from_resolution(
            &stable_resolution,
            Path::new("."),
            Vec::new(),
            &index_locations,
        )?;
        let lock_toml = lock.to_toml()?;
        let lock_value: toml::Value = toml::from_str(&lock_toml)?;
        let package = lock_value
            .get("package")
            .and_then(toml::Value::as_array)
            .and_then(|packages| {
                packages.iter().find(|package| {
                    package.get("name").and_then(toml::Value::as_str) == Some("example")
                })
            })
            .ok_or("expected example package in fresh lock")?;
        let wheel = package
            .get("wheels")
            .and_then(toml::Value::as_array)
            .and_then(|wheels| wheels.first())
            .ok_or("expected wheel in fresh lock")?;
        let locked_url = wheel
            .get("url")
            .and_then(toml::Value::as_str)
            .ok_or("expected wheel URL in fresh lock")?;
        let locked_filename = locked_url
            .rsplit('/')
            .next()
            .ok_or("expected filename in fresh lock URL")?;
        let locked_hash = wheel
            .get("hash")
            .and_then(toml::Value::as_str)
            .ok_or("expected wheel digest in fresh lock")?;
        assert_eq!(locked_filename, lock_filename);

        let digest = SHA256
            .strip_prefix("sha256:")
            .ok_or("expected SHA-256 test digest")?;
        Mock::given(method("GET"))
            .and(request_path("/simple/example/"))
            .respond_with(
                ResponseTemplate::new(200).set_body_raw(
                    json!({
                        "name": "example",
                        "files": [{
                            "filename": lock_filename,
                            "url": format!("{PHYSICAL_PREFIX}/{lock_filename}"),
                            "hashes": { "sha256": digest },
                        }],
                    })
                    .to_string(),
                    "application/vnd.pypi.simple.v1+json",
                ),
            )
            .expect(1)
            .mount(&server)
            .await;

        let client = RegistryClientBuilder::new(BaseClientBuilder::default(), Cache::temp()?)
            .index_locations(index_locations)
            .build()?;
        let locked_hashes = HashDigests::from(HashDigest::from_str(locked_hash)?);
        let artifact = client
            .proxy_artifact(
                &PackageName::from_str("example")?,
                &canonical,
                locked_filename,
                &locked_hashes,
                &IndexCapabilities::default(),
            )
            .await?
            .ok_or("expected proxy rediscovery for the fresh lock wheel")?;
        assert_eq!(artifact.file.filename.as_ref(), locked_filename);
        assert!(artifact.has_shared_hash);
        Ok(())
    }

    #[test]
    fn canonicalizes_source_and_only_retained_wheels() -> Result<(), Box<dyn Error>> {
        let canonical = IndexUrl::from_str(CANONICAL_INDEX)?;
        let proxy = IndexUrl::from_str(PROXY_INDEX)?;
        let source_filename = "example-1.0.0.tar.gz";
        let source_file = registry_file(
            source_filename,
            &format!("{PHYSICAL_PREFIX}/{source_filename}"),
            hashes()?,
        )?;
        let original_source_file = source_file.clone();
        let retained_filename = "example-1.0.0-py3-none-any.whl";
        let retained = registry_wheel(
            retained_filename,
            &format!("{PHYSICAL_PREFIX}/{retained_filename}"),
            &canonical,
            Some(&proxy),
            hashes()?,
        )?;
        let omitted_filename = "example-1.0.0-cp312-cp312-win_amd64.whl";
        let omitted = registry_wheel(
            omitted_filename,
            &format!("https://unmapped.example/{omitted_filename}"),
            &canonical,
            Some(&proxy),
            HashDigests::empty(),
        )?;
        let original_omitted = omitted.clone();
        let source = RegistrySourceDist {
            name: PackageName::from_str("example")?,
            version: Version::from_str("1.0.0")?,
            file: Box::new(source_file),
            ext: SourceDistExtension::TarGz,
            index: canonical,
            proxy: Some(proxy),
            wheels: vec![retained, omitted],
        };
        let (mut resolution, base_indexes, _) = resolver_output(
            vec![(
                PackageName::from_str("example")?,
                Dist::Source(SourceDist::Registry(source)),
            )],
            false,
        )?;
        let supported = [MarkerTree::from_str("sys_platform == 'linux'")?];

        resolution.canonicalize_proxy_artifact_urls_for_lock(&index_locations()?, &supported)?;

        let source = source_at(&resolution, base_indexes[0])?;
        let mut expected_source_file = original_source_file;
        expected_source_file.url = FileLocation::AbsoluteUrl(UrlString::from(
            DisplaySafeUrl::parse(&format!("{CANONICAL_PREFIX}/{source_filename}"))?,
        ));
        assert_eq!(source.file.as_ref(), &expected_source_file);
        assert!(source.proxy.is_none());
        assert_eq!(
            source.wheels[0].file.url.to_url()?.as_str(),
            format!("{CANONICAL_PREFIX}/{retained_filename}")
        );
        assert!(source.wheels[0].proxy.is_none());
        assert_eq!(source.wheels[1], original_omitted);
        Ok(())
    }

    #[test]
    fn skips_non_selected_registry_artifacts_before_validation() -> Result<(), Box<dyn Error>> {
        let canonical = IndexUrl::from_str(CANONICAL_INDEX)?;
        let proxy = IndexUrl::from_str(PROXY_INDEX)?;
        let non_selected = IndexUrl::from_str("https://find-links.example/packages")?;

        let selected_filename = "example-1.0.0-py3-none-any.whl";
        let selected = registry_wheel(
            selected_filename,
            &format!("{PHYSICAL_PREFIX}/{selected_filename}"),
            &canonical,
            Some(&proxy),
            hashes()?,
        )?;
        let non_selected_wheel_filename = "example-1.0.0-py2-none-any.whl";
        let non_selected_wheel = registry_wheel(
            non_selected_wheel_filename,
            &format!("https://unmapped.example/{non_selected_wheel_filename}"),
            &non_selected,
            Some(&proxy),
            HashDigests::empty(),
        )?;
        let original_non_selected_wheel = non_selected_wheel.clone();

        let non_selected_source_filename = "example-1.0.0.tar.gz";
        let non_selected_source = RegistrySourceDist {
            name: PackageName::from_str("example")?,
            version: Version::from_str("1.0.0")?,
            file: Box::new(registry_file(
                non_selected_source_filename,
                &format!("https://unmapped.example/{non_selected_source_filename}"),
                HashDigests::empty(),
            )?),
            ext: SourceDistExtension::TarGz,
            index: non_selected,
            proxy: Some(proxy),
            wheels: vec![],
        };
        let original_non_selected_source = non_selected_source.clone();
        let (mut resolution, base_indexes, _) = resolver_output(
            vec![(
                PackageName::from_str("example")?,
                Dist::Built(BuiltDist::Registry(RegistryBuiltDist {
                    wheels: vec![selected, non_selected_wheel],
                    best_wheel_index: 0,
                    sdist: Some(non_selected_source),
                })),
            )],
            false,
        )?;

        resolution.canonicalize_proxy_artifact_urls_for_lock(&index_locations()?, &[])?;

        let Dist::Built(BuiltDist::Registry(registry)) =
            installable_dist_at(&resolution, base_indexes[0])?.as_ref()
        else {
            return Err("expected registry built distribution".into());
        };
        assert_eq!(
            registry.wheels[0].file.url.to_url()?.as_str(),
            format!("{CANONICAL_PREFIX}/{selected_filename}")
        );
        assert!(registry.wheels[0].proxy.is_none());
        assert_eq!(registry.wheels[1], original_non_selected_wheel);
        assert_eq!(registry.sdist.as_ref(), Some(&original_non_selected_source));
        Ok(())
    }

    #[test]
    fn rejects_hashless_retained_wheel_and_source() -> Result<(), Box<dyn Error>> {
        let canonical = IndexUrl::from_str(CANONICAL_INDEX)?;
        let proxy = IndexUrl::from_str(PROXY_INDEX)?;
        let wheel_filename = "wheel-1.0.0-py3-none-any.whl";
        let wheel = registry_wheel(
            wheel_filename,
            &format!("{PHYSICAL_PREFIX}/{wheel_filename}"),
            &canonical,
            Some(&proxy),
            HashDigests::empty(),
        )?;
        let (mut wheel_resolution, _, _) = resolver_output(
            vec![(
                PackageName::from_str("wheel")?,
                Dist::Built(BuiltDist::Registry(RegistryBuiltDist {
                    wheels: vec![wheel],
                    best_wheel_index: 0,
                    sdist: None,
                })),
            )],
            false,
        )?;
        assert!(matches!(
            wheel_resolution.canonicalize_proxy_artifact_urls_for_lock(&index_locations()?, &[]),
            Err(ProxyCanonicalizationError::MissingDigest { filename, .. })
                if filename == wheel_filename
        ));

        let source_filename = "source-1.0.0.tar.gz";
        let source = RegistrySourceDist {
            name: PackageName::from_str("source")?,
            version: Version::from_str("1.0.0")?,
            file: Box::new(registry_file(
                source_filename,
                &format!("{PHYSICAL_PREFIX}/{source_filename}"),
                HashDigests::empty(),
            )?),
            ext: SourceDistExtension::TarGz,
            index: canonical,
            proxy: Some(proxy),
            wheels: vec![],
        };
        let (mut source_resolution, _, _) = resolver_output(
            vec![(
                PackageName::from_str("source")?,
                Dist::Source(SourceDist::Registry(source)),
            )],
            false,
        )?;
        assert!(matches!(
            source_resolution.canonicalize_proxy_artifact_urls_for_lock(
                &index_locations()?,
                &[]
            ),
            Err(ProxyCanonicalizationError::MissingDigest { filename, .. })
                if filename == source_filename
        ));
        Ok(())
    }

    #[test]
    fn canonicalization_errors_redact_index_url_secrets() -> Result<(), Box<dyn Error>> {
        let canonical = IndexUrl::from_str(
            "https://canonical-user:canonical-password@canonical.example/simple?canonical-query=secret#canonical-fragment=secret",
        )?;
        let physical = IndexUrl::from_str(
            "https://proxy-user:proxy-password@proxy.example/simple?proxy-query=secret#proxy-fragment=secret",
        )?;
        let filename = "example-1.0.0-py3-none-any.whl";

        let hashless_wheel = registry_wheel(
            filename,
            &format!("{PHYSICAL_PREFIX}/{filename}"),
            &canonical,
            Some(&physical),
            HashDigests::empty(),
        )?;
        let (mut hashless_resolution, _, _) = resolver_output(
            vec![(
                PackageName::from_str("example")?,
                Dist::Built(BuiltDist::Registry(RegistryBuiltDist {
                    wheels: vec![hashless_wheel],
                    best_wheel_index: 0,
                    sdist: None,
                })),
            )],
            false,
        )?;
        let missing_digest = hashless_resolution
            .canonicalize_proxy_artifact_urls_for_lock(&index_locations()?, &[])
            .expect_err("a hashless retained wheel should fail");
        let missing_digest_diagnostic = format!("{missing_digest}\n{missing_digest:?}");
        assert!(missing_digest_diagnostic.contains("https://proxy.example/simple"));
        for secret in [
            "proxy-user",
            "proxy-password",
            "proxy-query",
            "proxy-fragment",
        ] {
            assert!(!missing_digest_diagnostic.contains(secret));
        }

        let mapped_wheel = registry_wheel(
            filename,
            &format!("{PHYSICAL_PREFIX}/{filename}"),
            &canonical,
            Some(&physical),
            hashes()?,
        )?;
        let (mut missing_proxy_resolution, _, _) = resolver_output(
            vec![(
                PackageName::from_str("example")?,
                Dist::Built(BuiltDist::Registry(RegistryBuiltDist {
                    wheels: vec![mapped_wheel],
                    best_wheel_index: 0,
                    sdist: None,
                })),
            )],
            false,
        )?;
        let no_proxy = IndexLocations::new(vec![Index::from(canonical.clone())], Vec::new(), false);
        let missing_proxy = missing_proxy_resolution
            .canonicalize_proxy_artifact_urls_for_lock(&no_proxy, &[])
            .expect_err("a selected proxy artifact without a declaration should fail");
        let missing_proxy_diagnostic = format!("{missing_proxy}\n{missing_proxy:?}");
        assert!(missing_proxy_diagnostic.contains("https://canonical.example/simple"));
        for secret in [
            "canonical-user",
            "canonical-password",
            "canonical-query",
            "canonical-fragment",
        ] {
            assert!(!missing_proxy_diagnostic.contains(secret));
        }
        Ok(())
    }

    #[test]
    fn failure_does_not_partially_mutate_base_nodes() -> Result<(), Box<dyn Error>> {
        let canonical = IndexUrl::from_str(CANONICAL_INDEX)?;
        let proxy = IndexUrl::from_str(PROXY_INDEX)?;
        let first_filename = "first-1.0.0-py3-none-any.whl";
        let first = registry_wheel(
            first_filename,
            &format!("{PHYSICAL_PREFIX}/{first_filename}"),
            &canonical,
            Some(&proxy),
            hashes()?,
        )?;
        let second_filename = "second-1.0.0-py3-none-any.whl";
        let second = registry_wheel(
            second_filename,
            &format!("https://unmapped.example/{second_filename}"),
            &canonical,
            Some(&proxy),
            hashes()?,
        )?;
        let original_first = first.clone();
        let original_second = second.clone();
        let (mut resolution, base_indexes, _) = resolver_output(
            vec![
                (
                    PackageName::from_str("first")?,
                    Dist::Built(BuiltDist::Registry(RegistryBuiltDist {
                        wheels: vec![first],
                        best_wheel_index: 0,
                        sdist: None,
                    })),
                ),
                (
                    PackageName::from_str("second")?,
                    Dist::Built(BuiltDist::Registry(RegistryBuiltDist {
                        wheels: vec![second],
                        best_wheel_index: 0,
                        sdist: None,
                    })),
                ),
            ],
            false,
        )?;

        assert!(matches!(
            resolution.canonicalize_proxy_artifact_urls_for_lock(&index_locations()?, &[]),
            Err(ProxyCanonicalizationError::ArtifactUrl {
                source,
                ..
            }) if matches!(source.as_ref(), ArtifactUrlMapError::Unmapped { .. })
        ));
        assert_eq!(wheel_at(&resolution, base_indexes[0], 0)?, &original_first);
        assert_eq!(wheel_at(&resolution, base_indexes[1], 0)?, &original_second);
        Ok(())
    }

    #[test]
    fn rejects_mismatched_physical_route() -> Result<(), Box<dyn Error>> {
        let canonical = IndexUrl::from_str(CANONICAL_INDEX)?;
        let physical = IndexUrl::from_str("https://other-proxy.example/simple")?;
        let filename = "example-1.0.0-py3-none-any.whl";
        let wheel = registry_wheel(
            filename,
            &format!("{PHYSICAL_PREFIX}/{filename}"),
            &canonical,
            Some(&physical),
            hashes()?,
        )?;
        let original = wheel.clone();
        let (mut resolution, base_indexes, _) = resolver_output(
            vec![(
                PackageName::from_str("example")?,
                Dist::Built(BuiltDist::Registry(RegistryBuiltDist {
                    wheels: vec![wheel],
                    best_wheel_index: 0,
                    sdist: None,
                })),
            )],
            false,
        )?;

        assert!(matches!(
            resolution.canonicalize_proxy_artifact_urls_for_lock(&index_locations()?, &[]),
            Err(ProxyCanonicalizationError::RouteMismatch { .. })
        ));
        assert_eq!(wheel_at(&resolution, base_indexes[0], 0)?, &original);
        Ok(())
    }

    #[test]
    fn rejects_missing_proxy_declaration() -> Result<(), Box<dyn Error>> {
        let canonical = IndexUrl::from_str(CANONICAL_INDEX)?;
        let physical = IndexUrl::from_str(PROXY_INDEX)?;
        let filename = "example-1.0.0-py3-none-any.whl";
        let wheel = registry_wheel(
            filename,
            &format!("{PHYSICAL_PREFIX}/{filename}"),
            &canonical,
            Some(&physical),
            hashes()?,
        )?;
        let original = wheel.clone();
        let (mut resolution, base_indexes, _) = resolver_output(
            vec![(
                PackageName::from_str("example")?,
                Dist::Built(BuiltDist::Registry(RegistryBuiltDist {
                    wheels: vec![wheel],
                    best_wheel_index: 0,
                    sdist: None,
                })),
            )],
            false,
        )?;
        let index_locations = IndexLocations::new(vec![Index::from(canonical)], Vec::new(), false);

        assert!(matches!(
            resolution.canonicalize_proxy_artifact_urls_for_lock(&index_locations, &[]),
            Err(ProxyCanonicalizationError::MissingProxyIndex { .. })
        ));
        assert_eq!(wheel_at(&resolution, base_indexes[0], 0)?, &original);
        Ok(())
    }

    fn index_locations() -> Result<IndexLocations, Box<dyn Error>> {
        let canonical = IndexUrl::from_str(CANONICAL_INDEX)?;
        Ok(
            IndexLocations::new(vec![Index::from(canonical.clone())], Vec::new(), false)
                .with_proxy_indexes(vec![ProxyIndex {
                    index: IndexReference::Url(canonical),
                    url: IndexUrl::from_str(PROXY_INDEX)?,
                    artifact_url_map: ArtifactUrlMap::single(
                        DisplaySafeUrl::parse(PHYSICAL_PREFIX)?,
                        DisplaySafeUrl::parse(CANONICAL_PREFIX)?,
                    ),
                }]),
        )
    }

    fn resolver_output(
        dists: Vec<(PackageName, Dist)>,
        add_extra: bool,
    ) -> Result<ResolverFixture, Box<dyn Error>> {
        let version = Version::from_str("1.0.0")?;
        let mut graph: Graph<ResolutionGraphNode, UniversalMarker, Directed> = Graph::new();
        graph.add_node(ResolutionGraphNode::Root);
        let mut base_indexes = Vec::with_capacity(dists.len());
        let mut extra_index = None;
        for (position, (name, dist)) in dists.into_iter().enumerate() {
            let annotated = AnnotatedDist {
                dist: ResolvedDist::Installable {
                    dist: Arc::new(dist),
                    version: Some(version.clone()),
                },
                name,
                version: version.clone(),
                extra: None,
                group: None,
                hashes: HashDigests::empty(),
                metadata: None,
                marker: UniversalMarker::TRUE,
            };
            base_indexes.push(graph.add_node(ResolutionGraphNode::Dist(annotated.clone())));
            if add_extra && position == 0 {
                extra_index = Some(graph.add_node(ResolutionGraphNode::Dist(AnnotatedDist {
                    extra: Some(ExtraName::from_str("feature")?),
                    ..annotated
                })));
            }
        }
        Ok((
            ResolverOutput {
                graph,
                requires_python: RequiresPython::greater_than_equal_version(&Version::from_str(
                    "3.12",
                )?),
                fork_markers: vec![],
                diagnostics: vec![],
                requirements: vec![],
                constraints: Constraints::default(),
                overrides: Overrides::default(),
                options: Options::default(),
            },
            base_indexes,
            extra_index,
        ))
    }

    fn registry_wheel(
        filename: &str,
        url: &str,
        canonical: &IndexUrl,
        proxy: Option<&IndexUrl>,
        hashes: HashDigests,
    ) -> Result<RegistryBuiltWheel, Box<dyn Error>> {
        Ok(RegistryBuiltWheel {
            filename: WheelFilename::from_str(filename)?,
            file: Box::new(registry_file(filename, url, hashes)?),
            index: canonical.clone(),
            proxy: proxy.cloned(),
        })
    }

    fn registry_file(
        filename: &str,
        url: &str,
        hashes: HashDigests,
    ) -> Result<File, Box<dyn Error>> {
        Ok(File {
            dist_info_metadata: true,
            filename: filename.into(),
            hashes,
            requires_python: Some(Arc::new(VersionSpecifiers::from_str(">=3.9")?)),
            size: Some(42),
            upload_time_utc_ms: Some(1234),
            url: FileLocation::new(url.into(), &"".into()),
            yanked: Some(Box::new(Yanked::Reason("proxy reason".into()))),
            zstd: Some(Box::new(Zstd {
                hashes: HashDigests::from(HashDigest::from_str(SHA256)?),
                size: Some(21),
            })),
        })
    }

    fn hashes() -> Result<HashDigests, uv_pypi_types::HashError> {
        Ok(HashDigests::from(HashDigest::from_str(SHA256)?))
    }

    fn installable_dist_at(
        resolution: &ResolverOutput,
        node_index: NodeIndex,
    ) -> Result<&Arc<Dist>, Box<dyn Error>> {
        let ResolutionGraphNode::Dist(annotated) = &resolution.graph[node_index] else {
            return Err("expected distribution node".into());
        };
        let ResolvedDist::Installable { dist, .. } = &annotated.dist else {
            return Err("expected installable distribution".into());
        };
        Ok(dist)
    }

    fn registry_wheel_from_dist(
        dist: &Dist,
        wheel_index: usize,
    ) -> Result<&RegistryBuiltWheel, Box<dyn Error>> {
        let Dist::Built(BuiltDist::Registry(registry)) = dist else {
            return Err("expected registry built distribution".into());
        };
        registry
            .wheels
            .get(wheel_index)
            .ok_or_else(|| "expected registry wheel".into())
    }

    fn wheel_at(
        resolution: &ResolverOutput,
        node_index: NodeIndex,
        wheel_index: usize,
    ) -> Result<&RegistryBuiltWheel, Box<dyn Error>> {
        registry_wheel_from_dist(
            installable_dist_at(resolution, node_index)?.as_ref(),
            wheel_index,
        )
    }

    fn source_at(
        resolution: &ResolverOutput,
        node_index: NodeIndex,
    ) -> Result<&RegistrySourceDist, Box<dyn Error>> {
        let Dist::Source(SourceDist::Registry(source)) =
            installable_dist_at(resolution, node_index)?.as_ref()
        else {
            return Err("expected registry source distribution".into());
        };
        Ok(source)
    }
}
