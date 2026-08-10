//! Parses the common, unescaped form of PyPI Simple API responses directly.

use jiff::Timestamp;
use memchr::memchr;
use uv_small_str::SmallString;

use super::{CoreMetadata, Hashes, PypiFile, PypiSimpleDetail, RequiresPythonInterner, Yanked};
use crate::{ProjectStatus, Status};

/// Returns `None` for syntax that should be handled by the general JSON parser.
pub(super) fn parse(input: &[u8]) -> Option<PypiSimpleDetail> {
    if memchr(b'\\', input).is_some() {
        return None;
    }

    let input = std::str::from_utf8(input).ok()?;
    let mut parser = Parser { input, offset: 0 };
    let detail = parser.detail()?;
    parser.skip_whitespace();
    (parser.offset == input.len()).then_some(detail)
}

struct Parser<'input> {
    input: &'input str,
    offset: usize,
}

enum ObjectKey<'input> {
    Key(&'input str),
    End,
}

enum Nullable<T> {
    Value(T),
    Null,
}

impl<T> Nullable<T> {
    fn into_option(self) -> Option<T> {
        match self {
            Self::Value(value) => Some(value),
            Self::Null => None,
        }
    }
}

impl<'input> Parser<'input> {
    fn detail(&mut self) -> Option<PypiSimpleDetail> {
        self.skip_whitespace();
        self.consume(b'{')?;

        let mut files = None;
        let mut project_status = None;
        let mut first = true;

        while let ObjectKey::Key(key) = self.object_key(&mut first)? {
            match key {
                "files" => {
                    if files.is_some() {
                        return None;
                    }
                    files = Some(self.files()?);
                }
                "project-status" => {
                    if project_status.is_some() {
                        return None;
                    }
                    project_status = Some(self.project_status()?);
                }
                _ => self.skip_value(0)?,
            }
        }

        Some(PypiSimpleDetail {
            files: files?,
            project_status: project_status.unwrap_or_default(),
        })
    }

    fn files(&mut self) -> Option<Vec<PypiFile>> {
        self.consume(b'[')?;

        let mut files = Vec::new();
        let mut interner = RequiresPythonInterner::default();
        let mut first = true;

        while self.array_item(&mut first)? {
            files.push(self.file(&mut interner)?);
        }

        Some(files)
    }

    fn file(&mut self, interner: &mut RequiresPythonInterner) -> Option<PypiFile> {
        self.consume(b'{')?;

        let mut core_metadata = None;
        let mut filename = None;
        let mut hashes = None;
        let mut requires_python = None;
        let mut size = None;
        let mut upload_time = None;
        let mut url = None;
        let mut yanked = None;
        let mut first = true;

        while let ObjectKey::Key(key) = self.object_key(&mut first)? {
            match key {
                "core-metadata" | "dist-info-metadata" | "data-dist-info-metadata"
                    if core_metadata.is_none() =>
                {
                    core_metadata = self.core_metadata()?.into_option();
                }
                "filename" => filename = Some(SmallString::from(self.string()?)),
                "hashes" => hashes = Some(self.hashes()?),
                "requires-python" => requires_python = self.nullable_string()?.into_option(),
                "size" => size = Some(self.unsigned_integer()?),
                "upload-time" => upload_time = Some(self.string()?.parse::<Timestamp>().ok()?),
                "url" => url = Some(SmallString::from(self.string()?)),
                "yanked" => yanked = Some(Box::new(self.yanked()?)),
                _ => self.skip_value(0)?,
            }
        }

        Some(PypiFile {
            core_metadata,
            filename: filename?,
            hashes: hashes?,
            requires_python: requires_python.map(|value| interner.parse(value)),
            size,
            upload_time,
            url: url?,
            yanked,
        })
    }

    fn hashes(&mut self) -> Option<Hashes> {
        if let Some(digest) = self.single_sha256_digest() {
            return Some(Hashes {
                sha256: Some(SmallString::from(digest)),
                ..Hashes::default()
            });
        }

        self.consume(b'{')?;

        let mut hashes = Hashes::default();
        let mut seen = 0_u8;
        let mut first = true;

        while let ObjectKey::Key(key) = self.object_key(&mut first)? {
            let (bit, digest) = match key {
                "md5" => (1, &mut hashes.md5),
                "sha256" => (1 << 1, &mut hashes.sha256),
                "sha384" => (1 << 2, &mut hashes.sha384),
                "sha512" => (1 << 3, &mut hashes.sha512),
                "blake2b" => (1 << 4, &mut hashes.blake2b),
                _ => {
                    self.skip_value(0)?;
                    continue;
                }
            };

            if seen & bit != 0 {
                return None;
            }
            seen |= bit;

            *digest = self.nullable_string()?.into_option().map(SmallString::from);
        }

        Some(hashes)
    }

    fn single_sha256_digest(&mut self) -> Option<&'input str> {
        const PREFIX: &[u8] = b"{\"sha256\":\"";

        let remaining = self.input.as_bytes().get(self.offset..)?;
        let value = remaining.strip_prefix(PREFIX)?;
        let length = memchr(b'"', value)?;
        if value.get(length + 1) != Some(&b'}') {
            return None;
        }

        let bytes = &value[..length];
        if bytes.iter().copied().min().is_some_and(|byte| byte < 0x20) {
            return None;
        }

        let start = self.offset + PREFIX.len();
        self.offset += PREFIX.len() + length + 2;
        Some(&self.input[start..start + length])
    }

    fn core_metadata(&mut self) -> Option<Nullable<CoreMetadata>> {
        match self.peek()? {
            b'n' => {
                self.literal(b"null")?;
                Some(Nullable::Null)
            }
            b't' | b'f' => Some(Nullable::Value(CoreMetadata::Bool(self.boolean()?))),
            b'{' => Some(Nullable::Value(CoreMetadata::Hashes(self.hashes()?))),
            _ => None,
        }
    }

    fn yanked(&mut self) -> Option<Yanked> {
        match self.peek()? {
            b't' | b'f' => Some(Yanked::Bool(self.boolean()?)),
            b'"' => Some(Yanked::Reason(SmallString::from(self.string()?))),
            _ => None,
        }
    }

    fn project_status(&mut self) -> Option<ProjectStatus> {
        self.consume(b'{')?;

        let mut project_status = ProjectStatus::default();
        let mut seen_status = false;
        let mut seen_reason = false;
        let mut first = true;

        while let ObjectKey::Key(key) = self.object_key(&mut first)? {
            match key {
                "status" => {
                    if seen_status {
                        return None;
                    }
                    seen_status = true;
                    project_status.status = Status::new(self.string()?).unwrap_or_default();
                }
                "reason" => {
                    if seen_reason {
                        return None;
                    }
                    seen_reason = true;
                    project_status.reason =
                        self.nullable_string()?.into_option().map(SmallString::from);
                }
                _ => self.skip_value(0)?,
            }
        }

        Some(project_status)
    }

    fn object_key(&mut self, first: &mut bool) -> Option<ObjectKey<'input>> {
        self.skip_whitespace();

        if self.peek()? == b'}' {
            self.offset += 1;
            return Some(ObjectKey::End);
        }

        if !*first {
            self.consume(b',')?;
            self.skip_whitespace();
        }

        let key = self.string()?;
        self.skip_whitespace();
        self.consume(b':')?;
        self.skip_whitespace();
        *first = false;

        Some(ObjectKey::Key(key))
    }

    fn array_item(&mut self, first: &mut bool) -> Option<bool> {
        self.skip_whitespace();

        if self.peek()? == b']' {
            self.offset += 1;
            return Some(false);
        }

        if !*first {
            self.consume(b',')?;
            self.skip_whitespace();
            if self.peek()? == b']' {
                return None;
            }
        }

        *first = false;
        Some(true)
    }

    fn string(&mut self) -> Option<&'input str> {
        self.consume(b'"')?;
        let start = self.offset;
        let remaining = &self.input.as_bytes()[start..];
        let length = memchr(b'"', remaining)?;
        let bytes = &remaining[..length];

        if bytes.iter().copied().min().is_some_and(|byte| byte < 0x20) {
            return None;
        }

        self.offset += length + 1;
        Some(&self.input[start..start + length])
    }

    fn nullable_string(&mut self) -> Option<Nullable<&'input str>> {
        if self.peek()? == b'n' {
            self.literal(b"null")?;
            Some(Nullable::Null)
        } else {
            self.string().map(Nullable::Value)
        }
    }

    fn unsigned_integer(&mut self) -> Option<u64> {
        let first = self.peek()?;
        if !first.is_ascii_digit() {
            return None;
        }

        if first == b'0' {
            self.offset += 1;
            if self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
                return None;
            }
            return Some(0);
        }

        let mut value = 0_u64;
        while let Some(byte @ b'0'..=b'9') = self.peek() {
            value = value.checked_mul(10)?.checked_add(u64::from(byte - b'0'))?;
            self.offset += 1;
        }

        Some(value)
    }

    fn boolean(&mut self) -> Option<bool> {
        match self.peek()? {
            b't' => {
                self.literal(b"true")?;
                Some(true)
            }
            b'f' => {
                self.literal(b"false")?;
                Some(false)
            }
            _ => None,
        }
    }

    fn skip_value(&mut self, depth: u8) -> Option<()> {
        if depth >= 64 {
            return None;
        }

        match self.peek()? {
            b'{' => {
                self.offset += 1;
                let mut first = true;
                while let ObjectKey::Key(_) = self.object_key(&mut first)? {
                    self.skip_value(depth + 1)?;
                }
            }
            b'[' => {
                self.offset += 1;
                let mut first = true;
                while self.array_item(&mut first)? {
                    self.skip_value(depth + 1)?;
                }
            }
            b'"' => {
                self.string()?;
            }
            b't' | b'f' => {
                self.boolean()?;
            }
            b'n' => self.literal(b"null")?,
            b'-' | b'0'..=b'9' => self.skip_number()?,
            _ => return None,
        }

        Some(())
    }

    fn skip_number(&mut self) -> Option<()> {
        if self.peek()? == b'-' {
            self.offset += 1;
        }

        match self.peek()? {
            b'0' => {
                self.offset += 1;
                if self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
                    return None;
                }
            }
            b'1'..=b'9' => self.skip_digits(),
            _ => return None,
        }

        if self.peek() == Some(b'.') {
            self.offset += 1;
            let start = self.offset;
            self.skip_digits();
            if self.offset == start {
                return None;
            }
        }

        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.offset += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.offset += 1;
            }
            let start = self.offset;
            self.skip_digits();
            if self.offset == start {
                return None;
            }
        }

        Some(())
    }

    fn skip_digits(&mut self) {
        while self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
            self.offset += 1;
        }
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\n' | b'\r' | b'\t')) {
            self.offset += 1;
        }
    }

    fn literal(&mut self, literal: &[u8]) -> Option<()> {
        if self
            .input
            .as_bytes()
            .get(self.offset..)?
            .starts_with(literal)
        {
            self.offset += literal.len();
            Some(())
        } else {
            None
        }
    }

    fn consume(&mut self, expected: u8) -> Option<()> {
        if self.peek()? == expected {
            self.offset += 1;
            Some(())
        } else {
            None
        }
    }

    fn peek(&self) -> Option<u8> {
        self.input.as_bytes().get(self.offset).copied()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::parse;
    use crate::{CoreMetadata, PypiSimpleDetail, Status};

    #[test]
    fn parses_common_pypi_response_directly() -> Result<(), Box<dyn std::error::Error>> {
        let input = br#"{
            "meta": {"api-version": "1.4", "serial": 42},
            "files": [
                {
                    "provenance": null,
                    "size": 123,
                    "yanked": false,
                    "core-metadata": {"sha256": "metadata-digest"},
                    "data-dist-info-metadata": false,
                    "filename": "example-1.0.tar.gz",
                    "hashes": {"sha256": "file-digest"},
                    "url": "https://example.com/example-1.0.tar.gz",
                    "requires-python": ">=3.8",
                    "upload-time": "2022-08-04T10:42:02.190074Z"
                },
                {
                    "filename": "example-1.0-py3-none-any.whl",
                    "hashes": {},
                    "url": "https://example.com/example-1.0-py3-none-any.whl",
                    "requires-python": ">=3.8"
                }
            ],
            "project-status": {"status": "archived", "reason": "Retired"},
            "versions": ["1.0"]
        }"#;

        let Some(actual) = parse(input) else {
            return Err("the common PyPI response must use the direct parser".into());
        };
        let expected: PypiSimpleDetail = serde_json::from_slice(input)?;

        assert_eq!(format!("{actual:#?}"), format!("{expected:#?}"));
        assert_eq!(actual.project_status.status, Status::Archived);
        assert!(matches!(
            actual.files[0].core_metadata,
            Some(CoreMetadata::Hashes(_))
        ));

        let Some(Ok(first)) = actual.files[0].requires_python.as_ref() else {
            return Err("the first file must have a valid Python requirement".into());
        };
        let Some(Ok(second)) = actual.files[1].requires_python.as_ref() else {
            return Err("the second file must have a valid Python requirement".into());
        };
        assert!(Arc::ptr_eq(first, second));

        Ok(())
    }

    #[test]
    fn unsupported_strings_and_duplicate_fields_use_fallback() {
        for input in [
            br#"{"files":[],"ignored":"escaped\nvalue"}"#.as_slice(),
            br#"{"files":[],"files":[]}"#.as_slice(),
            br#"{"files":[],"project-status":{},"project-status":{}}"#.as_slice(),
            br#"{"files":[]} trailing"#.as_slice(),
            b"{\"files\":[],\"ignored\":\"raw\nnewline\"}",
        ] {
            assert!(parse(input).is_none());
        }
    }
}
