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
    ];
    for (input, expected) in cases {
        assert_eq!(normalize_path(Path::new(input)), Path::new(expected));
    }
}
