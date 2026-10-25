use std::borrow::Cow;
use std::str::from_utf8;

use http::HeaderMap;
use http::header::CONTENT_DISPOSITION;
use percent_encoding::percent_decode;
use rustc_hash::FxHashSet;

/// An invalid filename in a `Content-Disposition` response header.
#[derive(Debug, thiserror::Error)]
pub enum ContentDispositionError {
    #[error("Malformed `Content-Disposition` header")]
    InvalidHeader,
    #[error("Duplicate `Content-Disposition` parameter: `{0}`")]
    DuplicateParameter(String),
    #[error("Invalid filename in `Content-Disposition` header")]
    InvalidFilename,
}

/// Return the filename advertised by a `Content-Disposition` response header.
///
/// A valid UTF-8 `filename*` takes precedence over `filename`, regardless of parameter order.
/// Directory components are removed using both Unix and Windows separators. Missing headers or
/// filename parameters return [`None`]; malformed headers and unsafe filenames return an error.
pub fn content_disposition_filename(
    headers: &HeaderMap,
) -> Result<Option<String>, ContentDispositionError> {
    let mut headers = headers.get_all(CONTENT_DISPOSITION).iter();
    let Some(header) = headers.next() else {
        return Ok(None);
    };
    if headers.next().is_some() {
        return Err(ContentDispositionError::InvalidHeader);
    }

    let mut parser = Parser(header.as_bytes());
    parser.whitespace();
    parser.token()?;
    let mut parameters = FxHashSet::default();
    let mut filename = None;
    let mut extended_filename = None;
    loop {
        parser.whitespace();
        if parser.0.is_empty() {
            break;
        }
        parser.consume(b';')?;
        parser.whitespace();
        let parameter = parser.token()?.to_ascii_lowercase();
        let parameter =
            String::from_utf8(parameter).map_err(|_| ContentDispositionError::InvalidHeader)?;
        if !parameters.insert(parameter.clone()) {
            return Err(ContentDispositionError::DuplicateParameter(parameter));
        }
        parser.whitespace();
        parser.consume(b'=')?;
        parser.whitespace();
        let (value, quoted) = parser.value()?;
        if parameter == "filename" {
            filename = Some(value);
        } else if parameter == "filename*" {
            extended_filename = Some((value, quoted));
        }
    }

    if let Some((value, false)) = &extended_filename
        && let Some(filename) = extended_value(value).as_deref().and_then(sanitize_filename)
    {
        return Ok(Some(filename));
    }
    if let Some(value) = filename {
        return from_utf8(&value)
            .ok()
            .and_then(sanitize_filename)
            .map(Some)
            .ok_or(ContentDispositionError::InvalidFilename);
    }
    if extended_filename.is_some() {
        return Err(ContentDispositionError::InvalidFilename);
    }
    Ok(None)
}

/// Decode the UTF-8 form of an RFC 8187 extended parameter value.
fn extended_value(value: &[u8]) -> Option<String> {
    let mut parts = value.splitn(3, |byte| *byte == b'\'');
    let charset = parts.next()?;
    if !charset.eq_ignore_ascii_case(b"utf-8") {
        return None;
    }
    let _language = parts.next()?;
    let encoded = parts.next()?;
    let mut remaining = encoded;
    while let Some((&byte, rest)) = remaining.split_first() {
        remaining = rest;
        if byte == b'%' {
            let [first, second, rest @ ..] = remaining else {
                return None;
            };
            if !first.is_ascii_hexdigit() || !second.is_ascii_hexdigit() {
                return None;
            }
            remaining = rest;
        } else if !byte.is_ascii_alphanumeric() && !b"!#$&+-.^_`|~".contains(&byte) {
            return None;
        }
    }
    percent_decode(encoded)
        .decode_utf8()
        .ok()
        .map(Cow::into_owned)
}

fn sanitize_filename(filename: &str) -> Option<String> {
    if filename.chars().any(char::is_control) {
        return None;
    }
    let filename = filename.rsplit(['/', '\\']).next()?.trim();
    if filename.is_empty() || filename == "." || filename == ".." || filename.contains(':') {
        return None;
    }
    Some(filename.to_owned())
}

struct Parser<'a>(&'a [u8]);

impl<'a> Parser<'a> {
    fn whitespace(&mut self) {
        self.0 = self.0.trim_ascii_start();
    }

    fn consume(&mut self, expected: u8) -> Result<(), ContentDispositionError> {
        let Some((&byte, rest)) = self.0.split_first() else {
            return Err(ContentDispositionError::InvalidHeader);
        };
        if byte != expected {
            return Err(ContentDispositionError::InvalidHeader);
        }
        self.0 = rest;
        Ok(())
    }

    fn token(&mut self) -> Result<&'a [u8], ContentDispositionError> {
        let length = self
            .0
            .iter()
            .take_while(|byte| byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(byte))
            .count();
        if length == 0 {
            return Err(ContentDispositionError::InvalidHeader);
        }
        let (token, rest) = self.0.split_at(length);
        self.0 = rest;
        Ok(token)
    }

    fn value(&mut self) -> Result<(Vec<u8>, bool), ContentDispositionError> {
        if self.0.first() != Some(&b'"') {
            return self.token().map(|value| (value.to_vec(), false));
        }
        self.consume(b'"')?;
        let mut value = Vec::new();
        while let Some((&byte, rest)) = self.0.split_first() {
            self.0 = rest;
            if byte == b'"' {
                return Ok((value, true));
            }
            let byte = if byte == b'\\' {
                let Some((&byte, rest)) = self.0.split_first() else {
                    return Err(ContentDispositionError::InvalidHeader);
                };
                self.0 = rest;
                byte
            } else {
                byte
            };
            if byte.is_ascii_control() && byte != b'\t' {
                return Err(ContentDispositionError::InvalidHeader);
            }
            value.push(byte);
        }
        Err(ContentDispositionError::InvalidHeader)
    }
}

#[cfg(test)]
mod tests {
    use http::HeaderMap;
    use http::header::{CONTENT_DISPOSITION, HeaderValue};
    use insta::assert_debug_snapshot;

    use super::content_disposition_filename;

    #[test]
    fn filename_parameters() {
        let mut headers = HeaderMap::new();
        assert_debug_snapshot!(content_disposition_filename(&headers), @"
        Ok(
            None,
        )
        ");

        headers.insert(CONTENT_DISPOSITION, HeaderValue::from_static("attachment"));
        assert_debug_snapshot!(content_disposition_filename(&headers), @"
        Ok(
            None,
        )
        ");

        headers.insert(
            CONTENT_DISPOSITION,
            HeaderValue::from_static("attachment; filename=example-1.0.tar.gz"),
        );
        assert_debug_snapshot!(content_disposition_filename(&headers), @r#"
        Ok(
            Some(
                "example-1.0.tar.gz",
            ),
        )
        "#);

        headers.insert(
            CONTENT_DISPOSITION,
            HeaderValue::from_static(r#"INLINE; FILENAME="example; archive.tar.gz""#),
        );
        assert_debug_snapshot!(content_disposition_filename(&headers), @r#"
        Ok(
            Some(
                "example; archive.tar.gz",
            ),
        )
        "#);
    }

    #[test]
    fn extended_filename_precedence() {
        let mut headers = HeaderMap::new();
        headers.insert(
            CONTENT_DISPOSITION,
            HeaderValue::from_static(
                "attachment; filename=plain.tar.gz; filename*=UTF-8'en'caf%C3%A9.tar.gz",
            ),
        );
        assert_debug_snapshot!(content_disposition_filename(&headers), @r#"
        Ok(
            Some(
                "café.tar.gz",
            ),
        )
        "#);

        headers.insert(
            CONTENT_DISPOSITION,
            HeaderValue::from_static(
                "attachment; FILENAME*=utf-8''preferred.tar.gz; filename=plain.tar.gz",
            ),
        );
        assert_debug_snapshot!(content_disposition_filename(&headers), @r#"
        Ok(
            Some(
                "preferred.tar.gz",
            ),
        )
        "#);

        headers.insert(
            CONTENT_DISPOSITION,
            HeaderValue::from_static("attachment; filename*=UTF-8''example-1.0-py3-none-any.whl"),
        );
        assert_debug_snapshot!(content_disposition_filename(&headers), @r#"
        Ok(
            Some(
                "example-1.0-py3-none-any.whl",
            ),
        )
        "#);
    }

    #[test]
    fn invalid_extended_filename_fallback() {
        let mut headers = HeaderMap::new();
        headers.insert(
            CONTENT_DISPOSITION,
            HeaderValue::from_static(
                "attachment; filename*=UTF-8''invalid%xx.tar.gz; filename=fallback.tar.gz",
            ),
        );
        assert_debug_snapshot!(content_disposition_filename(&headers), @r#"
        Ok(
            Some(
                "fallback.tar.gz",
            ),
        )
        "#);

        headers.insert(
            CONTENT_DISPOSITION,
            HeaderValue::from_static(
                "attachment; filename*=UTF-8''%FF.tar.gz; filename=fallback.tar.gz",
            ),
        );
        assert_debug_snapshot!(content_disposition_filename(&headers), @r#"
        Ok(
            Some(
                "fallback.tar.gz",
            ),
        )
        "#);

        headers.insert(
            CONTENT_DISPOSITION,
            HeaderValue::from_static(
                "attachment; filename*=ISO-8859-1''example.tar.gz; filename=fallback.tar.gz",
            ),
        );
        assert_debug_snapshot!(content_disposition_filename(&headers), @r#"
        Ok(
            Some(
                "fallback.tar.gz",
            ),
        )
        "#);

        headers.insert(
            CONTENT_DISPOSITION,
            HeaderValue::from_static("attachment; filename*=UTF-8''invalid%"),
        );
        assert_debug_snapshot!(content_disposition_filename(&headers), @"
        Err(
            InvalidFilename,
        )
        ");
    }

    #[test]
    fn filename_paths() {
        let mut headers = HeaderMap::new();
        headers.insert(
            CONTENT_DISPOSITION,
            HeaderValue::from_static(r#"attachment; filename="../example.tar.gz""#),
        );
        assert_debug_snapshot!(content_disposition_filename(&headers), @r#"
        Ok(
            Some(
                "example.tar.gz",
            ),
        )
        "#);

        headers.insert(
            CONTENT_DISPOSITION,
            HeaderValue::from_static(r#"attachment; filename="C:\\downloads\\example.tar.gz""#),
        );
        assert_debug_snapshot!(content_disposition_filename(&headers), @r#"
        Ok(
            Some(
                "example.tar.gz",
            ),
        )
        "#);

        headers.insert(
            CONTENT_DISPOSITION,
            HeaderValue::from_static("attachment; filename*=UTF-8''..%2F..%5Cexample.tar.gz"),
        );
        assert_debug_snapshot!(content_disposition_filename(&headers), @r#"
        Ok(
            Some(
                "example.tar.gz",
            ),
        )
        "#);
    }

    #[test]
    fn invalid_filename() {
        let mut headers = HeaderMap::new();
        headers.insert(
            CONTENT_DISPOSITION,
            HeaderValue::from_static(r#"attachment; filename="..""#),
        );
        assert_debug_snapshot!(content_disposition_filename(&headers), @"
        Err(
            InvalidFilename,
        )
        ");

        headers.insert(
            CONTENT_DISPOSITION,
            HeaderValue::from_static(r#"attachment; filename="""#),
        );
        assert_debug_snapshot!(content_disposition_filename(&headers), @"
        Err(
            InvalidFilename,
        )
        ");

        headers.insert(
            CONTENT_DISPOSITION,
            HeaderValue::from_static("attachment; filename*=UTF-8''example%00.tar.gz"),
        );
        assert_debug_snapshot!(content_disposition_filename(&headers), @"
        Err(
            InvalidFilename,
        )
        ");
    }

    #[test]
    fn filename_encoding() -> Result<(), http::header::InvalidHeaderValue> {
        let mut headers = HeaderMap::new();
        headers.insert(
            CONTENT_DISPOSITION,
            HeaderValue::from_bytes(b"attachment; filename=\"caf\xc3\xa9.tar.gz\"")?,
        );
        assert_debug_snapshot!(content_disposition_filename(&headers), @r#"
        Ok(
            Some(
                "café.tar.gz",
            ),
        )
        "#);

        headers.insert(
            CONTENT_DISPOSITION,
            HeaderValue::from_bytes(b"attachment; filename=\"invalid\xff.tar.gz\"")?,
        );
        assert_debug_snapshot!(content_disposition_filename(&headers), @"
        Err(
            InvalidFilename,
        )
        ");
        Ok(())
    }

    #[test]
    fn invalid_header() {
        let mut headers = HeaderMap::new();
        headers.insert(
            CONTENT_DISPOSITION,
            HeaderValue::from_static("attachment; filename=first.tar.gz; FILENAME=second.tar.gz"),
        );
        assert_debug_snapshot!(content_disposition_filename(&headers), @r#"
        Err(
            DuplicateParameter(
                "filename",
            ),
        )
        "#);

        headers.insert(
            CONTENT_DISPOSITION,
            HeaderValue::from_static("attachment; filename=\"unclosed.tar.gz"),
        );
        assert_debug_snapshot!(content_disposition_filename(&headers), @"
        Err(
            InvalidHeader,
        )
        ");

        headers.insert(
            CONTENT_DISPOSITION,
            HeaderValue::from_static("attachment; filename=first.tar.gz"),
        );
        headers.append(
            CONTENT_DISPOSITION,
            HeaderValue::from_static("attachment; filename=second.tar.gz"),
        );
        assert_debug_snapshot!(content_disposition_filename(&headers), @"
        Err(
            InvalidHeader,
        )
        ");
    }
}
