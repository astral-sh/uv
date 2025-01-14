use memchr::memchr;

pub(crate) struct MemchrSplitter<'a> {
    haystack: &'a [u8],
    delimiter: u8,
    offset: usize,
}

impl<'a> MemchrSplitter<'a> {
    pub(crate) fn split(haystack: &'a [u8], delimiter: u8) -> Self {
        MemchrSplitter {
            haystack,
            delimiter,
            offset: 0,
        }
    }
}

impl<'a> Iterator for MemchrSplitter<'a> {
    type Item = &'a [u8];

    fn next(&mut self) -> Option<Self::Item> {
        if self.offset >= self.haystack.len() {
            return None;
        }

        // Find the next delimiter.
        let next_delimiter = memchr(
            self.delimiter,
            &self.haystack[self.offset..]
        );

        match next_delimiter {
            Some(index) => {
                // Get the slice from the current offset to the delimiter.
                let slice = &self.haystack[self.offset..self.offset + index];
                self.offset = self.offset + index + 1;
                Some(slice)
            }
            None => {
                // Return the remaining slice.
                let slice = &self.haystack[self.offset..];
                self.offset = self.haystack.len();
                Some(slice)
            }
        }
    }
}
