#![allow(clippy::print_stdout)]

use globset::GlobSetBuilder;
use std::env::args;
use tracing::trace;
use uv_globfilter::{parse_portable_glob, GlobDirFilter};
use walkdir::WalkDir;

fn main() {
    let includes = ["src/**", "pyproject.toml"];
    let excludes = ["__pycache__", "*.pyc", "*.pyo"];

    let mut include_globs = Vec::new();
    for include in includes {
        let glob = parse_portable_glob(include).unwrap();
        include_globs.push(glob.clone());
    }
    let include_matcher = GlobDirFilter::from_globs(&include_globs).unwrap();

    let mut exclude_builder = GlobSetBuilder::new();
    for exclude in excludes {
        // Excludes are unanchored
        let exclude = if let Some(exclude) = exclude.strip_prefix("/") {
            exclude.to_string()
        } else {
            format!("**/{exclude}").to_string()
        };
        let glob = parse_portable_glob(&exclude).unwrap();
        exclude_builder.add(glob);
    }
    // https://github.com/BurntSushi/ripgrep/discussions/2927
    let exclude_matcher = exclude_builder.build().unwrap();

    let walkdir_root = args().next().unwrap();
    for entry in WalkDir::new(&walkdir_root)
        .into_iter()
        .filter_entry(|entry| {
            // TODO(konsti): This should be prettier.
            let relative = entry
                .path()
                .strip_prefix(&walkdir_root)
                .expect("walkdir starts with root")
                .to_path_buf();

            include_matcher.match_directory(&relative) && !exclude_matcher.is_match(&relative)
        })
    {
        let entry = entry.unwrap();
        // TODO(konsti): This should be prettier.
        let relative = entry
            .path()
            .strip_prefix(&walkdir_root)
            .expect("walkdir starts with root")
            .to_path_buf();

        if !include_matcher.match_path(&relative) || exclude_matcher.is_match(&relative) {
            trace!("Excluding: `{}`", relative.display());
            continue;
        };
        println!("{}", relative.display());
    }
}
