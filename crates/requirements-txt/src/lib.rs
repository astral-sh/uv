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

use distribution_types::{UnresolvedRequirement, UnresolvedRequirementSpecification};
use pep508_rs::{expand_env_vars, Pep508Error, RequirementOrigin, VerbatimUrl};
use pypi_types::{Requirement, VerbatimParsedUrl};
#[cfg(feature = "http")]
use uv_client::BaseClient;
use uv_client::BaseClientBuilder;
use uv_configuration::{NoBinary, NoBuild, PackageNameSpecifier};
use uv_fs::Simplified;
use uv_warnings::warn_user;

use crate::requirement::EditableError;
pub use crate::requirement::RequirementsTxtRequirement;

mod requirement;

/// We emit one of those for each `requirements.txt` entry.
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
    EditableRequirementEntry(RequirementEntry),
    /// `--index-url`
    IndexUrl(VerbatimUrl),
    /// `--extra-index-url`
    ExtraIndexUrl(VerbatimUrl),
    /// `--find-links`
    FindLinks(VerbatimUrl),
    /// `--no-index`
    NoIndex,
    /// `--no-binary`
    NoBinary(NoBinary),
    /// `--only-binary`
    OnlyBinary(NoBuild),
}

/// A [Requirement] with additional metadata from the `requirements.txt`, currently only hashes but in
/// the future also editable and similar information.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct RequirementEntry {
    /// The actual PEP 508 requirement.
    pub requirement: RequirementsTxtRequirement,
    /// Hashes of the downloadable packages.
    pub hashes: Vec<String>,
}

// We place the impl here instead of next to `UnresolvedRequirementSpecification` because
// `UnresolvedRequirementSpecification` is defined in `distribution-types` and `requirements-txt`
// depends on `distribution-types`.
impl From<RequirementEntry> for UnresolvedRequirementSpecification {
    fn from(value: RequirementEntry) -> Self {
        Self {
            requirement: match value.requirement {
                RequirementsTxtRequirement::Named(named) => {
                    UnresolvedRequirement::Named(Requirement::from(named))
                }
                RequirementsTxtRequirement::Unnamed(unnamed) => {
                    UnresolvedRequirement::Unnamed(unnamed)
                }
            },
            hashes: value.hashes,
        }
    }
}

impl From<RequirementsTxtRequirement> for UnresolvedRequirementSpecification {
    fn from(value: RequirementsTxtRequirement) -> Self {
        Self::from(RequirementEntry {
            requirement: value,
            hashes: vec![],
        })
    }
}

/// Parsed and flattened requirements.txt with requirements and constraints
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct RequirementsTxt {
    /// The actual requirements with the hashes.
    pub requirements: Vec<RequirementEntry>,
    /// Constraints included with `-c`.
    pub constraints: Vec<pep508_rs::Requirement<VerbatimParsedUrl>>,
    /// Editables with `-e`.
    pub editables: Vec<RequirementEntry>,
    /// The index URL, specified with `--index-url`.
    pub index_url: Option<VerbatimUrl>,
    /// The extra index URLs, specified with `--extra-index-url`.
    pub extra_index_urls: Vec<VerbatimUrl>,
    /// The find links locations, specified with `--find-links`.
    pub find_links: Vec<VerbatimUrl>,
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
                        error: RequirementsTxtParserError::Io(io::Error::new(
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
                            error: RequirementsTxtParserError::Io(io::Error::new(
                                io::ErrorKind::InvalidInput,
                                format!("Network connectivity is disabled, but a remote requirements file was requested: {}", requirements_txt.display()),
                            )),
                        });
                    }

                    let client = client_builder.build();
                    read_url_to_string(&requirements_txt, client).await
                }
            } else {
                // Ex) `file:///home/ferris/project/requirements.txt`
                uv_fs::read_to_string_transcode(&requirements_txt)
                    .await
                    .map_err(RequirementsTxtParserError::Io)
            }
            .map_err(|err| RequirementsTxtFileError {
                file: requirements_txt.to_path_buf(),
                error: err,
            })?;

        let requirements_dir = requirements_txt.parent().unwrap_or(working_dir);
        let data = Self::parse_inner(
            &content,
            working_dir,
            requirements_dir,
            client_builder,
            requirements_txt,
        )
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
        requirements_txt: &Path,
    ) -> Result<Self, RequirementsTxtParserError> {
        let mut s = Scanner::new(content);

        let mut data = Self::default();
        while let Some(statement) = parse_entry(&mut s, content, working_dir, requirements_txt)? {
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
                        } else if filename.starts_with("file://") {
                            requirements_txt.join(
                                Url::parse(filename.as_ref())
                                    .map_err(|err| RequirementsTxtParserError::Url {
                                        source: err,
                                        url: filename.to_string(),
                                        start,
                                        end,
                                    })?
                                    .to_file_path()
                                    .map_err(|()| RequirementsTxtParserError::FileUrl {
                                        url: filename.to_string(),
                                        start,
                                        end,
                                    })?,
                            )
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
                        } else if filename.starts_with("file://") {
                            requirements_txt.join(
                                Url::parse(filename.as_ref())
                                    .map_err(|err| RequirementsTxtParserError::Url {
                                        source: err,
                                        url: filename.to_string(),
                                        start,
                                        end,
                                    })?
                                    .to_file_path()
                                    .map_err(|()| RequirementsTxtParserError::FileUrl {
                                        url: filename.to_string(),
                                        start,
                                        end,
                                    })?,
                            )
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
                            RequirementsTxtRequirement::Named(requirement) => {
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
                    for constraint in sub_constraints.constraints {
                        data.constraints.push(constraint);
                    }
                }
                RequirementsTxtStatement::RequirementEntry(requirement_entry) => {
                    data.requirements.push(requirement_entry);
                }
                RequirementsTxtStatement::EditableRequirementEntry(editable) => {
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
                RequirementsTxtStatement::FindLinks(url) => {
                    data.find_links.push(url);
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
    requirements_txt: &Path,
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
        s.eat_whitespace();

        let source = if requirements_txt == Path::new("-") {
            None
        } else {
            Some(requirements_txt)
        };

        let (requirement, hashes) =
            parse_requirement_and_hashes(s, content, source, working_dir, true)?;
        let requirement =
            requirement
                .into_editable()
                .map_err(|err| RequirementsTxtParserError::NonEditable {
                    source: err,
                    start,
                    end: s.cursor(),
                })?;
        RequirementsTxtStatement::EditableRequirementEntry(RequirementEntry {
            requirement,
            hashes,
        })
    } else if s.eat_if("-i") || s.eat_if("--index-url") {
        let given = parse_value(content, s, |c: char| !['\n', '\r', '#'].contains(&c))?;
        let expanded = expand_env_vars(given);
        let url = if let Ok(path) = Path::new(expanded.as_ref()).canonicalize() {
            VerbatimUrl::from_path(path).map_err(|err| RequirementsTxtParserError::VerbatimUrl {
                source: err,
                url: given.to_string(),
                start,
                end: s.cursor(),
            })?
        } else {
            VerbatimUrl::parse_url(expanded.as_ref()).map_err(|err| {
                RequirementsTxtParserError::Url {
                    source: err,
                    url: given.to_string(),
                    start,
                    end: s.cursor(),
                }
            })?
        };
        RequirementsTxtStatement::IndexUrl(url.with_given(given))
    } else if s.eat_if("--extra-index-url") {
        let given = parse_value(content, s, |c: char| !['\n', '\r', '#'].contains(&c))?;
        let expanded = expand_env_vars(given);
        let url = if let Ok(path) = Path::new(expanded.as_ref()).canonicalize() {
            VerbatimUrl::from_path(path).map_err(|err| RequirementsTxtParserError::VerbatimUrl {
                source: err,
                url: given.to_string(),
                start,
                end: s.cursor(),
            })?
        } else {
            VerbatimUrl::parse_url(expanded.as_ref()).map_err(|err| {
                RequirementsTxtParserError::Url {
                    source: err,
                    url: given.to_string(),
                    start,
                    end: s.cursor(),
                }
            })?
        };
        RequirementsTxtStatement::ExtraIndexUrl(url.with_given(given))
    } else if s.eat_if("--no-index") {
        RequirementsTxtStatement::NoIndex
    } else if s.eat_if("--find-links") || s.eat_if("-f") {
        let given = parse_value(content, s, |c: char| !['\n', '\r', '#'].contains(&c))?;
        let expanded = expand_env_vars(given);
        let url = if let Ok(path) = Path::new(expanded.as_ref()).canonicalize() {
            VerbatimUrl::from_path(path).map_err(|err| RequirementsTxtParserError::VerbatimUrl {
                source: err,
                url: given.to_string(),
                start,
                end: s.cursor(),
            })?
        } else {
            VerbatimUrl::parse_url(expanded.as_ref()).map_err(|err| {
                RequirementsTxtParserError::Url {
                    source: err,
                    url: given.to_string(),
                    start,
                    end: s.cursor(),
                }
            })?
        };
        RequirementsTxtStatement::FindLinks(url.with_given(given))
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
        RequirementsTxtStatement::NoBinary(NoBinary::from_pip_arg(specifier))
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
        RequirementsTxtStatement::OnlyBinary(NoBuild::from_pip_arg(specifier))
    } else if s.at(char::is_ascii_alphanumeric) || s.at(|char| matches!(char, '.' | '/' | '$')) {
        let source = if requirements_txt == Path::new("-") {
            None
        } else {
            Some(requirements_txt)
        };

        let (requirement, hashes) =
            parse_requirement_and_hashes(s, content, source, working_dir, false)?;
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
    source: Option<&Path>,
    working_dir: &Path,
    editable: bool,
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

    let requirement = RequirementsTxtRequirement::parse(requirement, working_dir, editable)
        .map(|requirement| {
            if let Some(source) = source {
                requirement.with_origin(RequirementOrigin::File(source.to_path_buf()))
            } else {
                requirement
            }
        })
        .map_err(|err| RequirementsTxtParserError::Pep508 {
            source: err,
            start,
            end,
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
    Io(io::Error),
    Url {
        source: url::ParseError,
        url: String,
        start: usize,
        end: usize,
    },
    FileUrl {
        url: String,
        start: usize,
        end: usize,
    },
    VerbatimUrl {
        source: pep508_rs::VerbatimUrlError,
        url: String,
        start: usize,
        end: usize,
    },
    UrlConversion(String),
    UnsupportedUrl(String),
    MissingRequirementPrefix(String),
    NonEditable {
        source: EditableError,
        start: usize,
        end: usize,
    },
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
        source: Box<Pep508Error<VerbatimParsedUrl>>,
        start: usize,
        end: usize,
    },
    Pep508 {
        source: Box<Pep508Error<VerbatimParsedUrl>>,
        start: usize,
        end: usize,
    },
    ParsedUrl {
        source: Box<Pep508Error<VerbatimParsedUrl>>,
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

impl Display for RequirementsTxtParserError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(err) => err.fmt(f),
            Self::Url { url, start, .. } => {
                write!(f, "Invalid URL at position {start}: `{url}`")
            }
            Self::FileUrl { url, start, .. } => {
                write!(f, "Invalid file URL at position {start}: `{url}`")
            }
            Self::VerbatimUrl { url, start, .. } => {
                write!(f, "Invalid URL at position {start}: `{url}`")
            }
            Self::UrlConversion(given) => {
                write!(f, "Unable to convert URL to path: {given}")
            }
            Self::UnsupportedUrl(url) => {
                write!(f, "Unsupported URL (expected a `file://` scheme): `{url}`")
            }
            Self::NonEditable { .. } => {
                write!(f, "Unsupported editable requirement")
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
            Self::Io(err) => err.source(),
            Self::Url { source, .. } => Some(source),
            Self::FileUrl { .. } => None,
            Self::VerbatimUrl { source, .. } => Some(source),
            Self::UrlConversion(_) => None,
            Self::UnsupportedUrl(_) => None,
            Self::NonEditable { source, .. } => Some(source),
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
            RequirementsTxtParserError::Io(err) => err.fmt(f),
            RequirementsTxtParserError::Url { url, start, .. } => {
                write!(
                    f,
                    "Invalid URL in `{}` at position {start}: `{url}`",
                    self.file.user_display(),
                )
            }
            RequirementsTxtParserError::FileUrl { url, start, .. } => {
                write!(
                    f,
                    "Invalid file URL in `{}` at position {start}: `{url}`",
                    self.file.user_display(),
                )
            }
            RequirementsTxtParserError::VerbatimUrl { url, start, .. } => {
                write!(
                    f,
                    "Invalid URL in `{}` at position {start}: `{url}`",
                    self.file.user_display(),
                )
            }
            RequirementsTxtParserError::UrlConversion(given) => {
                write!(
                    f,
                    "Unable to convert URL to path `{}`: {given}",
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
            RequirementsTxtParserError::NonEditable { .. } => {
                write!(
                    f,
                    "Unsupported editable requirement in `{}`",
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
        Self::Io(err)
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
                    .is_some_and(|&(_, next_char)| next_char == '\n')
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

    use crate::{calculate_row_column, RequirementsTxt};

    fn workspace_test_data_dir() -> PathBuf {
        Path::new("./test-data").simple_canonicalize().unwrap()
    }

    /// Filter a path for use in snapshots; in particular, match the Windows debug representation
    /// of a path.
    ///
    /// We replace backslashes to match the debug representation for paths, and match _either_
    /// backslashes or forward slashes as the latter appear when constructing a path from a URL.
    fn path_filter(path: &Path) -> String {
        regex::escape(&path.simplified_display().to_string()).replace(r"\\", r"(\\\\|/)")
    }

    /// Return the insta filters for a given path.
    fn path_filters(filter: &str) -> Vec<(&str, &str)> {
        vec![(filter, "<REQUIREMENTS_DIR>"), (r"\\\\", "/")]
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

        let actual = RequirementsTxt::parse(
            requirements_txt.clone(),
            &working_dir,
            &BaseClientBuilder::new(),
        )
        .await
        .unwrap();

        let snapshot = format!("parse-{}", path.to_string_lossy());

        insta::with_settings!({
            filters => path_filters(&path_filter(&working_dir)),
        }, {
            insta::assert_debug_snapshot!(snapshot, actual);
        });
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

        insta::with_settings!({
            filters => path_filters(&path_filter(temp_dir.path())),
        }, {
            insta::assert_debug_snapshot!(snapshot, actual);
        });
    }

    #[cfg(unix)]
    #[test_case(Path::new("bare-url.txt"))]
    #[test_case(Path::new("editable.txt"))]
    #[tokio::test]
    async fn parse_unix(path: &Path) {
        let working_dir = workspace_test_data_dir().join("requirements-txt");
        let requirements_txt = working_dir.join(path);

        let actual =
            RequirementsTxt::parse(requirements_txt, &working_dir, &BaseClientBuilder::new())
                .await
                .unwrap();

        let snapshot = format!("parse-unix-{}", path.to_string_lossy());

        insta::with_settings!({
            filters => path_filters(&path_filter(&working_dir)),
        }, {
            insta::assert_debug_snapshot!(snapshot, actual);
        });
    }

    #[cfg(unix)]
    #[test_case(Path::new("semicolon.txt"))]
    #[tokio::test]
    async fn parse_err(path: &Path) {
        let working_dir = workspace_test_data_dir().join("requirements-txt");
        let requirements_txt = working_dir.join(path);

        let actual =
            RequirementsTxt::parse(requirements_txt, &working_dir, &BaseClientBuilder::new())
                .await
                .unwrap_err();

        let snapshot = format!("parse-unix-{}", path.to_string_lossy());

        insta::with_settings!({
            filters => path_filters(&path_filter(&working_dir)),
        }, {
            insta::assert_debug_snapshot!(snapshot, actual);
        });
    }

    #[cfg(windows)]
    #[test_case(Path::new("bare-url.txt"))]
    #[test_case(Path::new("editable.txt"))]
    #[tokio::test]
    async fn parse_windows(path: &Path) {
        let working_dir = workspace_test_data_dir().join("requirements-txt");
        let requirements_txt = working_dir.join(path);

        let actual =
            RequirementsTxt::parse(requirements_txt, &working_dir, &BaseClientBuilder::new())
                .await
                .unwrap();

        let snapshot = format!("parse-windows-{}", path.to_string_lossy());

        insta::with_settings!({
            filters => path_filters(&path_filter(&working_dir)),
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
        let filters = vec![(requirement_txt.as_str(), "<REQUIREMENTS_TXT>")];
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
        let filters = vec![(requirement_txt.as_str(), "<REQUIREMENTS_TXT>")];
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
        let filters = vec![(requirement_txt.as_str(), "<REQUIREMENTS_TXT>")];
        insta::with_settings!({
            filters => filters
        }, {
            insta::assert_snapshot!(errors, @r###"
            Unsupported editable requirement in `<REQUIREMENTS_TXT>`
            Editable must refer to a local directory, not an HTTPS URL: `http://localhost:8080/`
            "###);
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
            Couldn't parse requirement in `<REQUIREMENTS_TXT>` at position 3
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
        let filters = vec![(requirement_txt.as_str(), "<REQUIREMENTS_TXT>")];
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
        let filters = vec![(requirement_txt.as_str(), "<REQUIREMENTS_TXT>")];
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
        let filters = vec![(requirement_txt.as_str(), "<REQUIREMENTS_TXT>")];
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

        insta::with_settings!({
            filters => path_filters(&path_filter(temp_dir.path())),
        }, {
            insta::assert_debug_snapshot!(requirements, @r###"
            RequirementsTxt {
                requirements: [
                    RequirementEntry {
                        requirement: Named(
                            Requirement {
                                name: PackageName(
                                    "flask",
                                ),
                                extras: [],
                                version_or_url: None,
                                marker: None,
                                origin: Some(
                                    File(
                                        "<REQUIREMENTS_DIR>/subdir/sibling.txt",
                                    ),
                                ),
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
        });

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

        insta::with_settings!({
            filters => path_filters(&path_filter(temp_dir.path())),
        }, {
            insta::assert_debug_snapshot!(requirements, @r###"
            RequirementsTxt {
                requirements: [
                    RequirementEntry {
                        requirement: Named(
                            Requirement {
                                name: PackageName(
                                    "flask",
                                ),
                                extras: [],
                                version_or_url: None,
                                marker: None,
                                origin: Some(
                                    File(
                                        "<REQUIREMENTS_DIR>/requirements.txt",
                                    ),
                                ),
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
        });

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

        insta::with_settings!({
            filters => path_filters(&path_filter(temp_dir.path())),
        }, {
            insta::assert_debug_snapshot!(requirements, @r###"
            RequirementsTxt {
                requirements: [],
                constraints: [],
                editables: [
                    RequirementEntry {
                        requirement: Unnamed(
                            UnnamedRequirement {
                                url: VerbatimParsedUrl {
                                    parsed_url: Directory(
                                        ParsedDirectoryUrl {
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
                                            install_path: "/foo/bar",
                                            lock_path: "/foo/bar",
                                            editable: true,
                                        },
                                    ),
                                    verbatim: VerbatimUrl {
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
                                },
                                extras: [],
                                marker: None,
                                origin: Some(
                                    File(
                                        "<REQUIREMENTS_DIR>/grandchild.txt",
                                    ),
                                ),
                            },
                        ),
                        hashes: [],
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
        });

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
        let filters = vec![(requirement_txt.as_str(), "<REQUIREMENTS_TXT>")];
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

        insta::with_settings!({
            filters => path_filters(&path_filter(temp_dir.path())),
        }, {
            insta::assert_debug_snapshot!(requirements, @r###"
            RequirementsTxt {
                requirements: [
                    RequirementEntry {
                        requirement: Named(
                            Requirement {
                                name: PackageName(
                                    "httpx",
                                ),
                                extras: [],
                                version_or_url: None,
                                marker: None,
                                origin: Some(
                                    File(
                                        "<REQUIREMENTS_DIR>/./sibling.txt",
                                    ),
                                ),
                            },
                        ),
                        hashes: [],
                    },
                    RequirementEntry {
                        requirement: Named(
                            Requirement {
                                name: PackageName(
                                    "flask",
                                ),
                                extras: [],
                                version_or_url: Some(
                                    VersionSpecifier(
                                        VersionSpecifiers(
                                            [
                                                VersionSpecifier {
                                                    operator: Equal,
                                                    version: "3.0.0",
                                                },
                                            ],
                                        ),
                                    ),
                                ),
                                marker: None,
                                origin: Some(
                                    File(
                                        "<REQUIREMENTS_DIR>/requirements.txt",
                                    ),
                                ),
                            },
                        ),
                        hashes: [
                            "sha256:1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
                        ],
                    },
                    RequirementEntry {
                        requirement: Named(
                            Requirement {
                                name: PackageName(
                                    "requests",
                                ),
                                extras: [],
                                version_or_url: Some(
                                    VersionSpecifier(
                                        VersionSpecifiers(
                                            [
                                                VersionSpecifier {
                                                    operator: Equal,
                                                    version: "2.26.0",
                                                },
                                            ],
                                        ),
                                    ),
                                ),
                                marker: None,
                                origin: Some(
                                    File(
                                        "<REQUIREMENTS_DIR>/requirements.txt",
                                    ),
                                ),
                            },
                        ),
                        hashes: [
                            "sha256:fedcba0987654321fedcba0987654321fedcba0987654321fedcba0987654321",
                        ],
                    },
                    RequirementEntry {
                        requirement: Named(
                            Requirement {
                                name: PackageName(
                                    "black",
                                ),
                                extras: [],
                                version_or_url: Some(
                                    VersionSpecifier(
                                        VersionSpecifiers(
                                            [
                                                VersionSpecifier {
                                                    operator: Equal,
                                                    version: "21.12b0",
                                                },
                                            ],
                                        ),
                                    ),
                                ),
                                marker: None,
                                origin: Some(
                                    File(
                                        "<REQUIREMENTS_DIR>/requirements.txt",
                                    ),
                                ),
                            },
                        ),
                        hashes: [],
                    },
                    RequirementEntry {
                        requirement: Named(
                            Requirement {
                                name: PackageName(
                                    "mypy",
                                ),
                                extras: [],
                                version_or_url: Some(
                                    VersionSpecifier(
                                        VersionSpecifiers(
                                            [
                                                VersionSpecifier {
                                                    operator: Equal,
                                                    version: "0.910",
                                                },
                                            ],
                                        ),
                                    ),
                                ),
                                marker: None,
                                origin: Some(
                                    File(
                                        "<REQUIREMENTS_DIR>/requirements.txt",
                                    ),
                                ),
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
        });

        Ok(())
    }

    #[tokio::test]
    async fn parser_error_line_and_column() -> Result<()> {
        let temp_dir = assert_fs::TempDir::new()?;
        let requirements_txt = temp_dir.child("requirements.txt");
        requirements_txt.write_str(indoc! {"
            numpy>=1,<2
              --broken
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
        let filters = vec![(requirement_txt.as_str(), "<REQUIREMENTS_TXT>")];
        insta::with_settings!({
            filters => filters
        }, {
            insta::assert_snapshot!(errors, @r###"
            Unexpected '-', expected '-c', '-e', '-r' or the start of a requirement at <REQUIREMENTS_TXT>:2:3
            "###);
        });

        Ok(())
    }

    #[test_case("numpy>=1,<2\n  @-broken\ntqdm", "2:4"; "ASCII Character with LF")]
    #[test_case("numpy>=1,<2\r\n  #-broken\ntqdm", "2:4"; "ASCII Character with CRLF")]
    #[test_case("numpy>=1,<2\n  \n-broken\ntqdm", "3:1"; "ASCII Character LF then LF")]
    #[test_case("numpy>=1,<2\n  \r-broken\ntqdm", "3:1"; "ASCII Character LF then CR but no LF")]
    #[test_case("numpy>=1,<2\n  \r\n-broken\ntqdm", "3:1"; "ASCII Character LF then CRLF")]
    #[test_case("numpy>=1,<2\n  🚀-broken\ntqdm", "2:4"; "Emoji (Wide) Character")]
    #[test_case("numpy>=1,<2\n  中-broken\ntqdm", "2:4"; "Fullwidth character")]
    #[test_case("numpy>=1,<2\n  e\u{0301}-broken\ntqdm", "2:5"; "Two codepoints")]
    #[test_case("numpy>=1,<2\n  a\u{0300}\u{0316}-broken\ntqdm", "2:6"; "Three codepoints")]
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
