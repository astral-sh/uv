use std::collections::HashSet;

use crate::rkyvutil::OwnedArchive;

/// Represents values for relevant cache-control directives.
///
/// This does include some directives that we don't use mostly because they are
/// trivial to support. (For example, `must-understand` at time of writing is
/// not used in our HTTP cache semantics. Neither is `proxy-revalidate` since
/// we are not a proxy.)
#[derive(
    Clone,
    Debug,
    Default,
    Eq,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    rkyv::Archive,
    rkyv::Deserialize,
    rkyv::Serialize,
)]
#[archive(check_bytes)]
#[archive_attr(derive(Debug))]
#[allow(clippy::struct_excessive_bools)]
pub struct CacheControl {
    // directives for requests and responses
    /// * <https://www.rfc-editor.org/rfc/rfc9111.html#name-max-age>
    /// * <https://www.rfc-editor.org/rfc/rfc9111.html#name-max-age-2>
    pub max_age_seconds: Option<u64>,
    /// * <https://www.rfc-editor.org/rfc/rfc9111.html#name-no-cache>
    /// * <https://www.rfc-editor.org/rfc/rfc9111.html#name-no-cache-2>
    pub no_cache: bool,
    /// * <https://www.rfc-editor.org/rfc/rfc9111.html#name-no-store>
    /// * <https://www.rfc-editor.org/rfc/rfc9111.html#name-no-store-2>
    pub no_store: bool,
    /// * <https://www.rfc-editor.org/rfc/rfc9111.html#name-no-transform>
    /// * <https://www.rfc-editor.org/rfc/rfc9111.html#name-no-transform-2>
    pub no_transform: bool,

    // request-only directives
    /// <https://www.rfc-editor.org/rfc/rfc9111.html#name-max-stale>
    pub max_stale_seconds: Option<u64>,
    /// <https://www.rfc-editor.org/rfc/rfc9111.html#name-min-fresh>
    pub min_fresh_seconds: Option<u64>,

    // response-only directives
    /// <https://www.rfc-editor.org/rfc/rfc9111.html#name-only-if-cached>
    pub only_if_cached: bool,
    /// <https://www.rfc-editor.org/rfc/rfc9111.html#name-must-revalidate>
    pub must_revalidate: bool,
    /// <https://www.rfc-editor.org/rfc/rfc9111.html#name-must-understand>
    pub must_understand: bool,
    /// <https://www.rfc-editor.org/rfc/rfc9111.html#name-private>
    pub private: bool,
    /// <https://www.rfc-editor.org/rfc/rfc9111.html#name-proxy-revalidate>
    pub proxy_revalidate: bool,
    /// <https://www.rfc-editor.org/rfc/rfc9111.html#name-public>
    pub public: bool,
    /// <https://www.rfc-editor.org/rfc/rfc9111.html#name-s-maxage>
    pub s_maxage_seconds: Option<u64>,
    /// <https://httpwg.org/specs/rfc8246.html>
    pub immutable: bool,
}

impl CacheControl {
    /// Convert this to an owned archive value.
    pub fn to_archived(&self) -> OwnedArchive<Self> {
        // There's no way (other than OOM) for serializing this type to fail.
        OwnedArchive::from_unarchived(self).expect("all possible values can be archived")
    }
}

impl<'b, B: 'b + ?Sized + AsRef<[u8]>> FromIterator<&'b B> for CacheControl {
    fn from_iter<T: IntoIterator<Item = &'b B>>(it: T) -> Self {
        CacheControlParser::new(it).collect()
    }
}

impl FromIterator<CacheControlDirective> for CacheControl {
    fn from_iter<T: IntoIterator<Item = CacheControlDirective>>(it: T) -> Self {
        fn parse_int(value: &[u8]) -> Option<u64> {
            if !value.iter().all(u8::is_ascii_digit) {
                return None;
            }
            std::str::from_utf8(value).ok()?.parse().ok()
        }

        let mut cc = Self::default();
        for ccd in it {
            // Note that when we see invalid directive values, we follow [RFC
            // 9111 S4.2.1]. It says that invalid cache-control directives
            // should result in treating the response as stale. (Which we
            // accomplished by setting `must_revalidate` to `true`.)
            //
            // [RFC 9111 S4.2.1]: https://www.rfc-editor.org/rfc/rfc9111.html#section-4.2.1
            match &*ccd.name {
                // request + response directives
                "max-age" => match parse_int(&ccd.value) {
                    None => cc.must_revalidate = true,
                    Some(seconds) => cc.max_age_seconds = Some(seconds),
                },
                "no-cache" => cc.no_cache = true,
                "no-store" => cc.no_store = true,
                "no-transform" => cc.no_transform = true,
                // request-only directives
                "max-stale" => {
                    // As per [RFC 9111 S5.2.1.2], "If no value is assigned to
                    // max-stale, then the client will accept a stale response
                    // of any age." We implement that by just using the maximum
                    // number of seconds.
                    //
                    // [RFC 9111 S5.2.1.2]: https://www.rfc-editor.org/rfc/rfc9111.html#section-5.2.1.2
                    if ccd.value.is_empty() {
                        cc.max_stale_seconds = Some(u64::MAX);
                    } else {
                        match parse_int(&ccd.value) {
                            None => cc.must_revalidate = true,
                            Some(seconds) => cc.max_stale_seconds = Some(seconds),
                        }
                    }
                }
                "min-fresh" => match parse_int(&ccd.value) {
                    None => cc.must_revalidate = true,
                    Some(seconds) => cc.min_fresh_seconds = Some(seconds),
                },
                "only-if-cached" => cc.only_if_cached = true,
                "must-revalidate" => cc.must_revalidate = true,
                "must-understand" => cc.must_understand = true,
                "private" => cc.private = true,
                "proxy-revalidate" => cc.proxy_revalidate = true,
                "public" => cc.public = true,
                "s-maxage" => match parse_int(&ccd.value) {
                    None => cc.must_revalidate = true,
                    Some(seconds) => cc.s_maxage_seconds = Some(seconds),
                },
                "immutable" => cc.immutable = true,
                _ => {}
            }
        }
        cc
    }
}

/// A parser for the HTTP `Cache-Control` header.
///
/// The parser is mostly defined across multiple parts of multiple RFCs.
/// Namely, [RFC 9110 S5.6.2] says how to parse the names (or "keys") of each
/// directive (whose format is a "token"). [RFC 9110 S5.6.4] says how to parse
/// quoted values. And finally, [RFC 9111 Appendix A] gives the ABNF for the
/// overall header value.
///
/// This parser accepts an iterator of anything that can be cheaply converted
/// to a byte string (e.g., `http::header::HeaderValue`). Directives are parsed
/// from zero or more of these byte strings. Parsing cannot return an error,
/// but if something unexpected is found, the rest of that header value is
/// skipped.
///
/// Duplicate directives provoke an automatic insertion of `must-revalidate`,
/// as implied by [RFC 9111 S4.2.1], to ensure that the client will talk to the
/// server before using anything in case.
///
/// This parser handles a bit more than what we actually need in
/// `uv-client`. For example, we don't need to handle quoted values at all
/// since either don't use or care about values that require quoted. With that
/// said, the parser handles these because it wasn't that much extra work to do
/// so and just generally seemed like good sense. (If we didn't handle them and
/// parsed them incorrectly, that might mean parsing subsequent directives that
/// we do care about incorrectly.)
///
/// [RFC 9110 S5.6.2]: https://www.rfc-editor.org/rfc/rfc9110.html#name-tokens
/// [RFC 9110 S5.6.4]: https://www.rfc-editor.org/rfc/rfc9110.html#name-quoted-strings
/// [RFC 9111 Appendix A]: https://www.rfc-editor.org/rfc/rfc9111.html#name-collected-abnf
/// [RFC 9111 S4.2.1]: https://www.rfc-editor.org/rfc/rfc9111.html#calculating.freshness.lifetime
struct CacheControlParser<'b, I> {
    cur: &'b [u8],
    directives: I,
    seen: HashSet<String>,
}

impl<'b, B: 'b + ?Sized + AsRef<[u8]>, I: Iterator<Item = &'b B>> CacheControlParser<'b, I> {
    /// Create a new parser of zero or more `Cache-Control` header values. The
    /// given iterator should yield elements that satisfy `AsRef<[u8]>`.
    fn new<II: IntoIterator<IntoIter = I>>(headers: II) -> CacheControlParser<'b, I> {
        let mut directives = headers.into_iter();
        let cur = directives
            .next()
            .map(std::convert::AsRef::as_ref)
            .unwrap_or(b"");
        CacheControlParser {
            cur,
            directives,
            seen: HashSet::new(),
        }
    }

    /// Parses a token according to [RFC 9110 S5.6.2].
    ///
    /// If no token is found at the current position, then this returns `None`.
    /// Usually this indicates an invalid cache-control directive.
    ///
    /// This does not trim whitespace before or after the token.
    ///
    /// [RFC 9110 S5.6.2]: https://www.rfc-editor.org/rfc/rfc9110.html#name-tokens
    fn parse_token(&mut self) -> Option<String> {
        /// Returns true when the given byte can appear in a token, as
        /// defined by [RFC 9110 S5.6.2].
        ///
        /// [RFC 9110 S5.6.2]: https://www.rfc-editor.org/rfc/rfc9110.html#name-tokens
        fn is_token_byte(byte: u8) -> bool {
            matches!(
                byte,
                | b'!' | b'#' | b'$' | b'%' | b'&' | b'\'' | b'*' | b'+'
                | b'-' | b'.' | b'^' | b'_' | b'`' | b'|' | b'~'
                | b'0'..=b'9' | b'A'..=b'Z' | b'a'..=b'z',
            )
        }
        let mut end = 0;
        while self.cur.get(end).copied().is_some_and(is_token_byte) {
            end += 1;
        }
        if end == 0 {
            None
        } else {
            let (token, rest) = self.cur.split_at(end);
            self.cur = rest;
            // This can't fail because `end` is only incremented when the
            // current byte is a valid token byte. And all valid token bytes
            // are ASCII and thus valid UTF-8.
            Some(String::from_utf8(token.to_vec()).expect("all valid token bytes are valid UTF-8"))
        }
    }

    /// Looks for an `=` as per [RFC 9111 Appendix A] which indicates that a
    /// cache directive has a value.
    ///
    /// This returns true if one was found. In which case, the `=` is consumed.
    ///
    /// This does not trim whitespace before or after the token.
    ///
    /// [RFC 9111 Appendix A]: https://www.rfc-editor.org/rfc/rfc9111.html#name-collected-abnf
    fn maybe_parse_equals(&mut self) -> bool {
        if self.cur.first().is_some_and(|&byte| byte == b'=') {
            self.cur = &self.cur[1..];
            true
        } else {
            false
        }
    }

    /// Parses a directive value as either an unquoted token or a quoted string
    /// as per [RFC 9111 Appendix A].
    ///
    /// If a valid value could not be found (for example, end-of-input or an
    /// opening quote without a closing quote), then `None` is returned. In
    /// this case, one should consider the cache-control header invalid.
    ///
    /// This does not trim whitespace before or after the token.
    ///
    /// Note that the returned value is *not* guaranteed to be valid UTF-8.
    /// Namely, it is possible for a quoted string to contain invalid UTF-8.
    ///
    /// [RFC 9111 Appendix A]: https://www.rfc-editor.org/rfc/rfc9111.html#name-collected-abnf
    fn parse_value(&mut self) -> Option<Vec<u8>> {
        if *self.cur.first()? == b'"' {
            self.cur = &self.cur[1..];
            self.parse_quoted_string()
        } else {
            self.parse_token().map(std::string::String::into_bytes)
        }
    }

    /// Parses a quoted string as per [RFC 9110 S5.6.4].
    ///
    /// This assumes the opening quote has already been consumed.
    ///
    /// If an invalid quoted string was found (e.g., no closing quote), then
    /// `None` is returned. An empty value may be returned.
    ///
    /// Note that the returned value is *not* guaranteed to be valid UTF-8.
    /// Namely, it is possible for a quoted string to contain invalid UTF-8.
    ///
    /// [RFC 9110 S5.6.4]: https://www.rfc-editor.org/rfc/rfc9110.html#name-quoted-strings
    fn parse_quoted_string(&mut self) -> Option<Vec<u8>> {
        fn is_qdtext_byte(byte: u8) -> bool {
            matches!(byte, b'\t' | b' ' | 0x21 | 0x23..=0x5B | 0x5D..=0x7E | 0x80..=0xFF)
        }
        fn is_quoted_pair_byte(byte: u8) -> bool {
            matches!(byte, b'\t' | b' ' | 0x21..=0x7E | 0x80..=0xFF)
        }
        let mut value = vec![];
        while !self.cur.is_empty() {
            let byte = self.cur[0];
            self.cur = &self.cur[1..];
            if byte == b'"' {
                return Some(value);
            } else if byte == b'\\' {
                let byte = *self.cur.first()?;
                self.cur = &self.cur[1..];
                // If we saw an escape but didn't see a valid
                // escaped byte, then we treat this value as
                // invalid.
                if !is_quoted_pair_byte(byte) {
                    return None;
                }
                value.push(byte);
            } else if is_qdtext_byte(byte) {
                value.push(byte);
            } else {
                break;
            }
        }
        // If we got here, it means we hit end-of-input before seeing a closing
        // quote. So we treat this as invalid and return `None`.
        None
    }

    /// Looks for a `,` as per [RFC 9111 Appendix A]. If one is found, then it
    /// is consumed and this returns true.
    ///
    /// This does not trim whitespace before or after the token.
    ///
    /// [RFC 9111 Appendix A]: https://www.rfc-editor.org/rfc/rfc9111.html#name-collected-abnf
    fn maybe_parse_directive_delimiter(&mut self) -> bool {
        if self.cur.first().is_some_and(|&byte| byte == b',') {
            self.cur = &self.cur[1..];
            true
        } else {
            false
        }
    }

    /// [RFC 9111 Appendix A] says that optional whitespace may appear between
    /// cache directives. We actually also allow whitespace to appear before
    /// the first directive and after the last directive.
    ///
    /// [RFC 9111 Appendix A]: https://www.rfc-editor.org/rfc/rfc9111.html#name-collected-abnf
    fn skip_whitespace(&mut self) {
        while self.cur.first().is_some_and(u8::is_ascii_whitespace) {
            self.cur = &self.cur[1..];
        }
    }

    fn emit_directive(
        &mut self,
        directive: CacheControlDirective,
    ) -> Option<CacheControlDirective> {
        let duplicate = !self.seen.insert(directive.name.clone());
        if duplicate {
            self.emit_revalidation()
        } else {
            Some(directive)
        }
    }

    fn emit_revalidation(&mut self) -> Option<CacheControlDirective> {
        if self.seen.insert("must-revalidate".to_string()) {
            Some(CacheControlDirective::must_revalidate())
        } else {
            // If we've already emitted a must-revalidate
            // directive, then don't do it again.
            None
        }
    }
}

impl<'b, B: 'b + ?Sized + AsRef<[u8]>, I: Iterator<Item = &'b B>> Iterator
    for CacheControlParser<'b, I>
{
    type Item = CacheControlDirective;

    fn next(&mut self) -> Option<CacheControlDirective> {
        loop {
            if self.cur.is_empty() {
                self.cur = self.directives.next().map(std::convert::AsRef::as_ref)?;
            }
            while !self.cur.is_empty() {
                self.skip_whitespace();
                let Some(mut name) = self.parse_token() else {
                    // If we fail to parse a token, then this header value is
                    // either corrupt or empty. So skip the rest of it.
                    let invalid = !self.cur.is_empty();
                    self.cur = b"";
                    // But if it was invalid, force revalidation.
                    if invalid {
                        if let Some(d) = self.emit_revalidation() {
                            return Some(d);
                        }
                    }
                    break;
                };
                name.make_ascii_lowercase();
                if !self.maybe_parse_equals() {
                    // Eat up whitespace and the next delimiter. We don't care
                    // if we find a terminator.
                    self.skip_whitespace();
                    self.maybe_parse_directive_delimiter();
                    let directive = CacheControlDirective {
                        name,
                        value: vec![],
                    };
                    match self.emit_directive(directive) {
                        None => continue,
                        Some(d) => return Some(d),
                    }
                }
                let Some(value) = self.parse_value() else {
                    // If we expected a value (we saw an =) but couldn't find a
                    // valid value, then this header value is probably corrupt.
                    // So skip the rest of it.
                    self.cur = b"";
                    match self.emit_revalidation() {
                        None => break,
                        Some(d) => return Some(d),
                    }
                };
                // Eat up whitespace and the next delimiter. We don't care if
                // we find a terminator.
                self.skip_whitespace();
                self.maybe_parse_directive_delimiter();
                let directive = CacheControlDirective { name, value };
                match self.emit_directive(directive) {
                    None => continue,
                    Some(d) => return Some(d),
                }
            }
        }
    }
}

/// A single directive from the `Cache-Control` header.
#[derive(Debug, Eq, PartialEq)]
struct CacheControlDirective {
    /// The name of the directive.
    name: String,
    /// A possibly empty value.
    ///
    /// Note that directive values may contain invalid UTF-8. (Although they
    /// cannot actually contain arbitrary bytes. For example, NUL bytes, among
    /// others, are not allowed.)
    value: Vec<u8>,
}

impl CacheControlDirective {
    /// Returns a `must-revalidate` directive. This is useful for forcing a
    /// cache decision that the response is stale, and thus the server should
    /// be consulted for whether the cached response ought to be used or not.
    fn must_revalidate() -> Self {
        Self {
            name: "must-revalidate".to_string(),
            value: vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_control_token() {
        let cc: CacheControl = CacheControlParser::new(["no-cache"]).collect();
        assert!(cc.no_cache);
        assert!(!cc.must_revalidate);
    }

    #[test]
    fn cache_control_max_age() {
        let cc: CacheControl = CacheControlParser::new(["max-age=60"]).collect();
        assert_eq!(Some(60), cc.max_age_seconds);
        assert!(!cc.must_revalidate);
    }

    // [RFC 9111 S5.2.1.1] says that client MUST NOT quote max-age, but we
    // support parsing it that way anyway.
    //
    // [RFC 9111 S5.2.1.1]: https://www.rfc-editor.org/rfc/rfc9111.html#section-5.2.1.1
    #[test]
    fn cache_control_max_age_quoted() {
        let cc: CacheControl = CacheControlParser::new([r#"max-age="60""#]).collect();
        assert_eq!(Some(60), cc.max_age_seconds);
        assert!(!cc.must_revalidate);
    }

    #[test]
    fn cache_control_max_age_invalid() {
        let cc: CacheControl = CacheControlParser::new(["max-age=6a0"]).collect();
        assert_eq!(None, cc.max_age_seconds);
        assert!(cc.must_revalidate);
    }

    #[test]
    fn cache_control_immutable() {
        let cc: CacheControl = CacheControlParser::new(["max-age=31536000, immutable"]).collect();
        assert_eq!(Some(31_536_000), cc.max_age_seconds);
        assert!(cc.immutable);
        assert!(!cc.must_revalidate);
    }

    #[test]
    fn cache_control_unrecognized() {
        let cc: CacheControl = CacheControlParser::new(["lion,max-age=60,zebra"]).collect();
        assert_eq!(Some(60), cc.max_age_seconds);
    }

    #[test]
    fn cache_control_invalid_squashes_remainder() {
        let cc: CacheControl = CacheControlParser::new(["no-cache,\x00,max-age=60"]).collect();
        // The invalid data doesn't impact things before it.
        assert!(cc.no_cache);
        // The invalid data precludes parsing anything after.
        assert_eq!(None, cc.max_age_seconds);
        // The invalid contents should force revalidation.
        assert!(cc.must_revalidate);
    }

    #[test]
    fn cache_control_invalid_squashes_remainder_but_not_other_header_values() {
        let cc: CacheControl =
            CacheControlParser::new(["no-cache,\x00,max-age=60", "max-stale=30"]).collect();
        // The invalid data doesn't impact things before it.
        assert!(cc.no_cache);
        // The invalid data precludes parsing anything after
        // in the same header value, but not in other
        // header values.
        assert_eq!(Some(30), cc.max_stale_seconds);
        // The invalid contents should force revalidation.
        assert!(cc.must_revalidate);
    }

    #[test]
    fn cache_control_parse_token() {
        let directives = CacheControlParser::new(["no-cache"]).collect::<Vec<_>>();
        assert_eq!(
            directives,
            vec![CacheControlDirective {
                name: "no-cache".to_string(),
                value: vec![]
            }]
        );
    }

    #[test]
    fn cache_control_parse_token_to_token_value() {
        let directives = CacheControlParser::new(["max-age=60"]).collect::<Vec<_>>();
        assert_eq!(
            directives,
            vec![CacheControlDirective {
                name: "max-age".to_string(),
                value: b"60".to_vec(),
            }]
        );
    }

    #[test]
    fn cache_control_parse_token_to_quoted_string() {
        let directives =
            CacheControlParser::new([r#"private="cookie,x-something-else""#]).collect::<Vec<_>>();
        assert_eq!(
            directives,
            vec![CacheControlDirective {
                name: "private".to_string(),
                value: b"cookie,x-something-else".to_vec(),
            }]
        );
    }

    #[test]
    fn cache_control_parse_token_to_quoted_string_with_escape() {
        let directives =
            CacheControlParser::new([r#"private="something\"crazy""#]).collect::<Vec<_>>();
        assert_eq!(
            directives,
            vec![CacheControlDirective {
                name: "private".to_string(),
                value: br#"something"crazy"#.to_vec(),
            }]
        );
    }

    #[test]
    fn cache_control_parse_multiple_directives() {
        let header = r#"max-age=60, no-cache, private="cookie", no-transform"#;
        let directives = CacheControlParser::new([header]).collect::<Vec<_>>();
        assert_eq!(
            directives,
            vec![
                CacheControlDirective {
                    name: "max-age".to_string(),
                    value: b"60".to_vec(),
                },
                CacheControlDirective {
                    name: "no-cache".to_string(),
                    value: vec![]
                },
                CacheControlDirective {
                    name: "private".to_string(),
                    value: b"cookie".to_vec(),
                },
                CacheControlDirective {
                    name: "no-transform".to_string(),
                    value: vec![]
                },
            ]
        );
    }

    #[test]
    fn cache_control_parse_multiple_directives_across_multiple_header_values() {
        let headers = [
            r"max-age=60, no-cache",
            r#"private="cookie""#,
            r"no-transform",
        ];
        let directives = CacheControlParser::new(headers).collect::<Vec<_>>();
        assert_eq!(
            directives,
            vec![
                CacheControlDirective {
                    name: "max-age".to_string(),
                    value: b"60".to_vec(),
                },
                CacheControlDirective {
                    name: "no-cache".to_string(),
                    value: vec![]
                },
                CacheControlDirective {
                    name: "private".to_string(),
                    value: b"cookie".to_vec(),
                },
                CacheControlDirective {
                    name: "no-transform".to_string(),
                    value: vec![]
                },
            ]
        );
    }

    #[test]
    fn cache_control_parse_one_header_invalid() {
        let headers = [
            r"max-age=60, no-cache",
            r#", private="cookie""#,
            r"no-transform",
        ];
        let directives = CacheControlParser::new(headers).collect::<Vec<_>>();
        assert_eq!(
            directives,
            vec![
                CacheControlDirective {
                    name: "max-age".to_string(),
                    value: b"60".to_vec(),
                },
                CacheControlDirective {
                    name: "no-cache".to_string(),
                    value: vec![]
                },
                CacheControlDirective {
                    name: "must-revalidate".to_string(),
                    value: vec![]
                },
                CacheControlDirective {
                    name: "no-transform".to_string(),
                    value: vec![]
                },
            ]
        );
    }

    #[test]
    fn cache_control_parse_invalid_directive_drops_remainder() {
        let header = r#"max-age=60, no-cache, ="cookie", no-transform"#;
        let directives = CacheControlParser::new([header]).collect::<Vec<_>>();
        assert_eq!(
            directives,
            vec![
                CacheControlDirective {
                    name: "max-age".to_string(),
                    value: b"60".to_vec(),
                },
                CacheControlDirective {
                    name: "no-cache".to_string(),
                    value: vec![]
                },
                CacheControlDirective {
                    name: "must-revalidate".to_string(),
                    value: vec![]
                },
            ]
        );
    }

    #[test]
    fn cache_control_parse_name_normalized() {
        let header = r"MAX-AGE=60";
        let directives = CacheControlParser::new([header]).collect::<Vec<_>>();
        assert_eq!(
            directives,
            vec![CacheControlDirective {
                name: "max-age".to_string(),
                value: b"60".to_vec(),
            },]
        );
    }

    // When a duplicate directive is found, we keep the first one
    // and add in a `must-revalidate` directive to indicate that
    // things are stale and the client should do a re-check.
    #[test]
    fn cache_control_parse_duplicate_directives() {
        let header = r"max-age=60, no-cache, max-age=30";
        let directives = CacheControlParser::new([header]).collect::<Vec<_>>();
        assert_eq!(
            directives,
            vec![
                CacheControlDirective {
                    name: "max-age".to_string(),
                    value: b"60".to_vec(),
                },
                CacheControlDirective {
                    name: "no-cache".to_string(),
                    value: vec![]
                },
                CacheControlDirective {
                    name: "must-revalidate".to_string(),
                    value: vec![]
                },
            ]
        );
    }

    #[test]
    fn cache_control_parse_duplicate_directives_across_headers() {
        let headers = [r"max-age=60, no-cache", r"max-age=30"];
        let directives = CacheControlParser::new(headers).collect::<Vec<_>>();
        assert_eq!(
            directives,
            vec![
                CacheControlDirective {
                    name: "max-age".to_string(),
                    value: b"60".to_vec(),
                },
                CacheControlDirective {
                    name: "no-cache".to_string(),
                    value: vec![]
                },
                CacheControlDirective {
                    name: "must-revalidate".to_string(),
                    value: vec![]
                },
            ]
        );
    }

    // Tests that we don't emit must-revalidate multiple times
    // even when something is duplicated multiple times.
    #[test]
    fn cache_control_parse_duplicate_redux() {
        let header = r"max-age=60, no-cache, no-cache, max-age=30";
        let directives = CacheControlParser::new([header]).collect::<Vec<_>>();
        assert_eq!(
            directives,
            vec![
                CacheControlDirective {
                    name: "max-age".to_string(),
                    value: b"60".to_vec(),
                },
                CacheControlDirective {
                    name: "no-cache".to_string(),
                    value: vec![]
                },
                CacheControlDirective {
                    name: "must-revalidate".to_string(),
                    value: vec![]
                },
            ]
        );
    }
}
