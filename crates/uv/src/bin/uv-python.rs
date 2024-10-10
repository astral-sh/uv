use std::convert::Infallible;
use std::io::Write;
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

#[derive(Debug)]
enum Error {
    Io(std::io::Error),
    Which(which::Error),
    NoInterpreter(String),
    RecursiveQuery,
}

#[derive(Debug, Default)]
struct Options {
    request: Option<String>,
    system: bool,
    managed: bool,
    verbose: bool,
}

impl Options {
    fn as_args(&self) -> Vec<&str> {
        let mut args = Vec::new();
        if let Some(request) = &self.request {
            args.push(request.as_str());
        } else {
            // By default, we should never select an alternative implementation with the shim
            args.push("cpython");
        }
        if self.system {
            args.push("--system");
        }
        if self.verbose {
            args.push("--verbose");
        }
        if self.managed {
            args.push("--python-preference");
            args.push("only-managed");
        }
        args
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::Io(err) => write!(f, "{err}"),
            Self::Which(err) => write!(f, "Failed to find uv binary: {err}"),
            Self::NoInterpreter(inner) => write!(f, "{inner}"),
            Self::RecursiveQuery => write!(f, "Ignoring recursive query from uv"),
        }
    }
}

/// Parse `+<option>` into [`Options`].
///
/// Supports the following options:
///
/// - `+system`: Use the system Python, ignore virtual environments.
/// - `+managed`: Use only managed Python installations.
/// - `+<request>`: Request a Python version
/// - `+v`: Enable verbose mode.
fn parse_options(mut args: Vec<OsString>) -> (Vec<OsString>, Options) {
    let mut position = 0;
    let mut options = Options::default();
    while position < args.len() {
        let arg = &args[position].to_string_lossy();

        // If the argument doesn't start with `+`, we're done.
        let Some(option) = arg.strip_prefix('+') else {
            break;
        };

        match option {
            "system" => options.system = true,
            "managed" => options.managed = true,
            "v" => options.verbose = true,
            _ => options.request = Some(option.to_string()),
        }

        position += 1;
    }

    (args.split_off(position), options)
}

/// Find the `uv` binary to use.
fn find_uv() -> Result<std::path::PathBuf, Error> {
    // We prefer one next to the current binary.
    let current_exe = std::env::current_exe().map_err(Error::Io)?;
    if let Some(bin) = current_exe.parent() {
        let uv = bin.join("uv");
        if uv.exists() {
            return Ok(uv);
        }
    }
    // Otherwise, we'll search for it on the `PATH`.
    which::which("uv").map_err(Error::Which)
}

fn run() -> Result<ExitStatus, Error> {
    if std::env::var_os("UV_INTERNAL__PYTHON_QUERY").is_some() {
        return Err(Error::RecursiveQuery);
    }

    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    let (args, options) = parse_options(args);
    let uv = find_uv()?;
    let mut cmd = Command::new(uv);
    let uv_args = ["python", "find"].iter().copied().chain(options.as_args());
    cmd.args(uv_args);
    let output = cmd.output().map_err(Error::Io)?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(output.stderr.as_slice());
        return Err(Error::NoInterpreter(
            stderr
                .strip_prefix("error: ")
                .unwrap_or(&*stderr)
                .to_string(),
        ));
    }

    // If verbose is enabled, print the output of the `uv python find` command
    if options.verbose {
        std::io::stderr()
            .write_all(&output.stderr)
            .map_err(Error::Io)?;
    }

    let python = std::path::PathBuf::from(String::from_utf8_lossy(output.stdout.as_slice()).trim());
    let mut cmd = Command::new(python);
    cmd.args(&args);
    match exec_spawn(&mut cmd).map_err(Error::Io)? {}
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
