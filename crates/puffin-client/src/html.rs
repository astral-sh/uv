use std::str::FromStr;

use tl::HTMLTag;
use tracing::instrument;
use url::Url;

use pep440_rs::VersionSpecifiers;
use pypi_types::{BaseUrl, DistInfoMetadata, File, Hashes, Yanked};

#[derive(Debug, Clone)]
pub(crate) struct SimpleHtml {
    /// The [`BaseUrl`] to which all relative URLs should be resolved.
    pub(crate) base: BaseUrl,
    /// The list of [`File`]s available for download.
    pub(crate) files: Vec<File>,
}

impl SimpleHtml {
    /// Parse the list of [`File`]s from the simple HTML page returned by the given URL.
    #[instrument(skip(text))]
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
        let files: Vec<File> = dom
            .nodes()
            .iter()
            .filter_map(|node| node.as_tag())
            .filter(|link| link.name().as_bytes() == b"a")
            .map(|link| Self::parse_anchor(link))
            .collect::<Result<Vec<_>, _>>()?;

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

    /// Parse the hash from a fragment, as in: `sha256=6088930bfe239f0e6710546ab9c19c9ef35e29792895fed6e6e31a023a182a61`
    fn parse_hash(fragment: &str) -> Result<Hashes, Error> {
        let mut parts = fragment.split('=');

        // Extract the key and value.
        let name = parts
            .next()
            .ok_or_else(|| Error::FragmentParse(fragment.to_string()))?;
        let value = parts
            .next()
            .ok_or_else(|| Error::FragmentParse(fragment.to_string()))?;

        // Ensure there are no more parts.
        if parts.next().is_some() {
            return Err(Error::FragmentParse(fragment.to_string()));
        }

        // TODO(charlie): Support all hash algorithms.
        if name != "sha256" {
            return Err(Error::UnsupportedHashAlgorithm(fragment.to_string()));
        }

        let sha256 = std::str::from_utf8(value.as_bytes())?;
        let sha256 = sha256.to_string();
        Ok(Hashes { sha256 })
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

        // Split the base and the fragment.
        let (path, fragment) = href
            .split_once('#')
            .ok_or_else(|| Error::MissingHash(href.to_string()))?;

        // Extract the filename from the body text, which MUST match that of
        // the final path component of the URL.
        let filename = path
            .split('/')
            .last()
            .ok_or_else(|| Error::MissingFilename(href.to_string()))?;

        // Extract the hash, which should be in the fragment.
        let hashes = Self::parse_hash(fragment)?;

        // Extract the `requires-python` field, which should be set on the
        // `data-requires-python` attribute.
        let requires_python = if let Some(requires_python) =
            link.attributes().get("data-requires-python").flatten()
        {
            let requires_python = std::str::from_utf8(requires_python.as_bytes())?;
            let requires_python = html_escape::decode_html_entities(requires_python);
            Some(VersionSpecifiers::from_str(&requires_python))
        } else {
            None
        };

        // Extract the `data-dist-info-metadata` field, which should be set on
        // the `data-dist-info-metadata` attribute.
        let dist_info_metadata = if let Some(dist_info_metadata) =
            link.attributes().get("data-dist-info-metadata").flatten()
        {
            let dist_info_metadata = std::str::from_utf8(dist_info_metadata.as_bytes())?;
            let dist_info_metadata = html_escape::decode_html_entities(dist_info_metadata);
            match dist_info_metadata.as_ref() {
                "true" => Some(DistInfoMetadata::Bool(true)),
                "false" => Some(DistInfoMetadata::Bool(false)),
                fragment => Some(DistInfoMetadata::Hashes(Self::parse_hash(fragment)?)),
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
            dist_info_metadata,
            yanked,
            requires_python,
            hashes,
            filename: filename.to_string(),
            url: href.to_string(),
            size: None,
            upload_time: None,
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Utf8(#[from] std::str::Utf8Error),

    #[error("Failed to parse URL: {0}")]
    UrlParse(String, #[source] url::ParseError),

    #[error(transparent)]
    HtmlParse(#[from] tl::ParseError),

    #[error("Missing href attribute on anchor link")]
    MissingHref,

    #[error("Expected distribution filename as last path component of URL: {0}")]
    MissingFilename(String),

    #[error("Missing hash attribute on URL: {0}")]
    MissingHash(String),

    #[error("Unexpected fragment (expected `#sha256=...`) on URL: {0}")]
    FragmentParse(String),

    #[error("Unsupported hash algorithm (expected `sha256`) on: {0}")]
    UnsupportedHashAlgorithm(String),

    #[error("Invalid `requires-python` specifier: {0}")]
    Pep440(#[source] pep440_rs::VersionSpecifiersParseError),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_file() {
        let text = r#"
<!DOCTYPE html>
<html>
  <body>
    <h1>Links for jinja2</h1>
    <a href="/whl/Jinja2-3.1.2-py3-none-any.whl#sha256=6088930bfe239f0e6710546ab9c19c9ef35e29792895fed6e6e31a023a182a61">Jinja2-3.1.2-py3-none-any.whl</a><br/>
  </body>
</html>
<!--TIMESTAMP 1703347410-->
        "#;
        let base = Url::parse("https://download.pytorch.org/whl/jinja2/").unwrap();
        let result = SimpleHtml::parse(text, &base).unwrap();
        insta::assert_debug_snapshot!(result, @r###"
        SimpleHtml {
            base: BaseUrl(
                Url {
                    scheme: "https",
                    cannot_be_a_base: false,
                    username: "",
                    password: None,
                    host: Some(
                        Domain(
                            "download.pytorch.org",
                        ),
                    ),
                    port: None,
                    path: "/whl/jinja2/",
                    query: None,
                    fragment: None,
                },
            ),
            files: [
                File {
                    dist_info_metadata: None,
                    filename: "Jinja2-3.1.2-py3-none-any.whl",
                    hashes: Hashes {
                        sha256: "6088930bfe239f0e6710546ab9c19c9ef35e29792895fed6e6e31a023a182a61",
                    },
                    requires_python: None,
                    size: None,
                    upload_time: None,
                    url: "/whl/Jinja2-3.1.2-py3-none-any.whl#sha256=6088930bfe239f0e6710546ab9c19c9ef35e29792895fed6e6e31a023a182a61",
                    yanked: None,
                },
            ],
        }
        "###);
    }

    #[test]
    fn parse_base() {
        let text = r#"
<!DOCTYPE html>
<html>
  <head>
    <base href="https://index.python.org/">
  </head>
  <body>
    <h1>Links for jinja2</h1>
    <a href="/whl/Jinja2-3.1.2-py3-none-any.whl#sha256=6088930bfe239f0e6710546ab9c19c9ef35e29792895fed6e6e31a023a182a61">Jinja2-3.1.2-py3-none-any.whl</a><br/>
  </body>
</html>
<!--TIMESTAMP 1703347410-->
        "#;
        let base = Url::parse("https://download.pytorch.org/whl/jinja2/").unwrap();
        let result = SimpleHtml::parse(text, &base).unwrap();
        insta::assert_debug_snapshot!(result, @r###"
        SimpleHtml {
            base: BaseUrl(
                Url {
                    scheme: "https",
                    cannot_be_a_base: false,
                    username: "",
                    password: None,
                    host: Some(
                        Domain(
                            "index.python.org",
                        ),
                    ),
                    port: None,
                    path: "/",
                    query: None,
                    fragment: None,
                },
            ),
            files: [
                File {
                    dist_info_metadata: None,
                    filename: "Jinja2-3.1.2-py3-none-any.whl",
                    hashes: Hashes {
                        sha256: "6088930bfe239f0e6710546ab9c19c9ef35e29792895fed6e6e31a023a182a61",
                    },
                    requires_python: None,
                    size: None,
                    upload_time: None,
                    url: "/whl/Jinja2-3.1.2-py3-none-any.whl#sha256=6088930bfe239f0e6710546ab9c19c9ef35e29792895fed6e6e31a023a182a61",
                    yanked: None,
                },
            ],
        }
        "###);
    }

    #[test]
    fn parse_missing_href() {
        let text = r#"
<!DOCTYPE html>
<html>
  <body>
    <h1>Links for jinja2</h1>
    <a>Jinja2-3.1.2-py3-none-any.whl</a><br/>
  </body>
</html>
<!--TIMESTAMP 1703347410-->
        "#;
        let base = Url::parse("https://download.pytorch.org/whl/jinja2/").unwrap();
        let result = SimpleHtml::parse(text, &base).unwrap_err();
        insta::assert_display_snapshot!(result, @"Missing href attribute on anchor link");
    }

    #[test]
    fn parse_empty_href() {
        let text = r#"
<!DOCTYPE html>
<html>
  <body>
    <h1>Links for jinja2</h1>
    <a href="">Jinja2-3.1.2-py3-none-any.whl</a><br/>
  </body>
</html>
<!--TIMESTAMP 1703347410-->
        "#;
        let base = Url::parse("https://download.pytorch.org/whl/jinja2/").unwrap();
        let result = SimpleHtml::parse(text, &base).unwrap_err();
        insta::assert_display_snapshot!(result, @"Missing href attribute on anchor link");
    }

    #[test]
    fn parse_missing_hash() {
        let text = r#"
<!DOCTYPE html>
<html>
  <body>
    <h1>Links for jinja2</h1>
    <a href="/whl/Jinja2-3.1.2-py3-none-any.whl">Jinja2-3.1.2-py3-none-any.whl</a><br/>
  </body>
</html>
<!--TIMESTAMP 1703347410-->
        "#;
        let base = Url::parse("https://download.pytorch.org/whl/jinja2/").unwrap();
        let result = SimpleHtml::parse(text, &base).unwrap_err();
        insta::assert_display_snapshot!(result, @"Missing hash attribute on URL: /whl/Jinja2-3.1.2-py3-none-any.whl");
    }

    #[test]
    fn parse_missing_hash_value() {
        let text = r#"
<!DOCTYPE html>
<html>
  <body>
    <h1>Links for jinja2</h1>
    <a href="/whl/Jinja2-3.1.2-py3-none-any.whl#sha256">Jinja2-3.1.2-py3-none-any.whl</a><br/>
  </body>
</html>
<!--TIMESTAMP 1703347410-->
        "#;
        let base = Url::parse("https://download.pytorch.org/whl/jinja2/").unwrap();
        let result = SimpleHtml::parse(text, &base).unwrap_err();
        insta::assert_display_snapshot!(result, @"Unexpected fragment (expected `#sha256=...`) on URL: sha256");
    }

    #[test]
    fn parse_unknown_hash() {
        let text = r#"
<!DOCTYPE html>
<html>
  <body>
    <h1>Links for jinja2</h1>
    <a href="/whl/Jinja2-3.1.2-py3-none-any.whl#sha512=6088930bfe239f0e6710546ab9c19c9ef35e29792895fed6e6e31a023a182a61">Jinja2-3.1.2-py3-none-any.whl</a><br/>
  </body>
</html>
<!--TIMESTAMP 1703347410-->
        "#;
        let base = Url::parse("https://download.pytorch.org/whl/jinja2/").unwrap();
        let result = SimpleHtml::parse(text, &base).unwrap_err();
        insta::assert_display_snapshot!(result, @"Unsupported hash algorithm (expected `sha256`) on: sha512=6088930bfe239f0e6710546ab9c19c9ef35e29792895fed6e6e31a023a182a61");
    }
}
