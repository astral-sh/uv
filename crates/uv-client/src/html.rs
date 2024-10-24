use std::str::FromStr;

use tl::HTMLTag;
use tracing::{instrument, warn};
use url::Url;

use uv_pep440::VersionSpecifiers;
use uv_pypi_types::LenientVersionSpecifiers;
use uv_pypi_types::{BaseUrl, CoreMetadata, File, Hashes, Yanked};

/// A parsed structure from PyPI "HTML" index format for a single package.
#[derive(Debug, Clone)]
pub(crate) struct SimpleHtml {
    /// The [`BaseUrl`] to which all relative URLs should be resolved.
    pub(crate) base: BaseUrl,
    /// The list of [`File`]s available for download sorted by filename.
    pub(crate) files: Vec<File>,
}

impl SimpleHtml {
    /// Parse the list of [`File`]s from the simple HTML page returned by the given URL.
    #[instrument(skip_all, fields(url = % url))]
    pub(crate) fn parse(text: &str, url: &Url) -> Result<Self, Error> {
        let dom = tl::parse(text, tl::ParserOptions::default())?;

        // Parse the first `<base>` tag, if any, to determine the base URL to which all
        // relative URLs should be resolved. The HTML spec requires that the `<base>` tag
        // appear before other tags with attribute values of URLs.
        let base = BaseUrl::from(
            dom.nodes()
                .iter()
                .filter_map(|node| node.as_tag())
                .take_while(|tag| !matches!(tag.name().as_bytes(), b"a" | b"link"))
                .find(|tag| tag.name().as_bytes() == b"base")
                .map(|base| Self::parse_base(base))
                .transpose()?
                .flatten()
                .unwrap_or_else(|| url.clone()),
        );

        // Parse each `<a>` tag, to extract the filename, hash, and URL.
        let mut files: Vec<File> = dom
            .nodes()
            .iter()
            .filter_map(|node| node.as_tag())
            .filter(|link| link.name().as_bytes() == b"a")
            .map(|link| Self::parse_anchor(link))
            .collect::<Result<Vec<_>, _>>()?;
        // While it has not been positively observed, we sort the files
        // to ensure we have a defined ordering. Otherwise, if we rely on
        // the API to provide a stable ordering and doesn't, it can lead
        // non-deterministic behavior elsewhere. (This is somewhat hand-wavy
        // and a bit of a band-aide, since arguably, the order of this API
        // response probably shouldn't have an impact on things downstream from
        // this. That is, if something depends on ordering, then it should
        // probably be the thing that does the sorting.)
        files.sort_unstable_by(|f1, f2| f1.filename.cmp(&f2.filename));

        Ok(Self { base, files })
    }

    /// Parse the `href` from a `<base>` tag.
    fn parse_base(base: &HTMLTag) -> Result<Option<Url>, Error> {
        let Some(Some(href)) = base.attributes().get("href") else {
            return Ok(None);
        };
        let href = std::str::from_utf8(href.as_bytes())?;
        let url = Url::parse(href).map_err(|err| Error::UrlParse(href.to_string(), err))?;
        Ok(Some(url))
    }

    /// Parse a [`File`] from an `<a>` tag.
    fn parse_anchor(link: &HTMLTag) -> Result<File, Error> {
        // Extract the href.
        let href = link
            .attributes()
            .get("href")
            .flatten()
            .filter(|bytes| !bytes.as_bytes().is_empty())
            .ok_or(Error::MissingHref)?;
        let href = std::str::from_utf8(href.as_bytes())?;

        // Extract the hash, which should be in the fragment.
        let decoded = html_escape::decode_html_entities(href);
        let (path, hashes) = if let Some((path, fragment)) = decoded.split_once('#') {
            let fragment = urlencoding::decode(fragment)?;
            (
                path,
                if fragment.trim().is_empty() {
                    Hashes::default()
                } else {
                    Hashes::parse_fragment(&fragment)?
                },
            )
        } else {
            (decoded.as_ref(), Hashes::default())
        };

        // Extract the filename from the body text, which MUST match that of
        // the final path component of the URL.
        let filename = path
            .split('/')
            .last()
            .ok_or_else(|| Error::MissingFilename(href.to_string()))?;

        // Strip any query string from the filename.
        let filename = filename.split('?').next().unwrap_or(filename);

        // Unquote the filename.
        let filename = urlencoding::decode(filename)
            .map_err(|_| Error::UnsupportedFilename(filename.to_string()))?;

        // Extract the `requires-python` value, which should be set on the
        // `data-requires-python` attribute.
        let requires_python = if let Some(requires_python) =
            link.attributes().get("data-requires-python").flatten()
        {
            let requires_python = std::str::from_utf8(requires_python.as_bytes())?;
            let requires_python = html_escape::decode_html_entities(requires_python);
            Some(LenientVersionSpecifiers::from_str(&requires_python).map(VersionSpecifiers::from))
        } else {
            None
        };

        // Extract the `core-metadata` field, which is either set on:
        // - `data-core-metadata`, per PEP 714.
        // - `data-dist-info-metadata`, per PEP 658.
        let core_metadata = if let Some(dist_info_metadata) = link
            .attributes()
            .get("data-core-metadata")
            .flatten()
            .or_else(|| link.attributes().get("data-dist-info-metadata").flatten())
        {
            let dist_info_metadata = std::str::from_utf8(dist_info_metadata.as_bytes())?;
            let dist_info_metadata = html_escape::decode_html_entities(dist_info_metadata);
            match dist_info_metadata.as_ref() {
                "true" => Some(CoreMetadata::Bool(true)),
                "false" => Some(CoreMetadata::Bool(false)),
                fragment => match Hashes::parse_fragment(fragment) {
                    Ok(hash) => Some(CoreMetadata::Hashes(hash)),
                    Err(err) => {
                        warn!("Failed to parse core metadata value `{fragment}`: {err}");
                        None
                    }
                },
            }
        } else {
            None
        };

        // Extract the `yanked` field, which should be set on the `data-yanked`
        // attribute.
        let yanked = if let Some(yanked) = link.attributes().get("data-yanked").flatten() {
            let yanked = std::str::from_utf8(yanked.as_bytes())?;
            let yanked = html_escape::decode_html_entities(yanked);
            Some(Yanked::Reason(yanked.to_string()))
        } else {
            None
        };

        Ok(File {
            core_metadata,
            dist_info_metadata: None,
            data_dist_info_metadata: None,
            yanked,
            requires_python,
            hashes,
            filename: filename.to_string(),
            url: decoded.to_string(),
            size: None,
            upload_time: None,
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Utf8(#[from] std::str::Utf8Error),

    #[error(transparent)]
    FromUtf8(#[from] std::string::FromUtf8Error),

    #[error("Failed to parse URL: {0}")]
    UrlParse(String, #[source] url::ParseError),

    #[error(transparent)]
    HtmlParse(#[from] tl::ParseError),

    #[error("Missing href attribute on anchor link")]
    MissingHref,

    #[error("Expected distribution filename as last path component of URL: {0}")]
    MissingFilename(String),

    #[error("Expected distribution filename to be UTF-8: {0}")]
    UnsupportedFilename(String),

    #[error("Missing hash attribute on URL: {0}")]
    MissingHash(String),

    #[error(transparent)]
    FragmentParse(#[from] uv_pypi_types::HashError),

    #[error("Invalid `requires-python` specifier: {0}")]
    Pep440(#[source] uv_pep440::VersionSpecifiersParseError),
}

#[cfg(test)]
mod tests;
