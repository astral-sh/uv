use std::borrow::Cow;
use std::path::Path;

pub trait Normalized {
    /// Normalize a [`Path`].
    ///
    /// On Windows, this will strip the `\\?\` prefix from paths. On other platforms, it's a no-op.
    fn normalized(&self) -> &Path;

    /// Render a [`Path`] for user-facing display.
    ///
    /// On Windows, this will strip the `\\?\` prefix from paths. On other platforms, it's
    /// equivalent to [`std::path::Display`].
    fn normalized_display(&self) -> std::path::Display;
}

impl<T: AsRef<Path>> Normalized for T {
    fn normalized(&self) -> &Path {
        dunce::simplified(self.as_ref())
    }

    fn normalized_display(&self) -> std::path::Display {
        dunce::simplified(self.as_ref()).display()
    }
}

/// Normalize the `path` component of a URL for use as a file path.
///
/// For example, on Windows, transforms `C:\Users\ferris\wheel-0.42.0.tar.gz` to
/// `/C:/Users/ferris/wheel-0.42.0.tar.gz`.
///
/// On other platforms, this is a no-op.
pub fn normalize_url_path(path: &str) -> Cow<'_, str> {
    // Apply percent-decoding to the URL.
    let path = urlencoding::decode(path).unwrap_or(Cow::Borrowed(path));

    // Return the path.
    if cfg!(windows) {
        Cow::Owned(
            path.strip_prefix('/')
                .unwrap_or(&path)
                .replace('/', std::path::MAIN_SEPARATOR_STR),
        )
    } else {
        path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize() {
        if cfg!(windows) {
            assert_eq!(
                normalize_url_path("/C:/Users/ferris/wheel-0.42.0.tar.gz"),
                "C:\\Users\\ferris\\wheel-0.42.0.tar.gz"
            );
        } else {
            assert_eq!(
                normalize_url_path("/C:/Users/ferris/wheel-0.42.0.tar.gz"),
                "/C:/Users/ferris/wheel-0.42.0.tar.gz"
            );
        }

        if cfg!(windows) {
            assert_eq!(
                normalize_url_path("./ferris/wheel-0.42.0.tar.gz"),
                ".\\ferris\\wheel-0.42.0.tar.gz"
            );
        } else {
            assert_eq!(
                normalize_url_path("./ferris/wheel-0.42.0.tar.gz"),
                "./ferris/wheel-0.42.0.tar.gz"
            );
        }

        if cfg!(windows) {
            assert_eq!(
                normalize_url_path("./wheel%20cache/wheel-0.42.0.tar.gz"),
                ".\\wheel cache\\wheel-0.42.0.tar.gz"
            );
        } else {
            assert_eq!(
                normalize_url_path("./wheel%20cache/wheel-0.42.0.tar.gz"),
                "./wheel cache/wheel-0.42.0.tar.gz"
            );
        }
    }
}
