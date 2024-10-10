use url::Url;

use uv_pep508::VerbatimUrl;

use crate::redirect::apply_redirect;

#[test]
fn test_apply_redirect() -> Result<(), url::ParseError> {
    // If there's no `@` in the original representation, we can just append the precise suffix
    // to the given representation.
    let verbatim = VerbatimUrl::parse_url("https://github.com/flask.git")?
        .with_given("git+https://github.com/flask.git");
    let redirect =
        Url::parse("https://github.com/flask.git@b90a4f1f4a370e92054b9cc9db0efcb864f87ebe")?;

    let expected = VerbatimUrl::parse_url(
        "https://github.com/flask.git@b90a4f1f4a370e92054b9cc9db0efcb864f87ebe",
    )?
    .with_given("https://github.com/flask.git@b90a4f1f4a370e92054b9cc9db0efcb864f87ebe");
    assert_eq!(apply_redirect(&verbatim, redirect), expected);

    // If there's an `@` in the original representation, and it's stable between the parsed and
    // given representations, we preserve everything that precedes the `@` in the precise
    // representation.
    let verbatim = VerbatimUrl::parse_url("https://github.com/flask.git@main")?
        .with_given("git+https://${DOMAIN}.com/flask.git@main");
    let redirect =
        Url::parse("https://github.com/flask.git@b90a4f1f4a370e92054b9cc9db0efcb864f87ebe")?;

    let expected = VerbatimUrl::parse_url(
        "https://github.com/flask.git@b90a4f1f4a370e92054b9cc9db0efcb864f87ebe",
    )?
    .with_given("https://${DOMAIN}.com/flask.git@b90a4f1f4a370e92054b9cc9db0efcb864f87ebe");
    assert_eq!(apply_redirect(&verbatim, redirect), expected);

    // If there's a conflict after the `@`, discard the original representation.
    let verbatim = VerbatimUrl::parse_url("https://github.com/flask.git@main")?
        .with_given("git+https://github.com/flask.git@${TAG}".to_string());
    let redirect =
        Url::parse("https://github.com/flask.git@b90a4f1f4a370e92054b9cc9db0efcb864f87ebe")?;

    let expected = VerbatimUrl::parse_url(
        "https://github.com/flask.git@b90a4f1f4a370e92054b9cc9db0efcb864f87ebe",
    )?;
    assert_eq!(apply_redirect(&verbatim, redirect), expected);

    // We should preserve subdirectory fragments.
    let verbatim = VerbatimUrl::parse_url("https://github.com/flask.git#subdirectory=src")?
        .with_given("git+https://github.com/flask.git#subdirectory=src");
    let redirect = Url::parse(
        "https://github.com/flask.git@b90a4f1f4a370e92054b9cc9db0efcb864f87ebe#subdirectory=src",
    )?;

    let expected = VerbatimUrl::parse_url(
        "https://github.com/flask.git@b90a4f1f4a370e92054b9cc9db0efcb864f87ebe#subdirectory=src",
    )?.with_given("git+https://github.com/flask.git@b90a4f1f4a370e92054b9cc9db0efcb864f87ebe#subdirectory=src");

    assert_eq!(apply_redirect(&verbatim, redirect), expected);

    Ok(())
}
