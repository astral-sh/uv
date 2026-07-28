// Helper to read a .python-version file from a directory tree.
use std::fs;
use std::path::{Path, PathBuf};

pub fn read_python_version_file(start: &Path) -> Option<String> {
    let mut dir: Option<PathBuf> = Some(start.to_path_buf());
    while let Some(d) = dir {
        let candidate = d.join(".python-version");
        if let Ok(s) = fs::read_to_string(&candidate) {
            let line = s.lines().find(|l| !l.trim().is_empty())?.trim().to_string();
            return Some(line);
        }
        dir = d.parent().map(|p| p.to_path_buf());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn finds_version() {
        let td = tempfile::tempdir().unwrap();
        let f = td.path().join(".python-version");
        let mut fh = std::fs::File::create(&f).unwrap();
        writeln!(fh, "3.11.9\n").unwrap();
        let got = read_python_version_file(td.path());
        assert_eq!(got.as_deref(), Some("3.11.9"));
    }
}
