use std::str::FromStr;

use tl::HTMLTag;
use tracing::instrument;
use url::Url;

use pep440_rs::VersionSpecifiers;
use pypi_types::LenientVersionSpecifiers;
use pypi_types::{BaseUrl, CoreMetadata, File, Hashes, Yanked};

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

        match name {
            "md5" => {
                let md5 = std::str::from_utf8(value.as_bytes())?;
                let md5 = md5.to_owned().into_boxed_str();
                Ok(Hashes {
                    md5: Some(md5),
                    sha256: None,
                    sha384: None,
                    sha512: None,
                })
            }
            "sha256" => {
                let sha256 = std::str::from_utf8(value.as_bytes())?;
                let sha256 = sha256.to_owned().into_boxed_str();
                Ok(Hashes {
                    md5: None,
                    sha256: Some(sha256),
                    sha384: None,
                    sha512: None,
                })
            }
            "sha384" => {
                let sha384 = std::str::from_utf8(value.as_bytes())?;
                let sha384 = sha384.to_owned().into_boxed_str();
                Ok(Hashes {
                    md5: None,
                    sha256: None,
                    sha384: Some(sha384),
                    sha512: None,
                })
            }
            "sha512" => {
                let sha512 = std::str::from_utf8(value.as_bytes())?;
                let sha512 = sha512.to_owned().into_boxed_str();
                Ok(Hashes {
                    md5: None,
                    sha256: None,
                    sha384: None,
                    sha512: Some(sha512),
                })
            }
            _ => Err(Error::UnsupportedHashAlgorithm(fragment.to_string())),
        }
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
            let fragment = urlencoding::decode(fragment)
                .map_err(|_| Error::FragmentParse(fragment.to_string()))?;
            (
                path,
                if fragment.trim().is_empty() {
                    Hashes::default()
                } else {
                    Self::parse_hash(&fragment)?
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
                fragment => Some(CoreMetadata::Hashes(Self::parse_hash(fragment)?)),
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

    #[error("Unexpected fragment (expected `#sha256=...` or similar) on URL: {0}")]
    FragmentParse(String),

    #[error(
        "Unsupported hash algorithm (expected `md5`, `sha256`, `sha384`, or `sha512`) on: {0}"
    )]
    UnsupportedHashAlgorithm(String),

    #[error("Invalid `requires-python` specifier: {0}")]
    Pep440(#[source] pep440_rs::VersionSpecifiersParseError),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_sha256() {
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
                    core_metadata: None,
                    dist_info_metadata: None,
                    data_dist_info_metadata: None,
                    filename: "Jinja2-3.1.2-py3-none-any.whl",
                    hashes: Hashes {
                        md5: None,
                        sha256: Some(
                            "6088930bfe239f0e6710546ab9c19c9ef35e29792895fed6e6e31a023a182a61",
                        ),
                        sha384: None,
                        sha512: None,
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
    fn parse_md5() {
        let text = r#"
<!DOCTYPE html>
<html>
  <body>
    <h1>Links for jinja2</h1>
    <a href="/whl/Jinja2-3.1.2-py3-none-any.whl#md5=6088930bfe239f0e6710546ab9c19c9ef35e29792895fed6e6e31a023a182a61">Jinja2-3.1.2-py3-none-any.whl</a><br/>
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
                    core_metadata: None,
                    dist_info_metadata: None,
                    data_dist_info_metadata: None,
                    filename: "Jinja2-3.1.2-py3-none-any.whl",
                    hashes: Hashes {
                        md5: Some(
                            "6088930bfe239f0e6710546ab9c19c9ef35e29792895fed6e6e31a023a182a61",
                        ),
                        sha256: None,
                        sha384: None,
                        sha512: None,
                    },
                    requires_python: None,
                    size: None,
                    upload_time: None,
                    url: "/whl/Jinja2-3.1.2-py3-none-any.whl#md5=6088930bfe239f0e6710546ab9c19c9ef35e29792895fed6e6e31a023a182a61",
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
                    core_metadata: None,
                    dist_info_metadata: None,
                    data_dist_info_metadata: None,
                    filename: "Jinja2-3.1.2-py3-none-any.whl",
                    hashes: Hashes {
                        md5: None,
                        sha256: Some(
                            "6088930bfe239f0e6710546ab9c19c9ef35e29792895fed6e6e31a023a182a61",
                        ),
                        sha384: None,
                        sha512: None,
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
    fn parse_escaped_fragment() {
        let text = r#"
<!DOCTYPE html>
<html>
  <body>
    <h1>Links for jinja2</h1>
    <a href="/whl/Jinja2-3.1.2&#43;233fca715f49-py3-none-any.whl#sha256=6088930bfe239f0e6710546ab9c19c9ef35e29792895fed6e6e31a023a182a61">Jinja2-3.1.2+233fca715f49-py3-none-any.whl</a><br/>
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
                    core_metadata: None,
                    dist_info_metadata: None,
                    data_dist_info_metadata: None,
                    filename: "Jinja2-3.1.2+233fca715f49-py3-none-any.whl",
                    hashes: Hashes {
                        md5: None,
                        sha256: Some(
                            "6088930bfe239f0e6710546ab9c19c9ef35e29792895fed6e6e31a023a182a61",
                        ),
                        sha384: None,
                        sha512: None,
                    },
                    requires_python: None,
                    size: None,
                    upload_time: None,
                    url: "/whl/Jinja2-3.1.2+233fca715f49-py3-none-any.whl#sha256=6088930bfe239f0e6710546ab9c19c9ef35e29792895fed6e6e31a023a182a61",
                    yanked: None,
                },
            ],
        }
        "###);
    }

    #[test]
    fn parse_encoded_fragment() {
        let text = r#"
<!DOCTYPE html>
<html>
  <body>
    <h1>Links for jinja2</h1>
    <a href="/whl/Jinja2-3.1.2-py3-none-any.whl#sha256%3D4095ada29e51070f7d199a0a5bdf5c8d8e238e03f0bf4dcc02571e78c9ae800d">Jinja2-3.1.2-py3-none-any.whl</a><br/>
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
                    core_metadata: None,
                    dist_info_metadata: None,
                    data_dist_info_metadata: None,
                    filename: "Jinja2-3.1.2-py3-none-any.whl",
                    hashes: Hashes {
                        md5: None,
                        sha256: Some(
                            "4095ada29e51070f7d199a0a5bdf5c8d8e238e03f0bf4dcc02571e78c9ae800d",
                        ),
                        sha384: None,
                        sha512: None,
                    },
                    requires_python: None,
                    size: None,
                    upload_time: None,
                    url: "/whl/Jinja2-3.1.2-py3-none-any.whl#sha256%3D4095ada29e51070f7d199a0a5bdf5c8d8e238e03f0bf4dcc02571e78c9ae800d",
                    yanked: None,
                },
            ],
        }
        "###);
    }

    #[test]
    fn parse_quoted_filepath() {
        let text = r#"
<!DOCTYPE html>
<html>
  <body>
    <h1>Links for jinja2</h1>
    <a href="cpu/torchtext-0.17.0%2Bcpu-cp39-cp39-win_amd64.whl">cpu/torchtext-0.17.0%2Bcpu-cp39-cp39-win_amd64.whl</a><br/>
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
                    core_metadata: None,
                    dist_info_metadata: None,
                    data_dist_info_metadata: None,
                    filename: "torchtext-0.17.0+cpu-cp39-cp39-win_amd64.whl",
                    hashes: Hashes {
                        md5: None,
                        sha256: None,
                        sha384: None,
                        sha512: None,
                    },
                    requires_python: None,
                    size: None,
                    upload_time: None,
                    url: "cpu/torchtext-0.17.0%2Bcpu-cp39-cp39-win_amd64.whl",
                    yanked: None,
                },
            ],
        }
        "###);
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
                    core_metadata: None,
                    dist_info_metadata: None,
                    data_dist_info_metadata: None,
                    filename: "Jinja2-3.1.2-py3-none-any.whl",
                    hashes: Hashes {
                        md5: None,
                        sha256: None,
                        sha384: None,
                        sha512: None,
                    },
                    requires_python: None,
                    size: None,
                    upload_time: None,
                    url: "/whl/Jinja2-3.1.2-py3-none-any.whl",
                    yanked: None,
                },
            ],
        }
        "###);
    }

    #[test]
    fn parse_missing_href() {
        let text = r"
<!DOCTYPE html>
<html>
  <body>
    <h1>Links for jinja2</h1>
    <a>Jinja2-3.1.2-py3-none-any.whl</a><br/>
  </body>
</html>
<!--TIMESTAMP 1703347410-->
        ";
        let base = Url::parse("https://download.pytorch.org/whl/jinja2/").unwrap();
        let result = SimpleHtml::parse(text, &base).unwrap_err();
        insta::assert_snapshot!(result, @"Missing href attribute on anchor link");
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
        insta::assert_snapshot!(result, @"Missing href attribute on anchor link");
    }

    #[test]
    fn parse_empty_fragment() {
        let text = r#"
<!DOCTYPE html>
<html>
  <body>
    <h1>Links for jinja2</h1>
    <a href="/whl/Jinja2-3.1.2-py3-none-any.whl#">Jinja2-3.1.2-py3-none-any.whl</a><br/>
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
                    core_metadata: None,
                    dist_info_metadata: None,
                    data_dist_info_metadata: None,
                    filename: "Jinja2-3.1.2-py3-none-any.whl",
                    hashes: Hashes {
                        md5: None,
                        sha256: None,
                        sha384: None,
                        sha512: None,
                    },
                    requires_python: None,
                    size: None,
                    upload_time: None,
                    url: "/whl/Jinja2-3.1.2-py3-none-any.whl#",
                    yanked: None,
                },
            ],
        }
        "###);
    }

    #[test]
    fn parse_query_string() {
        let text = r#"
<!DOCTYPE html>
<html>
  <body>
    <h1>Links for jinja2</h1>
    <a href="/whl/Jinja2-3.1.2-py3-none-any.whl?project=legacy">Jinja2-3.1.2-py3-none-any.whl</a><br/>
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
                    core_metadata: None,
                    dist_info_metadata: None,
                    data_dist_info_metadata: None,
                    filename: "Jinja2-3.1.2-py3-none-any.whl",
                    hashes: Hashes {
                        md5: None,
                        sha256: None,
                        sha384: None,
                        sha512: None,
                    },
                    requires_python: None,
                    size: None,
                    upload_time: None,
                    url: "/whl/Jinja2-3.1.2-py3-none-any.whl?project=legacy",
                    yanked: None,
                },
            ],
        }
        "###);
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
        insta::assert_snapshot!(result, @"Unexpected fragment (expected `#sha256=...` or similar) on URL: sha256");
    }

    #[test]
    fn parse_unknown_hash() {
        let text = r#"
<!DOCTYPE html>
<html>
  <body>
    <h1>Links for jinja2</h1>
    <a href="/whl/Jinja2-3.1.2-py3-none-any.whl#blake2=6088930bfe239f0e6710546ab9c19c9ef35e29792895fed6e6e31a023a182a61">Jinja2-3.1.2-py3-none-any.whl</a><br/>
  </body>
</html>
<!--TIMESTAMP 1703347410-->
        "#;
        let base = Url::parse("https://download.pytorch.org/whl/jinja2/").unwrap();
        let result = SimpleHtml::parse(text, &base).unwrap_err();
        insta::assert_snapshot!(result, @"Unsupported hash algorithm (expected `md5`, `sha256`, `sha384`, or `sha512`) on: blake2=6088930bfe239f0e6710546ab9c19c9ef35e29792895fed6e6e31a023a182a61");
    }

    #[test]
    fn parse_flat_index_html() {
        let text = r#"
            <!DOCTYPE html>
            <html>
            <head><meta http-equiv="Content-Type" content="text/html; charset=utf-8"></head>
            <body>
                <a href="https://storage.googleapis.com/jax-releases/cuda100/jaxlib-0.1.52+cuda100-cp36-none-manylinux2010_x86_64.whl">cuda100/jaxlib-0.1.52+cuda100-cp36-none-manylinux2010_x86_64.whl</a><br>
                <a href="https://storage.googleapis.com/jax-releases/cuda100/jaxlib-0.1.52+cuda100-cp37-none-manylinux2010_x86_64.whl">cuda100/jaxlib-0.1.52+cuda100-cp37-none-manylinux2010_x86_64.whl</a><br>
            </body>
            </html>
        "#;
        let base = Url::parse("https://storage.googleapis.com/jax-releases/jax_cuda_releases.html")
            .unwrap();
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
                            "storage.googleapis.com",
                        ),
                    ),
                    port: None,
                    path: "/jax-releases/jax_cuda_releases.html",
                    query: None,
                    fragment: None,
                },
            ),
            files: [
                File {
                    core_metadata: None,
                    dist_info_metadata: None,
                    data_dist_info_metadata: None,
                    filename: "jaxlib-0.1.52+cuda100-cp36-none-manylinux2010_x86_64.whl",
                    hashes: Hashes {
                        md5: None,
                        sha256: None,
                        sha384: None,
                        sha512: None,
                    },
                    requires_python: None,
                    size: None,
                    upload_time: None,
                    url: "https://storage.googleapis.com/jax-releases/cuda100/jaxlib-0.1.52+cuda100-cp36-none-manylinux2010_x86_64.whl",
                    yanked: None,
                },
                File {
                    core_metadata: None,
                    dist_info_metadata: None,
                    data_dist_info_metadata: None,
                    filename: "jaxlib-0.1.52+cuda100-cp37-none-manylinux2010_x86_64.whl",
                    hashes: Hashes {
                        md5: None,
                        sha256: None,
                        sha384: None,
                        sha512: None,
                    },
                    requires_python: None,
                    size: None,
                    upload_time: None,
                    url: "https://storage.googleapis.com/jax-releases/cuda100/jaxlib-0.1.52+cuda100-cp37-none-manylinux2010_x86_64.whl",
                    yanked: None,
                },
            ],
        }
        "###);
    }

    /// Test for AWS Code Artifact
    ///
    /// See: <https://github.com/astral-sh/uv/issues/1388#issuecomment-1947659088>
    #[test]
    fn parse_code_artifact_index_html() {
        let text = r#"
            <!DOCTYPE html>
            <html>
            <head>
                <title>Links for flask</title>
            </head>
            <body>
                <h1>Links for flask</h1>
                <a href="0.1/Flask-0.1.tar.gz#sha256=9da884457e910bf0847d396cb4b778ad9f3c3d17db1c5997cb861937bd284237" data-gpg-sig="false" >Flask-0.1.tar.gz</a>
                <br/>
                <a href="0.10.1/Flask-0.10.1.tar.gz#sha256=4c83829ff83d408b5e1d4995472265411d2c414112298f2eb4b359d9e4563373" data-gpg-sig="false" >Flask-0.10.1.tar.gz</a>
                <br/>
                <a href="3.0.1/flask-3.0.1.tar.gz#sha256=6489f51bb3666def6f314e15f19d50a1869a19ae0e8c9a3641ffe66c77d42403" data-requires-python="&gt;=3.8" data-gpg-sig="false" >flask-3.0.1.tar.gz</a>
                <br/>
            </body>
            </html>
        "#;
        let base = Url::parse("https://account.d.codeartifact.us-west-2.amazonaws.com/pypi/shared-packages-pypi/simple/flask/")
            .unwrap();
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
                            "account.d.codeartifact.us-west-2.amazonaws.com",
                        ),
                    ),
                    port: None,
                    path: "/pypi/shared-packages-pypi/simple/flask/",
                    query: None,
                    fragment: None,
                },
            ),
            files: [
                File {
                    core_metadata: None,
                    dist_info_metadata: None,
                    data_dist_info_metadata: None,
                    filename: "Flask-0.1.tar.gz",
                    hashes: Hashes {
                        md5: None,
                        sha256: Some(
                            "9da884457e910bf0847d396cb4b778ad9f3c3d17db1c5997cb861937bd284237",
                        ),
                        sha384: None,
                        sha512: None,
                    },
                    requires_python: None,
                    size: None,
                    upload_time: None,
                    url: "0.1/Flask-0.1.tar.gz#sha256=9da884457e910bf0847d396cb4b778ad9f3c3d17db1c5997cb861937bd284237",
                    yanked: None,
                },
                File {
                    core_metadata: None,
                    dist_info_metadata: None,
                    data_dist_info_metadata: None,
                    filename: "Flask-0.10.1.tar.gz",
                    hashes: Hashes {
                        md5: None,
                        sha256: Some(
                            "4c83829ff83d408b5e1d4995472265411d2c414112298f2eb4b359d9e4563373",
                        ),
                        sha384: None,
                        sha512: None,
                    },
                    requires_python: None,
                    size: None,
                    upload_time: None,
                    url: "0.10.1/Flask-0.10.1.tar.gz#sha256=4c83829ff83d408b5e1d4995472265411d2c414112298f2eb4b359d9e4563373",
                    yanked: None,
                },
                File {
                    core_metadata: None,
                    dist_info_metadata: None,
                    data_dist_info_metadata: None,
                    filename: "flask-3.0.1.tar.gz",
                    hashes: Hashes {
                        md5: None,
                        sha256: Some(
                            "6489f51bb3666def6f314e15f19d50a1869a19ae0e8c9a3641ffe66c77d42403",
                        ),
                        sha384: None,
                        sha512: None,
                    },
                    requires_python: Some(
                        Ok(
                            VersionSpecifiers(
                                [
                                    VersionSpecifier {
                                        operator: GreaterThanEqual,
                                        version: "3.8",
                                    },
                                ],
                            ),
                        ),
                    ),
                    size: None,
                    upload_time: None,
                    url: "3.0.1/flask-3.0.1.tar.gz#sha256=6489f51bb3666def6f314e15f19d50a1869a19ae0e8c9a3641ffe66c77d42403",
                    yanked: None,
                },
            ],
        }
        "###);
    }

    #[test]
    fn parse_file_requires_python_trailing_comma() {
        let text = r#"
<!DOCTYPE html>
<html>
  <body>
    <h1>Links for jinja2</h1>
    <a href="/whl/Jinja2-3.1.2-py3-none-any.whl#sha256=6088930bfe239f0e6710546ab9c19c9ef35e29792895fed6e6e31a023a182a61" data-requires-python="&gt;=3.8,">Jinja2-3.1.2-py3-none-any.whl</a><br/>
  </body>
</html>
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
                    core_metadata: None,
                    dist_info_metadata: None,
                    data_dist_info_metadata: None,
                    filename: "Jinja2-3.1.2-py3-none-any.whl",
                    hashes: Hashes {
                        md5: None,
                        sha256: Some(
                            "6088930bfe239f0e6710546ab9c19c9ef35e29792895fed6e6e31a023a182a61",
                        ),
                        sha384: None,
                        sha512: None,
                    },
                    requires_python: Some(
                        Ok(
                            VersionSpecifiers(
                                [
                                    VersionSpecifier {
                                        operator: GreaterThanEqual,
                                        version: "3.8",
                                    },
                                ],
                            ),
                        ),
                    ),
                    size: None,
                    upload_time: None,
                    url: "/whl/Jinja2-3.1.2-py3-none-any.whl#sha256=6088930bfe239f0e6710546ab9c19c9ef35e29792895fed6e6e31a023a182a61",
                    yanked: None,
                },
            ],
        }
        "###);
    }

    /// Respect PEP 714 (see: <https://peps.python.org/pep-0714/>).
    #[test]
    fn parse_core_metadata() {
        let text = r#"
<!DOCTYPE html>
<html>
  <body>
    <h1>Links for jinja2</h1>
    <a href="/whl/Jinja2-3.1.2-py3-none-any.whl" data-dist-info-metadata="true">Jinja2-3.1.2-py3-none-any.whl</a><br/>
    <a href="/whl/Jinja2-3.1.3-py3-none-any.whl" data-core-metadata="true">Jinja2-3.1.3-py3-none-any.whl</a><br/>
    <a href="/whl/Jinja2-3.1.4-py3-none-any.whl" data-dist-info-metadata="false">Jinja2-3.1.4-py3-none-any.whl</a><br/>
    <a href="/whl/Jinja2-3.1.5-py3-none-any.whl" data-core-metadata="false">Jinja2-3.1.5-py3-none-any.whl</a><br/>
    <a href="/whl/Jinja2-3.1.6-py3-none-any.whl" data-core-metadata="true" data-dist-info-metadata="false">Jinja2-3.1.6-py3-none-any.whl</a><br/>
  </body>
</html>
        "#;
        let base = Url::parse("https://account.d.codeartifact.us-west-2.amazonaws.com/pypi/shared-packages-pypi/simple/flask/")
            .unwrap();
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
                            "account.d.codeartifact.us-west-2.amazonaws.com",
                        ),
                    ),
                    port: None,
                    path: "/pypi/shared-packages-pypi/simple/flask/",
                    query: None,
                    fragment: None,
                },
            ),
            files: [
                File {
                    core_metadata: Some(
                        Bool(
                            true,
                        ),
                    ),
                    dist_info_metadata: None,
                    data_dist_info_metadata: None,
                    filename: "Jinja2-3.1.2-py3-none-any.whl",
                    hashes: Hashes {
                        md5: None,
                        sha256: None,
                        sha384: None,
                        sha512: None,
                    },
                    requires_python: None,
                    size: None,
                    upload_time: None,
                    url: "/whl/Jinja2-3.1.2-py3-none-any.whl",
                    yanked: None,
                },
                File {
                    core_metadata: Some(
                        Bool(
                            true,
                        ),
                    ),
                    dist_info_metadata: None,
                    data_dist_info_metadata: None,
                    filename: "Jinja2-3.1.3-py3-none-any.whl",
                    hashes: Hashes {
                        md5: None,
                        sha256: None,
                        sha384: None,
                        sha512: None,
                    },
                    requires_python: None,
                    size: None,
                    upload_time: None,
                    url: "/whl/Jinja2-3.1.3-py3-none-any.whl",
                    yanked: None,
                },
                File {
                    core_metadata: Some(
                        Bool(
                            false,
                        ),
                    ),
                    dist_info_metadata: None,
                    data_dist_info_metadata: None,
                    filename: "Jinja2-3.1.4-py3-none-any.whl",
                    hashes: Hashes {
                        md5: None,
                        sha256: None,
                        sha384: None,
                        sha512: None,
                    },
                    requires_python: None,
                    size: None,
                    upload_time: None,
                    url: "/whl/Jinja2-3.1.4-py3-none-any.whl",
                    yanked: None,
                },
                File {
                    core_metadata: Some(
                        Bool(
                            false,
                        ),
                    ),
                    dist_info_metadata: None,
                    data_dist_info_metadata: None,
                    filename: "Jinja2-3.1.5-py3-none-any.whl",
                    hashes: Hashes {
                        md5: None,
                        sha256: None,
                        sha384: None,
                        sha512: None,
                    },
                    requires_python: None,
                    size: None,
                    upload_time: None,
                    url: "/whl/Jinja2-3.1.5-py3-none-any.whl",
                    yanked: None,
                },
                File {
                    core_metadata: Some(
                        Bool(
                            true,
                        ),
                    ),
                    dist_info_metadata: None,
                    data_dist_info_metadata: None,
                    filename: "Jinja2-3.1.6-py3-none-any.whl",
                    hashes: Hashes {
                        md5: None,
                        sha256: None,
                        sha384: None,
                        sha512: None,
                    },
                    requires_python: None,
                    size: None,
                    upload_time: None,
                    url: "/whl/Jinja2-3.1.6-py3-none-any.whl",
                    yanked: None,
                },
            ],
        }
        "###);
    }
}
