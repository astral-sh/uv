#![cfg(unix)]

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

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
