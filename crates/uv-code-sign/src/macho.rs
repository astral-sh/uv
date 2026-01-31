use std::io::Read;
use std::path::Path;

use fs_err as fs;
use goblin::mach::fat::{FAT_CIGAM, FAT_MAGIC};
use goblin::mach::header::{MH_CIGAM, MH_CIGAM_64, MH_MAGIC, MH_MAGIC_64};
use goblin::mach::peek;

/// Returns `true` if the file at `path` starts with a Mach-O magic number.
///
/// Only reads the first 4 bytes of the file rather than loading it entirely.
pub fn is_macho(path: &Path) -> bool {
    let Ok(mut file) = fs::File::open(path).inspect_err(|err| {
        tracing::debug!("failed to open `{}`: {err}", path.display());
    }) else {
        return false;
    };
    let mut buf = [0u8; 4];
    if let Err(err) = file.read_exact(&mut buf) {
        tracing::debug!("failed to read `{}`: {err}", path.display());
        return false;
    }
    let Ok(magic) = peek(&buf, 0) else {
        return false;
    };
    matches!(
        magic,
        MH_MAGIC | MH_CIGAM | MH_MAGIC_64 | MH_CIGAM_64 | FAT_MAGIC | FAT_CIGAM
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_macho_plain_text() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("hello.py");
        fs::write(&path, "print('hello')").unwrap();
        assert!(!is_macho(&path));
    }

    #[test]
    fn test_is_macho_empty_file() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("empty");
        fs::write(&path, "").unwrap();
        assert!(!is_macho(&path));
    }

    #[test]
    fn test_is_macho_nonexistent() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("nope");
        assert!(!is_macho(&path));
    }

    #[test]
    fn test_is_macho_random_bytes() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("random");
        fs::write(&path, [0x00, 0x01, 0x02, 0x03, 0x04]).unwrap();
        assert!(!is_macho(&path));
    }

    #[test]
    fn test_is_macho_synthetic_magic() {
        let tmp = tempfile::tempdir().unwrap();

        // MH_MAGIC_64
        let path = tmp.path().join("macho64");
        fs::write(&path, MH_MAGIC_64.to_be_bytes()).unwrap();
        assert!(is_macho(&path));

        // FAT_MAGIC (universal)
        let path = tmp.path().join("fat");
        fs::write(&path, FAT_MAGIC.to_be_bytes()).unwrap();
        assert!(is_macho(&path));
    }

    #[test]
    fn test_is_macho_too_short() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("short");
        fs::write(&path, [0xFE, 0xED]).unwrap();
        assert!(!is_macho(&path));
    }

    /// Verify that a real Mach-O binary on the system is detected.
    #[cfg(target_os = "macos")]
    #[test]
    fn test_is_macho_real_binary() {
        use std::path::PathBuf;
        // /usr/bin/true is a Mach-O binary on macOS.
        let path = PathBuf::from("/usr/bin/true");
        if path.exists() {
            assert!(is_macho(&path), "/usr/bin/true should be Mach-O");
        }
    }
}
