//! POSIX Shell Compatible Argument Parser
//!
//! This implementation is vendored from the [`r-shquote`](https://github.com/r-util/r-shquote)
//! crate under the Apache 2.0 license:
//!
//! ```text
//! Licensed under the Apache License, Version 2.0 (the "License");
//! you may not use this file except in compliance with the License.
//! You may obtain a copy of the License at
//!
//!         https://www.apache.org/licenses/LICENSE-2.0
//!
//! Unless required by applicable law or agreed to in writing, software
//! distributed under the License is distributed on an "AS IS" BASIS,
//! WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
//! See the License for the specific language governing permissions and
//! limitations under the License.
//! ```
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) enum UnquoteError {
    UnterminatedSingleQuote {
        char_cursor: usize,
        byte_cursor: usize,
    },
    UnterminatedDoubleQuote {
        char_cursor: usize,
        byte_cursor: usize,
    },
}

impl std::fmt::Display for UnquoteError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for UnquoteError {}

fn unquote_open_single(
    acc: &mut String,
    cursor: &mut std::iter::Enumerate<std::str::CharIndices>,
) -> bool {
    // This decodes a single-quote sequence. The opening single-quote was already parsed by
    // the caller. Both `&source[start]` and `cursor` point to the first character following
    // the opening single-quote.
    // Anything inside the single-quote sequence is copied verbatim to the output until the
    // next single-quote. No escape sequences are supported, not even a single-quote can be
    // escaped. However, if the sequence is not terminated, the entire operation is considered
    // invalid.
    for i in cursor {
        match i {
            (_, (_, '\'')) => return true,
            (_, (_, c)) => acc.push(c),
        }
    }

    false
}

fn unquote_open_double(
    acc: &mut String,
    cursor: &mut std::iter::Enumerate<std::str::CharIndices>,
) -> bool {
    // This decodes a double-quote sequence. The opening double-quote was already parsed by
    // the caller. Both `&source[start]` and `cursor` point to the first character following
    // the opening double-quote.
    // A double-quote sequence allows escape-sequences and goes until the closing
    // double-quote. If the sequence is not terminated, though, the entire operation is
    // considered invalid.
    loop {
        match cursor.next() {
            Some((_, (_, '"'))) => {
                // An unescaped double-quote character terminates the double-quote sequence.
                // It produces no output.
                return true;
            }
            Some((_, (_, '\\'))) => {
                // Inside a double-quote sequence several escape sequences are allowed. In
                // general, any unknown sequence is copied verbatim in its entirety including
                // the backslash. Known sequences produce the escaped character in its output
                // and makes the parser not interpret it. If the sequence is non-terminated,
                // it implies that the double-quote sequence is non-terminated and thus
                // invokes the same behavior, meaning the entire operation is refused.
                match cursor.next() {
                    Some((_, (_, esc_ch)))
                        if esc_ch == '"'
                            || esc_ch == '\\'
                            || esc_ch == '`'
                            || esc_ch == '$'
                            || esc_ch == '\n' =>
                    {
                        acc.push(esc_ch);
                    }
                    Some((_, (_, esc_ch))) => {
                        acc.push('\\');
                        acc.push(esc_ch);
                    }
                    None => {
                        return false;
                    }
                }
            }
            Some((_, (_, inner_ch))) => {
                // Any non-special character inside a double-quote is copied
                // literally just like characters outside of it.
                acc.push(inner_ch);
            }
            None => {
                // The double-quote sequence was not terminated. The entire
                // operation is considered invalid and we have to refuse producing
                // any resulting value.
                return false;
            }
        }
    }
}

fn unquote_open_escape(acc: &mut String, cursor: &mut std::iter::Enumerate<std::str::CharIndices>) {
    // This decodes an escape sequence outside of any quote. The opening backslash was already
    // parsed by the caller. Both `&source[start]` and `cursor` point to the first character
    // following the opening backslash.
    // Outside of quotes, an escape sequence simply treats the next character literally, and
    // does not interpret it. The exceptions are literal <NL> (newline character) and a single
    // backslash as last character in the string. In these cases the escape-sequence is
    // stripped and produces no output. The <NL> case is a remnant of human shell input, where
    // you can input multiple lines by appending a backslash to the previous line. This causes
    // both the backslash and <NL> to be ignore, since they purely serve readability of user
    // input.
    if let Some((_, (_, esc_ch))) = cursor.next() {
        if esc_ch != '\n' {
            acc.push(esc_ch);
        }
    }
}

/// Unquote String
///
/// Unquote a single string according to POSIX Shell quoting and escaping rules. If the input
/// string is not a valid input, the operation will fail and provide diagnosis information on
/// where the first invalid part was encountered.
///
/// The result is canonical. There is only one valid unquoted result for a given input.
///
/// If the string does not require any quoting or escaping, returns `Ok(None)`.
///
/// # Examples
///
/// ```
/// assert_eq!(r_shquote::unquote("foobar").unwrap(), "foobar");
/// ```
pub(crate) fn unquote(source: &str) -> Result<Option<String>, UnquoteError> {
    // If the string does not contain any single-quotes, double-quotes, or escape sequences, it
    // does not require any unquoting.
    if memchr::memchr3(b'\'', b'"', b'\\', source.as_bytes()).is_none() {
        return Ok(None);
    }

    // An unquote-operation never results in a longer string. Furthermore, the common case is
    // most of the string is unquoted / unescaped. Hence, we simply allocate the same space
    // for the resulting string as the input.
    let mut acc = String::with_capacity(source.len());

    // We loop over the string. When a single-quote, double-quote, or escape sequence is
    // opened, we let our helpers parse the sub-strings. Anything else is copied over
    // literally until the end of the line.
    let mut cursor = source.char_indices().enumerate();
    loop {
        match cursor.next() {
            Some((next_idx, (next_pos, '\''))) => {
                if !unquote_open_single(&mut acc, &mut cursor) {
                    break Err(UnquoteError::UnterminatedSingleQuote {
                        char_cursor: next_idx,
                        byte_cursor: next_pos,
                    });
                }
            }
            Some((next_idx, (next_pos, '"'))) => {
                if !unquote_open_double(&mut acc, &mut cursor) {
                    break Err(UnquoteError::UnterminatedDoubleQuote {
                        char_cursor: next_idx,
                        byte_cursor: next_pos,
                    });
                }
            }
            Some((_, (_, '\\'))) => {
                unquote_open_escape(&mut acc, &mut cursor);
            }
            Some((_, (_, next_ch))) => {
                acc.push(next_ch);
            }
            None => {
                break Ok(Some(acc));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic() {
        assert_eq!(unquote("foobar").unwrap(), None);
        assert_eq!(unquote("foo'bar'").unwrap().unwrap(), "foobar");
        assert_eq!(unquote("foo\"bar\"").unwrap().unwrap(), "foobar");
        assert_eq!(unquote("\\foobar\\").unwrap().unwrap(), "foobar");
        assert_eq!(unquote("\\'foobar\\'").unwrap().unwrap(), "'foobar'");
    }
}
