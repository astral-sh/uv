//! Find a user requested python version/interpreter.

use std::path::PathBuf;

use crate::Error;

/// Find a user requested python version/interpreter.
///
/// Supported formats:
/// * `-p 3.10` searches for an installed Python 3.10 (`py --list-paths` on windows, `python3.10` on linux/mac).
///   Specifying a patch version is not supported
/// * `-p python3.10` or `-p python.exe` looks for a binary in `PATH`
/// * `-p /home/ferris/.local/bin/python3.10` uses this exact Python
pub fn find_requested_python(request: &str) -> Result<PathBuf, Error> {
    let major_minor = request
        .split_once('.')
        .and_then(|(major, minor)| Some((major.parse::<u8>().ok()?, minor.parse::<u8>().ok()?)));
    if let Some((major, minor)) = major_minor {
        // `-p 3.10`
        if cfg!(unix) {
            let formatted = PathBuf::from(format!("python{major}.{minor}"));
            which::which_global(&formatted).map_err(|err| Error::Which(formatted, err))
        } else {
            unimplemented!("Only Unix is supported")
        }
    } else if !request.contains(std::path::MAIN_SEPARATOR) {
        // `-p python3.10`
        let request = PathBuf::from(request);
        which::which_global(&request).map_err(|err| Error::Which(request, err))
    } else {
        // `-p /home/ferris/.local/bin/python3.10`
        Ok(fs_err::canonicalize(request)?)
    }
}

#[cfg(test)]
mod tests {
    use std::fmt::Debug;

    use insta::{assert_display_snapshot, assert_snapshot};
    use itertools::Itertools;

    use crate::python_query::find_requested_python;
    use crate::Error;

    fn format_err<T: Debug>(err: Result<T, Error>) -> String {
        anyhow::Error::new(err.unwrap_err())
            .chain()
            .join("\n  Caused by: ")
    }

    #[cfg(unix)]
    #[test]
    fn python312() {
        assert_eq!(
            find_requested_python("3.12").unwrap(),
            find_requested_python("python3.12").unwrap()
        );
    }

    #[test]
    fn no_such_python_version() {
        assert_snapshot!(format_err(find_requested_python("3.1000")), @r###"
        Couldn't find `3.1000` in PATH
          Caused by: cannot find binary path
        "###);
    }

    #[test]
    fn no_such_python_binary() {
        assert_display_snapshot!(format_err(find_requested_python("python3.1000")), @r###"
        Couldn't find `python3.1000` in PATH
          Caused by: cannot find binary path
        "###);
    }

    #[test]
    fn no_such_python_path() {
        assert_display_snapshot!(
            format_err(find_requested_python("/does/not/exists/python3.12")), @r###"
        failed to canonicalize path `/does/not/exists/python3.12`
          Caused by: No such file or directory (os error 2)
        "###);
    }
}
