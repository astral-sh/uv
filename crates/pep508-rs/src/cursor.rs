use std::fmt::{Display, Formatter};
use std::str::Chars;

use crate::{Pep508Error, Pep508ErrorSource, Pep508Url};

/// A [`Cursor`] over a string.
#[derive(Debug, Clone)]
pub(crate) struct Cursor<'a> {
    input: &'a str,
    chars: Chars<'a>,
    pos: usize,
}

impl<'a> Cursor<'a> {
    /// Convert from `&str`.
    pub(crate) fn new(input: &'a str) -> Self {
        Self {
            input,
            chars: input.chars(),
            pos: 0,
        }
    }

    /// Returns a new cursor starting at the given position.
    pub(crate) fn at(self, pos: usize) -> Self {
        Self {
            input: self.input,
            chars: self.input[pos..].chars(),
            pos,
        }
    }

    /// Returns the current byte position of the cursor.
    pub(crate) fn pos(&self) -> usize {
        self.pos
    }

    /// Returns a slice over the input string.
    pub(crate) fn slice(&self, start: usize, len: usize) -> &str {
        &self.input[start..start + len]
    }

    /// Peeks the next character and position from the input stream without consuming it.
    pub(crate) fn peek(&self) -> Option<(usize, char)> {
        self.chars.clone().next().map(|char| (self.pos, char))
    }

    /// Peeks the next character from the input stream without consuming it.
    pub(crate) fn peek_char(&self) -> Option<char> {
        self.chars.clone().next()
    }

    /// Eats the next character from the input stream if it matches the given token.
    pub(crate) fn eat_char(&mut self, token: char) -> Option<usize> {
        let (start_pos, peek_char) = self.peek()?;
        if peek_char == token {
            self.next();
            Some(start_pos)
        } else {
            None
        }
    }

    /// Consumes whitespace from the cursor.
    pub(crate) fn eat_whitespace(&mut self) {
        while let Some(char) = self.peek_char() {
            if char.is_whitespace() {
                self.next();
            } else {
                return;
            }
        }
    }

    /// Returns the next character and position from the input stream and consumes it.
    pub(crate) fn next(&mut self) -> Option<(usize, char)> {
        let pos = self.pos;
        let char = self.chars.next()?;
        self.pos += char.len_utf8();
        Some((pos, char))
    }

    pub(crate) fn remaining(&self) -> usize {
        self.chars.clone().count()
    }

    /// Peeks over the cursor as long as the condition is met, without consuming it.
    pub(crate) fn peek_while(&mut self, condition: impl Fn(char) -> bool) -> (usize, usize) {
        let peeker = self.chars.clone();
        let start = self.pos();
        let len = peeker.take_while(|c| condition(*c)).count();
        (start, len)
    }

    /// Consumes characters from the cursor as long as the condition is met.
    pub(crate) fn take_while(&mut self, condition: impl Fn(char) -> bool) -> (usize, usize) {
        let start = self.pos();
        let mut len = 0;
        while let Some(char) = self.peek_char() {
            if !condition(char) {
                break;
            }

            self.next();
            len += char.len_utf8();
        }
        (start, len)
    }

    /// Consumes characters from the cursor, raising an error if it doesn't match the given token.
    pub(crate) fn next_expect_char<T: Pep508Url>(
        &mut self,
        expected: char,
        span_start: usize,
    ) -> Result<(), Pep508Error<T>> {
        match self.next() {
            None => Err(Pep508Error {
                message: Pep508ErrorSource::String(format!(
                    "Expected '{expected}', found end of dependency specification"
                )),
                start: span_start,
                len: 1,
                input: self.to_string(),
            }),
            Some((_, value)) if value == expected => Ok(()),
            Some((pos, other)) => Err(Pep508Error {
                message: Pep508ErrorSource::String(format!(
                    "Expected '{expected}', found '{other}'"
                )),
                start: pos,
                len: other.len_utf8(),
                input: self.to_string(),
            }),
        }
    }
}

impl Display for Cursor<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.input)
    }
}
