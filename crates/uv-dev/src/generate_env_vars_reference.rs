//! Generate the environment variables reference from `uv_static::EnvVars`.

use anyhow::bail;
use pretty_assertions::StrComparison;
use std::collections::BTreeSet;
use std::path::PathBuf;

use uv_static::EnvVars;

use crate::ROOT_DIR;
use crate::generate_all::Mode;

#[derive(clap::Args)]
pub(crate) struct Args {
    #[arg(long, default_value_t, value_enum)]
    pub(crate) mode: Mode,
}

pub(crate) fn main(args: &Args) -> anyhow::Result<()> {
    let reference_string = generate();
    let filename = "environment.md";
    let reference_path = PathBuf::from(ROOT_DIR)
        .join("docs")
        .join("configuration")
        .join(filename);

    match args.mode {
        Mode::DryRun => {
            anstream::println!("{reference_string}");
        }
        Mode::Check => match fs_err::read_to_string(reference_path) {
            Ok(current) => {
                if current == reference_string {
                    anstream::println!("Up-to-date: {filename}");
                } else {
                    let comparison = StrComparison::new(&current, &reference_string);
                    bail!(
                        "{filename} changed, please run `cargo dev generate-env-vars-reference`:\n{comparison}"
                    );
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                bail!("{filename} not found, please run `cargo dev generate-env-vars-reference`");
            }
            Err(err) => {
                bail!(
                    "{filename} changed, please run `cargo dev generate-env-vars-reference`:\n{err}"
                );
            }
        },
        Mode::Write => match fs_err::read_to_string(&reference_path) {
            Ok(current) => {
                if current == reference_string {
                    anstream::println!("Up-to-date: {filename}");
                } else {
                    anstream::println!("Updating: {filename}");
                    fs_err::write(reference_path, reference_string.as_bytes())?;
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                anstream::println!("Updating: {filename}");
                fs_err::write(reference_path, reference_string.as_bytes())?;
            }
            Err(err) => {
                bail!(
                    "{filename} changed, please run `cargo dev generate-env-vars-reference`:\n{err}"
                );
            }
        },
    }

    Ok(())
}

fn generate() -> String {
    let mut output = String::new();

    output.push_str("# Environment variables\n\n");

    // Partition and sort environment variables into UV_ and external variables.
    let (uv_vars, external_vars): (BTreeSet<_>, BTreeSet<_>) = EnvVars::metadata()
        .iter()
        .partition(|(var, _)| var.starts_with("UV_"));

    output.push_str("uv defines and respects the following environment variables:\n\n");

    for (var, doc) in uv_vars {
        output.push_str(&render(var, doc));
    }

    output.push_str("\n\n## Externally defined variables\n\n");
    output.push_str("uv also reads the following externally defined environment variables:\n\n");

    for (var, doc) in external_vars {
        output.push_str(&render(var, doc));
    }

    output
}

/// Render an environment variable and its documentation.
fn render(var: &str, doc: &str) -> String {
    format!("### `{var}`\n\n{doc}\n\n")
}

#[cfg(test)]
mod tests {
    use std::env;

    use anyhow::Result;

    use uv_static::EnvVars;

    use crate::generate_all::Mode;

    use super::{Args, main};

    #[test]
    fn test_generate_env_vars_reference() -> Result<()> {
        let mode = if env::var(EnvVars::UV_UPDATE_SCHEMA).as_deref() == Ok("1") {
            Mode::Write
        } else {
            Mode::Check
        };
        main(&Args { mode })
    }
}
