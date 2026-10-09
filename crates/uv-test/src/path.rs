use std::io::ErrorKind;
use std::path::Path;

use uv_fs::Simplified;

/// Assert that no filesystem entry exists at `path`.
///
/// The final path component is not followed, so dangling symlinks and junctions count as present.
/// Only [`ErrorKind::NotFound`] is accepted; other metadata errors fail the assertion.
#[track_caller]
pub fn assert_path_missing(path: impl AsRef<Path>) {
    let path = path.as_ref();
    match fs_err::symlink_metadata(path) {
        Ok(metadata) => panic!(
            "Expected `{}` to be missing, found {:?}",
            path.user_display(),
            metadata.file_type()
        ),
        Err(error) => assert_eq!(
            error.kind(),
            ErrorKind::NotFound,
            "Could not read metadata for `{}`: {error}",
            path.user_display()
        ),
    }
}

#[cfg(test)]
mod tests {
    use assert_fs::prelude::*;

    use super::assert_path_missing;

    #[test]
    fn accepts_missing_path() -> anyhow::Result<()> {
        let directory = assert_fs::TempDir::new()?;

        assert_path_missing(directory.child("missing"));

        Ok(())
    }

    #[test]
    #[should_panic(expected = "to be missing, found")]
    fn rejects_file() {
        let directory = assert_fs::TempDir::new().expect("create temporary directory");
        let file = directory.child("file");
        file.write_str("contents").expect("create file");

        assert_path_missing(file);
    }

    #[test]
    #[should_panic(expected = "to be missing, found")]
    fn rejects_directory() {
        let directory = assert_fs::TempDir::new().expect("create temporary directory");

        assert_path_missing(directory.path());
    }

    #[test]
    #[should_panic(expected = "to be missing, found")]
    fn rejects_directory_link() {
        let directory = assert_fs::TempDir::new().expect("create temporary directory");
        let target = directory.child("target");
        target.create_dir_all().expect("create target directory");
        let link = directory.child("link");
        uv_fs::create_symlink(&target, &link).expect("create directory link");

        assert_path_missing(link);
    }

    #[test]
    #[should_panic(expected = "to be missing, found")]
    fn rejects_dangling_directory_link() {
        let directory = assert_fs::TempDir::new().expect("create temporary directory");
        let target = directory.child("target");
        target.create_dir_all().expect("create target directory");
        let link = directory.child("link");
        uv_fs::create_symlink(&target, &link).expect("create directory link");
        fs_err::remove_dir(&target).expect("remove link target");

        assert_path_missing(link);
    }

    #[test]
    #[should_panic(expected = "Could not read metadata")]
    fn rejects_other_metadata_errors() {
        assert_path_missing("invalid\0path");
    }
}
