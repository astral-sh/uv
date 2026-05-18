#![cfg(unix)]

use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const NONBLOCKING_CHILD_ENV: &str = "UV_CTRL_C_NONBLOCKING_CHILD";

#[test]
fn test_on_ctrl_c() {
    let flag = Arc::new(AtomicBool::new(false));
    let flag_handler = Arc::clone(&flag);

    uv_sys::on_ctrl_c(move || {
        flag_handler.store(true, Ordering::SeqCst);
    })
    .unwrap();

    // Raise SIGINT to ourselves.
    nix::sys::signal::raise(nix::sys::signal::Signal::SIGINT).unwrap();

    // Give the handler thread time to wake up and run.
    std::thread::sleep(Duration::from_millis(100));
    assert!(flag.load(Ordering::SeqCst));

    // A second registration must fail.
    let result = uv_sys::on_ctrl_c(|| {});
    assert!(result.is_err());
}

#[test]
fn test_on_ctrl_c_signal_delivery_remains_nonblocking() {
    if std::env::var_os(NONBLOCKING_CHILD_ENV).is_some() {
        run_nonblocking_child();
        return;
    }

    let mut child = Command::new(std::env::current_exe().expect("test binary path"))
        .arg("--exact")
        .arg("test_on_ctrl_c_signal_delivery_remains_nonblocking")
        .arg("--nocapture")
        .env(NONBLOCKING_CHILD_ENV, "1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn Ctrl-C nonblocking child");

    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(status) = child.try_wait().expect("poll Ctrl-C nonblocking child") {
            assert!(status.success(), "child exited with {status}");
            break;
        }

        if Instant::now() >= deadline {
            let _ = child.kill();
            panic!("Ctrl-C nonblocking child timed out");
        }

        std::thread::sleep(Duration::from_millis(10));
    }
}

fn run_nonblocking_child() {
    let (entered_tx, entered_rx) = mpsc::sync_channel(1);
    let (_release_tx, release_rx) = mpsc::sync_channel::<()>(1);

    uv_sys::on_ctrl_c(move || {
        let _ = entered_tx.send(());
        let _ = release_rx.recv();
    })
    .expect("register Ctrl-C handler");

    nix::sys::signal::raise(nix::sys::signal::Signal::SIGINT).expect("raise initial SIGINT");
    entered_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("Ctrl-C handler should start");

    for _ in 0..100_000 {
        nix::sys::signal::raise(nix::sys::signal::Signal::SIGINT).expect("raise SIGINT");
    }
}
