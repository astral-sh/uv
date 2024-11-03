//! Generate the environment variables reference from `uv_static::EnvVars`.

use anyhow::bail;
use pretty_assertions::StrComparison;
use std::path::PathBuf;

use uv_static::EnvVars;

use crate::generate_all::Mode;
use crate::ROOT_DIR;

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
                    bail!("{filename} changed, please run `cargo dev generate-env-vars-reference`:\n{comparison}");
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
                bail!("{filename} changed, please run `cargo dev generate-env-vars-reference`:\n{err}");
            }
        },
    }

    Ok(())
}

fn generate() -> String {
    let mut output = String::new();

    output.push_str("# Environment variables\n\n");
    output.push_str("uv respects the following environment variables:\n\n");

    for (var, doc) in EnvVars::metadata() {
        // Remove empty lines and ddd two spaces to the beginning from the second line.
        let doc = doc
            .lines()
            .enumerate()
            .filter(|(_, line)| !line.trim().is_empty())
            .map(|(i, line)| {
                if i == 0 {
                    line.to_string()
                } else {
                    format!("  {line}")
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        output.push_str(&format!("- <a id=\"{var}\"></a> [`{var}`](#{var}): {doc}\n"));
    }

    output
}

#[cfg(test)]
mod tests;
