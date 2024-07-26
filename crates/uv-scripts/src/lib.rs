use memchr::memmem::Finder;
use pypi_types::VerbatimParsedUrl;
use serde::{Deserialize, Serialize};
use std::io;
use std::path::Path;
use std::sync::LazyLock;
use thiserror::Error;

static FINDER: LazyLock<Finder> = LazyLock::new(|| Finder::new(b"# /// script"));

/// PEP 723 metadata as parsed from a `script` comment block.
///
/// See: <https://peps.python.org/pep-0723/>
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Pep723Metadata {
    pub dependencies: Vec<pep508_rs::Requirement<VerbatimParsedUrl>>,
    pub requires_python: Option<pep440_rs::VersionSpecifiers>,
}

#[derive(Debug, Error)]
pub enum Pep723Error {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Utf8(#[from] std::str::Utf8Error),
    #[error(transparent)]
    Toml(#[from] toml::de::Error),
}

/// Read the PEP 723 `script` metadata from a Python file, if it exists.
///
/// See: <https://peps.python.org/pep-0723/>
pub async fn read_pep723_metadata(
    file: impl AsRef<Path>,
) -> Result<Option<Pep723Metadata>, Pep723Error> {
    let contents = match fs_err::tokio::read(file).await {
        Ok(contents) => contents,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err.into()),
    };

    // Extract the `script` tag.
    let Some(contents) = extract_script_tag(&contents)? else {
        return Ok(None);
    };

    // Parse the metadata.
    let metadata = toml::from_str(&contents)?;

    Ok(Some(metadata))
}

/// Given the contents of a Python file, extract the `script` metadata block, with leading comment
/// hashes removed.
///
/// See: <https://peps.python.org/pep-0723/>
fn extract_script_tag(contents: &[u8]) -> Result<Option<String>, Pep723Error> {
    // Identify the opening pragma.
    let Some(index) = FINDER.find(contents) else {
        return Ok(None);
    };

    // The opening pragma must be the first line, or immediately preceded by a newline.
    if !(index == 0 || matches!(contents[index - 1], b'\r' | b'\n')) {
        return Ok(None);
    }

    // Decode as UTF-8.
    let contents = &contents[index..];
    let contents = std::str::from_utf8(contents)?;

    let mut lines = contents.lines();

    // Ensure that the first line is exactly `# /// script`.
    if !lines.next().is_some_and(|line| line == "# /// script") {
        return Ok(None);
    }

    // > Every line between these two lines (# /// TYPE and # ///) MUST be a comment starting
    // > with #. If there are characters after the # then the first character MUST be a space. The
    // > embedded content is formed by taking away the first two characters of each line if the
    // > second character is a space, otherwise just the first character (which means the line
    // > consists of only a single #).
    let mut toml = vec![];
    for line in lines {
        // Remove the leading `#`.
        let Some(line) = line.strip_prefix('#') else {
            break;
        };

        // If the line is empty, continue.
        if line.is_empty() {
            toml.push("");
            continue;
        }

        // Otherwise, the line _must_ start with ` `.
        let Some(line) = line.strip_prefix(' ') else {
            break;
        };
        toml.push(line);
    }

    // Find the closing `# ///`. The precedence is such that we need to identify the _last_ such
    // line.
    //
    // For example, given:
    // ```python
    // # /// script
    // #
    // # ///
    // #
    // # ///
    // ```
    //
    // The latter `///` is the closing pragma
    let Some(index) = toml.iter().rev().position(|line| *line == "///") else {
        return Ok(None);
    };
    let index = toml.len() - index;

    // Discard any lines after the closing `# ///`.
    //
    // For example, given:
    // ```python
    // # /// script
    // #
    // # ///
    // #
    // #
    // ```
    //
    // We need to discard the last two lines.
    toml.truncate(index - 1);

    // Join the lines into a single string.
    let toml = toml.join("\n") + "\n";

    Ok(Some(toml))
}

#[cfg(test)]
mod tests {
    #[test]
    fn missing_space() {
        let contents = indoc::indoc! {r"
            # /// script
            #requires-python = '>=3.11'
            # ///
        "};

        assert_eq!(
            super::extract_script_tag(contents.as_bytes()).unwrap(),
            None
        );
    }

    #[test]
    fn no_closing_pragma() {
        let contents = indoc::indoc! {r"
            # /// script
            # requires-python = '>=3.11'
            # dependencies = [
            #   'requests<3',
            #   'rich',
            # ]
        "};

        assert_eq!(
            super::extract_script_tag(contents.as_bytes()).unwrap(),
            None
        );
    }

    #[test]
    fn leading_content() {
        let contents = indoc::indoc! {r"
            pass # /// script
            # requires-python = '>=3.11'
            # dependencies = [
            #   'requests<3',
            #   'rich',
            # ]
            # ///
            #
            #
        "};

        assert_eq!(
            super::extract_script_tag(contents.as_bytes()).unwrap(),
            None
        );
    }

    #[test]
    fn simple() {
        let contents = indoc::indoc! {r"
            # /// script
            # requires-python = '>=3.11'
            # dependencies = [
            #   'requests<3',
            #   'rich',
            # ]
            # ///
        "};

        let expected = indoc::indoc! {r"
            requires-python = '>=3.11'
            dependencies = [
              'requests<3',
              'rich',
            ]
        "};

        let actual = super::extract_script_tag(contents.as_bytes())
            .unwrap()
            .unwrap();

        assert_eq!(actual, expected);
    }

    #[test]
    fn embedded_comment() {
        let contents = indoc::indoc! {r"
            # /// script
            # embedded-csharp = '''
            # /// <summary>
            # /// text
            # ///
            # /// </summary>
            # public class MyClass { }
            # '''
            # ///
        "};

        let expected = indoc::indoc! {r"
            embedded-csharp = '''
            /// <summary>
            /// text
            ///
            /// </summary>
            public class MyClass { }
            '''
        "};

        let actual = super::extract_script_tag(contents.as_bytes())
            .unwrap()
            .unwrap();

        assert_eq!(actual, expected);
    }

    #[test]
    fn trailing_lines() {
        let contents = indoc::indoc! {r"
            # /// script
            # requires-python = '>=3.11'
            # dependencies = [
            #   'requests<3',
            #   'rich',
            # ]
            # ///
            #
            #
        "};

        let expected = indoc::indoc! {r"
            requires-python = '>=3.11'
            dependencies = [
              'requests<3',
              'rich',
            ]
        "};

        let actual = super::extract_script_tag(contents.as_bytes())
            .unwrap()
            .unwrap();

        assert_eq!(actual, expected);
    }
}
