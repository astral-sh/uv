use std::path::PathBuf;

use anstream::println;
use anyhow::{Result, bail};
use pretty_assertions::StrComparison;
use schemars::JsonSchema;
use serde::Deserialize;

use uv_settings::Options as SettingsOptions;
use uv_workspace::pyproject::ToolUv as WorkspaceOptions;

use crate::ROOT_DIR;
use crate::generate_all::Mode;

#[derive(Deserialize, JsonSchema)]
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
    // Generate the schema.
    let schema_string = generate();
    let filename = "uv.schema.json";
    let schema_path = PathBuf::from(ROOT_DIR).join(filename);

    match args.mode {
        Mode::DryRun => {
            println!("{schema_string}");
        }
        Mode::Check => match fs_err::read_to_string(schema_path) {
            Ok(current) => {
                if current == schema_string {
                    println!("Up-to-date: {filename}");
                } else {
                    let comparison = StrComparison::new(&current, &schema_string);
                    bail!(
                        "{filename} changed, please run `cargo dev generate-json-schema`:\n{comparison}"
                    );
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                bail!("{filename} not found, please run `cargo dev generate-json-schema`");
            }
            Err(err) => {
                bail!("{filename} changed, please run `cargo dev generate-json-schema`:\n{err}");
            }
        },
        Mode::Write => match fs_err::read_to_string(&schema_path) {
            Ok(current) => {
                if current == schema_string {
                    println!("Up-to-date: {filename}");
                } else {
                    println!("Updating: {filename}");
                    fs_err::write(schema_path, schema_string.as_bytes())?;
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                println!("Updating: {filename}");
                fs_err::write(schema_path, schema_string.as_bytes())?;
            }
            Err(err) => {
                bail!("{filename} changed, please run `cargo dev generate-json-schema`:\n{err}");
            }
        },
    }

    Ok(())
}

const REPLACEMENTS: &[(&str, &str)] = &[
    // Use the fully-resolved URL rather than the relative Markdown path.
    (
        "(../concepts/projects/dependencies.md)",
        "(https://docs.astral.sh/uv/concepts/projects/dependencies/)",
    ),
];

/// Generate the JSON schema for the combined options as a string.
fn generate() -> String {
    let settings = schemars::generate::SchemaSettings::draft07();
    let generator = schemars::SchemaGenerator::new(settings);
    let schema = generator.into_root_schema_for::<CombinedOptions>();

    let mut output = serde_json::to_string_pretty(&schema).unwrap();

    for (value, replacement) in REPLACEMENTS {
        assert_ne!(
            value, replacement,
            "`value` and `replacement` must be different, but both are `{value}`"
        );
        let before = &output;
        let after = output.replace(value, replacement);
        assert_ne!(*before, after, "Could not find `{value}` in the output");
        output = after;
    }

    output
}

#[cfg(test)]
mod tests {
    use std::env;

    use anyhow::Result;

    use uv_static::EnvVars;

    use crate::generate_all::Mode;

    use super::{Args, main};

    #[test]
    fn test_generate_json_schema() -> Result<()> {
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
