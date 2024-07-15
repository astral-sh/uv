//! Generate a Markdown-compatible listing of configuration options for `pyproject.toml`.
//!
//! Based on: <https://github.com/astral-sh/ruff/blob/dc8db1afb08704ad6a788c497068b01edf8b460d/crates/ruff_dev/src/generate_options.rs>
use std::fmt::Write;

use itertools::Itertools;
use schemars::JsonSchema;
use serde::Deserialize;

use uv_distribution::pyproject::ToolUv as WorkspaceOptions;
use uv_macros::OptionsMetadata;
use uv_options_metadata::{OptionField, OptionSet, OptionsMetadata, Visit};
use uv_settings::Options as SettingsOptions;

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

pub(crate) fn generate() -> String {
    let mut output = String::new();

    generate_set(
        &mut output,
        Set::Global(CombinedOptions::metadata()),
        &mut Vec::new(),
    );

    output
}

fn generate_set(output: &mut String, set: Set, parents: &mut Vec<Set>) {
    match &set {
        Set::Global(_) => {
            output.push_str("### Global\n");
        }
        Set::Named { name, .. } => {
            let title = parents
                .iter()
                .filter_map(|set| set.name())
                .chain(std::iter::once(name.as_str()))
                .join(".");
            writeln!(output, "#### `{title}`\n").unwrap();

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
    Global(OptionSet),
    Named { name: String, set: OptionSet },
}

impl Set {
    fn name(&self) -> Option<&str> {
        match self {
            Set::Global(_) => None,
            Set::Named { name, .. } => Some(name),
        }
    }

    fn metadata(&self) -> &OptionSet {
        match self {
            Set::Global(set) => set,
            Set::Named { set, .. } => set,
        }
    }
}

fn emit_field(output: &mut String, name: &str, field: &OptionField, parents: &[Set]) {
    let header_level = if parents.is_empty() { "####" } else { "#####" };
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
    output.push_str(&format!("**Type**: `{}`\n", field.value_type));
    output.push('\n');
    output.push_str("**Example usage**:\n\n");
    output.push_str(&format_tab(
        "pyproject.toml",
        &format_header(field.scope, parents, ConfigurationFile::PyprojectToml),
        field.example,
    ));
    output.push_str(&format_tab(
        "uv.toml",
        &format_header(field.scope, parents, ConfigurationFile::UvToml),
        field.example,
    ));
    output.push('\n');
}

fn format_tab(tab_name: &str, header: &str, content: &str) -> String {
    format!(
        "=== \"{}\"\n\n    ```toml\n    {}\n{}\n    ```\n",
        tab_name,
        header,
        textwrap::indent(content, "    ")
    )
}

/// Format the TOML header for the example usage for a given option.
///
/// For example: `[tool.uv.pip]`.
fn format_header(scope: Option<&str>, parents: &[Set], configuration: ConfigurationFile) -> String {
    let tool_parent = match configuration {
        ConfigurationFile::PyprojectToml => Some("tool.uv"),
        ConfigurationFile::UvToml => None,
    };

    let header = tool_parent
        .into_iter()
        .chain(parents.iter().filter_map(|parent| parent.name()))
        .chain(scope)
        .join(".");

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
