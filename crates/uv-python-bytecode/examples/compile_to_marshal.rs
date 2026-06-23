#![allow(clippy::print_stderr)]

use std::env;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use uv_python_bytecode::compile;

fn main() -> ExitCode {
    let Some(path) = env::args_os().nth(1).map(PathBuf::from) else {
        eprintln!("usage: compile_to_marshal PATH");
        return ExitCode::from(2);
    };
    let source = match fs_err::read_to_string(&path) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("{}: {error}", path.display());
            return ExitCode::FAILURE;
        }
    };
    let filename = path.to_string_lossy();
    let module = match compile(&source, &filename) {
        Ok(module) => module,
        Err(error) => {
            eprintln!("{}: {error}", path.display());
            return ExitCode::FAILURE;
        }
    };
    if let Err(error) = io::stdout().write_all(&module.marshal()) {
        eprintln!("failed to write marshal output: {error}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
