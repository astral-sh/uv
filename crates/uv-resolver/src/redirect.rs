use url::Url;

use uv_git::GitResolver;
use uv_pep508::VerbatimUrl;
use uv_pypi_types::{ParsedGitUrl, ParsedUrl, VerbatimParsedUrl};

/// Map a URL to a precise URL, if possible.
pub(crate) fn url_to_precise(url: VerbatimParsedUrl, git: &GitResolver) -> VerbatimParsedUrl {
    let ParsedUrl::Git(ParsedGitUrl {
        url: git_url,
        subdirectory,
    }) = &url.parsed_url
    else {
        return url;
    };

    let Some(new_git_url) = git.precise(git_url.clone()) else {
        if cfg!(debug_assertions) {
            panic!("Unresolved Git URL: {}, {git_url:?}", url.verbatim);
        } else {
            return url;
        }
    };

    let new_parsed_url = ParsedGitUrl {
        url: new_git_url,
        subdirectory: subdirectory.clone(),
    };
    let new_url = Url::from(new_parsed_url.clone());
    let new_verbatim_url = apply_redirect(&url.verbatim, new_url);
    VerbatimParsedUrl {
        parsed_url: ParsedUrl::Git(new_parsed_url),
        verbatim: new_verbatim_url,
    }
}

/// Given a [`VerbatimUrl`] and a redirect, apply the redirect to the URL while preserving as much
/// of the verbatim representation as possible.
fn apply_redirect(url: &VerbatimUrl, redirect: Url) -> VerbatimUrl {
    let redirect = VerbatimUrl::from_url(redirect);

    // The redirect should be the "same" URL, but with a specific commit hash added after the `@`.
    // We take advantage of this to preserve as much of the verbatim representation as possible.
    if let Some(given) = url.given() {
        let (given, fragment) = given
            .split_once('#')
            .map_or((given, None), |(prefix, suffix)| (prefix, Some(suffix)));
        if let Some(precise_suffix) = redirect
            .raw()
            .path()
            .rsplit_once('@')
            .map(|(_, suffix)| suffix.to_owned())
        {
            // If there was an `@` in the original representation...
            if let Some((.., parsed_suffix)) = url.raw().path().rsplit_once('@') {
                if let Some((given_prefix, given_suffix)) = given.rsplit_once('@') {
                    // And the portion after the `@` is stable between the parsed and given representations...
                    if given_suffix == parsed_suffix {
                        // Preserve everything that precedes the `@` in the precise representation.
                        let given = format!("{given_prefix}@{precise_suffix}");
                        let given = if let Some(fragment) = fragment {
                            format!("{given}#{fragment}")
                        } else {
                            given
                        };
                        return redirect.with_given(given);
                    }
                }
            } else {
                // If there was no `@` in the original representation, we can just append the
                // precise suffix to the given representation.
                let given = format!("{given}@{precise_suffix}");
                let given = if let Some(fragment) = fragment {
                    format!("{given}#{fragment}")
                } else {
                    given
                };
                return redirect.with_given(given);
            }
        }
    }

    redirect
}

#[cfg(test)]
mod tests {
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
            .with_given("git+https://github.com/flask.git@${TAG}");
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
}
