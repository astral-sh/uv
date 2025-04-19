use futures::{channel::mpsc::channel, SinkExt, StreamExt};
use notify_debouncer_mini::{new_debouncer, notify::RecursiveMode};
use std::{path::Path, time::Duration};
use tokio::process::Command;
use tokio::select;
use tokio::signal::unix::{signal as handle_signal, SignalKind};
use tracing::debug;

use crate::commands::ExitStatus;

const DEBOUNCE_DURATION_MS: u64 = 10;

pub(crate) async fn run_and_watch(
    process: &mut Command,
    path: &Path,
    no_clear_screen: bool,
) -> anyhow::Result<ExitStatus> {
    let (mut file_change_sender, mut file_change_receiver) = channel(1);
    let mut debouncer = new_debouncer(Duration::from_millis(DEBOUNCE_DURATION_MS), move |res| {
        futures::executor::block_on(async {
            file_change_sender.send(res).await.unwrap();
        });
    })?;
    debouncer
        .watcher()
        .watch(path, RecursiveMode::NonRecursive)?;
    if !no_clear_screen {
        clearscreen::clear()?;
    }
    let mut handle = process.spawn()?;
    let mut sigterm_handle = handle_signal(SignalKind::terminate())?;
    let mut sigint_handle = handle_signal(SignalKind::interrupt())?;
    loop {
        select! {
            Some(Ok(events)) = file_change_receiver.next() => {
                for event in &events {
                    debug!("File change detected: {:?}", event.path);
                }
                // kill currently running command
                handle.kill().await?;
                if !no_clear_screen {
                    clearscreen::clear()?;
                }
                // run command
                handle = process.spawn()?;
            },
            _ = sigint_handle.recv() => {
                // kill command and exit
                handle.kill().await?;
                break;
            },
            _ = sigterm_handle.recv() => {
                // kill command and exit
                handle.kill().await?;
                break;
            },
        }
    }
    Ok(ExitStatus::Success)
}
