use crate::commands::ExitStatus;
use tokio::process::Child;
use tracing::debug;

/// Wait for the child process to complete, handling signals and error codes.
pub(crate) async fn run_to_completion(mut handle: Child) -> anyhow::Result<ExitStatus> {
    // Ignore signals in the parent process, deferring them to the child. This is safe as long as
    // the command is the last thing that runs in this process; otherwise, we'd need to restore the
    // signal handlers after the command completes.
    let _handler = tokio::spawn(async { while tokio::signal::ctrl_c().await.is_ok() {} });

    // Exit based on the result of the command.
    #[cfg(unix)]
    let status = {
        use tokio::select;
        use tokio::signal::unix::{signal, SignalKind};

        let mut term_signal = signal(SignalKind::terminate())?;
        loop {
            select! {
                result = handle.wait() => {
                    break result;
                },

                // `SIGTERM`
                _ = term_signal.recv() => {
                    let _ = terminate_process(&mut handle);
                }
            };
        }
    }?;

    #[cfg(not(unix))]
    let status = handle.wait().await?;

    if let Some(code) = status.code() {
        debug!("Command exited with code: {code}");
        if let Ok(code) = u8::try_from(code) {
            Ok(ExitStatus::External(code))
        } else {
            #[allow(clippy::exit)]
            std::process::exit(code);
        }
    } else {
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            debug!("Command exited with signal: {:?}", status.signal());
            // Following https://tldp.org/LDP/abs/html/exitcodes.html, a fatal signal n gets the
            // exit code 128+n
            if let Some(mapped_code) = status
                .signal()
                .and_then(|signal| u8::try_from(signal).ok())
                .and_then(|signal| 128u8.checked_add(signal))
            {
                return Ok(ExitStatus::External(mapped_code));
            }
        }
        Ok(ExitStatus::Failure)
    }
}

#[cfg(unix)]
fn terminate_process(child: &mut Child) -> anyhow::Result<()> {
    use anyhow::Context;
    use nix::sys::signal::{self, Signal};
    use nix::unistd::Pid;

    let pid = child.id().context("Failed to get child process ID")?;
    signal::kill(Pid::from_raw(pid.try_into()?), Signal::SIGTERM).context("Failed to send SIGTERM")
}
