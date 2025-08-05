//! Generate a Markdown-compatible listing of configuration options for `pyproject.toml`.
//!
//! Based on: <https://github.com/astral-sh/ruff/blob/dc8db1afb08704ad6a788c497068b01edf8b460d/crates/ruff_dev/src/generate_options.rs>
use std::fmt::Write;
use std::path::PathBuf;

use anstream::println;
use anyhow::{Result, bail};
use itertools::Itertools;
use pretty_assertions::StrComparison;
use schemars::JsonSchema;
use serde::Deserialize;

use uv_macros::OptionsMetadata;
use uv_options_metadata::{OptionField, OptionSet, OptionsMetadata, Visit};
use uv_settings::Options as SettingsOptions;
use uv_workspace::pyproject::ToolUv as WorkspaceOptions;

use crate::ROOT_DIR;
use crate::generate_all::Mode;

#[derive(Deserialize, JsonSchema, OptionsMetadata)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
// The names and docstrings of this struct and the types it contains are used as `title` and
// `description` in uv.schema.json, see https://github.com/SchemaStore/schemastore/blob/master/editor-features.md#title-as-an-expected-object-type
/// Metadata and configuration for uv.
struct CombinedOptions {
    #[serde(flatten)]
    options: SettingsOptions,
    #[serde(flatten)]
    workspace: WorkspaceOptions,
}

#[derive(clap::Args)]
pub(crate) struct Args {
    #[arg(long, default_value_t, value_enum)]
    pub(crate) mode: Mode,
}

pub(crate) fn main(args: &Args) -> Result<()> {
    let reference_string = generate();
    let filename = "settings.md";
    let reference_path = PathBuf::from(ROOT_DIR)
        .join("docs")
        .join("reference")
        .join(filename);

    match args.mode {
        Mode::DryRun => {
            println!("{reference_string}");
        }
        Mode::Check => match fs_err::read_to_string(reference_path) {
            Ok(current) => {
                if current == reference_string {
                    println!("Up-to-date: {filename}");
                } else {
                    let comparison = StrComparison::new(&current, &reference_string);
                    bail!(
                        "{filename} changed, please run `cargo dev generate-options-reference`:\n{comparison}"
                    );
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                bail!("{filename} not found, please run `cargo dev generate-options-reference`");
            }
            Err(err) => {
                bail!(
                    "{filename} changed, please run `cargo dev generate-options-reference`:\n{err}"
                );
            }
        },
        Mode::Write => match fs_err::read_to_string(&reference_path) {
            Ok(current) => {
                if current == reference_string {
                    println!("Up-to-date: {filename}");
                } else {
                    println!("Updating: {filename}");
                    fs_err::write(reference_path, reference_string.as_bytes())?;
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                println!("Updating: {filename}");
                fs_err::write(reference_path, reference_string.as_bytes())?;
            }
            Err(err) => {
                bail!(
                    "{filename} changed, please run `cargo dev generate-options-reference`:\n{err}"
                );
            }
        },
    }

    Ok(())
}

enum OptionType {
    Configuration,
    ProjectMetadata,
}

fn generate() -> String {
    let mut output = String::new();

    generate_set(
        &mut output,
        Set::Global {
            set: WorkspaceOptions::metadata(),
            option_type: OptionType::ProjectMetadata,
        },
        &mut Vec::new(),
    );

    generate_set(
        &mut output,
        Set::Global {
            set: SettingsOptions::metadata(),
            option_type: OptionType::Configuration,
        },
        &mut Vec::new(),
    );

    output
}

fn generate_set(output: &mut String, set: Set, parents: &mut Vec<Set>) {
    match &set {
        Set::Global { option_type, .. } => {
            let header = match option_type {
                OptionType::Configuration => "## Configuration\n",
                OptionType::ProjectMetadata => "## Project metadata\n",
            };
            output.push_str(header);
        }
        Set::Named { name, .. } => {
            let title = parents
                .iter()
                .filter_map(|set| set.name())
                .chain(std::iter::once(name.as_str()))
                .join(".");
            writeln!(output, "### `{title}`\n").unwrap();

            if let Some(documentation) = set.metadata().documentation() {
                output.push_str(documentation);
                output.push('\n');
                output.push('\n');
            }
        }
    }

    let mut visitor = CollectOptionsVisitor::default();
    set.metadata().record(&mut visitor);

    let (mut fields, mut sets) = (visitor.fields, visitor.groups);

    fields.sort_unstable_by(|(name, _), (name2, _)| name.cmp(name2));
    sets.sort_unstable_by(|(name, _), (name2, _)| name.cmp(name2));

    parents.push(set);

    // Generate the fields.
    for (name, field) in &fields {
        emit_field(output, name, field, parents.as_slice());
        output.push_str("---\n\n");
    }

    // Generate all the sub-sets.
    for (set_name, sub_set) in &sets {
        generate_set(
            output,
            Set::Named {
                name: set_name.to_string(),
                set: *sub_set,
            },
            parents,
        );
    }

    parents.pop();
}

enum Set {
    Global {
        option_type: OptionType,
        set: OptionSet,
    },
    Named {
        name: String,
        set: OptionSet,
    },
}

impl Set {
    fn name(&self) -> Option<&str> {
        match self {
            Self::Global { .. } => None,
            Self::Named { name, .. } => Some(name),
        }
    }

    fn metadata(&self) -> &OptionSet {
        match self {
            Self::Global { set, .. } | Self::Named { set, .. } => set,
        }
    }
}

#[allow(clippy::format_push_string)]
fn emit_field(output: &mut String, name: &str, field: &OptionField, parents: &[Set]) {
    let header_level = if parents.len() > 1 { "####" } else { "###" };
    let parents_anchor = parents.iter().filter_map(|parent| parent.name()).join("_");

    if parents_anchor.is_empty() {
        output.push_str(&format!(
            "{header_level} [`{name}`](#{name}) {{: #{name} }}\n"
        ));
    } else {
        output.push_str(&format!(
            "{header_level} [`{name}`](#{parents_anchor}_{name}) {{: #{parents_anchor}_{name} }}\n"
        ));

        // the anchor used to just be the name, but now it's the group name
        // for backwards compatibility, we need to keep the old anchor
        output.push_str(&format!("<span id=\"{name}\"></span>\n"));
    }

    output.push('\n');

    if let Some(deprecated) = &field.deprecated {
        output.push_str("!!! warning \"Deprecated\"\n");
        output.push_str("    This option has been deprecated");

        if let Some(since) = deprecated.since {
            write!(output, " in {since}").unwrap();
        }

        output.push('.');

        if let Some(message) = deprecated.message {
            writeln!(output, " {message}").unwrap();
        }

        output.push('\n');
    }

    output.push_str(field.doc);
    output.push_str("\n\n");
    output.push_str(&format!("**Default value**: `{}`\n", field.default));
    output.push('\n');
    if let Some(possible_values) = field
        .possible_values
        .as_ref()
        .filter(|values| !values.is_empty())
    {
        output.push_str("**Possible values**:\n\n");
        for value in possible_values {
            output.push_str(format!("- {value}\n").as_str());
        }
    } else {
        output.push_str(&format!("**Type**: `{}`\n", field.value_type));
    }
    output.push('\n');
    output.push_str("**Example usage**:\n\n");

    match parents[0] {
        Set::Global {
            option_type: OptionType::ProjectMetadata,
            ..
        } => {
            output.push_str(&format_code(
                "pyproject.toml",
                &format_header(
                    field.scope,
                    field.example,
                    parents,
                    ConfigurationFile::PyprojectToml,
                ),
                field.example,
            ));
        }
        Set::Global {
            option_type: OptionType::Configuration,
            ..
        } => {
            output.push_str(&format_tab(
                "pyproject.toml",
                &format_header(
                    field.scope,
                    field.example,
                    parents,
                    ConfigurationFile::PyprojectToml,
                ),
                field.example,
            ));
            output.push_str(&format_tab(
                "uv.toml",
                &format_header(
                    field.scope,
                    field.example,
                    parents,
                    ConfigurationFile::UvToml,
                ),
                field.example,
            ));
        }
        _ => {}
    }
    output.push('\n');
}

fn format_tab(tab_name: &str, header: &str, content: &str) -> String {
    if header.is_empty() {
        format!(
            "=== \"{}\"\n\n    ```toml\n{}\n    ```\n",
            tab_name,
            textwrap::indent(content, "    ")
        )
    } else {
        format!(
            "=== \"{}\"\n\n    ```toml\n    {}\n{}\n    ```\n",
            tab_name,
            header,
            textwrap::indent(content, "    ")
        )
    }
}

fn format_code(file_name: &str, header: &str, content: &str) -> String {
    format!("```toml title=\"{file_name}\"\n{header}\n{content}\n```\n")
}

/// Format the TOML header for the example usage for a given option.
///
/// For example: `[tool.uv.pip]`.
fn format_header(
    scope: Option<&str>,
    example: &str,
    parents: &[Set],
    configuration: ConfigurationFile,
) -> String {
    let tool_parent = match configuration {
        ConfigurationFile::PyprojectToml => Some("tool.uv"),
        ConfigurationFile::UvToml => None,
    };

    let header = tool_parent
        .into_iter()
        .chain(parents.iter().filter_map(|parent| parent.name()))
        .chain(scope)
        .join(".");

    // Ex) `[[tool.uv.index]]`
    if example.starts_with(&format!("[[{header}")) {
        return String::new();
    }
    // Ex) `[tool.uv.sources]`
    if example.starts_with(&format!("[{header}")) {
        return String::new();
    }

    if header.is_empty() {
        String::new()
    } else {
        format!("[{header}]")
    }
}

#[derive(Debug, Copy, Clone)]
enum ConfigurationFile {
    PyprojectToml,
    UvToml,
}

#[derive(Default)]
struct CollectOptionsVisitor {
    groups: Vec<(String, OptionSet)>,
    fields: Vec<(String, OptionField)>,
}

impl Visit for CollectOptionsVisitor {
    fn record_set(&mut self, name: &str, group: OptionSet) {
        self.groups.push((name.to_owned(), group));
    }

    fn record_field(&mut self, name: &str, field: OptionField) {
        self.fields.push((name.to_owned(), field));
    }
}

#[cfg(test)]
mod tests {
    use std::env;

    use anyhow::Result;

    use uv_static::EnvVars;

    use crate::generate_all::Mode;

    use super::{Args, main};

    #[test]
    fn test_generate_options_reference() -> Result<()> {
        // Skip this test in CI to avoid redundancy with the dedicated CI job
        if env::var_os(EnvVars::CI).is_some() {
            return Ok(());
        }

        let mode = if env::var(EnvVars::UV_UPDATE_SCHEMA).as_deref() == Ok("1") {
            Mode::Write
        } else {
            Mode::Check
        };
        main(&Args { mode })
    }
}
