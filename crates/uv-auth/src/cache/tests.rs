use super::*;

#[test]
fn test_trie() {
    let credentials1 = Arc::new(Credentials::new(
        Some("username1".to_string()),
        Some("password1".to_string()),
    ));
    let credentials2 = Arc::new(Credentials::new(
        Some("username2".to_string()),
        Some("password2".to_string()),
    ));
    let credentials3 = Arc::new(Credentials::new(
        Some("username3".to_string()),
        Some("password3".to_string()),
    ));
    let credentials4 = Arc::new(Credentials::new(
        Some("username4".to_string()),
        Some("password4".to_string()),
    ));

    let mut trie = UrlTrie::new();
    trie.insert(
        &Url::parse("https://burntsushi.net").unwrap(),
        credentials1.clone(),
    );
    trie.insert(
        &Url::parse("https://astral.sh").unwrap(),
        credentials2.clone(),
    );
    trie.insert(
        &Url::parse("https://example.com/foo").unwrap(),
        credentials3.clone(),
    );
    trie.insert(
        &Url::parse("https://example.com/bar").unwrap(),
        credentials4.clone(),
    );

    let url = Url::parse("https://burntsushi.net/regex-internals").unwrap();
    assert_eq!(trie.get(&url), Some(&credentials1));

    let url = Url::parse("https://burntsushi.net/").unwrap();
    assert_eq!(trie.get(&url), Some(&credentials1));

    let url = Url::parse("https://astral.sh/about").unwrap();
    assert_eq!(trie.get(&url), Some(&credentials2));

    let url = Url::parse("https://example.com/foo").unwrap();
    assert_eq!(trie.get(&url), Some(&credentials3));

    let url = Url::parse("https://example.com/foo/").unwrap();
    assert_eq!(trie.get(&url), Some(&credentials3));

    let url = Url::parse("https://example.com/foo/bar").unwrap();
    assert_eq!(trie.get(&url), Some(&credentials3));

    let url = Url::parse("https://example.com/bar").unwrap();
    assert_eq!(trie.get(&url), Some(&credentials4));

    let url = Url::parse("https://example.com/bar/").unwrap();
    assert_eq!(trie.get(&url), Some(&credentials4));

    let url = Url::parse("https://example.com/bar/foo").unwrap();
    assert_eq!(trie.get(&url), Some(&credentials4));

    let url = Url::parse("https://example.com/about").unwrap();
    assert_eq!(trie.get(&url), None);

    let url = Url::parse("https://example.com/foobar").unwrap();
    assert_eq!(trie.get(&url), None);
}
