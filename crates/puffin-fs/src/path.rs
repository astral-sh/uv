use std::path::Path;

pub trait NormalizedDisplay {
    /// Render a [`Path`] for user-facing display.
    ///
    /// On Windows, this will strip the `\\?\` prefix from paths. On other platforms, it's
    /// equivalent to [`std::path::Display`].
    fn normalized_display(&self) -> std::path::Display;
}

impl<T: AsRef<Path>> NormalizedDisplay for T {
    fn normalized_display(&self) -> std::path::Display {
        dunce::simplified(self.as_ref()).display()
    }
}
