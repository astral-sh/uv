use super::*;

#[test]
fn scheme() {
    assert_eq!(
        split_scheme("file:///home/ferris/project/scripts"),
        Some(("file", "///home/ferris/project/scripts"))
    );
    assert_eq!(
        split_scheme("file:home/ferris/project/scripts"),
        Some(("file", "home/ferris/project/scripts"))
    );
    assert_eq!(
        split_scheme("https://example.com"),
        Some(("https", "//example.com"))
    );
    assert_eq!(split_scheme("https:"), Some(("https", "")));
}

#[test]
fn fragment() {
    assert_eq!(
        split_fragment(Path::new(
            "file:///home/ferris/project/scripts#hash=somehash"
        )),
        (
            Cow::Owned(PathBuf::from("file:///home/ferris/project/scripts")),
            Some("hash=somehash")
        )
    );
    assert_eq!(
        split_fragment(Path::new("file:home/ferris/project/scripts#hash=somehash")),
        (
            Cow::Owned(PathBuf::from("file:home/ferris/project/scripts")),
            Some("hash=somehash")
        )
    );
    assert_eq!(
        split_fragment(Path::new("/home/ferris/project/scripts#hash=somehash")),
        (
            Cow::Owned(PathBuf::from("/home/ferris/project/scripts")),
            Some("hash=somehash")
        )
    );
    assert_eq!(
        split_fragment(Path::new("file:///home/ferris/project/scripts")),
        (
            Cow::Borrowed(Path::new("file:///home/ferris/project/scripts")),
            None
        )
    );
    assert_eq!(
        split_fragment(Path::new("")),
        (Cow::Borrowed(Path::new("")), None)
    );
}
