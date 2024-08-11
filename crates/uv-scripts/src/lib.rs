use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::LazyLock;

use memchr::memmem::Finder;
use pep440_rs::VersionSpecifiers;
use serde::Deserialize;
use thiserror::Error;

use pep508_rs::PackageName;
use pypi_types::VerbatimParsedUrl;
use uv_settings::{GlobalOptions, ResolverInstallerOptions};
use uv_workspace::pyproject::Source;

static FINDER: LazyLock<Finder> = LazyLock::new(|| Finder::new(b"# /// script"));

/// A PEP 723 script, including its [`Pep723Metadata`].
#[derive(Debug)]
pub struct Pep723Script {
    /// The path to the Python script.
    pub path: PathBuf,
    /// The parsed [`Pep723Metadata`] table from the script.
    pub metadata: Pep723Metadata,
    /// The content of the script after the metadata table.
    pub raw: String,
    /// The content of the script before the metadata table.
    pub prelude: String,
}

impl Pep723Script {
    /// Read the PEP 723 `script` metadata from a Python file, if it exists.
    ///
    /// See: <https://peps.python.org/pep-0723/>
    pub async fn read(file: impl AsRef<Path>) -> Result<Option<Self>, Pep723Error> {
        let contents = match fs_err::tokio::read(&file).await {
            Ok(contents) => contents,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(err) => return Err(err.into()),
        };

        // Extract the `script` tag.
        let Some(script_tag) = ScriptTag::parse(&contents)? else {
            return Ok(None);
        };

        // Parse the metadata.
        let metadata = Pep723Metadata::from_str(&script_tag.metadata)?;

        Ok(Some(Self {
            path: file.as_ref().to_path_buf(),
            metadata,
            raw: script_tag.script,
            prelude: script_tag.prelude,
        }))
    }

    /// Reads a Python script and generates a default PEP 723 metadata table.
    ///
    /// See: <https://peps.python.org/pep-0723/>
    pub async fn create(
        file: impl AsRef<Path>,
        requires_python: &VersionSpecifiers,
    ) -> Result<Self, Pep723Error> {
        let contents = match fs_err::tokio::read(&file).await {
            Ok(contents) => contents,
            Err(err) => return Err(err.into()),
        };

        // Extract the `script` tag.
        let default_metadata = indoc::formatdoc! {r#"
            requires-python = "{requires_python}"
            dependencies = []
            "#,
            requires_python = requires_python,
        };

        let (prelude, raw) = extract_shebang(&contents)?;

        // Parse the metadata.
        let metadata = Pep723Metadata::from_str(&default_metadata)?;

        Ok(Self {
            path: file.as_ref().to_path_buf(),
            prelude: prelude.unwrap_or_default(),
            metadata,
            raw,
        })
    }

    /// Replace the existing metadata in the file with new metadata and write the updated content.
    pub async fn replace_metadata(&self, new_metadata: &str) -> Result<(), Pep723Error> {
        let new_content = format!(
            "{}{}{}",
            if self.prelude.is_empty() {
                String::new()
            } else {
                format!("{}\n", self.prelude)
            },
            serialize_metadata(new_metadata),
            self.raw
        );

        fs_err::tokio::write(&self.path, new_content)
            .await
            .map_err(std::convert::Into::into)
    }
}

/// PEP 723 metadata as parsed from a `script` comment block.
///
/// See: <https://peps.python.org/pep-0723/>
#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Pep723Metadata {
    pub dependencies: Option<Vec<pep508_rs::Requirement<VerbatimParsedUrl>>>,
    pub requires_python: Option<VersionSpecifiers>,
    pub tool: Option<Tool>,
    /// The raw unserialized document.
    #[serde(skip)]
    pub raw: String,
}

impl FromStr for Pep723Metadata {
    type Err = Pep723Error;

    /// Parse `Pep723Metadata` from a raw TOML string.
    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        let metadata = toml::from_str(raw)?;
        Ok(Pep723Metadata {
            raw: raw.to_string(),
            ..metadata
        })
    }
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
pub struct Tool {
    pub uv: Option<ToolUv>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolUv {
    #[serde(flatten)]
    pub globals: GlobalOptions,
    #[serde(flatten)]
    pub top_level: ResolverInstallerOptions,
    pub sources: Option<BTreeMap<PackageName, Source>>,
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

#[derive(Debug, Clone, Eq, PartialEq)]
struct ScriptTag {
    /// The content of the script before the metadata block.
    prelude: String,
    /// The metadata block.
    metadata: String,
    /// The content of the script after the metadata block.
    script: String,
}

impl ScriptTag {
    /// Given the contents of a Python file, extract the `script` metadata block with leading
    /// comment hashes removed, any preceding shebang or content (prelude), and the remaining Python
    /// script.
    ///
    /// Given the following input string representing the contents of a Python script:
    ///
    /// ```python
    /// #!/usr/bin/env python3
    /// # /// script
    /// # requires-python = '>=3.11'
    /// # dependencies = [
    /// #   'requests<3',
    /// #   'rich',
    /// # ]
    /// # ///
    ///
    /// import requests
    ///
    /// print("Hello, World!")
    /// ```
    ///
    /// This function would return:
    ///
    /// - Preamble: `#!/usr/bin/env python3\n`
    /// - Metadata: `requires-python = '>=3.11'\ndependencies = [\n  'requests<3',\n  'rich',\n]`
    /// - Script: `import requests\n\nprint("Hello, World!")\n`
    ///
    /// See: <https://peps.python.org/pep-0723/>
    fn parse(contents: &[u8]) -> Result<Option<Self>, Pep723Error> {
        // Identify the opening pragma.
        let Some(index) = FINDER.find(contents) else {
            return Ok(None);
        };

        // The opening pragma must be the first line, or immediately preceded by a newline.
        if !(index == 0 || matches!(contents[index - 1], b'\r' | b'\n')) {
            return Ok(None);
        }

        // Extract the preceding content.
        let prelude = std::str::from_utf8(&contents[..index])?;

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

        // Extract the content that follows the metadata block.
        let mut python_script = vec![];

        while let Some(line) = lines.next() {
            // Remove the leading `#`.
            let Some(line) = line.strip_prefix('#') else {
                python_script.push(line);
                python_script.extend(lines);
                break;
            };

            // If the line is empty, continue.
            if line.is_empty() {
                toml.push("");
                continue;
            }

            // Otherwise, the line _must_ start with ` `.
            let Some(line) = line.strip_prefix(' ') else {
                python_script.push(line);
                python_script.extend(lines);
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
        let prelude = prelude.to_string();
        let metadata = toml.join("\n") + "\n";
        let script = python_script.join("\n") + "\n";

        Ok(Some(Self {
            prelude,
            metadata,
            script,
        }))
    }
}

/// Extracts the shebang line from the given file contents and returns it along with the remaining
/// content.
fn extract_shebang(contents: &[u8]) -> Result<(Option<String>, String), Pep723Error> {
    let contents = std::str::from_utf8(contents)?;

    let mut lines = contents.lines();

    // Check the first line for a shebang
    if let Some(first_line) = lines.next() {
        if first_line.starts_with("#!") {
            let shebang = first_line.to_string();
            let remaining_content: String = lines.collect::<Vec<&str>>().join("\n");
            return Ok((Some(shebang), remaining_content));
        }
    }

    Ok((None, contents.to_string()))
}

/// Formats the provided metadata by prefixing each line with `#` and wrapping it with script markers.
fn serialize_metadata(metadata: &str) -> String {
    let mut output = String::with_capacity(metadata.len() + 2);

    output.push_str("# /// script\n");

    for line in metadata.lines() {
        if line.is_empty() {
            output.push('\n');
        } else {
            output.push_str("# ");
            output.push_str(line);
            output.push('\n');
        }
    }

    output.push_str("# ///\n");

    output
}

#[cfg(test)]
mod tests {
    use crate::{serialize_metadata, ScriptTag};

    #[test]
    fn missing_space() {
        let contents = indoc::indoc! {r"
            # /// script
            #requires-python = '>=3.11'
            # ///
        "};

        assert_eq!(ScriptTag::parse(contents.as_bytes()).unwrap(), None);
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

        assert_eq!(ScriptTag::parse(contents.as_bytes()).unwrap(), None);
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

        assert_eq!(ScriptTag::parse(contents.as_bytes()).unwrap(), None);
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

            import requests
            from rich.pretty import pprint

            resp = requests.get('https://peps.python.org/api/peps.json')
            data = resp.json()
        "};

        let expected_metadata = indoc::indoc! {r"
            requires-python = '>=3.11'
            dependencies = [
              'requests<3',
              'rich',
            ]
        "};

        let expected_data = indoc::indoc! {r"

            import requests
            from rich.pretty import pprint

            resp = requests.get('https://peps.python.org/api/peps.json')
            data = resp.json()
        "};

        let actual = ScriptTag::parse(contents.as_bytes()).unwrap().unwrap();

        assert_eq!(actual.prelude, String::new());
        assert_eq!(actual.metadata, expected_metadata);
        assert_eq!(actual.script, expected_data);
    }

    #[test]
    fn simple_with_shebang() {
        let contents = indoc::indoc! {r"
            #!/usr/bin/env python3
            # /// script
            # requires-python = '>=3.11'
            # dependencies = [
            #   'requests<3',
            #   'rich',
            # ]
            # ///

            import requests
            from rich.pretty import pprint

            resp = requests.get('https://peps.python.org/api/peps.json')
            data = resp.json()
        "};

        let expected_metadata = indoc::indoc! {r"
            requires-python = '>=3.11'
            dependencies = [
              'requests<3',
              'rich',
            ]
        "};

        let expected_data = indoc::indoc! {r"

            import requests
            from rich.pretty import pprint

            resp = requests.get('https://peps.python.org/api/peps.json')
            data = resp.json()
        "};

        let actual = ScriptTag::parse(contents.as_bytes()).unwrap().unwrap();

        assert_eq!(actual.prelude, "#!/usr/bin/env python3\n".to_string());
        assert_eq!(actual.metadata, expected_metadata);
        assert_eq!(actual.script, expected_data);
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

        let actual = ScriptTag::parse(contents.as_bytes())
            .unwrap()
            .unwrap()
            .metadata;

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

        let actual = ScriptTag::parse(contents.as_bytes())
            .unwrap()
            .unwrap()
            .metadata;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_serialize_metadata_formatting() {
        let metadata = indoc::indoc! {r"
            requires-python = '>=3.11'
            dependencies = [
              'requests<3',
              'rich',
            ]
        "};

        let expected_output = indoc::indoc! {r"
            # /// script
            # requires-python = '>=3.11'
            # dependencies = [
            #   'requests<3',
            #   'rich',
            # ]
            # ///
        "};

        let result = serialize_metadata(metadata);
        assert_eq!(result, expected_output);
    }

    #[test]
    fn test_serialize_metadata_empty() {
        let metadata = "";
        let expected_output = "# /// script\n# ///\n";

        let result = serialize_metadata(metadata);
        assert_eq!(result, expected_output);
    }
}
