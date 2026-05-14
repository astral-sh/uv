/*!
The `uv-netrc` crate provides a parser for the netrc file.

# Setup

```text
$ cargo add uv-netrc
```

# Example

```ignore
use uv_netrc::Netrc;

// ...

let nrc = Netrc::new().unwrap();

// ...
println!(
    "login = {}\naccount = {}\npassword = {}",
    nrc.hosts["my.host"].login,
    nrc.hosts["my.host"].account,
    nrc.hosts["my.host"].password,
);
```

*/

pub use netrc::{Authenticator, Netrc};
use std::fs;
use std::io;
use std::io::ErrorKind;
#[cfg(windows)]
use std::iter::repeat;
use std::path::{Path, PathBuf};
use std::result;

mod lex;
mod netrc;

pub type Result<T> = result::Result<T, Error>;

/// An error that can occur when processing a Netrc file.
#[derive(thiserror::Error, Debug)]
pub enum Error {
    /// Wrap `std::io::Error` when we try to open the netrc file.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Parsing error.
    #[error("{parser} in the file '{filename}'")]
    Parsing {
        parser: netrc::ParsingError,
        filename: String,
    },
}

impl Netrc {
    /// Create a new `Netrc` object.
    ///
    /// Look up the `NETRC` environment variable if it is defined else that the
    /// default `~/.netrc` file.
    pub fn new() -> Result<Self> {
        Self::get_file()
            .ok_or(Error::Io(io::Error::new(
                ErrorKind::NotFound,
                "no netrc file found",
            )))
            .and_then(|f| Self::from_file(f.as_path()))
    }

    /// Create a new `Netrc` object from a file.
    #[expect(
        clippy::disallowed_methods,
        reason = "Preserve the upstream I/O error surface."
    )]
    pub fn from_file(file: &Path) -> Result<Self> {
        String::from_utf8_lossy(&fs::read(file)?)
            .parse()
            .map_err(|e| Error::Parsing {
                parser: e,
                filename: file.display().to_string(),
            })
    }

    /// Search a netrc file.
    ///
    /// Look up the `NETRC` environment variable if it is defined else use the .netrc (or _netrc
    /// file on windows) in the user's home directory.
    pub fn get_file() -> Option<PathBuf> {
        let env_var = std::env::var("NETRC")
            .map(PathBuf::from)
            .map(|f| shellexpand::path::tilde(&f).into_owned());

        #[cfg(windows)]
        let default = std::env::var("USERPROFILE")
            .into_iter()
            .flat_map(|home| repeat(home).zip([".netrc", "_netrc"]))
            .map(|(home, file)| PathBuf::from(home).join(file));

        #[cfg(not(windows))]
        let default = std::env::var("HOME").map(|home| PathBuf::from(home).join(".netrc"));

        env_var.into_iter().chain(default).find(|f| f.exists())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NETRC_COUNTER: AtomicUsize = AtomicUsize::new(0);

    const CONTENT: &str = "\
machine cocolog-nifty.com
login jmarten0
password cC2&yt7ZX

machine wired.com
login mstanlack1
password gH4={wx=>VixU

machine joomla.org
login mbutterley2
password hY5>yKqU&$vq&0
";

    fn create_netrc_file() -> PathBuf {
        let id = NETRC_COUNTER.fetch_add(1, Ordering::Relaxed);
        let dest = std::env::temp_dir().join(format!("mynetrc-{}-{id}", std::process::id()));
        fs_err::write(&dest, CONTENT).unwrap();
        dest
    }

    fn check_nrc(nrc: &Netrc) {
        assert_eq!(nrc.hosts.len(), 3);
        assert_eq!(
            nrc.hosts["cocolog-nifty.com"],
            Authenticator::new("jmarten0", "", "cC2&yt7ZX")
        );
        assert_eq!(
            nrc.hosts["wired.com"],
            Authenticator::new("mstanlack1", "", "gH4={wx=>VixU")
        );
        assert_eq!(
            nrc.hosts["joomla.org"],
            Authenticator::new("mbutterley2", "", "hY5>yKqU&$vq&0")
        );
    }

    #[test]
    fn test_new_env() {
        let fi = create_netrc_file();
        temp_env::with_var("NETRC", Some(fi.as_os_str()), || {
            let nrc = Netrc::new().unwrap();
            check_nrc(&nrc);
        });
    }

    #[test]
    fn test_from_file_failed() {
        let err = Netrc::from_file(Path::new("/netrc/file/not/exists/on/no/netrc")).unwrap_err();
        assert!(
            matches!(&err, Error::Io(err) if err.kind() == ErrorKind::NotFound),
            "expected NotFound I/O error, got {err}",
        );
    }

    #[test]
    fn test_from_file() {
        let fi = create_netrc_file();
        let nrc = Netrc::from_file(fi.as_path()).unwrap();
        check_nrc(&nrc);
    }
}
