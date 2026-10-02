use uv_distribution_types::{ToUrlError, UrlString};
use uv_pep508::split_scheme;
use uv_redacted::DisplaySafeUrl;

use super::{Package, SourceDist, WheelWireSource};

/// A directory URL shared by a package's remote artifacts.
#[derive(Clone, Debug, serde::Deserialize)]
#[serde(try_from = "DisplaySafeUrl")]
pub(super) struct ArtifactBase(DisplaySafeUrl);

impl ArtifactBase {
    /// Finds a common directory prefix when factoring it out reduces the serialized size.
    pub(super) fn for_package(package: &Package) -> Option<Self> {
        let urls = package
            .sdist
            .iter()
            .filter_map(SourceDist::url)
            .chain(package.wheels.iter().filter_map(|wheel| match &wheel.url {
                WheelWireSource::Url { url } => Some(url),
                WheelWireSource::Path { .. } | WheelWireSource::Filename { .. } => None,
            }))
            .collect::<Vec<_>>();
        if urls.len() < 2 {
            return None;
        }
        let first = urls.first()?.base_str();
        let mut prefix = &first[..=first.rfind('/')?];
        for url in &urls {
            while !url.as_ref().starts_with(prefix) {
                prefix = &prefix[..=prefix[..prefix.len() - 1].rfind('/')?];
            }
        }
        if (urls.len() - 1) * prefix.len() <= "artifact-base = \"\"\n".len() {
            return None;
        }
        let base = Self::try_from(DisplaySafeUrl::parse(prefix).ok()?).ok()?;
        // URL resolution can normalize paths. Only shorten URLs that reconstruct exactly.
        urls.iter()
            .all(|url| base.relative(url).is_some())
            .then_some(base)
    }

    /// Returns the base URL as it appears in the lockfile.
    pub(super) fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Returns a relative URL only if resolving it recovers the original URL byte for byte.
    pub(super) fn relative<'a>(&self, url: &'a UrlString) -> Option<&'a str> {
        let relative = url.as_ref().strip_prefix(self.as_str())?;
        let absolute = self.0.join(relative).ok()?;
        (absolute.as_str() == url.as_ref()).then_some(relative)
    }

    /// Expands relative artifact URLs before any installer or resolver consumes them.
    pub(super) fn expand(&self, url: &mut UrlString) -> Result<(), ToUrlError> {
        if split_scheme(url.as_ref()).is_none() {
            *url = self
                .0
                .join(url.as_ref())
                .map_err(|err| ToUrlError::InvalidJoin {
                    base: self.as_str().to_owned(),
                    path: url.to_string(),
                    err,
                })?
                .into();
        }
        Ok(())
    }
}

impl TryFrom<DisplaySafeUrl> for ArtifactBase {
    type Error = &'static str;

    fn try_from(url: DisplaySafeUrl) -> Result<Self, Self::Error> {
        if !["http", "https"].contains(&url.scheme())
            || url.host_str().is_none()
            || !url.path().ends_with('/')
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(
                "artifact-base must be an HTTP(S) directory URL without a query or fragment",
            );
        }
        Ok(Self(url))
    }
}
