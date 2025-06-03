#![cfg_attr(windows, windows_subsystem = "windows")]

use std::convert::Infallible;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, ExitStatus};

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
        use std::os::windows::process::CommandExt;

        const CREATE_NO_WINDOW: u32 = 0x0800_0000;

        cmd.stdin(std::process::Stdio::inherit());
        let status = cmd.creation_flags(CREATE_NO_WINDOW).status()?;

        #[allow(clippy::exit)]
        std::process::exit(status.code().unwrap())
    }
}

/// Assuming the binary is called something like `uvw@1.2.3(.exe)`, compute the `@1.2.3(.exe)` part
/// so that we can preferentially find `uv@1.2.3(.exe)`, for folks who like managing multiple
/// installs in this way.
fn get_uvw_suffix(current_exe: &Path) -> Option<&str> {
    let os_file_name = current_exe.file_name()?;
    let file_name_str = os_file_name.to_str()?;
    file_name_str.strip_prefix("uvw")
}

/// Gets the path to `uv`, given info about `uvw`
fn get_uv_path(current_exe_parent: &Path, uvw_suffix: Option<&str>) -> std::io::Result<PathBuf> {
    // First try to find a matching suffixed `uv`, e.g. `uv@1.2.3(.exe)`
    let uv_with_suffix = uvw_suffix.map(|suffix| current_exe_parent.join(format!("uv{suffix}")));
    if let Some(uv_with_suffix) = &uv_with_suffix {
        #[allow(clippy::print_stderr, reason = "printing a very rare warning")]
        match uv_with_suffix.try_exists() {
            Ok(true) => return Ok(uv_with_suffix.to_owned()),
            Ok(false) => { /* definitely not there, proceed to fallback */ }
            Err(err) => {
                // We don't know if `uv@1.2.3` exists, something errored when checking.
                // We *could* blindly use `uv@1.2.3` in this case, as the code below does, however
                // in this extremely narrow corner case it's *probably* better to default to `uv`,
                // since we don't want to mess up existing users who weren't using suffixes?
                eprintln!(
                    "warning: failed to determine if `{}` exists, trying `uv` instead: {err}",
                    uv_with_suffix.display()
                );
            }
        }
    }

    // Then just look for good ol' `uv`
    let uv = current_exe_parent.join(format!("uv{}", std::env::consts::EXE_SUFFIX));
    // If we are sure the `uv` binary does not exist, display a clearer error message.
    // If we're not certain if uv exists (try_exists == Err), keep going and hope it works.
    if matches!(uv.try_exists(), Ok(false)) {
        let message = if let Some(uv_with_suffix) = uv_with_suffix {
            format!(
                "Could not find the `uv` binary at either of:\n  {}\n  {}",
                uv_with_suffix.display(),
                uv.display(),
            )
        } else {
            format!("Could not find the `uv` binary at: {}", uv.display())
        };
        Err(std::io::Error::new(std::io::ErrorKind::NotFound, message))
    } else {
        Ok(uv)
    }
}

fn run() -> std::io::Result<ExitStatus> {
    let current_exe = std::env::current_exe()?;
    let Some(bin) = current_exe.parent() else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Could not determine the location of the `uvw` binary",
        ));
    };
    let uvw_suffix = get_uvw_suffix(&current_exe);
    let uv = get_uv_path(bin, uvw_suffix)?;
    let args = std::env::args_os()
        // Skip the `uvw` name
        .skip(1)
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
            eprintln!("error: {err}");
            ExitCode::from(2)
        }
    }
}
