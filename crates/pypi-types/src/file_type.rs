use std::path::Path;

#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum FileKind {
    Wheel,
    Zip,
    TarGz,
    TarBz2,
    TarXz,
    TarZstd,
}

impl FileKind {
    pub fn from_path(path: impl AsRef<Path>) -> Option<Self> {
        let extension = path.as_ref().extension()?.to_str()?;
        match extension {
            "whl" => Some(Self::Wheel),
            "zip" => Some(Self::Zip),
            "gz" if path.as_ref().file_stem().is_some_and(|stem| {
                Path::new(stem)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("tar"))
            }) =>
            {
                Some(Self::TarGz)
            }
            "bz2"
                if path.as_ref().file_stem().is_some_and(|stem| {
                    Path::new(stem)
                        .extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("tar"))
                }) =>
            {
                Some(Self::TarBz2)
            }
            "xz" if path.as_ref().file_stem().is_some_and(|stem| {
                Path::new(stem)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("tar"))
            }) =>
            {
                Some(Self::TarXz)
            }
            "zst"
                if path.as_ref().file_stem().is_some_and(|stem| {
                    Path::new(stem)
                        .extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("tar"))
                }) =>
            {
                Some(Self::TarZstd)
            }
            _ => None,
        }
    }
}
