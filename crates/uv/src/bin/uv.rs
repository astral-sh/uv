use std::process::ExitCode;

use uv::main as uv_main;

fn main() -> ExitCode {
    uv_main(std::env::args_os())
}
