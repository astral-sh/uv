use std::borrow::Cow;
use std::ffi::OsString;
use std::path::{Component, Path, PathBuf, Prefix};
use std::sync::LazyLock;

use either::Either;
use path_slash::PathExt;

/// The current working directory.
#[expect(clippy::print_stderr)]
pub static CWD: LazyLock<PathBuf> = LazyLock::new(|| {
    std::env::current_dir().unwrap_or_else(|_e| {
        eprintln!("Current directory does not exist");
        std::process::exit(1);
    })
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

    /// Canonicalize a path, stripping the `\\?\` prefix on Windows when possible.
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

        // If current working directory is root, display the path as-is.
        if CWD.ancestors().nth(1).is_none() {
            return path.display();
        }

        // Attempt to strip the current working directory, then the canonicalized current working
        // directory, in case they differ.
        let path = path.strip_prefix(CWD.simplified()).unwrap_or(path);

        if path.as_os_str() == "" {
            // Avoid printing an empty string for the current directory
            return Path::new(".").display();
        }

        path.display()
    }

    fn user_display_from(&self, base: impl AsRef<Path>) -> impl std::fmt::Display {
        let path = dunce::simplified(self.as_ref());

        // If current working directory is root, display the path as-is.
        if CWD.ancestors().nth(1).is_none() {
            return path.display();
        }

        // Attempt to strip the base, then the current working directory, then the canonicalized
        // current working directory, in case they differ.
        let path = path
            .strip_prefix(base.as_ref())
            .unwrap_or_else(|_| path.strip_prefix(CWD.simplified()).unwrap_or(path));

        if path.as_os_str() == "" {
            // Avoid printing an empty string for the current directory
            return Path::new(".").display();
        }

        path.display()
    }

    fn portable_display(&self) -> impl std::fmt::Display {
        let path = dunce::simplified(self.as_ref());

        // Attempt to strip the current working directory, then the canonicalized current working
        // directory, in case they differ.
        let path = path.strip_prefix(CWD.simplified()).unwrap_or(path);

        // Use a portable representation for relative paths.
        path.to_slash()
            .map(Either::Left)
            .unwrap_or_else(|| Either::Right(path.display()))
    }
}

pub trait PythonExt {
    /// Render a [`Path`] as a Python expression that evaluates to a [`str`].
    ///
    /// On Unix, paths are arbitrary byte strings, so this uses [`os.fsdecode()`] to produce a
    /// surrogate-escaped `str`. On Windows, paths are encoded as UTF-16, so this returns a `str`
    /// literal that preserves the original UTF-16 code units.
    ///
    /// [`str`]: https://docs.python.org/3/library/stdtypes.html#text-sequence-type-str
    /// [`os.fsdecode()`]: https://docs.python.org/3/library/os.html#os.fsdecode
    fn escape_for_python(&self) -> String;
}

impl<T: AsRef<Path>> PythonExt for T {
    fn escape_for_python(&self) -> String {
        escape_path_for_python(self)
    }
}

/// Serialize a path as a Python expression that evaluates to a `str`.
///
/// Note: Due to the fun quirks *nix paths, how python handles them, and expectations of things that
/// might consume those paths, this produces an expression wrapped in `__import__("os").fsdecode()`.
#[cfg(unix)]
fn escape_path_for_python<P: AsRef<Path>>(path: P) -> String {
    use std::os::unix::ffi::OsStrExt;
    format!(
        r#"__import__("os").fsdecode(b"{}")"#,
        path.as_ref().as_os_str().as_bytes().escape_ascii()
    )
}

/// Serialize a path as a Python expression that evaluates to a `str`.
#[cfg(windows)]
fn escape_path_for_python<P: AsRef<Path>>(path: P) -> String {
    use std::fmt::Write;
    use std::os::windows::ffi::OsStrExt;

    let mut literal = String::new();
    literal.push('"');
    for character in char::decode_utf16(path.as_ref().as_os_str().encode_wide()) {
        match character {
            Ok(character) if character.is_ascii() => {
                literal.extend((character as u8).escape_ascii().map(char::from));
            }
            // `is_control` also covers the non-ASCII C1 range (`U+0080..=U+009F`).
            Ok(character) if character.is_control() => {
                let _ = write!(literal, r"\u{:04x}", u32::from(character));
            }
            Ok(character) => literal.push(character),
            Err(error) => {
                let _ = write!(literal, r"\u{:04x}", error.unpaired_surrogate());
            }
        }
    }
    literal.push('"');
    literal
}

/// Normalize the `path` component of a URL for use as a file path.
///
/// For example, on Windows, transforms `C:\Users\ferris\wheel-0.42.0.tar.gz` to
/// `/C:/Users/ferris/wheel-0.42.0.tar.gz`.
///
/// On other platforms, this is a no-op.
pub fn normalize_url_path(path: &str) -> Cow<'_, str> {
    // Apply percent-decoding to the URL.
    let path = percent_encoding::percent_decode_str(path)
        .decode_utf8()
        .unwrap_or(Cow::Borrowed(path));

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
pub fn normalize_absolute_path(path: &Path) -> Result<PathBuf, std::io::Error> {
    let mut components = path.components().peekable();
    let mut ret = components
        .next_if_map_mut(|component| match component {
            Component::Prefix(..) => Some(PathBuf::from(component.as_os_str())),
            _ => None,
        })
        .unwrap_or_default();

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
                        format!(
                            "cannot normalize a relative path beyond the base directory: {}",
                            path.display()
                        ),
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

/// Returns `false` if [`Path::components`] discarded any bytes from `path`, without allocating.
///
/// [`Path::components`] silently strips interior `.` segments, repeated separators, and
/// trailing separators. If the `path` length differs from the computed byte length from
/// `path.components().collect()`, the path isn't normalized (or there is a special case we handle
/// here, in which case we perform a redundant normalization pass later).
fn path_equals_components(path: &Path) -> bool {
    // We count the length in bytes; the encoding scheme doesn't matter as we count bytes in
    // both expected and the input path
    let mut expected_len = 0;
    let mut next_needs_separator = false;
    for component in path.components() {
        let bytes = component.as_os_str().as_encoded_bytes();
        // `PathBuf::push` inserts a separator between components unless the previous one
        // already ends in one, or the new component is itself the root (which embeds it).
        if next_needs_separator && !matches!(component, Component::RootDir) {
            // Assumption: forward and backwards slashes encode with the same length.
            expected_len += Path::new("/").as_os_str().as_encoded_bytes().len();
        }
        expected_len += bytes.len();
        next_needs_separator = match component {
            // The root dir is the slash.
            Component::RootDir => false,
            // Prefix has `RootDir` after it if it requires a slash.
            Component::Prefix(_) => false,
            _ => true,
        };
    }
    expected_len == path.as_os_str().as_encoded_bytes().len()
}

/// Normalize a [`Cow`] path, removing `.`, `..`, repeated separators (`//`), and trailing slashes.
///
/// Paths that point to the current directory (`.` or `.\.`) are normalized to the empty path.
///
/// When the path is already normalized, returns it as-is without allocating.
pub fn normalize_path<'path>(path: impl Into<Cow<'path, Path>>) -> Cow<'path, Path> {
    let path = path.into();
    // A path with leading `.` or `..` is not normalized.
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir | Component::CurDir))
    {
        return Cow::Owned(normalized(&path));
    }

    // A path with non-leading `.`, repeated separators (`//`) or trailing slashes is not
    // normalized.
    if !path_equals_components(&path) {
        return Cow::Owned(normalized(&path));
    }

    // Fast path: already normalized, return as-is.
    path
}

/// Normalize `path`, returning it when it remains strictly under a non-empty `root`.
///
/// This comparison is lexical and does not resolve symlinks. The returned path is normalized, and
/// equality with `root` is rejected.
pub fn normalize_path_under(path: impl AsRef<Path>, root: impl AsRef<Path>) -> Option<PathBuf> {
    let path = normalize_path(path.as_ref()).into_owned();
    let root = normalize_path(root.as_ref());

    if root.as_os_str().is_empty() || path.as_path() == root.as_ref() {
        None
    } else {
        path.starts_with(root.as_ref()).then_some(path)
    }
}

/// Normalize a [`Path`].
///
/// Unlike [`normalize_absolute_path`], this works with relative paths and does never error.
///
/// Note that we can theoretically go beyond the root dir here (e.g. `/usr/../../foo` becomes
/// `/../foo`), but that's not a (correctness) problem, we will fail later with a file not found
/// error with a path computed from the user's input.
///
/// # Examples
///
/// In: `../../workspace-git-path-dep-test/packages/c/../../packages/d`
/// Out: `../../workspace-git-path-dep-test/packages/d`
///
/// In: `workspace-git-path-dep-test/packages/c/../../packages/d`
/// Out: `workspace-git-path-dep-test/packages/d`
///
/// In: `./a/../../b`
fn normalized(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                // Preserve filesystem roots and regular path components.
                normalized.push(component);
            }
            Component::ParentDir => {
                match normalized.components().next_back() {
                    None | Some(Component::ParentDir | Component::RootDir) => {
                        // Preserve leading and above-root `..`
                        normalized.push(component);
                    }
                    Some(Component::Normal(_) | Component::Prefix(_) | Component::CurDir) => {
                        // Remove inner `..`
                        normalized.pop();
                    }
                }
            }
            Component::CurDir => {
                // Remove `.`
            }
        }
    }
    normalized
}

/// Compute a path describing `path` relative to `base`.
///
/// `lib/python/site-packages/foo/__init__.py` and `lib/python/site-packages` -> `foo/__init__.py`
/// `lib/marker.txt` and `lib/python/site-packages` -> `../../marker.txt`
/// `bin/foo_launcher` and `lib/python/site-packages` -> `../../../bin/foo_launcher`
///
/// Returns `Err` if there is no relative path between `path` and `base` (for example, if the paths
/// are on different drives on Windows).
pub fn relative_to(
    path: impl AsRef<Path>,
    base: impl AsRef<Path>,
) -> Result<PathBuf, std::io::Error> {
    // Normalize both paths, to avoid intermediate `..` components.
    let path = normalize_path(path.as_ref());
    let base = normalize_path(base.as_ref());

    // Find the longest common prefix, and also return the path stripped from that prefix
    let (stripped, common_prefix) = base
        .ancestors()
        .find_map(|ancestor| {
            // Simplifying removes the UNC path prefix on windows.
            dunce::simplified(&path)
                .strip_prefix(dunce::simplified(ancestor))
                .ok()
                .map(|stripped| (stripped, ancestor))
        })
        .ok_or_else(|| {
            std::io::Error::other(format!(
                "Trivial strip failed: {} vs. {}",
                path.simplified_display(),
                base.simplified_display()
            ))
        })?;

    // go as many levels up as required
    let levels_up = base.components().count() - common_prefix.components().count();
    let up = std::iter::repeat_n("..", levels_up).collect::<PathBuf>();

    Ok(up.join(stripped))
}

/// Find the root of the nearest Git repository containing `path`.
///
/// A `.git` directory or file is treated as a repository marker to support both regular
/// repositories and linked worktrees.
pub fn find_git_repository_root(path: &Path) -> Option<&Path> {
    // TODO: Consider supporting GIT_CEILING_DIRECTORIES here.
    path.ancestors()
        .find(|ancestor| ancestor.join(".git").exists())
}

/// Try to compute a path relative to `base` if `should_relativize` is true, otherwise return
/// the absolute path. Falls back to absolute if relativization fails.
pub fn try_relative_to_if(
    path: impl AsRef<Path>,
    base: impl AsRef<Path>,
    should_relativize: bool,
) -> Result<PathBuf, std::io::Error> {
    if should_relativize {
        relative_to(&path, &base).or_else(|_| std::path::absolute(path.as_ref()))
    } else {
        std::path::absolute(path.as_ref())
    }
}

/// Convert a [`Path`] to a Windows `verbatim` path (prefixed with `\\?\`) when possible to bypass
/// Win32 path normalization such as [`MAX_PATH`] and removed trailing characters (dot, space).
/// Other characters as defined by [`Path.GetInvalidFileNameChars`] are still prohibited. This
/// function will attempt to perform path normalization similar to Win32 default normalization
/// without triggering the existing Win32 limitations.
///
/// Only [`Prefix::UNC`] and [`Prefix::Disk`] conversion compatible components are supported.
///   * [`Prefix::UNC`] `\\server\share` becomes `\\?\UNC\server\share`
///   * [`Prefix::Disk`] `DriveLetter:` becomes `\\?\DriveLetter:`
///
/// Other representations do not yield a `verbatim` path. The following cases are returned as-is:
///   * Non-Windows systems.
///   * Device paths such as those starting with `\\.\`.
///   * Paths already prefixed with `\\?\` or `\\?\UNC\`.
///
/// WARNING: Adding the `\\?\` prefix effectively skips Win32 default path normalization. Even
/// though it allows operations on paths that are normally unavailable, it can also be used to
/// create entries that can potentially lead to further issues with operations that expect
/// normalization such as symbolic links, junctions or reparse points.
///
/// [`MAX_PATH`]: https://learn.microsoft.com/en-us/windows/win32/fileio/maximum-file-path-limitation
/// [`Path.GetInvalidFileNameChars`]: https://learn.microsoft.com/en-us/dotnet/api/system.io.path.getinvalidfilenamechars
///
/// See:
///   * <https://learn.microsoft.com/en-us/windows/win32/fileio/naming-a-file>
///   * <https://learn.microsoft.com/en-us/dotnet/standard/io/file-path-formats>
pub fn verbatim_path(path: &Path) -> Cow<'_, Path> {
    if !cfg!(windows) {
        return Cow::Borrowed(path);
    }

    // Attempt to resolve a fully qualified path just like Win32 path normalization would.
    // std::path::absolute calls GetFullPathNameW which defeats the purpose of this function
    // as it results in Win32 default path normalization.
    let resolved_path = if path.is_relative() {
        Cow::Owned(CWD.join(path))
    } else {
        Cow::Borrowed(path)
    };

    // Fast Path: we only support verbatim conversion for Prefix::UNC and Prefix::Disk
    if let Some(Component::Prefix(prefix)) = resolved_path.components().next() {
        match prefix.kind() {
            Prefix::UNC(..) | Prefix::Disk(_) => {},
            // return as-is as there's no verbatim equivalent for `\\.\device`
            Prefix::DeviceNS(_)
            // return as-is as its already verbatim
            | Prefix::Verbatim(_)
            | Prefix::VerbatimDisk(_)
            | Prefix::VerbatimUNC(..) => return Cow::Borrowed(path)
        }
    }

    // Resolve relative directory components while avoiding default Win32 path normalization
    let normalized_path = normalized(&resolved_path);

    let mut components = normalized_path.components();
    let Some(Component::Prefix(prefix)) = components.next() else {
        return Cow::Borrowed(path);
    };

    match prefix.kind() {
        // `DriveLetter:` -> `\\?\DriveLetter:`
        Prefix::Disk(_) => {
            let mut result = OsString::from(r"\\?\");
            result.push(normalized_path.as_os_str()); // e.g. "C:"
            Cow::Owned(PathBuf::from(result))
        }
        // `\\server\share` -> `\\?\UNC\server\share`
        Prefix::UNC(server, share) => {
            let mut result = OsString::from(r"\\?\UNC\");
            result.push(server);
            result.push(r"\");
            result.push(share);
            for component in components {
                match component {
                    Component::RootDir => {} // being cautious
                    Component::Prefix(_) => {
                        debug_assert!(false, "prefix already consumed");
                    }
                    Component::CurDir | Component::ParentDir => {
                        debug_assert!(false, "path already normalized");
                    }
                    Component::Normal(_) => {
                        result.push(r"\");
                        result.push(component.as_os_str());
                    }
                }
            }
            Cow::Owned(PathBuf::from(result))
        }
        Prefix::DeviceNS(_)
        | Prefix::Verbatim(_)
        | Prefix::VerbatimDisk(_)
        | Prefix::VerbatimUNC(..) => {
            debug_assert!(false, "skipped via fast path");
            Cow::Borrowed(path)
        }
    }
}

/// A path that can be serialized and deserialized in a portable way by converting Windows-style
/// backslashes to forward slashes, and using a `.` for an empty path.
///
/// This implementation assumes that the path is valid UTF-8; otherwise, it won't roundtrip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortablePath<'a>(&'a Path);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortablePathBuf(Box<Path>);

#[cfg(feature = "schemars")]
impl schemars::JsonSchema for PortablePathBuf {
    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("PortablePathBuf")
    }

    fn json_schema(_gen: &mut schemars::generate::SchemaGenerator) -> schemars::Schema {
        PathBuf::json_schema(_gen)
    }
}

impl AsRef<Path> for PortablePath<'_> {
    fn as_ref(&self) -> &Path {
        self.0
    }
}

impl<'a, T> From<&'a T> for PortablePath<'a>
where
    T: AsRef<Path> + ?Sized,
{
    fn from(path: &'a T) -> Self {
        PortablePath(path.as_ref())
    }
}

impl std::fmt::Display for PortablePath<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let path = self.0.to_slash_lossy();
        if path.is_empty() {
            write!(f, ".")
        } else {
            write!(f, "{path}")
        }
    }
}

impl std::fmt::Display for PortablePathBuf {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let path = self.0.to_slash_lossy();
        if path.is_empty() {
            write!(f, ".")
        } else {
            write!(f, "{path}")
        }
    }
}

impl From<&str> for PortablePathBuf {
    fn from(path: &str) -> Self {
        if path == "." {
            Self(PathBuf::new().into_boxed_path())
        } else {
            Self(PathBuf::from(path).into_boxed_path())
        }
    }
}

impl From<PortablePathBuf> for Box<Path> {
    fn from(portable: PortablePathBuf) -> Self {
        portable.0
    }
}

impl From<Box<Path>> for PortablePathBuf {
    fn from(path: Box<Path>) -> Self {
        Self(path)
    }
}

impl<'a> From<&'a Path> for PortablePathBuf {
    fn from(path: &'a Path) -> Self {
        Box::<Path>::from(path).into()
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for PortablePathBuf {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        self.to_string().serialize(serializer)
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for PortablePath<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        self.to_string().serialize(serializer)
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::de::Deserialize<'de> for PortablePathBuf {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        let s = <Cow<'_, str>>::deserialize(deserializer)?;
        if s == "." {
            Ok(Self(PathBuf::new().into_boxed_path()))
        } else {
            Ok(Self(PathBuf::from(s.as_ref()).into_boxed_path()))
        }
    }
}

impl AsRef<Path> for PortablePathBuf {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_git_repository_root() -> std::io::Result<()> {
        let temp_dir = tempfile::tempdir()?;

        let repository = temp_dir.path().join("repository");
        let nested = repository.join("packages/project");
        fs_err::create_dir_all(repository.join(".git"))?;
        fs_err::create_dir_all(&nested)?;
        assert_eq!(
            find_git_repository_root(&nested),
            Some(repository.as_path())
        );

        let worktree = temp_dir.path().join("worktree");
        let nested = worktree.join("packages/project");
        fs_err::create_dir_all(&nested)?;
        fs_err::write(worktree.join(".git"), "gitdir: ../repository/.git")?;
        assert_eq!(find_git_repository_root(&nested), Some(worktree.as_path()));

        Ok(())
    }

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
        let normalized = normalize_absolute_path(path).unwrap();
        assert_eq!(normalized, Path::new("/a/c/d"));

        let path = Path::new("/a/../c/./d");
        let normalized = normalize_absolute_path(path).unwrap();
        assert_eq!(normalized, Path::new("/c/d"));

        // This should be an error.
        let path = Path::new("/a/../../c/./d");
        let err = normalize_absolute_path(path).unwrap_err();
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

    #[test]
    fn test_normalize_relative() {
        let cases = [
            (
                "../../workspace-git-path-dep-test/packages/c/../../packages/d",
                "../../workspace-git-path-dep-test/packages/d",
            ),
            (
                "workspace-git-path-dep-test/packages/c/../../packages/d",
                "workspace-git-path-dep-test/packages/d",
            ),
            ("./a/../../b", "../b"),
            ("/usr/../../foo", "/../foo"),
            // Interior `.` segments (stripped by `Path::components`).
            ("foo/./bar", "foo/bar"),
            ("/a/./b/./c", "/a/b/c"),
            ("./foo/bar", "foo/bar"),
            (".", ""),
            ("./.", ""),
            ("foo/.", "foo"),
            // Repeated separators (also stripped by `Path::components`).
            ("foo//bar", "foo/bar"),
            ("/a///b//c", "/a/b/c"),
            // Mixed `.` and `..`.
            ("foo/./../bar", "bar"),
            ("foo/bar/./../baz", "foo/baz"),
            // Already-normalized paths.
            ("foo/bar", "foo/bar"),
            ("/a/b/c", "/a/b/c"),
            ("", ""),
        ];
        for (input, expected) in cases {
            assert_eq!(
                normalize_path(Path::new(input)),
                Path::new(expected),
                "input: {input:?}"
            );
        }

        // Verify the fast path: already-normalized inputs are returned borrowed.
        for already_normalized in ["foo/bar", "/a/b/c", "foo", "/", ""] {
            let path = Path::new(already_normalized);
            assert!(
                matches!(normalize_path(path), Cow::Borrowed(_)),
                "expected borrowed for {already_normalized:?}"
            );
        }
    }

    #[test]
    fn test_normalize_path_under() {
        assert_eq!(
            normalize_path_under("scripts/script", "scripts"),
            Some(PathBuf::from("scripts/script"))
        );
        assert_eq!(
            normalize_path_under("/scripts/script", "/scripts"),
            Some(PathBuf::from("/scripts/script"))
        );
        assert_eq!(
            normalize_path_under("scripts/nested/../script", "scripts"),
            Some(PathBuf::from("scripts/script"))
        );
        assert_eq!(
            normalize_path_under("/scripts/nested/../script", "/scripts"),
            Some(PathBuf::from("/scripts/script"))
        );
        assert_eq!(normalize_path_under("scripts/.", "scripts"), None);
        assert_eq!(normalize_path_under("/scripts/.", "/scripts"), None);
        assert_eq!(normalize_path_under("scripts/../script", "scripts"), None);
        assert_eq!(normalize_path_under("/scripts/../script", "/scripts"), None);
        assert_eq!(normalize_path_under("scripts/script", "."), None);
        assert_eq!(normalize_path_under("scripts/script", ""), None);
    }

    #[test]
    fn test_normalize_trailing_path_separator() {
        let cases = [
            (
                "/home/ferris/projects/python/",
                "/home/ferris/projects/python",
            ),
            ("python/", "python"),
            ("/", "/"),
            ("foo/bar/", "foo/bar"),
            ("foo//", "foo"),
        ];
        for (input, expected) in cases {
            assert_eq!(normalize_path(Path::new(input)), Path::new(expected));
        }
    }

    #[test]
    #[cfg(windows)]
    fn test_normalize_windows() {
        let cases = [
            (
                r"C:\Users\Ferris\projects\python\",
                r"C:\Users\Ferris\projects\python",
            ),
            (r"C:\foo\.\bar", r"C:\foo\bar"),
            (r"C:\foo\\bar", r"C:\foo\bar"),
            (r"C:\foo\bar\..\baz", r"C:\foo\baz"),
            (r"foo\.\bar", r"foo\bar"),
            (r"C:foo", r"C:foo"),
            (r"C:\foo", r"C:\foo"),
            (r"C:\\foo", r"C:\foo"),
            (r"\\?\C:foo", r"\\?\C:foo"),
            (r"\\?\C:\foo", r"\\?\C:\foo"),
            (r"\\?\C:\\foo", r"\\?\C:\foo"),
            (r"\\server\share\foo", r"\\server\share\foo"),
        ];
        for (input, expected) in cases {
            assert_eq!(normalize_path(Path::new(input)), Path::new(expected));
        }
    }

    #[cfg(windows)]
    #[test]
    fn test_verbatim_path() {
        let relative_path = format!(r"\\?\{}\path\to\logging.", CWD.simplified_display());
        let relative_root = format!(
            r"\\?\{}\path\to\logging.",
            CWD.components()
                .next()
                .expect("expected a drive letter prefix")
                .simplified_display()
        );
        let cases = [
            // Non-Verbatim disk
            (r"C:\path\to\logging.", r"\\?\C:\path\to\logging."),
            (r"C:\path\to\.\logging.", r"\\?\C:\path\to\logging."),
            (r"C:\path\to\..\to\logging.", r"\\?\C:\path\to\logging."),
            (r"C:/path/to/../to/./logging.", r"\\?\C:\path\to\logging."),
            (r"C:path\to\..\to\logging.", r"\\?\C:path\to\logging."), // @TODO(samypr100) we do not support expanding drive-relative paths
            (r".\path\to\.\logging.", relative_path.as_str()),
            (r"path\to\..\to\logging.", relative_path.as_str()),
            (r"./path/to/logging.", relative_path.as_str()),
            (r"\path\to\logging.", relative_root.as_str()),
            // Non-Verbatim UNC
            (
                r"\\127.0.0.1\c$\path\to\logging.",
                r"\\?\UNC\127.0.0.1\c$\path\to\logging.",
            ),
            (
                r"\\127.0.0.1\c$\path\to\.\logging.",
                r"\\?\UNC\127.0.0.1\c$\path\to\logging.",
            ),
            (
                r"\\127.0.0.1\c$\path\to\..\to\logging.",
                r"\\?\UNC\127.0.0.1\c$\path\to\logging.",
            ),
            (
                r"//127.0.0.1/c$/path/to/../to/./logging.",
                r"\\?\UNC\127.0.0.1\c$\path\to\logging.",
            ),
            // Verbatim Disk
            (r"\\?\C:\path\to\logging.", r"\\?\C:\path\to\logging."),
            // Verbatim UNC
            (
                r"\\?\UNC\127.0.0.1\c$\path\to\logging.",
                r"\\?\UNC\127.0.0.1\c$\path\to\logging.",
            ),
            // Device Namespace
            (r"\\.\PhysicalDrive0", r"\\.\PhysicalDrive0"),
            (r"\\.\NUL", r"\\.\NUL"),
        ];

        for (input, expected) in cases {
            assert_eq!(verbatim_path(Path::new(input)), Path::new(expected));
        }
    }

    #[test]
    #[cfg(unix)]
    fn test_path_escape_for_python() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;

        // A *nix path can contain any byte except NUL.
        // Each expected value is intentionally Python pastable for verification.
        for (range, expected) in [
            (
                1..=0x5e,
                r##"__import__("os").fsdecode(b"/foo/\x01\x02\x03\x04\x05\x06\x07\x08\t\n\x0b\x0c\r\x0e\x0f\x10\x11\x12\x13\x14\x15\x16\x17\x18\x19\x1a\x1b\x1c\x1d\x1e\x1f !\"#$%&\'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^/bar")"##,
            ),
            (
                0x5f..=0xa3,
                r#"__import__("os").fsdecode(b"/foo/_`abcdefghijklmnopqrstuvwxyz{|}~\x7f\x80\x81\x82\x83\x84\x85\x86\x87\x88\x89\x8a\x8b\x8c\x8d\x8e\x8f\x90\x91\x92\x93\x94\x95\x96\x97\x98\x99\x9a\x9b\x9c\x9d\x9e\x9f\xa0\xa1\xa2\xa3/bar")"#,
            ),
            (
                0xa4..=0xd1,
                r#"__import__("os").fsdecode(b"/foo/\xa4\xa5\xa6\xa7\xa8\xa9\xaa\xab\xac\xad\xae\xaf\xb0\xb1\xb2\xb3\xb4\xb5\xb6\xb7\xb8\xb9\xba\xbb\xbc\xbd\xbe\xbf\xc0\xc1\xc2\xc3\xc4\xc5\xc6\xc7\xc8\xc9\xca\xcb\xcc\xcd\xce\xcf\xd0\xd1/bar")"#,
            ),
            (
                0xd2..=u8::MAX,
                r#"__import__("os").fsdecode(b"/foo/\xd2\xd3\xd4\xd5\xd6\xd7\xd8\xd9\xda\xdb\xdc\xdd\xde\xdf\xe0\xe1\xe2\xe3\xe4\xe5\xe6\xe7\xe8\xe9\xea\xeb\xec\xed\xee\xef\xf0\xf1\xf2\xf3\xf4\xf5\xf6\xf7\xf8\xf9\xfa\xfb\xfc\xfd\xfe\xff/bar")"#,
            ),
        ] {
            let bytes = b"/foo/"
                .iter()
                .copied()
                .chain(range)
                .chain(b"/bar".iter().copied())
                .collect::<Box<[_]>>();
            let path = Path::new(OsStr::from_bytes(&bytes));
            assert_eq!(path.escape_for_python(), expected);
        }
    }

    #[cfg(windows)]
    #[test]
    fn test_path_escape_for_python() {
        use std::os::windows::ffi::OsStringExt;

        // Exhaustive testing for windows is impractical, this has ASCII range (including NUL and
        // control chars), surrogate pairs, and unpaired surrogates.
        let path_osstr = OsString::from_wide(&[
            0x5c, 0x5c, 0x3f, 0x5c, 0x43, 0x3a, 0x5c, 0x63, 0x61, 0x66, 0x00e9, 0x5c, 0xd83d,
            0xde00, 0x5c, 0xd800, 0x78, 0xdc00, 0x22, 0x09, 0x0a, 0x0d,
        ]);
        let path = Path::new(&path_osstr);
        // This should be copy-pastable into a python interpreter and reproduce the path.
        assert_eq!(
            path.escape_for_python(),
            r#""\\\\?\\C:\\café\\😀\\\ud800x\udc00\"\t\n\r""#
        );
    }
}
