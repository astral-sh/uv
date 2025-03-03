use std::convert::Infallible;
use std::path::Path;
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

fn get_uvx_suffix(current_exe: &Path) -> std::io::Result<&str> {
    let Some(os_file_name) = current_exe.file_name() else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Could not determine the file name of the `uvx` binary",
        ));
    };
    let Some(file_name_str) = os_file_name.to_str() else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Unable to convert executable name of `uvx` binary into a valid UTF-8 string",
        ));
    };
    let file_name_no_ext = file_name_str
        .strip_suffix(std::env::consts::EXE_SUFFIX)
        .unwrap_or(file_name_str);
    let Some(uvx_suffix) = file_name_no_ext.strip_prefix("uvx") else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Current executable name does not contain `uvx`",
        ));
    };
    Ok(uvx_suffix)
}

fn run() -> std::io::Result<ExitStatus> {
    let current_exe = std::env::current_exe()?;
    let Some(bin) = current_exe.parent() else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Could not determine the location of the `uvx` binary",
        ));
    };
    let uvx_suffix = get_uvx_suffix(&current_exe)?;
    let uv = bin.join(format!("uv{}{}", uvx_suffix, std::env::consts::EXE_SUFFIX));
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
