use either::Either;
use std::borrow::Cow;
use std::path::{Component, Path, PathBuf};
use std::{io, iter};

use once_cell::sync::Lazy;
use path_slash::PathExt;

/// The current working directory.
pub static CWD: Lazy<PathBuf> =
    Lazy::new(|| std::env::current_dir().expect("The current directory must exist"));

/// The current working directory, canonicalized.
pub static CANONICAL_CWD: Lazy<PathBuf> = Lazy::new(|| {
    std::env::current_dir()
        .expect("The current directory must exist")
        .canonicalize()
        .expect("The current directory must be canonicalized")
});

pub trait Simplified {
    /// Simplify a [`Path`].
    ///
    /// On Windows, this will strip the `\\?\` prefix from paths. On other platforms, it's a no-op.
    fn simplified(&self) -> &Path;

    /// Render a [`Path`] for display.
    ///
    /// On Windows, this will strip the `\\?\` prefix from paths. On other platforms, it's
    /// equivalent to [`std::path::Display`].
    fn simplified_display(&self) -> impl std::fmt::Display;

    /// Canonicalize a path without a `\\?\` prefix on Windows.
    fn simple_canonicalize(&self) -> std::io::Result<PathBuf>;

    /// Render a [`Path`] for user-facing display.
    ///
    /// Like [`simplified_display`], but relativizes the path against the current working directory.
    fn user_display(&self) -> impl std::fmt::Display;

    /// Render a [`Path`] for user-facing display, where the [`Path`] is relative to a base path.
    ///
    /// If the [`Path`] is not relative to the base path, will attempt to relativize the path
    /// against the current working directory.
    fn user_display_from(&self, base: impl AsRef<Path>) -> impl std::fmt::Display;

    /// Render a [`Path`] for user-facing display using a portable representation.
    ///
    /// Like [`user_display`], but uses a portable representation for relative paths.
    fn portable_display(&self) -> impl std::fmt::Display;
}

impl<T: AsRef<Path>> Simplified for T {
    fn simplified(&self) -> &Path {
        dunce::simplified(self.as_ref())
    }

    fn simplified_display(&self) -> impl std::fmt::Display {
        dunce::simplified(self.as_ref()).display()
    }

    fn simple_canonicalize(&self) -> std::io::Result<PathBuf> {
        dunce::canonicalize(self.as_ref())
    }

    fn user_display(&self) -> impl std::fmt::Display {
        let path = dunce::simplified(self.as_ref());

        // Attempt to strip the current working directory, then the canonicalized current working
        // directory, in case they differ.
        let path = path.strip_prefix(CWD.simplified()).unwrap_or_else(|_| {
            path.strip_prefix(CANONICAL_CWD.simplified())
                .unwrap_or(path)
        });

        path.display()
    }

    fn user_display_from(&self, base: impl AsRef<Path>) -> impl std::fmt::Display {
        let path = dunce::simplified(self.as_ref());

        // Attempt to strip the base, then the current working directory, then the canonicalized
        // current working directory, in case they differ.
        let path = path.strip_prefix(base.as_ref()).unwrap_or_else(|_| {
            path.strip_prefix(CWD.simplified()).unwrap_or_else(|_| {
                path.strip_prefix(CANONICAL_CWD.simplified())
                    .unwrap_or(path)
            })
        });

        path.display()
    }

    fn portable_display(&self) -> impl std::fmt::Display {
        let path = dunce::simplified(self.as_ref());

        // Attempt to strip the current working directory, then the canonicalized current working
        // directory, in case they differ.
        let path = path.strip_prefix(CWD.simplified()).unwrap_or_else(|_| {
            path.strip_prefix(CANONICAL_CWD.simplified())
                .unwrap_or(path)
        });

        // Use a portable representation for relative paths.
        path.to_slash()
            .map(Either::Left)
            .unwrap_or_else(|| Either::Right(path.display()))
    }
}

pub trait PythonExt {
    /// Escape a [`Path`] for use in Python code.
    fn escape_for_python(&self) -> String;
}

impl<T: AsRef<Path>> PythonExt for T {
    fn escape_for_python(&self) -> String {
        self.as_ref()
            .to_string_lossy()
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
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
///
/// CAUTION: Assumes that the path is already absolute.
///
/// CAUTION: This does not resolve symlinks (unlike
/// [`std::fs::canonicalize`]). This may cause incorrect or surprising
/// behavior at times. This should be used carefully. Unfortunately,
/// [`std::fs::canonicalize`] can be hard to use correctly, since it can often
/// fail, or on Windows returns annoying device paths.
///
/// # Errors
///
/// When a relative path is provided with `..` components that extend beyond the base directory.
/// For example, `./a/../../b` cannot be normalized because it escapes the base directory.
pub fn normalize_path(path: &Path) -> Result<PathBuf, std::io::Error> {
    let mut components = path.components().peekable();
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
                if !ret.pop() {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "cannot normalize a relative path beyond the base directory",
                    ));
                }
            }
            Component::Normal(c) => {
                ret.push(c);
            }
        }
    }
    Ok(ret)
}

/// Convert a path to an absolute path, relative to the current working directory.
///
/// Unlike [`std::fs::canonicalize`], this function does not resolve symlinks and does not require
/// the path to exist.
pub fn absolutize_path(path: &Path) -> Result<Cow<Path>, std::io::Error> {
    use path_absolutize::Absolutize;

    path.absolutize_from(CWD.simplified())
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

/// Compute a path describing `path` relative to `base`.
///
/// `lib/python/site-packages/foo/__init__.py` and `lib/python/site-packages` -> `foo/__init__.py`
/// `lib/marker.txt` and `lib/python/site-packages` -> `../../marker.txt`
/// `bin/foo_launcher` and `lib/python/site-packages` -> `../../../bin/foo_launcher`
pub fn relative_to(path: impl AsRef<Path>, base: impl AsRef<Path>) -> Result<PathBuf, io::Error> {
    // Find the longest common prefix, and also return the path stripped from that prefix
    let (stripped, common_prefix) = base
        .as_ref()
        .ancestors()
        .find_map(|ancestor| {
            // Simplifying removes the UNC path prefix on windows.
            dunce::simplified(path.as_ref())
                .strip_prefix(dunce::simplified(ancestor))
                .ok()
                .map(|stripped| (stripped, ancestor))
        })
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::Other,
                format!(
                    "Trivial strip failed: {} vs. {}",
                    path.as_ref().simplified_display(),
                    base.as_ref().simplified_display()
                ),
            )
        })?;

    // go as many levels up as required
    let levels_up = base.as_ref().components().count() - common_prefix.components().count();
    let up = iter::repeat("..").take(levels_up).collect::<PathBuf>();

    Ok(up.join(stripped))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_url() {
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

    #[test]
    fn test_normalize_path() {
        let path = Path::new("/a/b/../c/./d");
        let normalized = normalize_path(path).unwrap();
        assert_eq!(normalized, Path::new("/a/c/d"));

        let path = Path::new("/a/../c/./d");
        let normalized = normalize_path(path).unwrap();
        assert_eq!(normalized, Path::new("/c/d"));

        // This should be an error.
        let path = Path::new("/a/../../c/./d");
        let err = normalize_path(path).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
    }

    #[test]
    fn test_relative_to() {
        assert_eq!(
            relative_to(
                Path::new("/home/ferris/carcinization/lib/python/site-packages/foo/__init__.py"),
                Path::new("/home/ferris/carcinization/lib/python/site-packages"),
            )
            .unwrap(),
            Path::new("foo/__init__.py")
        );
        assert_eq!(
            relative_to(
                Path::new("/home/ferris/carcinization/lib/marker.txt"),
                Path::new("/home/ferris/carcinization/lib/python/site-packages"),
            )
            .unwrap(),
            Path::new("../../marker.txt")
        );
        assert_eq!(
            relative_to(
                Path::new("/home/ferris/carcinization/bin/foo_launcher"),
                Path::new("/home/ferris/carcinization/lib/python/site-packages"),
            )
            .unwrap(),
            Path::new("../../../bin/foo_launcher")
        );
    }
}
