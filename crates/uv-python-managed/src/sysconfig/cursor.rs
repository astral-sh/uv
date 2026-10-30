use std::str::Chars;

const EOF_CHAR: char = '\0';

/// A cursor represents a pointer in the source code.
///
/// Based on [`rustc`'s `Cursor`](https://github.com/rust-lang/rust/blob/d1b7355d3d7b4ead564dbecb1d240fcc74fff21b/compiler/rustc_lexer/src/cursor.rs)
#[derive(Clone, Debug)]
pub(super) struct Cursor<'src> {
    /// An iterator over the [`char`]'s of the source code.
    chars: Chars<'src>,
}

impl<'src> Cursor<'src> {
    pub(super) fn new(source: &'src str) -> Self {
        Self {
            chars: source.chars(),
        }
    }

    /// Peeks the next character from the input stream without consuming it.
    /// Returns [`EOF_CHAR`] if the position is past the end of the file.
    pub(super) fn first(&self) -> char {
        self.chars.clone().next().unwrap_or(EOF_CHAR)
    }

    /// Returns `true` if the cursor is at the end of file.
    fn is_eof(&self) -> bool {
        self.chars.as_str().is_empty()
    }

    /// Moves the cursor to the next character, returning the previous character.
    /// Returns [`None`] if there is no next character.
    pub(super) fn bump(&mut self) -> Option<char> {
        self.chars.next()
    }

    pub(super) fn eat_char(&mut self, c: char) -> bool {
        if self.first() == c {
            self.bump();
            true
        } else {
            false
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
}
