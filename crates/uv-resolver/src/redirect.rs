use url::Url;

use distribution_types::{ParsedGitUrl, ParsedUrl, VerbatimParsedUrl};
use pep508_rs::VerbatimUrl;
use uv_distribution::git_url_to_precise;
use uv_git::GitReference;

/// Given a [`VerbatimUrl`] and a redirect, apply the redirect to the URL while preserving as much
/// of the verbatim representation as possible.
fn apply_redirect(url: &VerbatimUrl, redirect: Url) -> VerbatimUrl {
    let redirect = VerbatimUrl::from_url(redirect);

    // The redirect should be the "same" URL, but with a specific commit hash added after the `@`.
    // We take advantage of this to preserve as much of the verbatim representation as possible.
    if let Some(given) = url.given() {
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
                        return redirect.with_given(format!("{given_prefix}@{precise_suffix}"));
                    }
                }
            } else {
                // If there was no `@` in the original representation, we can just append the
                // precise suffix to the given representation.
                return redirect.with_given(format!("{given}@{precise_suffix}"));
            }
        }
    }

    redirect
}

pub(crate) fn url_to_precise(url: VerbatimParsedUrl) -> VerbatimParsedUrl {
    let ParsedUrl::Git(ParsedGitUrl {
        url: git_url,
        subdirectory,
    }) = url.parsed_url.clone()
    else {
        return url;
    };

    let Some(new_git_url) = git_url_to_precise(git_url.clone()) else {
        debug_assert!(
            matches!(git_url.reference(), GitReference::FullCommit(_)),
            "Unseen Git URL: {}, {:?}",
            url.verbatim,
            git_url
        );
        return url;
    };

    let new_parsed_url = ParsedGitUrl {
        url: new_git_url,
        subdirectory,
    };
    let new_url = Url::from(new_parsed_url.clone());
    let new_verbatim_url = apply_redirect(&url.verbatim, new_url);
    VerbatimParsedUrl {
        parsed_url: ParsedUrl::Git(new_parsed_url),
        verbatim: new_verbatim_url,
    }
}

#[cfg(test)]
mod tests {
    use url::Url;

    use pep508_rs::VerbatimUrl;

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

        Ok(())
    }
}
