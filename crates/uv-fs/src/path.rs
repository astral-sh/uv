use std::borrow::Cow;
use std::path::{Component, Path, PathBuf};

pub trait Simplified {
    /// Simplify a [`Path`].
    ///
    /// On Windows, this will strip the `\\?\` prefix from paths. On other platforms, it's a no-op.
    fn simplified(&self) -> &Path;

    /// Render a [`Path`] for user-facing display.
    ///
    /// On Windows, this will strip the `\\?\` prefix from paths. On other platforms, it's
    /// equivalent to [`std::path::Display`].
    fn simplified_display(&self) -> std::path::Display;
}

impl<T: AsRef<Path>> Simplified for T {
    fn simplified(&self) -> &Path {
        dunce::simplified(self.as_ref())
    }

    fn simplified_display(&self) -> std::path::Display {
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

/// Normalize a path, removing things like `.` and `..`.
///
/// Source: <https://github.com/rust-lang/cargo/blob/b48c41aedbd69ee3990d62a0e2006edbb506a480/crates/cargo-util/src/paths.rs#L76C1-L109C2>
pub fn normalize_path(path: impl AsRef<Path>) -> PathBuf {
    let mut components = path.as_ref().components().peekable();
    let mut ret = if let Some(c @ Component::Prefix(..)) = components.peek().copied() {
        components.next();
        PathBuf::from(c.as_os_str())
    } else {
        PathBuf::new()
    };

    for component in components {
        match component {
            Component::Prefix(..) => unreachable!(),
            Component::RootDir => {
                ret.push(component.as_os_str());
            }
            Component::CurDir => {}
            Component::ParentDir => {
                ret.pop();
            }
            Component::Normal(c) => {
                ret.push(c);
            }
        }
    }
    ret
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
