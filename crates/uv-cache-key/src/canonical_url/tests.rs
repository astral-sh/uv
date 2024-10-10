use super::*;

#[test]
fn user_credential_does_not_affect_cache_key() -> Result<(), url::ParseError> {
    let mut hasher = CacheKeyHasher::new();
    CanonicalUrl::parse("https://example.com/pypa/sample-namespace-packages.git@2.0.0")?
        .cache_key(&mut hasher);
    let hash_without_creds = hasher.finish();

    let mut hasher = CacheKeyHasher::new();
    CanonicalUrl::parse("https://user:foo@example.com/pypa/sample-namespace-packages.git@2.0.0")?
        .cache_key(&mut hasher);
    let hash_with_creds = hasher.finish();
    assert_eq!(
             hash_without_creds, hash_with_creds,
             "URLs with no user credentials should hash the same as URLs with different user credentials",
         );

    let mut hasher = CacheKeyHasher::new();
    CanonicalUrl::parse("https://user:bar@example.com/pypa/sample-namespace-packages.git@2.0.0")?
        .cache_key(&mut hasher);
    let hash_with_creds = hasher.finish();
    assert_eq!(
        hash_without_creds, hash_with_creds,
        "URLs with different user credentials should hash the same",
    );

    let mut hasher = CacheKeyHasher::new();
    CanonicalUrl::parse("https://:bar@example.com/pypa/sample-namespace-packages.git@2.0.0")?
        .cache_key(&mut hasher);
    let hash_with_creds = hasher.finish();
    assert_eq!(
             hash_without_creds, hash_with_creds,
             "URLs with no username, though with a password, should hash the same as URLs with different user credentials",
         );

    let mut hasher = CacheKeyHasher::new();
    CanonicalUrl::parse("https://user:@example.com/pypa/sample-namespace-packages.git@2.0.0")?
        .cache_key(&mut hasher);
    let hash_with_creds = hasher.finish();
    assert_eq!(
             hash_without_creds, hash_with_creds,
             "URLs with no password, though with a username, should hash the same as URLs with different user credentials",
         );

    Ok(())
}

#[test]
fn canonical_url() -> Result<(), url::ParseError> {
    // Two URLs should be considered equal regardless of the `.git` suffix.
    assert_eq!(
        CanonicalUrl::parse("git+https://github.com/pypa/sample-namespace-packages.git")?,
        CanonicalUrl::parse("git+https://github.com/pypa/sample-namespace-packages")?,
    );

    // Two URLs should be considered equal regardless of the `.git` suffix.
    assert_eq!(
        CanonicalUrl::parse("git+https://github.com/pypa/sample-namespace-packages.git@2.0.0")?,
        CanonicalUrl::parse("git+https://github.com/pypa/sample-namespace-packages@2.0.0")?,
    );

    // Two URLs should be _not_ considered equal if they point to different repositories.
    assert_ne!(
        CanonicalUrl::parse("git+https://github.com/pypa/sample-namespace-packages.git")?,
        CanonicalUrl::parse("git+https://github.com/pypa/sample-packages.git")?,
    );

    // Two URLs should _not_ be considered equal if they request different subdirectories.
    assert_ne!(
             CanonicalUrl::parse("git+https://github.com/pypa/sample-namespace-packages.git#subdirectory=pkg_resources/pkg_a")?,
             CanonicalUrl::parse("git+https://github.com/pypa/sample-namespace-packages.git#subdirectory=pkg_resources/pkg_b")?,
         );

    // Two URLs should _not_ be considered equal if they request different commit tags.
    assert_ne!(
        CanonicalUrl::parse("git+https://github.com/pypa/sample-namespace-packages.git@v1.0.0")?,
        CanonicalUrl::parse("git+https://github.com/pypa/sample-namespace-packages.git@v2.0.0")?,
    );

    // Two URLs that cannot be a base should be considered equal.
    assert_eq!(
        CanonicalUrl::parse("git+https:://github.com/pypa/sample-namespace-packages.git")?,
        CanonicalUrl::parse("git+https:://github.com/pypa/sample-namespace-packages.git")?,
    );

    Ok(())
}

#[test]
fn repository_url() -> Result<(), url::ParseError> {
    // Two URLs should be considered equal regardless of the `.git` suffix.
    assert_eq!(
        RepositoryUrl::parse("git+https://github.com/pypa/sample-namespace-packages.git")?,
        RepositoryUrl::parse("git+https://github.com/pypa/sample-namespace-packages")?,
    );

    // Two URLs should be considered equal regardless of the `.git` suffix.
    assert_eq!(
        RepositoryUrl::parse("git+https://github.com/pypa/sample-namespace-packages.git@2.0.0")?,
        RepositoryUrl::parse("git+https://github.com/pypa/sample-namespace-packages@2.0.0")?,
    );

    // Two URLs should be _not_ considered equal if they point to different repositories.
    assert_ne!(
        RepositoryUrl::parse("git+https://github.com/pypa/sample-namespace-packages.git")?,
        RepositoryUrl::parse("git+https://github.com/pypa/sample-packages.git")?,
    );

    // Two URLs should be considered equal if they map to the same repository, even if they
    // request different subdirectories.
    assert_eq!(
             RepositoryUrl::parse("git+https://github.com/pypa/sample-namespace-packages.git#subdirectory=pkg_resources/pkg_a")?,
             RepositoryUrl::parse("git+https://github.com/pypa/sample-namespace-packages.git#subdirectory=pkg_resources/pkg_b")?,
         );

    // Two URLs should be considered equal if they map to the same repository, even if they
    // request different commit tags.
    assert_eq!(
        RepositoryUrl::parse("git+https://github.com/pypa/sample-namespace-packages.git@v1.0.0")?,
        RepositoryUrl::parse("git+https://github.com/pypa/sample-namespace-packages.git@v2.0.0")?,
    );

    Ok(())
}
