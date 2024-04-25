//! Parses a subset of requirement.txt syntax
//!
//! <https://pip.pypa.io/en/stable/reference/requirements-file-format/>
//!
//! Supported:
//!  * [PEP 508 requirements](https://packaging.python.org/en/latest/specifications/dependency-specifiers/)
//!  * `-r`
//!  * `-c`
//!  * `--hash` (postfix)
//!  * `-e`
//!
//! Unsupported:
//!  * `<path>`. TBD
//!  * `<archive_url>`. TBD
//!  * Options without a requirement, such as `--find-links` or `--index-url`
//!
//! Grammar as implemented:
//!
//! ```text
//! file = (statement | empty ('#' any*)? '\n')*
//! empty = whitespace*
//! statement = constraint_include | requirements_include | editable_requirement | requirement
//! constraint_include = '-c' ('=' | wrappable_whitespaces) filepath
//! requirements_include = '-r' ('=' | wrappable_whitespaces) filepath
//! editable_requirement = '-e' ('=' | wrappable_whitespaces) requirement
//! # We check whether the line starts with a letter or a number, in that case we assume it's a
//! # PEP 508 requirement
//! # https://packaging.python.org/en/latest/specifications/name-normalization/#valid-non-normalized-names
//! # This does not (yet?) support plain files or urls, we use a letter or a number as first
//! # character to assume a PEP 508 requirement
//! requirement = [a-zA-Z0-9] pep508_grammar_tail wrappable_whitespaces hashes
//! hashes = ('--hash' ('=' | wrappable_whitespaces) [a-zA-Z0-9-_]+ ':' [a-zA-Z0-9-_] wrappable_whitespaces+)*
//! # This should indicate a single backslash before a newline
//! wrappable_whitespaces = whitespace ('\\\n' | whitespace)*
//! ```

use std::borrow::Cow;
use std::fmt::{Display, Formatter};
use std::io;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use tracing::instrument;
use unscanny::{Pattern, Scanner};
use url::Url;

use distribution_types::{ParsedUrlError, UvRequirement};
use pep508_rs::{
    expand_env_vars, split_scheme, strip_host, Extras, Pep508Error, Pep508ErrorSource, Requirement,
    Scheme, VerbatimUrl,
};
#[cfg(feature = "http")]
use uv_client::BaseClient;
use uv_client::BaseClientBuilder;
use uv_configuration::{NoBinary, NoBuild, PackageNameSpecifier};
use uv_fs::{normalize_url_path, Simplified};
use uv_normalize::ExtraName;
use uv_warnings::warn_user;

pub use crate::requirement::{RequirementsTxtRequirement, RequirementsTxtRequirementError};

mod requirement;

/// We emit one of those for each requirements.txt entry
enum RequirementsTxtStatement {
    /// `-r` inclusion filename
    Requirements {
        filename: String,
        start: usize,
        end: usize,
    },
    /// `-c` inclusion filename
    Constraint {
        filename: String,
        start: usize,
        end: usize,
    },
    /// PEP 508 requirement plus metadata
    RequirementEntry(RequirementEntry),
    /// `-e`
    EditableRequirement(EditableRequirement),
    /// `--index-url`
    IndexUrl(VerbatimUrl),
    /// `--extra-index-url`
    ExtraIndexUrl(VerbatimUrl),
    /// `--find-links`
    FindLinks(FindLink),
    /// `--no-index`
    NoIndex,
    /// `--no-binary`
    NoBinary(NoBinary),
    /// `only-binary`
    OnlyBinary(NoBuild),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FindLink {
    Path(PathBuf),
    Url(Url),
}

impl FindLink {
    /// Parse a raw string for a `--find-links` entry, which could be a URL or a local path.
    ///
    /// For example:
    /// - `file:///home/ferris/project/scripts/...`
    /// - `file:../ferris/`
    /// - `../ferris/`
    /// - `https://download.pytorch.org/whl/torch_stable.html`
    pub fn parse(given: &str, working_dir: impl AsRef<Path>) -> Result<Self, url::ParseError> {
        // Expand environment variables.
        let expanded = expand_env_vars(given);

        if let Some((scheme, path)) = split_scheme(&expanded) {
            match Scheme::parse(scheme) {
                // Ex) `file:///home/ferris/project/scripts/...`, `file://localhost/home/ferris/project/scripts/...`, or `file:../ferris/`
                Some(Scheme::File) => {
                    // Strip the leading slashes, along with the `localhost` host, if present.
                    let path = strip_host(path);

                    // Transform, e.g., `/C:/Users/ferris/wheel-0.42.0.tar.gz` to `C:\Users\ferris\wheel-0.42.0.tar.gz`.
                    let path = normalize_url_path(path);

                    let path = PathBuf::from(path.as_ref());
                    let path = if path.is_absolute() {
                        path
                    } else {
                        working_dir.as_ref().join(path)
                    };
                    Ok(Self::Path(path))
                }

                // Ex) `https://download.pytorch.org/whl/torch_stable.html`
                Some(_) => {
                    let url = Url::parse(&expanded)?;
                    Ok(Self::Url(url))
                }

                // Ex) `C:/Users/ferris/wheel-0.42.0.tar.gz`
                _ => {
                    let path = PathBuf::from(expanded.as_ref());
                    let path = if path.is_absolute() {
                        path
                    } else {
                        working_dir.as_ref().join(path)
                    };
                    Ok(Self::Path(path))
                }
            }
        } else {
            // Ex) `../ferris/`
            let path = PathBuf::from(expanded.as_ref());
            let path = if path.is_absolute() {
                path
            } else {
                working_dir.as_ref().join(path)
            };
            Ok(Self::Path(path))
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct EditableRequirement {
    pub url: VerbatimUrl,
    pub extras: Vec<ExtraName>,
    pub path: PathBuf,
}

impl EditableRequirement {
    pub fn url(&self) -> &VerbatimUrl {
        &self.url
    }

    pub fn raw(&self) -> &Url {
        self.url.raw()
    }
}

impl EditableRequirement {
    /// Parse a raw string for an editable requirement (`pip install -e <editable>`), which could be
    /// a URL or a local path, and could contain unexpanded environment variables.
    ///
    /// For example:
    /// - `file:///home/ferris/project/scripts/...`
    /// - `file:../editable/`
    /// - `../editable/`
    ///
    /// We disallow URLs with schemes other than `file://` (e.g., `https://...`).
    pub fn parse(
        given: &str,
        working_dir: impl AsRef<Path>,
    ) -> Result<Self, RequirementsTxtParserError> {
        // Identify the extras.
        let (requirement, extras) = if let Some((requirement, extras)) = Self::split_extras(given) {
            let extras = Extras::parse(extras).map_err(|err| {
                // Map from error on the extras to error on the whole requirement.
                let err = Pep508Error {
                    message: err.message,
                    start: requirement.len() + err.start,
                    len: err.len,
                    input: given.to_string(),
                };
                match err.message {
                    Pep508ErrorSource::String(_) | Pep508ErrorSource::UrlError(_) => {
                        RequirementsTxtParserError::Pep508 {
                            start: err.start,
                            end: err.start + err.len,
                            source: err,
                        }
                    }
                    Pep508ErrorSource::UnsupportedRequirement(_) => {
                        RequirementsTxtParserError::UnsupportedRequirement {
                            start: err.start,
                            end: err.start + err.len,
                            source: err,
                        }
                    }
                }
            })?;
            (requirement, extras.into_vec())
        } else {
            (given, vec![])
        };

        // Expand environment variables.
        let expanded = expand_env_vars(requirement);

        // Create a `VerbatimUrl` to represent the editable requirement.
        let url = if let Some((scheme, path)) = split_scheme(&expanded) {
            match Scheme::parse(scheme) {
                // Ex) `file:///home/ferris/project/scripts/...` or `file:../editable/`
                Some(Scheme::File) => {
                    // Strip the leading slashes, along with the `localhost` host, if present.
                    let path = strip_host(path);

                    // Transform, e.g., `/C:/Users/ferris/wheel-0.42.0.tar.gz` to `C:\Users\ferris\wheel-0.42.0.tar.gz`.
                    let path = normalize_url_path(path);

                    VerbatimUrl::parse_path(path.as_ref(), working_dir.as_ref())
                }

                // Ex) `https://download.pytorch.org/whl/torch_stable.html`
                Some(_) => {
                    return Err(RequirementsTxtParserError::UnsupportedUrl(
                        expanded.to_string(),
                    ));
                }

                // Ex) `C:/Users/ferris/wheel-0.42.0.tar.gz`
                _ => VerbatimUrl::parse_path(expanded.as_ref(), working_dir.as_ref()),
            }
        } else {
            // Ex) `../editable/`
            VerbatimUrl::parse_path(expanded.as_ref(), working_dir.as_ref())
        };

        // Create a `PathBuf`.
        let path = url
            .to_file_path()
            .map_err(|()| RequirementsTxtParserError::InvalidEditablePath(expanded.to_string()))?;

        // Add the verbatim representation of the URL to the `VerbatimUrl`.
        let url = url.with_given(requirement.to_string());

        Ok(Self { url, extras, path })
    }

    /// Identify the extras in an editable URL (e.g., `../editable[dev]`).
    ///
    /// Pip uses `m = re.match(r'^(.+)(\[[^]]+])$', path)`. Our strategy is:
    /// - If the string ends with a closing bracket (`]`)...
    /// - Iterate backwards until you find the open bracket (`[`)...
    /// - But abort if you find another closing bracket (`]`) first.
    pub fn split_extras(given: &str) -> Option<(&str, &str)> {
        let mut chars = given.char_indices().rev();

        // If the string ends with a closing bracket (`]`)...
        if !matches!(chars.next(), Some((_, ']'))) {
            return None;
        }

        // Iterate backwards until you find the open bracket (`[`)...
        let (index, _) = chars
            .take_while(|(_, c)| *c != ']')
            .find(|(_, c)| *c == '[')?;

        Some(given.split_at(index))
    }
}

impl Display for EditableRequirement {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.url, f)
    }
}

/// A [Requirement] with additional metadata from the requirements.txt, currently only hashes but in
/// the future also editable an similar information
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct RequirementEntry {
    /// The actual PEP 508 requirement
    pub requirement: RequirementsTxtRequirement,
    /// Hashes of the downloadable packages
    pub hashes: Vec<String>,
}

/// Parsed and flattened requirements.txt with requirements and constraints
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct RequirementsTxt {
    /// The actual requirements with the hashes.
    pub requirements: Vec<RequirementEntry>,
    /// Constraints included with `-c`.
    pub constraints: Vec<UvRequirement>,
    /// Editables with `-e`.
    pub editables: Vec<EditableRequirement>,
    /// The index URL, specified with `--index-url`.
    pub index_url: Option<VerbatimUrl>,
    /// The extra index URLs, specified with `--extra-index-url`.
    pub extra_index_urls: Vec<VerbatimUrl>,
    /// The find links locations, specified with `--find-links`.
    pub find_links: Vec<FindLink>,
    /// Whether to ignore the index, specified with `--no-index`.
    pub no_index: bool,
    /// Whether to disallow wheels, specified with `--no-binary`.
    pub no_binary: NoBinary,
    /// Whether to allow only wheels, specified with `--only-binary`.
    pub only_binary: NoBuild,
}

impl RequirementsTxt {
    /// See module level documentation
    #[instrument(skip_all, fields(requirements_txt = requirements_txt.as_ref().as_os_str().to_str()))]
    pub async fn parse(
        requirements_txt: impl AsRef<Path>,
        working_dir: impl AsRef<Path>,
        client_builder: &BaseClientBuilder<'_>,
    ) -> Result<Self, RequirementsTxtFileError> {
        let requirements_txt = requirements_txt.as_ref();
        let working_dir = working_dir.as_ref();

        let content =
            if requirements_txt.starts_with("http://") | requirements_txt.starts_with("https://") {
                #[cfg(not(feature = "http"))]
                {
                    return Err(RequirementsTxtFileError {
                        file: requirements_txt.to_path_buf(),
                        error: RequirementsTxtParserError::IO(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "Remote file not supported without `http` feature",
                        )),
                    });
                }

                #[cfg(feature = "http")]
                {
                    // Avoid constructing a client if network is disabled already
                    if client_builder.is_offline() {
                        return Err(RequirementsTxtFileError {
                            file: requirements_txt.to_path_buf(),
                            error: RequirementsTxtParserError::IO(io::Error::new(
                                io::ErrorKind::InvalidInput,
                                format!("Network connectivity is disabled, but a remote requirements file was requested: {}", requirements_txt.display()),
                            )),
                        });
                    }

                    let client = client_builder.build();
                    read_url_to_string(&requirements_txt, client).await
                }
            } else {
                uv_fs::read_to_string_transcode(&requirements_txt)
                    .await
                    .map_err(RequirementsTxtParserError::IO)
            }
            .map_err(|err| RequirementsTxtFileError {
                file: requirements_txt.to_path_buf(),
                error: err,
            })?;

        let requirements_dir = requirements_txt.parent().unwrap_or(working_dir);
        let data = Self::parse_inner(&content, working_dir, requirements_dir, client_builder)
            .await
            .map_err(|err| RequirementsTxtFileError {
                file: requirements_txt.to_path_buf(),
                error: err,
            })?;
        if data == Self::default() {
            warn_user!(
                "Requirements file {} does not contain any dependencies",
                requirements_txt.user_display()
            );
        }

        Ok(data)
    }

    /// See module level documentation.
    ///
    /// When parsing, relative paths to requirements (e.g., `-e ../editable/`) are resolved against
    /// the current working directory. However, relative paths to sub-files (e.g., `-r ../requirements.txt`)
    /// are resolved against the directory of the containing `requirements.txt` file, to match
    /// `pip`'s behavior.
    pub async fn parse_inner(
        content: &str,
        working_dir: &Path,
        requirements_dir: &Path,
        client_builder: &BaseClientBuilder<'_>,
    ) -> Result<Self, RequirementsTxtParserError> {
        let mut s = Scanner::new(content);

        let mut data = Self::default();
        while let Some(statement) = parse_entry(&mut s, content, working_dir)? {
            match statement {
                RequirementsTxtStatement::Requirements {
                    filename,
                    start,
                    end,
                } => {
                    let filename = expand_env_vars(&filename);
                    let sub_file =
                        if filename.starts_with("http://") || filename.starts_with("https://") {
                            PathBuf::from(filename.as_ref())
                        } else {
                            requirements_dir.join(filename.as_ref())
                        };
                    let sub_requirements =
                        Box::pin(Self::parse(&sub_file, working_dir, client_builder))
                            .await
                            .map_err(|err| RequirementsTxtParserError::Subfile {
                                source: Box::new(err),
                                start,
                                end,
                            })?;

                    // Disallow conflicting `--index-url` in nested `requirements` files.
                    if sub_requirements.index_url.is_some()
                        && data.index_url.is_some()
                        && sub_requirements.index_url != data.index_url
                    {
                        let (line, column) = calculate_row_column(content, s.cursor());
                        return Err(RequirementsTxtParserError::Parser {
                            message:
                                "Nested `requirements` file contains conflicting `--index-url`"
                                    .to_string(),
                            line,
                            column,
                        });
                    }

                    // Add each to the correct category.
                    data.update_from(sub_requirements);
                }
                RequirementsTxtStatement::Constraint {
                    filename,
                    start,
                    end,
                } => {
                    let filename = expand_env_vars(&filename);
                    let sub_file =
                        if filename.starts_with("http://") || filename.starts_with("https://") {
                            PathBuf::from(filename.as_ref())
                        } else {
                            requirements_dir.join(filename.as_ref())
                        };
                    let sub_constraints =
                        Box::pin(Self::parse(&sub_file, working_dir, client_builder))
                            .await
                            .map_err(|err| RequirementsTxtParserError::Subfile {
                                source: Box::new(err),
                                start,
                                end,
                            })?;

                    // Treat any nested requirements or constraints as constraints. This differs
                    // from `pip`, which seems to treat `-r` requirements in constraints files as
                    // _requirements_, but we don't want to support that.
                    for entry in sub_constraints.requirements {
                        match entry.requirement {
                            RequirementsTxtRequirement::Uv(requirement) => {
                                data.constraints.push(requirement);
                            }
                            RequirementsTxtRequirement::Unnamed(_) => {
                                return Err(RequirementsTxtParserError::UnnamedConstraint {
                                    start,
                                    end,
                                });
                            }
                        }
                    }
                    data.constraints.extend(sub_constraints.constraints);
                }
                RequirementsTxtStatement::RequirementEntry(requirement_entry) => {
                    data.requirements.push(requirement_entry);
                }
                RequirementsTxtStatement::EditableRequirement(editable) => {
                    data.editables.push(editable);
                }
                RequirementsTxtStatement::IndexUrl(url) => {
                    if data.index_url.is_some() {
                        let (line, column) = calculate_row_column(content, s.cursor());
                        return Err(RequirementsTxtParserError::Parser {
                            message: "Multiple `--index-url` values provided".to_string(),
                            line,
                            column,
                        });
                    }
                    data.index_url = Some(url);
                }
                RequirementsTxtStatement::ExtraIndexUrl(url) => {
                    data.extra_index_urls.push(url);
                }
                RequirementsTxtStatement::FindLinks(path_or_url) => {
                    data.find_links.push(path_or_url);
                }
                RequirementsTxtStatement::NoIndex => {
                    data.no_index = true;
                }
                RequirementsTxtStatement::NoBinary(no_binary) => {
                    data.no_binary.extend(no_binary);
                }
                RequirementsTxtStatement::OnlyBinary(only_binary) => {
                    data.only_binary.extend(only_binary);
                }
            }
        }
        Ok(data)
    }

    /// Merge the data from a nested `requirements` file (`other`) into this one.
    pub fn update_from(&mut self, other: Self) {
        let RequirementsTxt {
            requirements,
            constraints,
            editables,
            index_url,
            extra_index_urls,
            find_links,
            no_index,
            no_binary,
            only_binary,
        } = other;
        self.requirements.extend(requirements);
        self.constraints.extend(constraints);
        self.editables.extend(editables);
        if self.index_url.is_none() {
            self.index_url = index_url;
        }
        self.extra_index_urls.extend(extra_index_urls);
        self.find_links.extend(find_links);
        self.no_index = self.no_index || no_index;
        self.no_binary.extend(no_binary);
        self.only_binary.extend(only_binary);
    }
}

/// Parse a single entry, that is a requirement, an inclusion or a comment line
///
/// Consumes all preceding trivia (whitespace and comments). If it returns None, we've reached
/// the end of file
fn parse_entry(
    s: &mut Scanner,
    content: &str,
    working_dir: &Path,
) -> Result<Option<RequirementsTxtStatement>, RequirementsTxtParserError> {
    // Eat all preceding whitespace, this may run us to the end of file
    eat_wrappable_whitespace(s);
    while s.at(['\n', '\r', '#']) {
        // skip comments
        eat_trailing_line(content, s)?;
        eat_wrappable_whitespace(s);
    }

    let start = s.cursor();
    Ok(Some(if s.eat_if("-r") || s.eat_if("--requirement") {
        let requirements_file = parse_value(content, s, |c: char| !['\n', '\r', '#'].contains(&c))?;
        let end = s.cursor();
        RequirementsTxtStatement::Requirements {
            filename: requirements_file.to_string(),
            start,
            end,
        }
    } else if s.eat_if("-c") || s.eat_if("--constraint") {
        let constraints_file = parse_value(content, s, |c: char| !['\n', '\r', '#'].contains(&c))?;
        let end = s.cursor();
        RequirementsTxtStatement::Constraint {
            filename: constraints_file.to_string(),
            start,
            end,
        }
    } else if s.eat_if("-e") || s.eat_if("--editable") {
        let path_or_url = parse_value(content, s, |c: char| !['\n', '\r', '#'].contains(&c))?;
        let editable_requirement = EditableRequirement::parse(path_or_url, working_dir)
            .map_err(|err| err.with_offset(start))?;
        RequirementsTxtStatement::EditableRequirement(editable_requirement)
    } else if s.eat_if("-i") || s.eat_if("--index-url") {
        let given = parse_value(content, s, |c: char| !['\n', '\r', '#'].contains(&c))?;
        let expanded = expand_env_vars(given);
        let url = VerbatimUrl::parse_url(expanded.as_ref())
            .map(|url| url.with_given(given.to_owned()))
            .map_err(|err| RequirementsTxtParserError::Url {
                source: err,
                url: given.to_string(),
                start,
                end: s.cursor(),
            })?;
        RequirementsTxtStatement::IndexUrl(url)
    } else if s.eat_if("--extra-index-url") {
        let given = parse_value(content, s, |c: char| !['\n', '\r', '#'].contains(&c))?;
        let expanded = expand_env_vars(given);
        let url = VerbatimUrl::parse_url(expanded.as_ref())
            .map(|url| url.with_given(given.to_owned()))
            .map_err(|err| RequirementsTxtParserError::Url {
                source: err,
                url: given.to_string(),
                start,
                end: s.cursor(),
            })?;
        RequirementsTxtStatement::ExtraIndexUrl(url)
    } else if s.eat_if("--no-index") {
        RequirementsTxtStatement::NoIndex
    } else if s.eat_if("--find-links") || s.eat_if("-f") {
        let path_or_url = parse_value(content, s, |c: char| !['\n', '\r', '#'].contains(&c))?;
        let path_or_url = FindLink::parse(path_or_url, working_dir).map_err(|err| {
            RequirementsTxtParserError::Url {
                source: err,
                url: path_or_url.to_string(),
                start,
                end: s.cursor(),
            }
        })?;
        RequirementsTxtStatement::FindLinks(path_or_url)
    } else if s.eat_if("--no-binary") {
        let given = parse_value(content, s, |c: char| !['\n', '\r', '#'].contains(&c))?;
        let specifier = PackageNameSpecifier::from_str(given).map_err(|err| {
            RequirementsTxtParserError::NoBinary {
                source: err,
                specifier: given.to_string(),
                start,
                end: s.cursor(),
            }
        })?;
        RequirementsTxtStatement::NoBinary(NoBinary::from_arg(specifier))
    } else if s.eat_if("--only-binary") {
        let given = parse_value(content, s, |c: char| !['\n', '\r', '#'].contains(&c))?;
        let specifier = PackageNameSpecifier::from_str(given).map_err(|err| {
            RequirementsTxtParserError::NoBinary {
                source: err,
                specifier: given.to_string(),
                start,
                end: s.cursor(),
            }
        })?;
        RequirementsTxtStatement::OnlyBinary(NoBuild::from_arg(specifier))
    } else if s.at(char::is_ascii_alphanumeric) || s.at(|char| matches!(char, '.' | '/' | '$')) {
        let (requirement, hashes) = parse_requirement_and_hashes(s, content, working_dir)?;
        RequirementsTxtStatement::RequirementEntry(RequirementEntry {
            requirement,
            hashes,
        })
    } else if let Some(char) = s.peek() {
        let (line, column) = calculate_row_column(content, s.cursor());
        return Err(RequirementsTxtParserError::Parser {
            message: format!(
                "Unexpected '{char}', expected '-c', '-e', '-r' or the start of a requirement"
            ),
            line,
            column,
        });
    } else {
        // EOF
        return Ok(None);
    }))
}

/// Eat whitespace and ignore newlines escaped with a backslash
fn eat_wrappable_whitespace<'a>(s: &mut Scanner<'a>) -> &'a str {
    let start = s.cursor();
    s.eat_while([' ', '\t']);
    // Allow multiple escaped line breaks
    // With the order we support `\n`, `\r`, `\r\n` without accidentally eating a `\n\r`
    while s.eat_if("\\\n") || s.eat_if("\\\r\n") || s.eat_if("\\\r") {
        s.eat_while([' ', '\t']);
    }
    s.from(start)
}

/// Eats the end of line or a potential trailing comma
fn eat_trailing_line(content: &str, s: &mut Scanner) -> Result<(), RequirementsTxtParserError> {
    s.eat_while([' ', '\t']);
    match s.eat() {
        None | Some('\n') => {} // End of file or end of line, nothing to do
        Some('\r') => {
            s.eat_if('\n'); // `\r\n`, but just `\r` is also accepted
        }
        Some('#') => {
            s.eat_until(['\r', '\n']);
            if s.at('\r') {
                s.eat_if('\n'); // `\r\n`, but just `\r` is also accepted
            }
        }
        Some(other) => {
            let (line, column) = calculate_row_column(content, s.cursor());
            return Err(RequirementsTxtParserError::Parser {
                message: format!("Expected comment or end-of-line, found '{other}'"),
                line,
                column,
            });
        }
    }
    Ok(())
}

/// Parse a PEP 508 requirement with optional trailing hashes
fn parse_requirement_and_hashes(
    s: &mut Scanner,
    content: &str,
    working_dir: &Path,
) -> Result<(RequirementsTxtRequirement, Vec<String>), RequirementsTxtParserError> {
    // PEP 508 requirement
    let start = s.cursor();
    // Termination: s.eat() eventually becomes None
    let (end, has_hashes) = loop {
        let end = s.cursor();

        //  We look for the end of the line ...
        if s.eat_if('\n') {
            break (end, false);
        }
        if s.eat_if('\r') {
            s.eat_if('\n'); // Support `\r\n` but also accept stray `\r`
            break (end, false);
        }
        // ... or `--hash`, an escaped newline or a comment separated by whitespace ...
        if !eat_wrappable_whitespace(s).is_empty() {
            if s.after().starts_with("--") {
                break (end, true);
            } else if s.eat_if('#') {
                s.eat_until(['\r', '\n']);
                if s.at('\r') {
                    s.eat_if('\n'); // `\r\n`, but just `\r` is also accepted
                }
                break (end, false);
            }
            continue;
        }
        // ... or the end of the file, which works like the end of line
        if s.eat().is_none() {
            break (end, false);
        }
    };

    let requirement = &content[start..end];

    // If the requirement looks like a `requirements.txt` file (with a missing `-r`), raise an
    // error.
    //
    // While `requirements.txt` is a valid package name (per the spec), PyPI disallows
    // `requirements.txt` and some other variants anyway.
    #[allow(clippy::case_sensitive_file_extension_comparisons)]
    if requirement.ends_with(".txt") || requirement.ends_with(".in") {
        let path = Path::new(requirement);
        let path = if path.is_absolute() {
            Cow::Borrowed(path)
        } else {
            Cow::Owned(working_dir.join(path))
        };
        if path.is_file() {
            return Err(RequirementsTxtParserError::MissingRequirementPrefix(
                requirement.to_string(),
            ));
        }
    }

    let requirement =
        RequirementsTxtRequirement::parse(requirement, working_dir).map_err(|err| match err {
            RequirementsTxtRequirementError::ParsedUrl(err) => {
                RequirementsTxtParserError::ParsedUrl {
                    source: err,
                    start,
                    end,
                }
            }
            RequirementsTxtRequirementError::Pep508(err) => match err.message {
                Pep508ErrorSource::String(_) | Pep508ErrorSource::UrlError(_) => {
                    RequirementsTxtParserError::Pep508 {
                        source: err,
                        start,
                        end,
                    }
                }
                Pep508ErrorSource::UnsupportedRequirement(_) => {
                    RequirementsTxtParserError::UnsupportedRequirement {
                        source: err,
                        start,
                        end,
                    }
                }
            },
        })?;

    let hashes = if has_hashes {
        parse_hashes(content, s)?
    } else {
        Vec::new()
    };
    Ok((requirement, hashes))
}

/// Parse `--hash=... --hash ...` after a requirement
fn parse_hashes(content: &str, s: &mut Scanner) -> Result<Vec<String>, RequirementsTxtParserError> {
    let mut hashes = Vec::new();
    if s.eat_while("--hash").is_empty() {
        let (line, column) = calculate_row_column(content, s.cursor());
        return Err(RequirementsTxtParserError::Parser {
            message: format!(
                "Expected '--hash', found '{:?}'",
                s.eat_while(|c: char| !c.is_whitespace())
            ),
            line,
            column,
        });
    }
    let hash = parse_value(content, s, |c: char| !c.is_whitespace())?;
    hashes.push(hash.to_string());
    loop {
        eat_wrappable_whitespace(s);
        if !s.eat_if("--hash") {
            break;
        }
        let hash = parse_value(content, s, |c: char| !c.is_whitespace())?;
        hashes.push(hash.to_string());
    }
    Ok(hashes)
}

/// In `-<key>=<value>` or `-<key> value`, this parses the part after the key
fn parse_value<'a, T>(
    content: &str,
    s: &mut Scanner<'a>,
    while_pattern: impl Pattern<T>,
) -> Result<&'a str, RequirementsTxtParserError> {
    if s.eat_if('=') {
        // Explicit equals sign
        Ok(s.eat_while(while_pattern).trim_end())
    } else if s.eat_if(char::is_whitespace) {
        // Key and value are separated by whitespace instead
        s.eat_whitespace();
        Ok(s.eat_while(while_pattern).trim_end())
    } else {
        let (line, column) = calculate_row_column(content, s.cursor());
        Err(RequirementsTxtParserError::Parser {
            message: format!("Expected '=' or whitespace, found {:?}", s.peek()),
            line,
            column,
        })
    }
}

/// Fetch the contents of a URL and return them as a string.
#[cfg(feature = "http")]
async fn read_url_to_string(
    path: impl AsRef<Path>,
    client: BaseClient,
) -> Result<String, RequirementsTxtParserError> {
    // pip would URL-encode the non-UTF-8 bytes of the string; we just don't support them.
    let path_utf8 =
        path.as_ref()
            .to_str()
            .ok_or_else(|| RequirementsTxtParserError::NonUnicodeUrl {
                url: path.as_ref().to_owned(),
            })?;

    Ok(client
        .client()
        .get(path_utf8)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?)
}

/// Error parsing requirements.txt, wrapper with filename
#[derive(Debug)]
pub struct RequirementsTxtFileError {
    file: PathBuf,
    error: RequirementsTxtParserError,
}

/// Error parsing requirements.txt, error disambiguation
#[derive(Debug)]
pub enum RequirementsTxtParserError {
    IO(io::Error),
    Url {
        source: url::ParseError,
        url: String,
        start: usize,
        end: usize,
    },
    InvalidEditablePath(String),
    UnsupportedUrl(String),
    MissingRequirementPrefix(String),
    NoBinary {
        source: uv_normalize::InvalidNameError,
        specifier: String,
        start: usize,
        end: usize,
    },
    OnlyBinary {
        source: uv_normalize::InvalidNameError,
        specifier: String,
        start: usize,
        end: usize,
    },
    UnnamedConstraint {
        start: usize,
        end: usize,
    },
    Parser {
        message: String,
        line: usize,
        column: usize,
    },
    UnsupportedRequirement {
        source: Pep508Error,
        start: usize,
        end: usize,
    },
    Pep508 {
        source: Pep508Error,
        start: usize,
        end: usize,
    },
    ParsedUrl {
        source: Box<ParsedUrlError>,
        start: usize,
        end: usize,
    },
    Subfile {
        source: Box<RequirementsTxtFileError>,
        start: usize,
        end: usize,
    },
    NonUnicodeUrl {
        url: PathBuf,
    },
    #[cfg(feature = "http")]
    Reqwest(reqwest_middleware::Error),
}

impl RequirementsTxtParserError {
    /// Add a fixed offset to the location of the error.
    #[must_use]
    fn with_offset(self, offset: usize) -> Self {
        match self {
            Self::IO(err) => Self::IO(err),
            Self::InvalidEditablePath(given) => Self::InvalidEditablePath(given),
            Self::Url {
                source,
                url,
                start,
                end,
            } => Self::Url {
                source,
                url,
                start: start + offset,
                end: end + offset,
            },
            Self::UnsupportedUrl(url) => Self::UnsupportedUrl(url),
            Self::MissingRequirementPrefix(given) => Self::MissingRequirementPrefix(given),
            Self::NoBinary {
                source,
                specifier,
                start,
                end,
            } => Self::NoBinary {
                source,
                specifier,
                start: start + offset,
                end: end + offset,
            },
            Self::OnlyBinary {
                source,
                specifier,
                start,
                end,
            } => Self::OnlyBinary {
                source,
                specifier,
                start: start + offset,
                end: end + offset,
            },
            Self::UnnamedConstraint { start, end } => Self::UnnamedConstraint {
                start: start + offset,
                end: end + offset,
            },
            Self::Parser {
                message,
                line,
                column,
            } => Self::Parser {
                message,
                line,
                column,
            },
            Self::UnsupportedRequirement { source, start, end } => Self::UnsupportedRequirement {
                source,
                start: start + offset,
                end: end + offset,
            },
            Self::Pep508 { source, start, end } => Self::Pep508 {
                source,
                start: start + offset,
                end: end + offset,
            },
            Self::ParsedUrl { source, start, end } => Self::ParsedUrl {
                source,
                start: start + offset,
                end: end + offset,
            },
            Self::Subfile { source, start, end } => Self::Subfile {
                source,
                start: start + offset,
                end: end + offset,
            },
            Self::NonUnicodeUrl { url } => Self::NonUnicodeUrl { url },
            #[cfg(feature = "http")]
            Self::Reqwest(err) => Self::Reqwest(err),
        }
    }
}

impl Display for RequirementsTxtParserError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IO(err) => err.fmt(f),
            Self::InvalidEditablePath(given) => {
                write!(f, "Invalid editable path: {given}")
            }
            Self::Url { url, start, .. } => {
                write!(f, "Invalid URL at position {start}: `{url}`")
            }
            Self::UnsupportedUrl(url) => {
                write!(f, "Unsupported URL (expected a `file://` scheme): `{url}`")
            }
            Self::MissingRequirementPrefix(given) => {
                write!(f, "Requirement `{given}` looks like a requirements file but was passed as a package name. Did you mean `-r {given}`?")
            }
            Self::NoBinary { specifier, .. } => {
                write!(f, "Invalid specifier for `--no-binary`: {specifier}")
            }
            Self::OnlyBinary { specifier, .. } => {
                write!(f, "Invalid specifier for `--only-binary`: {specifier}")
            }
            Self::UnnamedConstraint { .. } => {
                write!(f, "Unnamed requirements are not allowed as constraints")
            }
            Self::Parser {
                message,
                line,
                column,
            } => {
                write!(f, "{message} at {line}:{column}")
            }
            Self::UnsupportedRequirement { start, end, .. } => {
                write!(f, "Unsupported requirement in position {start} to {end}")
            }
            Self::Pep508 { start, .. } => {
                write!(f, "Couldn't parse requirement at position {start}")
            }
            Self::ParsedUrl { start, .. } => {
                write!(f, "Couldn't URL at position {start}")
            }
            Self::Subfile { start, .. } => {
                write!(f, "Error parsing included file at position {start}")
            }
            Self::NonUnicodeUrl { url } => {
                write!(
                    f,
                    "Remote requirements URL contains non-unicode characters: {}",
                    url.display(),
                )
            }
            #[cfg(feature = "http")]
            Self::Reqwest(err) => {
                write!(f, "Error while accessing remote requirements file {err}")
            }
        }
    }
}

impl std::error::Error for RequirementsTxtParserError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self {
            Self::IO(err) => err.source(),
            Self::Url { source, .. } => Some(source),
            Self::InvalidEditablePath(_) => None,
            Self::UnsupportedUrl(_) => None,
            Self::MissingRequirementPrefix(_) => None,
            Self::NoBinary { source, .. } => Some(source),
            Self::OnlyBinary { source, .. } => Some(source),
            Self::UnnamedConstraint { .. } => None,
            Self::UnsupportedRequirement { source, .. } => Some(source),
            Self::Pep508 { source, .. } => Some(source),
            Self::ParsedUrl { source, .. } => Some(source),
            Self::Subfile { source, .. } => Some(source.as_ref()),
            Self::Parser { .. } => None,
            Self::NonUnicodeUrl { .. } => None,
            #[cfg(feature = "http")]
            Self::Reqwest(err) => err.source(),
        }
    }
}

impl Display for RequirementsTxtFileError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match &self.error {
            RequirementsTxtParserError::IO(err) => err.fmt(f),
            RequirementsTxtParserError::Url { url, start, .. } => {
                write!(
                    f,
                    "Invalid URL in `{}` at position {start}: `{url}`",
                    self.file.user_display(),
                )
            }
            RequirementsTxtParserError::InvalidEditablePath(given) => {
                write!(
                    f,
                    "Invalid editable path in `{}`: {given}",
                    self.file.user_display()
                )
            }
            RequirementsTxtParserError::UnsupportedUrl(url) => {
                write!(
                    f,
                    "Unsupported URL (expected a `file://` scheme) in `{}`: `{url}`",
                    self.file.user_display(),
                )
            }
            RequirementsTxtParserError::MissingRequirementPrefix(given) => {
                write!(
                    f,
                    "Requirement `{given}` in `{}` looks like a requirements file but was passed as a package name. Did you mean `-r {given}`?",
                    self.file.user_display(),
                )
            }
            RequirementsTxtParserError::NoBinary { specifier, .. } => {
                write!(
                    f,
                    "Invalid specifier for `--no-binary` in `{}`: {specifier}",
                    self.file.user_display(),
                )
            }
            RequirementsTxtParserError::OnlyBinary { specifier, .. } => {
                write!(
                    f,
                    "Invalid specifier for `--only-binary` in `{}`: {specifier}",
                    self.file.user_display(),
                )
            }
            RequirementsTxtParserError::UnnamedConstraint { .. } => {
                write!(
                    f,
                    "Unnamed requirements are not allowed as constraints in `{}`",
                    self.file.user_display(),
                )
            }
            RequirementsTxtParserError::Parser {
                message,
                line,
                column,
            } => {
                write!(
                    f,
                    "{message} at {}:{line}:{column}",
                    self.file.user_display(),
                )
            }
            RequirementsTxtParserError::UnsupportedRequirement { start, .. } => {
                write!(
                    f,
                    "Unsupported requirement in {} at position {start}",
                    self.file.user_display(),
                )
            }
            RequirementsTxtParserError::Pep508 { start, .. } => {
                write!(
                    f,
                    "Couldn't parse requirement in `{}` at position {start}",
                    self.file.user_display(),
                )
            }
            RequirementsTxtParserError::ParsedUrl { start, .. } => {
                write!(
                    f,
                    "Couldn't parse URL in `{}` at position {start}",
                    self.file.user_display(),
                )
            }
            RequirementsTxtParserError::Subfile { start, .. } => {
                write!(
                    f,
                    "Error parsing included file in `{}` at position {start}",
                    self.file.user_display(),
                )
            }
            RequirementsTxtParserError::NonUnicodeUrl { url } => {
                write!(
                    f,
                    "Remote requirements URL contains non-unicode characters: {}",
                    url.display(),
                )
            }
            #[cfg(feature = "http")]
            RequirementsTxtParserError::Reqwest(err) => {
                write!(
                    f,
                    "Error while accessing remote requirements file {}: {err}",
                    self.file.user_display(),
                )
            }
        }
    }
}

impl std::error::Error for RequirementsTxtFileError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.error.source()
    }
}

impl From<io::Error> for RequirementsTxtParserError {
    fn from(err: io::Error) -> Self {
        Self::IO(err)
    }
}

#[cfg(feature = "http")]
impl From<reqwest::Error> for RequirementsTxtParserError {
    fn from(err: reqwest::Error) -> Self {
        Self::Reqwest(reqwest_middleware::Error::Reqwest(err))
    }
}

#[cfg(feature = "http")]
impl From<reqwest_middleware::Error> for RequirementsTxtParserError {
    fn from(err: reqwest_middleware::Error) -> Self {
        Self::Reqwest(err)
    }
}

/// Calculates the column and line offset of a given cursor based on the
/// number of Unicode codepoints.
fn calculate_row_column(content: &str, position: usize) -> (usize, usize) {
    let mut line = 1;
    let mut column = 1;

    let mut chars = content.char_indices().peekable();
    while let Some((index, char)) = chars.next() {
        if index >= position {
            break;
        }
        match char {
            '\r' => {
                // If the next character is a newline, skip it.
                if chars
                    .peek()
                    .map_or(false, |&(_, next_char)| next_char == '\n')
                {
                    chars.next();
                }

                // Reset.
                line += 1;
                column = 1;
            }
            '\n' => {
                //
                line += 1;
                column = 1;
            }
            // Increment column by Unicode codepoint. We don't use visual width
            // (e.g., `UnicodeWidthChar::width(char).unwrap_or(0)`), since that's
            // not what editors typically count.
            _ => column += 1,
        }
    }

    (line, column)
}

#[cfg(test)]
mod test {
    use std::path::{Path, PathBuf};

    use anyhow::Result;
    use assert_fs::prelude::*;
    use fs_err as fs;
    use indoc::indoc;
    use itertools::Itertools;
    use tempfile::tempdir;
    use test_case::test_case;
    use unscanny::Scanner;

    use uv_client::BaseClientBuilder;
    use uv_fs::Simplified;

    use crate::{calculate_row_column, EditableRequirement, RequirementsTxt};

    fn workspace_test_data_dir() -> PathBuf {
        PathBuf::from("./test-data").canonicalize().unwrap()
    }

    #[test_case(Path::new("basic.txt"))]
    #[test_case(Path::new("constraints-a.txt"))]
    #[test_case(Path::new("constraints-b.txt"))]
    #[test_case(Path::new("empty.txt"))]
    #[test_case(Path::new("for-poetry.txt"))]
    #[test_case(Path::new("include-a.txt"))]
    #[test_case(Path::new("include-b.txt"))]
    #[test_case(Path::new("poetry-with-hashes.txt"))]
    #[test_case(Path::new("small.txt"))]
    #[test_case(Path::new("whitespace.txt"))]
    #[tokio::test]
    async fn parse(path: &Path) {
        let working_dir = workspace_test_data_dir().join("requirements-txt");
        let requirements_txt = working_dir.join(path);

        let actual =
            RequirementsTxt::parse(requirements_txt, &working_dir, &BaseClientBuilder::new())
                .await
                .unwrap();

        let snapshot = format!("parse-{}", path.to_string_lossy());
        insta::assert_debug_snapshot!(snapshot, actual);
    }

    #[test_case(Path::new("basic.txt"))]
    #[test_case(Path::new("constraints-a.txt"))]
    #[test_case(Path::new("constraints-b.txt"))]
    #[test_case(Path::new("empty.txt"))]
    #[test_case(Path::new("for-poetry.txt"))]
    #[test_case(Path::new("include-a.txt"))]
    #[test_case(Path::new("include-b.txt"))]
    #[test_case(Path::new("poetry-with-hashes.txt"))]
    #[test_case(Path::new("small.txt"))]
    #[test_case(Path::new("whitespace.txt"))]
    #[test_case(Path::new("editable.txt"))]
    #[tokio::test]
    async fn line_endings(path: &Path) {
        let working_dir = workspace_test_data_dir().join("requirements-txt");
        let requirements_txt = working_dir.join(path);

        // Copy the existing files over to a temporary directory.
        let temp_dir = tempdir().unwrap();
        for entry in fs::read_dir(&working_dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            let dest = temp_dir.path().join(path.file_name().unwrap());
            fs::copy(&path, &dest).unwrap();
        }

        // Replace line endings with the other choice. This works even if you use git with LF
        // only on windows.
        let contents = fs::read_to_string(requirements_txt).unwrap();
        let contents = if contents.contains("\r\n") {
            contents.replace("\r\n", "\n")
        } else {
            contents.replace('\n', "\r\n")
        };
        let requirements_txt = temp_dir.path().join(path);
        fs::write(&requirements_txt, contents).unwrap();

        let actual =
            RequirementsTxt::parse(&requirements_txt, &working_dir, &BaseClientBuilder::new())
                .await
                .unwrap();

        let snapshot = format!("line-endings-{}", path.to_string_lossy());
        insta::assert_debug_snapshot!(snapshot, actual);
    }

    #[cfg(unix)]
    #[test_case(Path::new("bare-url.txt"))]
    #[tokio::test]
    async fn parse_unnamed_unix(path: &Path) {
        let working_dir = workspace_test_data_dir().join("requirements-txt");
        let requirements_txt = working_dir.join(path);

        let actual =
            RequirementsTxt::parse(requirements_txt, &working_dir, &BaseClientBuilder::new())
                .await
                .unwrap();

        let snapshot = format!("parse-unix-{}", path.to_string_lossy());
        let pattern = regex::escape(&working_dir.simplified_display().to_string());
        let filters = vec![(pattern.as_str(), "[WORKSPACE_DIR]")];
        insta::with_settings!({
            filters => filters
        }, {
            insta::assert_debug_snapshot!(snapshot, actual);
        });
    }

    #[cfg(windows)]
    #[test_case(Path::new("bare-url.txt"))]
    #[tokio::test]
    async fn parse_unnamed_windows(path: &Path) {
        let working_dir = workspace_test_data_dir().join("requirements-txt");
        let requirements_txt = working_dir.join(path);

        let actual =
            RequirementsTxt::parse(requirements_txt, &working_dir, &BaseClientBuilder::new())
                .await
                .unwrap();

        let snapshot = format!("parse-windows-{}", path.to_string_lossy());
        let pattern = regex::escape(
            &working_dir
                .simplified_display()
                .to_string()
                .replace('\\', "/"),
        );
        let filters = vec![(pattern.as_str(), "[WORKSPACE_DIR]")];
        insta::with_settings!({
            filters => filters
        }, {
            insta::assert_debug_snapshot!(snapshot, actual);
        });
    }

    #[tokio::test]
    async fn invalid_include_missing_file() -> Result<()> {
        let temp_dir = assert_fs::TempDir::new()?;
        let missing_txt = temp_dir.child("missing.txt");
        let requirements_txt = temp_dir.child("requirements.txt");
        requirements_txt.write_str(indoc! {"
            -r missing.txt
        "})?;

        let error = RequirementsTxt::parse(
            requirements_txt.path(),
            temp_dir.path(),
            &BaseClientBuilder::new(),
        )
        .await
        .unwrap_err();
        let errors = anyhow::Error::new(error)
            .chain()
            // The last error is operating-system specific.
            .take(2)
            .join("\n");

        let requirement_txt = regex::escape(&requirements_txt.path().user_display().to_string());
        let missing_txt = regex::escape(&missing_txt.path().user_display().to_string());
        let filters = vec![
            (requirement_txt.as_str(), "<REQUIREMENTS_TXT>"),
            (missing_txt.as_str(), "<MISSING_TXT>"),
            (r"\\", "/"),
        ];
        insta::with_settings!({
            filters => filters,
        }, {
            insta::assert_snapshot!(errors, @r###"
            Error parsing included file in `<REQUIREMENTS_TXT>` at position 0
            failed to read from file `<MISSING_TXT>`
            "###);
        });

        Ok(())
    }

    #[tokio::test]
    async fn invalid_requirement_version() -> Result<()> {
        let temp_dir = assert_fs::TempDir::new()?;
        let requirements_txt = temp_dir.child("requirements.txt");
        requirements_txt.write_str(indoc! {"
            numpy[ö]==1.29
        "})?;

        let error = RequirementsTxt::parse(
            requirements_txt.path(),
            temp_dir.path(),
            &BaseClientBuilder::new(),
        )
        .await
        .unwrap_err();
        let errors = anyhow::Error::new(error).chain().join("\n");

        let requirement_txt = regex::escape(&requirements_txt.path().user_display().to_string());
        let filters = vec![
            (requirement_txt.as_str(), "<REQUIREMENTS_TXT>"),
            (r"\\", "/"),
        ];
        insta::with_settings!({
            filters => filters
        }, {
            insta::assert_snapshot!(errors, @r###"
            Couldn't parse requirement in `<REQUIREMENTS_TXT>` at position 0
            Expected an alphanumeric character starting the extra name, found 'ö'
            numpy[ö]==1.29
                  ^
            "###);
        });

        Ok(())
    }

    #[tokio::test]
    async fn invalid_requirement_url() -> Result<()> {
        let temp_dir = assert_fs::TempDir::new()?;
        let requirements_txt = temp_dir.child("requirements.txt");
        requirements_txt.write_str(indoc! {"
            numpy @ https:///
        "})?;

        let error = RequirementsTxt::parse(
            requirements_txt.path(),
            temp_dir.path(),
            &BaseClientBuilder::new(),
        )
        .await
        .unwrap_err();
        let errors = anyhow::Error::new(error).chain().join("\n");

        let requirement_txt = regex::escape(&requirements_txt.path().user_display().to_string());
        let filters = vec![
            (requirement_txt.as_str(), "<REQUIREMENTS_TXT>"),
            (r"\\", "/"),
        ];
        insta::with_settings!({
            filters => filters
        }, {
            insta::assert_snapshot!(errors, @r###"
            Couldn't parse requirement in `<REQUIREMENTS_TXT>` at position 0
            empty host
            numpy @ https:///
                    ^^^^^^^^^
            "###);
        });

        Ok(())
    }

    #[tokio::test]
    async fn unsupported_editable() -> Result<()> {
        let temp_dir = assert_fs::TempDir::new()?;
        let requirements_txt = temp_dir.child("requirements.txt");
        requirements_txt.write_str(indoc! {"
            -e http://localhost:8080/
        "})?;

        let error = RequirementsTxt::parse(
            requirements_txt.path(),
            temp_dir.path(),
            &BaseClientBuilder::new(),
        )
        .await
        .unwrap_err();
        let errors = anyhow::Error::new(error).chain().join("\n");

        let requirement_txt = regex::escape(&requirements_txt.path().user_display().to_string());
        let filters = vec![
            (requirement_txt.as_str(), "<REQUIREMENTS_TXT>"),
            (r"\\", "/"),
        ];
        insta::with_settings!({
            filters => filters
        }, {
            insta::assert_snapshot!(errors, @"Unsupported URL (expected a `file://` scheme) in `<REQUIREMENTS_TXT>`: `http://localhost:8080/`");
        });

        Ok(())
    }

    #[tokio::test]
    async fn invalid_editable_extra() -> Result<()> {
        let temp_dir = assert_fs::TempDir::new()?;
        let requirements_txt = temp_dir.child("requirements.txt");
        requirements_txt.write_str(indoc! {"
            -e black[,abcdef]
        "})?;

        let error = RequirementsTxt::parse(
            requirements_txt.path(),
            temp_dir.path(),
            &BaseClientBuilder::new(),
        )
        .await
        .unwrap_err();
        let errors = anyhow::Error::new(error).chain().join("\n");

        let requirement_txt = regex::escape(&requirements_txt.path().user_display().to_string());
        let filters = vec![(requirement_txt.as_str(), "<REQUIREMENTS_TXT>")];
        insta::with_settings!({
            filters => filters
        }, {
            insta::assert_snapshot!(errors, @r###"
            Couldn't parse requirement in `<REQUIREMENTS_TXT>` at position 6
            Expected either alphanumerical character (starting the extra name) or ']' (ending the extras section), found ','
            black[,abcdef]
                  ^
            "###);
        });

        Ok(())
    }

    #[tokio::test]
    async fn relative_index_url() -> Result<()> {
        let temp_dir = assert_fs::TempDir::new()?;
        let requirements_txt = temp_dir.child("requirements.txt");
        requirements_txt.write_str(indoc! {"
            --index-url 123
        "})?;

        let error = RequirementsTxt::parse(
            requirements_txt.path(),
            temp_dir.path(),
            &BaseClientBuilder::new(),
        )
        .await
        .unwrap_err();
        let errors = anyhow::Error::new(error).chain().join("\n");

        let requirement_txt = regex::escape(&requirements_txt.path().user_display().to_string());
        let filters = vec![
            (requirement_txt.as_str(), "<REQUIREMENTS_TXT>"),
            (r"\\", "/"),
        ];
        insta::with_settings!({
            filters => filters
        }, {
            insta::assert_snapshot!(errors, @r###"
            Invalid URL in `<REQUIREMENTS_TXT>` at position 0: `123`
            relative URL without a base
            "###);
        });

        Ok(())
    }

    #[tokio::test]
    async fn invalid_index_url() -> Result<()> {
        let temp_dir = assert_fs::TempDir::new()?;
        let requirements_txt = temp_dir.child("requirements.txt");
        requirements_txt.write_str(indoc! {"
            --index-url https:////
        "})?;

        let error = RequirementsTxt::parse(
            requirements_txt.path(),
            temp_dir.path(),
            &BaseClientBuilder::new(),
        )
        .await
        .unwrap_err();
        let errors = anyhow::Error::new(error).chain().join("\n");

        let requirement_txt = regex::escape(&requirements_txt.path().user_display().to_string());
        let filters = vec![
            (requirement_txt.as_str(), "<REQUIREMENTS_TXT>"),
            (r"\\", "/"),
        ];
        insta::with_settings!({
            filters => filters
        }, {
            insta::assert_snapshot!(errors, @r###"
            Invalid URL in `<REQUIREMENTS_TXT>` at position 0: `https:////`
            empty host
            "###);
        });

        Ok(())
    }

    #[tokio::test]
    async fn missing_r() -> Result<()> {
        let temp_dir = assert_fs::TempDir::new()?;

        let file_txt = temp_dir.child("file.txt");
        file_txt.touch()?;

        let requirements_txt = temp_dir.child("requirements.txt");
        requirements_txt.write_str(indoc! {"
            flask
            file.txt
        "})?;

        let error = RequirementsTxt::parse(
            requirements_txt.path(),
            temp_dir.path(),
            &BaseClientBuilder::new(),
        )
        .await
        .unwrap_err();
        let errors = anyhow::Error::new(error).chain().join("\n");

        let requirement_txt = regex::escape(&requirements_txt.path().user_display().to_string());
        let filters = vec![
            (requirement_txt.as_str(), "<REQUIREMENTS_TXT>"),
            (r"\\", "/"),
        ];
        insta::with_settings!({
            filters => filters
        }, {
            insta::assert_snapshot!(errors, @"Requirement `file.txt` in `<REQUIREMENTS_TXT>` looks like a requirements file but was passed as a package name. Did you mean `-r file.txt`?");
        });

        Ok(())
    }

    #[tokio::test]
    async fn relative_requirement() -> Result<()> {
        let temp_dir = assert_fs::TempDir::new()?;

        // Create a requirements file with a relative entry, in a subdirectory.
        let sub_dir = temp_dir.child("subdir");

        let sibling_txt = sub_dir.child("sibling.txt");
        sibling_txt.write_str(indoc! {"
            flask
        "})?;

        let child_txt = sub_dir.child("child.txt");
        child_txt.write_str(indoc! {"
            -r sibling.txt
        "})?;

        // Create a requirements file that points at `requirements.txt`.
        let parent_txt = temp_dir.child("parent.txt");
        parent_txt.write_str(indoc! {"
            -r subdir/child.txt
        "})?;

        let requirements = RequirementsTxt::parse(
            parent_txt.path(),
            temp_dir.path(),
            &BaseClientBuilder::new(),
        )
        .await
        .unwrap();
        insta::assert_debug_snapshot!(requirements, @r###"
        RequirementsTxt {
            requirements: [
                RequirementEntry {
                    requirement: Uv(
                        UvRequirement {
                            name: PackageName(
                                "flask",
                            ),
                            extras: [],
                            marker: None,
                            source: Registry {
                                version: VersionSpecifiers(
                                    [],
                                ),
                                index: None,
                            },
                        },
                    ),
                    hashes: [],
                },
            ],
            constraints: [],
            editables: [],
            index_url: None,
            extra_index_urls: [],
            find_links: [],
            no_index: false,
            no_binary: None,
            only_binary: None,
        }
        "###);

        Ok(())
    }

    #[tokio::test]
    async fn nested_no_binary() -> Result<()> {
        let temp_dir = assert_fs::TempDir::new()?;

        let requirements_txt = temp_dir.child("requirements.txt");
        requirements_txt.write_str(indoc! {"
            flask
            --no-binary :none:
            -r child.txt
        "})?;

        let child = temp_dir.child("child.txt");
        child.write_str(indoc! {"
            --no-binary flask
        "})?;

        let requirements = RequirementsTxt::parse(
            requirements_txt.path(),
            temp_dir.path(),
            &BaseClientBuilder::new(),
        )
        .await
        .unwrap();
        insta::assert_debug_snapshot!(requirements, @r###"
        RequirementsTxt {
            requirements: [
                RequirementEntry {
                    requirement: Uv(
                        UvRequirement {
                            name: PackageName(
                                "flask",
                            ),
                            extras: [],
                            marker: None,
                            source: Registry {
                                version: VersionSpecifiers(
                                    [],
                                ),
                                index: None,
                            },
                        },
                    ),
                    hashes: [],
                },
            ],
            constraints: [],
            editables: [],
            index_url: None,
            extra_index_urls: [],
            find_links: [],
            no_index: false,
            no_binary: Packages(
                [
                    PackageName(
                        "flask",
                    ),
                ],
            ),
            only_binary: None,
        }
        "###);

        Ok(())
    }

    #[tokio::test]
    #[cfg(not(windows))]
    async fn nested_editable() -> Result<()> {
        let temp_dir = assert_fs::TempDir::new()?;

        let requirements_txt = temp_dir.child("requirements.txt");
        requirements_txt.write_str(indoc! {"
            -r child.txt
        "})?;

        let child = temp_dir.child("child.txt");
        child.write_str(indoc! {"
            -r grandchild.txt
        "})?;

        let grandchild = temp_dir.child("grandchild.txt");
        grandchild.write_str(indoc! {"
            -e /foo/bar
            --no-index
        "})?;

        let requirements = RequirementsTxt::parse(
            requirements_txt.path(),
            temp_dir.path(),
            &BaseClientBuilder::new(),
        )
        .await
        .unwrap();

        insta::assert_debug_snapshot!(requirements, @r###"
        RequirementsTxt {
            requirements: [],
            constraints: [],
            editables: [
                EditableRequirement {
                    url: VerbatimUrl {
                        url: Url {
                            scheme: "file",
                            cannot_be_a_base: false,
                            username: "",
                            password: None,
                            host: None,
                            port: None,
                            path: "/foo/bar",
                            query: None,
                            fragment: None,
                        },
                        given: Some(
                            "/foo/bar",
                        ),
                    },
                    extras: [],
                    path: "/foo/bar",
                },
            ],
            index_url: None,
            extra_index_urls: [],
            find_links: [],
            no_index: true,
            no_binary: None,
            only_binary: None,
        }
        "###);

        Ok(())
    }

    #[tokio::test]
    async fn nested_conflicting_index_url() -> Result<()> {
        let temp_dir = assert_fs::TempDir::new()?;

        let requirements_txt = temp_dir.child("requirements.txt");
        requirements_txt.write_str(indoc! {"
            --index-url https://test.pypi.org/simple
            -r child.txt
        "})?;

        let child = temp_dir.child("child.txt");
        child.write_str(indoc! {"
            -r grandchild.txt
        "})?;

        let grandchild = temp_dir.child("grandchild.txt");
        grandchild.write_str(indoc! {"
            --index-url https://fake.pypi.org/simple
        "})?;

        let error = RequirementsTxt::parse(
            requirements_txt.path(),
            temp_dir.path(),
            &BaseClientBuilder::new(),
        )
        .await
        .unwrap_err();
        let errors = anyhow::Error::new(error).chain().join("\n");

        let requirement_txt = regex::escape(&requirements_txt.path().user_display().to_string());
        let filters = vec![
            (requirement_txt.as_str(), "<REQUIREMENTS_TXT>"),
            (r"\\", "/"),
        ];
        insta::with_settings!({
            filters => filters
        }, {
            insta::assert_snapshot!(errors, @"Nested `requirements` file contains conflicting `--index-url` at <REQUIREMENTS_TXT>:2:13");
        });

        Ok(())
    }

    #[tokio::test]
    async fn comments() -> Result<()> {
        let temp_dir = assert_fs::TempDir::new()?;

        let requirements_txt = temp_dir.child("requirements.txt");
        requirements_txt.write_str(indoc! {r"
            -r ./sibling.txt  # comment
            --index-url https://test.pypi.org/simple/  # comment
            --no-binary :all:  # comment

            flask==3.0.0 \
                --hash=sha256:1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef \
                # comment

            requests==2.26.0 \
                --hash=sha256:fedcba0987654321fedcba0987654321fedcba0987654321fedcba0987654321 # comment

            black==21.12b0 # comment

            mypy==0.910 \
              # comment
        "})?;

        let sibling_txt = temp_dir.child("sibling.txt");
        sibling_txt.write_str(indoc! {"
            httpx # comment
        "})?;

        let requirements = RequirementsTxt::parse(
            requirements_txt.path(),
            temp_dir.path(),
            &BaseClientBuilder::new(),
        )
        .await
        .unwrap();
        insta::assert_debug_snapshot!(requirements, @r###"
        RequirementsTxt {
            requirements: [
                RequirementEntry {
                    requirement: Uv(
                        UvRequirement {
                            name: PackageName(
                                "httpx",
                            ),
                            extras: [],
                            marker: None,
                            source: Registry {
                                version: VersionSpecifiers(
                                    [],
                                ),
                                index: None,
                            },
                        },
                    ),
                    hashes: [],
                },
                RequirementEntry {
                    requirement: Uv(
                        UvRequirement {
                            name: PackageName(
                                "flask",
                            ),
                            extras: [],
                            marker: None,
                            source: Registry {
                                version: VersionSpecifiers(
                                    [
                                        VersionSpecifier {
                                            operator: Equal,
                                            version: "3.0.0",
                                        },
                                    ],
                                ),
                                index: None,
                            },
                        },
                    ),
                    hashes: [
                        "sha256:1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
                    ],
                },
                RequirementEntry {
                    requirement: Uv(
                        UvRequirement {
                            name: PackageName(
                                "requests",
                            ),
                            extras: [],
                            marker: None,
                            source: Registry {
                                version: VersionSpecifiers(
                                    [
                                        VersionSpecifier {
                                            operator: Equal,
                                            version: "2.26.0",
                                        },
                                    ],
                                ),
                                index: None,
                            },
                        },
                    ),
                    hashes: [
                        "sha256:fedcba0987654321fedcba0987654321fedcba0987654321fedcba0987654321",
                    ],
                },
                RequirementEntry {
                    requirement: Uv(
                        UvRequirement {
                            name: PackageName(
                                "black",
                            ),
                            extras: [],
                            marker: None,
                            source: Registry {
                                version: VersionSpecifiers(
                                    [
                                        VersionSpecifier {
                                            operator: Equal,
                                            version: "21.12b0",
                                        },
                                    ],
                                ),
                                index: None,
                            },
                        },
                    ),
                    hashes: [],
                },
                RequirementEntry {
                    requirement: Uv(
                        UvRequirement {
                            name: PackageName(
                                "mypy",
                            ),
                            extras: [],
                            marker: None,
                            source: Registry {
                                version: VersionSpecifiers(
                                    [
                                        VersionSpecifier {
                                            operator: Equal,
                                            version: "0.910",
                                        },
                                    ],
                                ),
                                index: None,
                            },
                        },
                    ),
                    hashes: [],
                },
            ],
            constraints: [],
            editables: [],
            index_url: Some(
                VerbatimUrl {
                    url: Url {
                        scheme: "https",
                        cannot_be_a_base: false,
                        username: "",
                        password: None,
                        host: Some(
                            Domain(
                                "test.pypi.org",
                            ),
                        ),
                        port: None,
                        path: "/simple/",
                        query: None,
                        fragment: None,
                    },
                    given: Some(
                        "https://test.pypi.org/simple/",
                    ),
                },
            ),
            extra_index_urls: [],
            find_links: [],
            no_index: false,
            no_binary: All,
            only_binary: None,
        }
        "###);

        Ok(())
    }

    #[test]
    fn editable_extra() {
        assert_eq!(
            EditableRequirement::split_extras("../editable[dev]"),
            Some(("../editable", "[dev]"))
        );
        assert_eq!(
            EditableRequirement::split_extras("../editable[dev]more[extra]"),
            Some(("../editable[dev]more", "[extra]"))
        );
        assert_eq!(
            EditableRequirement::split_extras("../editable[[dev]]"),
            None
        );
        assert_eq!(
            EditableRequirement::split_extras("../editable[[dev]"),
            Some(("../editable[", "[dev]"))
        );
    }

    #[tokio::test]
    async fn parser_error_line_and_column() -> Result<()> {
        let temp_dir = assert_fs::TempDir::new()?;
        let requirements_txt = temp_dir.child("requirements.txt");
        requirements_txt.write_str(indoc! {"
            numpy>=1,<2
              --borken
            tqdm
        "})?;

        let error = RequirementsTxt::parse(
            requirements_txt.path(),
            temp_dir.path(),
            &BaseClientBuilder::new(),
        )
        .await
        .unwrap_err();
        let errors = anyhow::Error::new(error).chain().join("\n");

        let requirement_txt = regex::escape(&requirements_txt.path().user_display().to_string());
        let filters = vec![
            (requirement_txt.as_str(), "<REQUIREMENTS_TXT>"),
            (r"\\", "/"),
        ];
        insta::with_settings!({
            filters => filters
        }, {
            insta::assert_snapshot!(errors, @r###"
            Unexpected '-', expected '-c', '-e', '-r' or the start of a requirement at <REQUIREMENTS_TXT>:2:3
            "###);
        });

        Ok(())
    }

    #[test_case("numpy>=1,<2\n  @-borken\ntqdm", "2:4"; "ASCII Character with LF")]
    #[test_case("numpy>=1,<2\r\n  #-borken\ntqdm", "2:4"; "ASCII Character with CRLF")]
    #[test_case("numpy>=1,<2\n  \n-borken\ntqdm", "3:1"; "ASCII Character LF then LF")]
    #[test_case("numpy>=1,<2\n  \r-borken\ntqdm", "3:1"; "ASCII Character LF then CR but no LF")]
    #[test_case("numpy>=1,<2\n  \r\n-borken\ntqdm", "3:1"; "ASCII Character LF then CRLF")]
    #[test_case("numpy>=1,<2\n  🚀-borken\ntqdm", "2:4"; "Emoji (Wide) Character")]
    #[test_case("numpy>=1,<2\n  中-borken\ntqdm", "2:4"; "Fullwidth character")]
    #[test_case("numpy>=1,<2\n  e\u{0301}-borken\ntqdm", "2:5"; "Two codepoints")]
    #[test_case("numpy>=1,<2\n  a\u{0300}\u{0316}-borken\ntqdm", "2:6"; "Three codepoints")]
    fn test_calculate_line_column_pair(input: &str, expected: &str) {
        let mut s = Scanner::new(input);
        // Place cursor right after the character we want to test
        s.eat_until('-');

        // Compute line/column
        let (line, column) = calculate_row_column(input, s.cursor());
        let line_column = format!("{line}:{column}");

        // Assert line and columns are expected
        assert_eq!(line_column, expected, "Issues with input: {input}");
    }
}
