//! Generate the test vectors for [`uv_extract::dirhash`].

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use indexmap::IndexMap;
use pretty_assertions::StrComparison;
use serde::Serialize;
use uv_extract::dirhash::DirhashTree;

use crate::ROOT_DIR;
use crate::generate_all::Mode;

const FILENAME: &str = "test_vectors.json";

#[derive(clap::Args)]
pub(crate) struct Args {
    #[arg(long, default_value_t, value_enum)]
    pub(crate) mode: Mode,
}

type Directory = IndexMap<&'static str, FileOrDirectory>;

#[derive(Serialize)]
#[serde(untagged)]
enum FileOrDirectory {
    File(&'static str),
    Directory(Directory),
}

fn file(contents: &'static str) -> FileOrDirectory {
    FileOrDirectory::File(contents)
}

fn directory(entries: impl IntoIterator<Item = (&'static str, FileOrDirectory)>) -> Directory {
    entries.into_iter().collect()
}

fn subdirectory(
    entries: impl IntoIterator<Item = (&'static str, FileOrDirectory)>,
) -> FileOrDirectory {
    FileOrDirectory::Directory(directory(entries))
}

// Some interfaces, like `dirhash_path`, can handle both filepaths and directory paths. Other
// interfaces, like `DirhashTree`, expect to represent a directory. Most archive formats work
// similarly, where the assumption is that their root is a directory and not the recursive base
// case of "just the bytes of a nameless file". (Apparently the NAR format from Nix is a rare
// exception, but certainly Tar and Zip work this way.) To avoid overcomplicating the tests that
// read this list of vectors, don't include any cases that are "just the bytes of a nameless file".
// The dirhash of a file is its ordinary BLAKE3 hash, so there's not a lot of dirhash-specific code
// that needs testing in these cases anyway.
fn cases() -> Vec<Directory> {
    vec![
        // An empty directory.
        directory([]),
        // A non-empty directory.
        directory([("a", file("hello"))]),
        // Three files. `IndexMap` preserves the order of these keys, and they're deliberately
        // arranged in non-alphabetical order here to test that the caller sorts them.
        directory([("b", file("world")), ("a", file("hello")), ("c", file("!"))]),
        // A nested empty directory.
        directory([("a", subdirectory([("b", subdirectory([]))]))]),
        // A mixed hierarchy, again in non-alphabetical order.
        directory([
            (
                "b",
                subdirectory([("c", file("world")), ("!", subdirectory([]))]),
            ),
            ("a", file("hello")),
        ]),
    ]
}

#[derive(Serialize)]
struct TestVector {
    input: Directory,
    dirhash: String,
}

pub(crate) fn main(args: &Args) -> Result<()> {
    let generated = generate()?;
    let test_vectors_path = PathBuf::from(ROOT_DIR)
        .join("crates")
        .join("uv-extract")
        .join("test_vectors")
        .join(FILENAME);

    match args.mode {
        Mode::DryRun => anstream::print!("{generated}"),
        Mode::Check => {
            let current = fs_err::read_to_string(&test_vectors_path).with_context(|| {
                format!("failed to read {FILENAME}; run `cargo dev generate-dirhash-test-vectors`")
            })?;
            if current != generated {
                let comparison = StrComparison::new(&current, &generated);
                bail!(
                    "{FILENAME} changed, please run `cargo dev generate-dirhash-test-vectors`:\n{comparison}"
                );
            }
            anstream::println!("Up-to-date: {FILENAME}");
        }
        Mode::Write => {
            fs_err::write(&test_vectors_path, generated)
                .with_context(|| format!("failed to write {}", test_vectors_path.display()))?;
            anstream::println!("Updating: {FILENAME}");
        }
    }

    Ok(())
}

fn generate() -> Result<String> {
    let test_vectors = cases()
        .into_iter()
        .map(|input| {
            let mut tree = DirhashTree::new();
            add_entries(&mut tree, &input, None)?;
            Ok(TestVector {
                input,
                dirhash: tree.hash().to_hex().to_string(),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let mut generated = serde_json::to_string_pretty(&test_vectors)?;
    generated.push('\n');
    Ok(generated)
}

fn add_entries(tree: &mut DirhashTree, entries: &Directory, parent: Option<&str>) -> Result<()> {
    for (name, contents) in entries {
        let path = match parent {
            Some(parent) => format!("{parent}/{name}"),
            None => name.to_string(),
        };
        match contents {
            FileOrDirectory::File(contents) => {
                tree.add_file(&path, blake3::hash(contents.as_bytes()))?;
            }
            FileOrDirectory::Directory(directory) => {
                tree.add_empty_dir(&path)?;
                add_entries(tree, directory, Some(&path))?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use anyhow::Result;

    use super::generate;

    #[test]
    fn generates_current_test_vectors() -> Result<()> {
        assert_eq!(
            generate()?,
            include_str!("../../uv-extract/test_vectors/test_vectors.json")
        );
        Ok(())
    }
}
