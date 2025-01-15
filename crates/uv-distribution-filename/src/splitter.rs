/// A simple splitter that uses `memchr` to find the next delimiter.
pub(crate) struct MemchrSplitter<'a> {
    memchr: memchr::Memchr<'a>,
    haystack: &'a str,
    offset: usize,
}

impl<'a> MemchrSplitter<'a> {
    #[inline]
    pub(crate) fn split(haystack: &'a str, delimiter: u8) -> Self {
        Self {
            memchr: memchr::Memchr::new(delimiter, haystack.as_bytes()),
            haystack,
            offset: 0,
        }
    }
}

impl<'a> Iterator for MemchrSplitter<'a> {
    type Item = &'a str;

    #[inline(always)]
    #[allow(clippy::inline_always)]
    fn next(&mut self) -> Option<Self::Item> {
        match self.memchr.next() {
            Some(index) => {
                let start = self.offset;
                self.offset = index + 1;
                Some(&self.haystack[start..index])
            }
            None if self.offset < self.haystack.len() => {
                let start = self.offset;
                self.offset = self.haystack.len();
                Some(&self.haystack[start..])
            }
            None => None,
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        // We know we'll return at least one item if there's remaining text.
        let min = usize::from(self.offset < self.haystack.len());

        // Maximum possible splits is remaining length divided by 2 (minimum one char between delimiters).
        let max = (self.haystack.len() - self.offset + 1) / 2 + min;

        (min, Some(max))
    }
}
