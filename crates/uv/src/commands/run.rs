use crate::commands::ExitStatus;
use tokio::process::Child;
use tracing::debug;

/// Wait for the child process to complete, handling signals and error codes.
///
/// Note that this registers handles to ignore some signals in the parent process. This is safe as
/// long as the command is the last thing that runs in this process; otherwise, we'd need to restore
/// the default signal handlers after the command completes.
pub(crate) async fn run_to_completion(mut handle: Child) -> anyhow::Result<ExitStatus> {
    // On Unix, shells will send SIGINT to the active process group when a user presses `Ctrl-C`. In
    // general, this means that uv should ignore SIGINT, allowing the child process to cleanly exit
    // instead. If uv forwarded the SIGINT immediately, the child process would receive _two_ SIGINT
    // signals which has semantic meaning for some programs, i.e., slow exit on the first signal and
    // fast exit on the second. The exception to this is if a child process changes its process
    // group, in which case the shell will _not_ send SIGINT to the child process and uv must take
    // ownership of forwarding the signal.
    //
    // Note this assumes an interactive shell. If a signal is sent directly to the uv parent process
    // (e.g., `kill -2 <pid>`), the process group is not involved and a signal is not sent to the
    // child by default. In this context, uv must forward the signal to the child. We work around
    // this by forwarding SIGINT if it is received more than once. We could attempt to infer if the
    // parent is a shell using TTY detection(?), but there hasn't been sufficient motivation to
    // explore alternatives yet.
    //
    // Use of SIGTERM is also a bit complicated. If a shell receives a SIGTERM, it just waits for
    // its children to exit — multiple SIGTERMs do not have any effect and the signals are not
    // forwarded to the children. Consequently, the description for SIGINT above does not apply to
    // SIGTERM in shells. It is _possible_ to have a parent process that sends a SIGTERM to the
    // process group; for example, `tini` supports this via a `-g` option. In this case, it's
    // possible that uv will improperly send a second SIGTERM to the child process. However,
    // this seems preferable to not forwarding it in the first place. In the Docker case, if `uv`
    // is invoked directly (instead of via an init system), it's PID 1 which has a special-cased
    // default signal handler for SIGTERM by default. Generally, if a process receives a SIGTERM and
    // does not have a SIGTERM handler, it is terminated. However, if PID 1 receives a SIGTERM, it
    // is not terminated. In this context, it is essential for uv to forward the SIGTERM to the
    // child process or the process will not be killable.
    #[cfg(unix)]
    let status = {
        use std::ops::Deref;

        use nix::sys::signal;
        use nix::unistd::{getpgid, Pid};
        use tokio::select;
        use tokio::signal::unix::{signal as handle_signal, SignalKind};

        /// Simple new type for `Pid` allowing construction from [`Child`].
        ///
        /// `None` if the child process has exited or the PID is invalid.
        struct ChildPid(Option<Pid>);

        impl Deref for ChildPid {
            type Target = Option<Pid>;
            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }

        impl From<&Child> for ChildPid {
            fn from(child: &Child) -> Self {
                Self(
                    child
                        .id()
                        .and_then(|id| id.try_into().ok())
                        .map(Pid::from_raw),
                )
            }
        }

        // Get the parent PGID
        let parent_pgid = getpgid(None)?;
        if let Some(child_pid) = *ChildPid::from(&handle) {
            debug!("Spawned child {child_pid} in process group {parent_pgid}");
        }

        let mut sigterm_handle = handle_signal(SignalKind::terminate())?;
        let mut sigint_handle = handle_signal(SignalKind::interrupt())?;
        let mut sigint_count = 0;

        loop {
            select! {
                result = handle.wait() => {
                    break result;
                },
                _ = sigint_handle.recv() => {
                    // See above for commentary on handling of SIGINT.

                    // If the child has already exited, we can't send it signals
                    let Some(child_pid) = *ChildPid::from(&handle) else {
                        debug!("Received SIGINT, but the child has already exited");
                        continue;
                    };

                    // Check if the child pgid has changed
                    let child_pgid = getpgid(Some(child_pid))?;

                    // Increment the number of interrupts seen
                    sigint_count += 1;

                    // If the pgid _differs_ from the parent, the child will not receive a SIGINT
                    // and we should forward it. If we've received multiple SIGINTs, forward it
                    // regardless.
                    if child_pgid == parent_pgid && sigint_count < 2 {
                        debug!("Received SIGINT, assuming the child received it as part of the process group");
                        continue;
                    }

                    debug!("Received SIGINT, forwarding to child at {child_pid}");
                    let _ = signal::kill(child_pid, signal::Signal::SIGINT);
                },
                _ = sigterm_handle.recv() => {
                    // If the child has already exited, we can't send it signals
                    let Some(child_pid) = *ChildPid::from(&handle) else {
                        debug!("Received SIGINT, but the child has already exited");
                        continue;
                    };

                    // We unconditionally forward SIGTERM to the child process; unlike SIGINT, this
                    // isn't usually handled by the shell and in cases like
                    debug!("Received SIGTERM, forwarding to child at {child_pid}");
                    let _ = signal::kill(child_pid, signal::Signal::SIGTERM);
                }
            };
        }
    }?;

    // On Windows, we just ignore the console CTRL_C_EVENT and assume it will always be sent to the
    // child by the console. There's not a clear programmatic way to forward the signal anyway.
    #[cfg(not(unix))]
    let status = {
        let _ctrl_c_handler =
            tokio::spawn(async { while tokio::signal::ctrl_c().await.is_ok() {} });
        handle.wait().await?
    };

    // Exit based on the result of the command.
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
