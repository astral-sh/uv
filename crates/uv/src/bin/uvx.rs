use std::convert::Infallible;
use std::{
    ffi::OsString,
    process::{Command, ExitCode, ExitStatus},
};

/// Spawns a command exec style.
fn exec_spawn(cmd: &mut Command) -> std::io::Result<Infallible> {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let err = cmd.exec();
        Err(err)
    }
    #[cfg(windows)]
    {
        cmd.stdin(std::process::Stdio::inherit());
        let status = cmd.status()?;

        #[allow(clippy::exit)]
        std::process::exit(status.code().unwrap())
    }
}

fn run() -> std::io::Result<ExitStatus> {
    let current_exe = std::env::current_exe()?;
    let Some(bin) = current_exe.parent() else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Could not determine the location of the `uvx` binary",
        ));
    };
    let uv = bin.join(format!("uv{}", std::env::consts::EXE_SUFFIX));
    let args = ["tool", "uvx"]
        .iter()
        .map(OsString::from)
        // Skip the `uvx` name
        .chain(std::env::args_os().skip(1))
        .collect::<Vec<_>>();

    // If we are sure the uv binary does not exist, display a clearer error message
    if matches!(uv.try_exists(), Ok(false)) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("Could not find the `uv` binary at: {}", uv.display()),
        ));
    }

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
            eprintln!("error: {err}");
            ExitCode::from(2)
        }
    }
}
