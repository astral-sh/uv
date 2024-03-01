use std::path::PathBuf;

use clap::Parser;
use tracing::info;
use walkdir::WalkDir;

#[derive(Parser)]
pub(crate) struct ClearCompileArgs {
    /// Compile all `.py` in this or any subdirectory to bytecode
    root: PathBuf,
}

pub(crate) fn clear_compile(args: &ClearCompileArgs) -> anyhow::Result<()> {
    let mut removed_files = 0;
    let mut removed_directories = 0;
    for entry in WalkDir::new(&args.root).contents_first(true) {
        let entry = entry?;
        let metadata = entry.metadata()?;
        if metadata.is_file() {
            if entry.path().extension().is_some_and(|ext| ext == "pyc") {
                fs_err::remove_file(entry.path())?;
                removed_files += 1;
            }
        } else if metadata.is_dir() {
            if entry.file_name() == "__pycache__" {
                fs_err::remove_dir(entry.path())?;
                removed_directories += 1;
            }
        }
    }
    info!("Removed {removed_files} files and {removed_directories} directories");
    Ok(())
}
