use glob::{Pattern, glob};
use std::env;
use std::error::Error;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn Error>> {
    let root = env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .ok_or("usage: glob-appledouble-repro <directory>")?;

    let pattern = format!("{}/**/*", Pattern::escape(&root.to_string_lossy()));
    println!("Iterating glob pattern: {pattern}");

    for entry in glob(&pattern)? {
        match entry {
            Ok(path) => {
                let relative = path.strip_prefix(&root)?;
                let file_type = if path.is_dir() { "directory" } else { "file" };
                let is_appledouble = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with("._"));

                println!(
                    "  [{file_type:9}] [appledouble={is_appledouble:<5}] {}",
                    relative.display()
                );
            }
            Err(error) => println!("  [glob error] {error}"),
        }
    }

    Ok(())
}
