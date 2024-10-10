use super::*;
use insta::assert_snapshot;

#[test]
fn test_error() {
    let parse_err = |glob| parse_pep639_glob(glob).unwrap_err().to_string();
    assert_snapshot!(
        parse_err(".."),
        @"The parent directory operator (`..`) at position 0 is not allowed in license file globs"
    );
    assert_snapshot!(
        parse_err("licenses/.."),
        @"The parent directory operator (`..`) at position 9 is not allowed in license file globs"
    );
    assert_snapshot!(
        parse_err("licenses/LICEN!E.txt"),
        @"Glob contains invalid character at position 14: `!`"
    );
    assert_snapshot!(
        parse_err("licenses/LICEN[!C]E.txt"),
        @"Glob contains invalid character in range at position 15: `!`"
    );
    assert_snapshot!(
        parse_err("licenses/LICEN[C?]E.txt"),
        @"Glob contains invalid character in range at position 16: `?`"
    );
    assert_snapshot!(parse_err("******"), @"Pattern syntax error near position 2: wildcards are either regular `*` or recursive `**`");
    assert_snapshot!(
        parse_err(r"licenses\eula.txt"),
        @r"Glob contains invalid character at position 8: `\`"
    );
}

#[test]
fn test_valid() {
    let cases = [
        "licenses/*.txt",
        "licenses/**/*.txt",
        "LICEN[CS]E.txt",
        "LICEN?E.txt",
        "[a-z].txt",
        "[a-z._-].txt",
        "*/**",
        "LICENSE..txt",
        "LICENSE_file-1.txt",
        // (google translate)
        "licenses/라이센스*.txt",
        "licenses/ライセンス*.txt",
        "licenses/执照*.txt",
    ];
    for case in cases {
        parse_pep639_glob(case).unwrap();
    }
}
