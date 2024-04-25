use std::path::PathBuf;

use anstream::println;
use anyhow::{bail, Result};
use pretty_assertions::StrComparison;
use schemars::{schema_for, JsonSchema};
use serde::Deserialize;

use uv_workspace::Options;

use crate::ROOT_DIR;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
// The name is used in the schema
struct Uv {
    #[serde(flatten)]
    options: Options,
    #[serde(flatten)]
    dep_spec: uv_requirements::pyproject::Uv,
}

#[derive(clap::Args)]
pub(crate) struct GenerateJsonSchemaArgs {
    /// Write the generated table to stdout (rather than to `uv.schema.json`).
    #[arg(long, default_value_t, value_enum)]
    mode: Mode,
}

#[derive(Copy, Clone, PartialEq, Eq, clap::ValueEnum, Default)]
enum Mode {
    /// Update the content in the `configuration.md`.
    #[default]
    Write,

    /// Don't write to the file, check if the file is up-to-date and error if not.
    Check,

    /// Write the generated help to stdout.
    DryRun,
}

pub(crate) fn main(args: &GenerateJsonSchemaArgs) -> Result<()> {
    let schema = schema_for!(Uv);
    let schema_string = serde_json::to_string_pretty(&schema).unwrap();
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
                    bail!("{filename} changed, please run `cargo dev generate-json-schema`:\n{comparison}");
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

#[cfg(test)]
mod tests {
    use std::env;

    use anyhow::Result;

    use crate::generate_json_schema::Mode;

    use super::{main, GenerateJsonSchemaArgs};

    #[test]
    fn test_generate_json_schema() -> Result<()> {
        let mode = if env::var("UV_UPDATE_SCHEMA").as_deref() == Ok("1") {
            Mode::Write
        } else {
            Mode::Check
        };
        main(&GenerateJsonSchemaArgs { mode })
    }
}
