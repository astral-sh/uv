use std::convert::Infallible;
use std::{
    ffi::OsString,
    process::{Command, ExitCode, ExitStatus},
};

use anyhow::{bail, Error};

/// Spawns a command exec style.
fn exec_spawn(cmd: &mut Command) -> Result<Infallible, Error> {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let err = cmd.exec();
        Err(err.into())
    }
    #[cfg(windows)]
    {
        cmd.stdin(std::process::Stdio::inherit());
        let status = cmd.status()?;

        #[allow(clippy::exit)]
        std::process::exit(status.code().unwrap())
    }
}

fn run() -> Result<ExitStatus, Error> {
    let current_exe = std::env::current_exe()?;
    let Some(bin) = current_exe.parent() else {
        bail!("Could not determine the location of the `uvx` binary")
    };
    let uv = bin.join("uv");
    let args = ["tool", "uvx"]
        .iter()
        .map(OsString::from)
        // Skip the `uvx` name
        .chain(std::env::args_os().skip(1))
        .collect::<Vec<_>>();

    let mut cmd = Command::new(uv);
    cmd.args(&args);
    match exec_spawn(&mut cmd)? {}
}

#[allow(clippy::print_stderr)]
fn main() -> ExitCode {
    let result = run();
    match result {
        // Fail with 2 if the status cannot be cast to an exit code
        Ok(status) => u8::try_from(status.code().unwrap_or(2)).unwrap_or(2).into(),
        Err(err) => {
            let mut causes = err.chain();
            eprintln!("error: {}", causes.next().unwrap());
            for err in causes {
                eprintln!("  Caused by: {err}");
            }
            ExitCode::from(2)
        }
    }
}
