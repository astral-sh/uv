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

/// Like `fs_err::canonicalize`, but with permissive failures on Windows.
///
/// On Windows, we can't canonicalize the resolved path to Pythons that are installed via the
/// Windows Store. For example, if you install Python via the Windows Store, then run `python`
/// and print the `sys.executable` path, you'll get a path like:
///
/// ```text
/// C:\Users\crmar\AppData\Local\Microsoft\WindowsApps\PythonSoftwareFoundation.Python.3.11_qbs5n2kfra8p0\python.exe
/// ```
///
/// Attempting to canonicalize this path will fail with `ErrorKind::Uncategorized`.
pub fn canonicalize_executable(path: impl AsRef<Path>) -> std::io::Result<PathBuf> {
    let path = path.as_ref();
    if is_windows_store_python(path) {
        Ok(path.to_path_buf())
    } else {
        fs_err::canonicalize(path)
    }
}

/// Returns `true` if this is a Python executable or shim installed via the Windows Store, based on
/// the path.
///
/// This method does _not_ introspect the filesystem to determine if the shim is a redirect to the
/// Windows Store installer. In other words, it assumes that the path represents a Python
/// executable, not a redirect.
fn is_windows_store_python(path: &Path) -> bool {
    /// Returns `true` if this is a Python executable shim installed via the Windows Store, like:
    ///
    /// ```text
    /// C:\Users\crmar\AppData\Local\Microsoft\WindowsApps\python3.exe
    /// ```
    fn is_windows_store_python_shim(path: &Path) -> bool {
        let mut components = path.components().rev();

        // Ex) `python.exe`, or `python3.exe`, or `python3.12.exe`
        if !components
            .next()
            .and_then(|component| component.as_os_str().to_str())
            .is_some_and(|component| component.starts_with("python"))
        {
            return false;
        }

        // Ex) `WindowsApps`
        if !components
            .next()
            .is_some_and(|component| component.as_os_str() == "WindowsApps")
        {
            return false;
        }

        // Ex) `Microsoft`
        if !components
            .next()
            .is_some_and(|component| component.as_os_str() == "Microsoft")
        {
            return false;
        }

        true
    }

    /// Returns `true` if this is a Python executable installed via the Windows Store, like:
    ///
    /// ```text
    /// C:\Users\crmar\AppData\Local\Microsoft\WindowsApps\PythonSoftwareFoundation.Python.3.11_qbs5n2kfra8p0\python.exe
    /// ```
    fn is_windows_store_python_executable(path: &Path) -> bool {
        let mut components = path.components().rev();

        // Ex) `python.exe`
        if !components
            .next()
            .and_then(|component| component.as_os_str().to_str())
            .is_some_and(|component| component.starts_with("python"))
        {
            return false;
        }

        // Ex) `PythonSoftwareFoundation.Python.3.11_qbs5n2kfra8p0`
        if !components
            .next()
            .and_then(|component| component.as_os_str().to_str())
            .is_some_and(|component| component.starts_with("PythonSoftwareFoundation.Python.3."))
        {
            return false;
        }

        // Ex) `WindowsApps`
        if !components
            .next()
            .is_some_and(|component| component.as_os_str() == "WindowsApps")
        {
            return false;
        }

        // Ex) `Microsoft`
        if !components
            .next()
            .is_some_and(|component| component.as_os_str() == "Microsoft")
        {
            return false;
        }

        true
    }

    if !cfg!(windows) {
        return false;
    }

    if !path.is_absolute() {
        return false;
    }

    is_windows_store_python_shim(path) || is_windows_store_python_executable(path)
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
