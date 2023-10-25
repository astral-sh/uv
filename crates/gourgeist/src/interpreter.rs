use camino::Utf8PathBuf;
use tracing::debug;

/// Parse the value of the `-p`/`--python` option, which can be e.g. `3.11`, `python3.11`,
/// `tools/bin/python3.11` or `/usr/bin/python3.11`.
pub fn parse_python_cli(cli_python: Option<Utf8PathBuf>) -> Result<Utf8PathBuf, crate::Error> {
    let python = if let Some(python) = cli_python {
        if let Some((major, minor)) = python
            .as_str()
            .split_once('.')
            .and_then(|(major, minor)| Some((major.parse::<u8>().ok()?, minor.parse::<u8>().ok()?)))
        {
            if major != 3 {
                return Err(crate::Error::InvalidPythonInterpreter(
                    "Only python 3 is supported".into(),
                ));
            }
            debug!("Looking for python {major}.{minor}");
            Utf8PathBuf::from(format!("python{major}.{minor}"))
        } else {
            python
        }
    } else {
        Utf8PathBuf::from("python3".to_string())
    };

    // Call `which` to find it in path, if not given a path
    let python = if python.components().count() > 1 {
        // Does this path contain a slash (unix) or backslash (windows)? In that case, assume it's
        // relative or absolute path that we don't need to resolve
        debug!("Assuming {python} is a path");
        python
    } else {
        let python_in_path = which::which(python.as_std_path())
            .map_err(|err| {
                crate::Error::InvalidPythonInterpreter(
                    format!("Can't find {python} ({err})").into(),
                )
            })?
            .try_into()
            .map_err(camino::FromPathBufError::into_io_error)?;
        debug!("Resolved {python} to {python_in_path}");
        python_in_path
    };
    Ok(python)
}
