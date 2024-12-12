#![allow(dead_code)]

use std::str::Chars;

pub(super) const EOF_CHAR: char = '\0';

/// A cursor represents a pointer in the source code.
///
/// Based on [`rustc`'s `Cursor`](https://github.com/rust-lang/rust/blob/d1b7355d3d7b4ead564dbecb1d240fcc74fff21b/compiler/rustc_lexer/src/cursor.rs)
#[derive(Clone, Debug)]
pub(super) struct Cursor<'src> {
    /// An iterator over the [`char`]'s of the source code.
    chars: Chars<'src>,

    /// Stores the previous character for debug assertions.
    #[cfg(debug_assertions)]
    prev_char: char,
}

impl<'src> Cursor<'src> {
    pub(super) fn new(source: &'src str) -> Self {
        Self {
            chars: source.chars(),
            #[cfg(debug_assertions)]
            prev_char: EOF_CHAR,
        }
    }

    /// Returns the previous character. Useful for debug assertions.
    #[cfg(debug_assertions)]
    pub(super) const fn previous(&self) -> char {
        self.prev_char
    }

    /// Peeks the next character from the input stream without consuming it.
    /// Returns [`EOF_CHAR`] if the position is past the end of the file.
    pub(super) fn first(&self) -> char {
        self.chars.clone().next().unwrap_or(EOF_CHAR)
    }

    /// Peeks the second character from the input stream without consuming it.
    /// Returns [`EOF_CHAR`] if the position is past the end of the file.
    pub(super) fn second(&self) -> char {
        let mut chars = self.chars.clone();
        chars.next();
        chars.next().unwrap_or(EOF_CHAR)
    }

    /// Returns the remaining text to lex.
    ///
    /// Use [`Cursor::text_len`] to get the length of the remaining text.
    pub(super) fn rest(&self) -> &'src str {
        self.chars.as_str()
    }

    /// Returns `true` if the cursor is at the end of file.
    pub(super) fn is_eof(&self) -> bool {
        self.chars.as_str().is_empty()
    }

    /// Moves the cursor to the next character, returning the previous character.
    /// Returns [`None`] if there is no next character.
    pub(super) fn bump(&mut self) -> Option<char> {
        let prev = self.chars.next()?;

        #[cfg(debug_assertions)]
        {
            self.prev_char = prev;
        }

        Some(prev)
    }

    pub(super) fn eat_char(&mut self, c: char) -> bool {
        if self.first() == c {
            self.bump();
            true
        } else {
            false
        }
    }

    pub(super) fn eat_char2(&mut self, c1: char, c2: char) -> bool {
        let mut chars = self.chars.clone();
        if chars.next() == Some(c1) && chars.next() == Some(c2) {
            self.bump();
            self.bump();
            true
        } else {
            false
        }
    }

    pub(super) fn eat_char3(&mut self, c1: char, c2: char, c3: char) -> bool {
        let mut chars = self.chars.clone();
        if chars.next() == Some(c1) && chars.next() == Some(c2) && chars.next() == Some(c3) {
            self.bump();
            self.bump();
            self.bump();
            true
        } else {
            false
        }
    }

    pub(super) fn eat_if<F>(&mut self, mut predicate: F) -> Option<char>
    where
        F: FnMut(char) -> bool,
    {
        if predicate(self.first()) && !self.is_eof() {
            self.bump()
        } else {
            None
        }
    }

    /// Eats symbols while predicate returns true or until the end of file is reached.
    #[inline]
    pub(super) fn eat_while(&mut self, mut predicate: impl FnMut(char) -> bool) {
        // It was tried making optimized version of this for eg. line comments, but
        // LLVM can inline all of this and compile it down to fast iteration over bytes.
        while predicate(self.first()) && !self.is_eof() {
            self.bump();
        }
    }

    /// Skips the next `count` bytes.
    ///
    /// ## Panics
    ///  - If `count` is larger than the remaining bytes in the input stream.
    ///  - If `count` indexes into a multi-byte character.
    pub(super) fn skip_bytes(&mut self, count: usize) {
        #[cfg(debug_assertions)]
        {
            self.prev_char = self.chars.as_str()[..count]
                .chars()
                .next_back()
                .unwrap_or('\0');
        }

        self.chars = self.chars.as_str()[count..].chars();
    }

    /// Skips to the end of the input stream.
    pub(super) fn skip_to_end(&mut self) {
        self.chars = "".chars();
    }
}
